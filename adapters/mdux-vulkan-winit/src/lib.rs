//! Reusable Vulkan 1.0 + winit presentation adapter for MduX applications (ADR-005/ADR-012 edge
//! adapter crate). Consumes only owned data from governed `mdux` crates — a built `Framework` and
//! a `&'static CompiledScreenPackage` — and owns everything platform-specific (window creation,
//! the Vulkan instance/device/swapchain/pipeline, glyph-atlas upload, and the winit event loop)
//! so applications never need to depend on `ash`/`winit`/`shaderc` themselves.
//!
//! ```no_run
//! # fn build_framework() -> mdux::MduxResult<mdux::Framework> { unimplemented!() }
//! # fn screen() -> &'static mdux::CompiledScreenPackage { unimplemented!() }
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let framework = build_framework()?;
//! mdux_vulkan_winit::App::new(framework, screen()).run_from_env()
//! # }
//! ```

mod renderer;

use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

use mdux::input::{FrameEvents, WidgetEvent};
use mdux::realtime::{ButtonBinding, FrameInputs, ScreenBindings, TextInputBinding};
use mdux::{
    screen_text::ScreenTextLayout, CompiledNodeKind, CompiledScreenPackage, Framework,
    GraphicsProfile, Rect, SystemEvent,
};
use renderer::{civil_from_unix, BoxError, InteractionSnapshot, VulkanRenderer};
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::WindowBuilder,
};

/// Flags controlling how [`App::run`] behaves; [`App::run_from_env`] derives this from
/// `std::env::args()` so applications get a consistent CLI without wiring it up themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Skip opening a window entirely (no Vulkan instance/device is created); prints the same
    /// framework diagnostics and exits. Intended for CI / non-graphical hosts.
    pub headless_smoke: bool,
    /// Close the window automatically after this duration, for scripted/manual smoke checks.
    pub auto_close_after: Option<Duration>,
}

/// Runs a compiled MedUI screen against a built [`Framework`] through a Vulkan-backed winit
/// window.
pub struct App {
    framework: Framework,
    screen: &'static CompiledScreenPackage,
    locale: String,
    realtime: Option<Box<dyn FnMut(&mut FrameInputs)>>,
    input: Option<Box<dyn FnMut(&mut FrameEvents, &mut FrameInputs)>>,
}

impl App {
    pub fn new(framework: Framework, screen: &'static CompiledScreenPackage) -> Self {
        Self {
            framework,
            screen,
            locale: "en-US".to_string(),
            realtime: None,
            input: None,
        }
    }

    /// Overrides the locale used to resolve the screen's approved text (default `en-US`).
    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = locale.into();
        self
    }

    /// Registers the application's realtime closure, invoked once per frame before recording:
    /// write live values into the [`FrameInputs`] mailbox (`set_number`, `set_status`,
    /// `push_row`). The clock needs no code — the adapter feeds it from the platform clock.
    /// Without a closure, dynamic widgets render their initial values (zeros / first state).
    pub fn with_realtime(mut self, closure: impl FnMut(&mut FrameInputs) + 'static) -> Self {
        self.realtime = Some(Box::new(closure));
        self
    }

    /// Registers the application's input closure (ADR-015), invoked once per frame *before* the
    /// realtime closure: drain the [`FrameEvents`] queue, apply the drained events to your own
    /// state (typically a `TextInputModel` per text source), and echo text content back through
    /// [`FrameInputs::set_text`]. Without a closure, interaction events accumulate and are
    /// dropped-and-counted once the bounded queue fills; existing `with_realtime` applications
    /// compile and behave unchanged.
    pub fn with_input(
        mut self,
        closure: impl FnMut(&mut FrameEvents, &mut FrameInputs) + 'static,
    ) -> Self {
        self.input = Some(Box::new(closure));
        self
    }

    /// Parses `--headless-smoke` and `--auto-close-ms=<millis>` from `std::env::args()` and
    /// calls [`run`](Self::run).
    pub fn run_from_env(self) -> Result<(), BoxError> {
        let mut headless_smoke = false;
        let mut auto_close_after = None;

        for argument in std::env::args().skip(1) {
            if argument == "--headless-smoke" {
                headless_smoke = true;
            } else if let Some(value) = argument.strip_prefix("--auto-close-ms=") {
                let millis: u64 = value
                    .parse()
                    .map_err(|error| format!("invalid --auto-close-ms value '{value}': {error}"))?;
                auto_close_after = Some(Duration::from_millis(millis));
            }
        }

        self.run(RunOptions {
            headless_smoke,
            auto_close_after,
        })
    }

    /// Prints framework/compliance diagnostics, then either exits immediately
    /// (`options.headless_smoke`) or opens a window and drives the Vulkan renderer until closed.
    ///
    /// A framework carrying the `VulkanSc` graphics profile runs as an ADR-013 **host
    /// development preview**: the banner below is printed among the diagnostics and a
    /// `Runtime` audit event is recorded before anything renders, so every preview execution is
    /// self-documenting in the exported audit log. No governed validation is relaxed.
    pub fn run(mut self, options: RunOptions) -> Result<(), BoxError> {
        let is_sc_preview =
            self.framework.ui_runtime().config().graphics_profile == GraphicsProfile::VulkanSc;
        if is_sc_preview {
            self.framework.record_runtime_event(
                "vulkan sc host preview: rendering on standard Vulkan for development only",
            );
        }

        println!(
            "screen id={} nodes={} golden_refs={}",
            self.screen.screen_id,
            self.screen.nodes.len(),
            self.screen.golden_references.len()
        );
        if is_sc_preview {
            println!(
                "profile=Vulkan SC (HOST PREVIEW on standard Vulkan — not the certified pipeline)"
            );
        }
        println!("{}", self.framework.release_summary());
        let frame = self.framework.render_preview_frame(1);
        println!(
            "preview_frame index={} draw_calls={} frame_time_ms={} dynamic_allocations={}",
            frame.frame_index, frame.draw_calls, frame.frame_time_ms, frame.dynamic_allocations
        );
        println!("trace_matrix\n{}", self.framework.trace_matrix_export());
        println!("audit_log\n{}", self.framework.audit_export());

        if options.headless_smoke {
            println!("headless_smoke=ok");
            return Ok(());
        }

        let standard_package = mdux::default_standard_text_package()?;
        let display_packages = mdux::default_display_text_packages()?;
        let image_packages = mdux::default_image_packages()?;
        let layout =
            ScreenTextLayout::from_screen(self.screen, standard_package.clone(), &self.locale)?;
        let bindings = ScreenBindings::from_screen(
            self.screen,
            standard_package,
            display_packages,
            &image_packages,
            &self.locale,
        )?;
        let frame_inputs = FrameInputs::from_bindings(&bindings)?;
        let app_name = self.framework.identity().name.clone();
        let config = self.framework.ui_runtime().config().clone();

        // CriticalButtons carry no realtime binding (static text kinds), so their dispatch
        // targets are collected from the compiled screen directly.
        let critical_buttons: Vec<CriticalButtonTarget> = self
            .screen
            .nodes
            .iter()
            .filter_map(|node| match node.kind {
                CompiledNodeKind::CriticalButton(spec) => Some(CriticalButtonTarget {
                    node_id: node.id,
                    bounds: node.bounds,
                    on_press: spec.on_press,
                }),
                _ => None,
            })
            .collect();

        run_windowed(
            &app_name,
            config.width,
            config.height,
            layout,
            bindings,
            frame_inputs,
            self.realtime,
            self.input,
            critical_buttons,
            self.framework,
            options.auto_close_after,
        )
    }
}

/// A `CriticalButton`'s dispatch target: unlike `Button`, its press semantics are
/// framework-governed (ADR-015 — `TriggerHalt` is audited and halts the loop; `NoOp` is
/// forwarded to the application).
struct CriticalButtonTarget {
    node_id: &'static str,
    bounds: Rect,
    on_press: SystemEvent,
}

/// What the pointer pressed on and has not yet released (standard button arming: the press is
/// delivered only when the release lands inside the same target).
#[derive(Clone, Copy, Eq, PartialEq)]
enum PressTarget {
    Button(usize),
    Critical(usize),
}

/// Fixed-size interaction bookkeeping owned by the event loop (ADR-015): pointer position,
/// armed press target, the single focus slot, and the caret — presentation state, like the
/// platform-fed clock, never application state. The application's authoritative caret lives in
/// its `TextInputModel`; both follow the same transition rules over the same event stream, and
/// this caret re-clamps against the echoed text every frame.
struct InteractionState {
    cursor: (f64, f64),
    armed: Option<PressTarget>,
    focused_input: Option<usize>,
    caret: u16,
}

impl InteractionState {
    fn new() -> Self {
        Self {
            cursor: (-1.0, -1.0),
            armed: None,
            focused_input: None,
            caret: 0,
        }
    }
}

fn rect_contains_point(bounds: &Rect, x: f64, y: f64) -> bool {
    x >= f64::from(bounds.x)
        && y >= f64::from(bounds.y)
        && x < f64::from(bounds.x) + f64::from(bounds.width)
        && y < f64::from(bounds.y) + f64::from(bounds.height)
}

#[allow(clippy::too_many_arguments)]
fn run_windowed(
    app_name: &str,
    width: u32,
    height: u32,
    layout: ScreenTextLayout,
    bindings: ScreenBindings,
    mut frame_inputs: FrameInputs,
    mut realtime: Option<Box<dyn FnMut(&mut FrameInputs)>>,
    mut input: Option<Box<dyn FnMut(&mut FrameEvents, &mut FrameInputs)>>,
    critical_buttons: Vec<CriticalButtonTarget>,
    mut framework: Framework,
    auto_close_after: Option<Duration>,
) -> Result<(), BoxError> {
    let title = format!("{app_name} - Vulkan");
    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title(&title)
        .with_inner_size(LogicalSize::new(width as f64, height as f64))
        .build(&event_loop)?;

    // Interaction targets are cloned out of the bindings before they move into the renderer:
    // hit-test bounds for buttons and text inputs, plus each input's allowed characters (the
    // adapter filters typed characters against the baked charset so an out-of-charset key can
    // never reach the application's echo path).
    let button_targets: Vec<ButtonBinding> = bindings.buttons.clone();
    let input_targets: Vec<TextInputBinding> = bindings.text_inputs.clone();
    let input_charsets: Vec<Vec<char>> = input_targets
        .iter()
        .map(|binding| {
            bindings
                .standard
                .find_numeric_glyph_set(&binding.glyph_set_id)
                .map(|glyph_set| glyph_set.entries.iter().map(|entry| entry.character).collect())
                .unwrap_or_default()
        })
        .collect();

    let mut renderer = Some(VulkanRenderer::new(
        &window, app_name, layout, bindings, width, height,
    )?);
    println!(
        "vulkan_device={}",
        renderer
            .as_ref()
            .map(VulkanRenderer::device_name)
            .unwrap_or("unknown")
    );

    let started_at = Instant::now();
    let window_id = window.id();
    let render_error: Rc<RefCell<Option<BoxError>>> = Rc::new(RefCell::new(None));
    let render_error_for_closure = Rc::clone(&render_error);

    let mut interaction = InteractionState::new();
    let mut events = FrameEvents::new();

    event_loop.run(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent {
                window_id: id,
                event,
            } if id == window_id => match event {
                WindowEvent::CloseRequested => {
                    report_input_drops(&events);
                    shutdown_renderer(&mut renderer);
                    event_loop_window_target.exit();
                }
                WindowEvent::Resized(_) => {}
                WindowEvent::CursorMoved { position, .. } => {
                    // Hit-testing happens in the logical UI surface the screen was authored in;
                    // the pointer arrives in physical pixels (HiDPI scale ≥ 1).
                    let logical = position.to_logical::<f64>(window.scale_factor());
                    interaction.cursor = (logical.x, logical.y);
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    let (x, y) = interaction.cursor;
                    interaction.armed = button_targets
                        .iter()
                        .position(|target| rect_contains_point(&target.bounds, x, y))
                        .map(PressTarget::Button)
                        .or_else(|| {
                            critical_buttons
                                .iter()
                                .position(|target| rect_contains_point(&target.bounds, x, y))
                                .map(PressTarget::Critical)
                        });

                    // Single focus slot: clicking an input focuses it (caret at the end of the
                    // echoed content); clicking anywhere else clears focus.
                    let hit_input = input_targets
                        .iter()
                        .position(|target| rect_contains_point(&target.bounds, x, y));
                    if hit_input != interaction.focused_input {
                        interaction.focused_input = hit_input;
                        events.push(WidgetEvent::FocusChanged {
                            source: hit_input.map(|index| input_targets[index].source),
                        });
                        if let Some(index) = hit_input {
                            let source = input_targets[index].source;
                            interaction.caret = echoed_len(&frame_inputs, source);
                            events.push(WidgetEvent::CaretMoved {
                                source,
                                position: interaction.caret,
                            });
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some(armed) = interaction.armed.take() {
                        let (x, y) = interaction.cursor;
                        match armed {
                            PressTarget::Button(index)
                                if rect_contains_point(&button_targets[index].bounds, x, y) =>
                            {
                                events.push(WidgetEvent::ButtonPressed {
                                    source: button_targets[index].source,
                                });
                            }
                            PressTarget::Critical(index)
                                if rect_contains_point(
                                    &critical_buttons[index].bounds,
                                    x,
                                    y,
                                ) =>
                            {
                                let target = &critical_buttons[index];
                                match target.on_press {
                                    // Framework-governed dispatch (ADR-015): the halt is
                                    // audited evidence, then the loop exits in order.
                                    SystemEvent::TriggerHalt => {
                                        framework.record_runtime_event(format!(
                                            "critical button {} pressed: trigger halt — orderly shutdown",
                                            target.node_id
                                        ));
                                        println!(
                                            "critical_halt node={} (runtime audit event recorded)",
                                            target.node_id
                                        );
                                        report_input_drops(&events);
                                        shutdown_renderer(&mut renderer);
                                        event_loop_window_target.exit();
                                    }
                                    SystemEvent::NoOp => {
                                        events.push(WidgetEvent::CriticalButtonPressed {
                                            node_id: target.node_id,
                                            action: SystemEvent::NoOp,
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                WindowEvent::KeyboardInput { event: key_event, .. }
                    if key_event.state == ElementState::Pressed =>
                {
                    match &key_event.logical_key {
                        Key::Named(NamedKey::Tab) => {
                            // Tab cycles focus through text inputs in document order.
                            if !input_targets.is_empty() {
                                let next = interaction
                                    .focused_input
                                    .map(|index| (index + 1) % input_targets.len())
                                    .unwrap_or(0);
                                interaction.focused_input = Some(next);
                                let source = input_targets[next].source;
                                interaction.caret = echoed_len(&frame_inputs, source);
                                events.push(WidgetEvent::FocusChanged { source: Some(source) });
                                events.push(WidgetEvent::CaretMoved {
                                    source,
                                    position: interaction.caret,
                                });
                            }
                        }
                        Key::Named(NamedKey::Escape) => {
                            if interaction.focused_input.take().is_some() {
                                events.push(WidgetEvent::FocusChanged { source: None });
                            }
                        }
                        logical_key => {
                            if let Some(index) = interaction.focused_input {
                                handle_editing_key(
                                    logical_key,
                                    &input_targets[index],
                                    &input_charsets[index],
                                    &frame_inputs,
                                    &mut interaction,
                                    &mut events,
                                );
                            }
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Some(active_renderer) = renderer.as_mut() {
                        // Per-frame order (ADR-015): drain events into the application, echo
                        // text, then the realtime closure, then draw.
                        if let Some(input) = input.as_mut() {
                            input(&mut events, &mut frame_inputs);
                        }
                        if let Some(index) = interaction.focused_input {
                            let len = echoed_len(&frame_inputs, input_targets[index].source);
                            interaction.caret = interaction.caret.min(len);
                        }
                        if let Some(realtime) = realtime.as_mut() {
                            realtime(&mut frame_inputs);
                        }
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|elapsed| elapsed.as_secs() as i64)
                            .unwrap_or(0);
                        let clock = civil_from_unix(now);
                        // A button face renders pressed while armed AND still under the
                        // pointer — releasing outside disarms visually, matching dispatch.
                        let (cursor_x, cursor_y) = interaction.cursor;
                        let snapshot = InteractionSnapshot {
                            pressed_button: match interaction.armed {
                                Some(PressTarget::Button(index))
                                    if rect_contains_point(
                                        &button_targets[index].bounds,
                                        cursor_x,
                                        cursor_y,
                                    ) =>
                                {
                                    Some(index)
                                }
                                _ => None,
                            },
                            focused_input: interaction.focused_input,
                            caret: interaction.caret,
                        };
                        if let Err(error) =
                            active_renderer.draw_frame(&window, &frame_inputs, clock, snapshot)
                        {
                            eprintln!("failed to render frame: {error}");
                            *render_error_for_closure.borrow_mut() = Some(error);
                            report_input_drops(&events);
                            shutdown_renderer(&mut renderer);
                            event_loop_window_target.exit();
                        }
                    }
                }
                _ => {}
            },
            Event::AboutToWait => {
                if let Some(auto_close_after) = auto_close_after {
                    if started_at.elapsed() >= auto_close_after {
                        report_input_drops(&events);
                        shutdown_renderer(&mut renderer);
                        event_loop_window_target.exit();
                    }
                }

                window.request_redraw();
            }
            _ => {}
        }
    })?;

    if let Some(error) = render_error.borrow_mut().take() {
        return Err(error);
    }

    Ok(())
}

/// Routes one pressed key to the focused text input: editing keys become `WidgetEvent`s and the
/// adapter-side caret mirrors the transition the application's `TextInputModel` will apply to
/// the same event. Typed characters are filtered against the input's baked charset here, so an
/// out-of-charset key never reaches the application's echo path.
fn handle_editing_key(
    logical_key: &Key,
    binding: &TextInputBinding,
    allowed: &[char],
    frame_inputs: &FrameInputs,
    interaction: &mut InteractionState,
    events: &mut FrameEvents,
) {
    let source = binding.source;
    let len = echoed_len(frame_inputs, source);
    match logical_key {
        Key::Named(NamedKey::Backspace) => {
            if interaction.caret > 0 {
                interaction.caret -= 1;
            }
            events.push(WidgetEvent::Backspace { source });
        }
        Key::Named(NamedKey::Delete) => {
            events.push(WidgetEvent::Delete { source });
        }
        Key::Named(NamedKey::ArrowLeft) => {
            interaction.caret = interaction.caret.saturating_sub(1);
            events.push(WidgetEvent::CaretMoved { source, position: interaction.caret });
        }
        Key::Named(NamedKey::ArrowRight) => {
            interaction.caret = interaction.caret.saturating_add(1).min(len);
            events.push(WidgetEvent::CaretMoved { source, position: interaction.caret });
        }
        Key::Named(NamedKey::Home) => {
            interaction.caret = 0;
            events.push(WidgetEvent::CaretMoved { source, position: 0 });
        }
        Key::Named(NamedKey::End) => {
            interaction.caret = len;
            events.push(WidgetEvent::CaretMoved { source, position: len });
        }
        Key::Named(NamedKey::Enter) => {
            events.push(WidgetEvent::TextCommitted { source });
        }
        Key::Named(NamedKey::Space) => {
            push_character(' ', binding, allowed, len, interaction, events);
        }
        Key::Character(text) => {
            for character in text.chars() {
                push_character(character, binding, allowed, len, interaction, events);
            }
        }
        _ => {}
    }
}

fn push_character(
    character: char,
    binding: &TextInputBinding,
    allowed: &[char],
    echoed_length: u16,
    interaction: &mut InteractionState,
    events: &mut FrameEvents,
) {
    if !allowed.contains(&character) {
        return;
    }
    if echoed_length < binding.max_length {
        interaction.caret = interaction.caret.saturating_add(1).min(binding.max_length);
    }
    events.push(WidgetEvent::CharTyped { source: binding.source, character });
}

/// Character count of a text source's last echoed content.
fn echoed_len(frame_inputs: &FrameInputs, source: &str) -> u16 {
    frame_inputs
        .text(source)
        .map(|text| text.chars().count().min(usize::from(u16::MAX)) as u16)
        .unwrap_or(0)
}

/// Surfaces the bounded queue's overflow counter at exit (ADR-015: a dropped burst is a
/// visible, auditable fact, never a silent one).
fn report_input_drops(events: &FrameEvents) {
    if events.dropped_events() > 0 {
        println!("input_dropped_events={}", events.dropped_events());
    }
}

/// Drops the renderer (releasing every Vulkan resource through `VulkanRenderer::drop`) while the
/// window's platform resources are still alive, before asking the event loop to exit.
///
/// Historical note (issue #28): this used to `mem::forget` the renderer because dropping it
/// segfaulted inside `vkDestroyDevice` on real hardware. The root cause was not a driver or
/// teardown-ordering bug: `VulkanRenderer::new` let ash's `Entry` — which owns the dlopened
/// libvulkan — drop at the end of the constructor, unmapping the Vulkan loader while direct-ICD
/// device calls kept working. The first loader-trampoline call (`vkDestroyDevice`) then jumped
/// into unmapped memory. The renderer now keeps the `Entry` alive for its whole lifetime, and
/// normal drops are safe again.
fn shutdown_renderer(renderer: &mut Option<VulkanRenderer>) {
    *renderer = None;
}
