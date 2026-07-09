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

mod offscreen;
mod renderer;
mod verify;

use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};

use mdux::input::{FrameEvents, WidgetEvent};
use mdux::realtime::{ButtonBinding, FrameInputs, ScreenBindings, TextInputBinding};
use mdux::verify_scenario::ScenarioScript;
use mdux::{
    screen_text::ScreenTextLayout, CompiledNodeKind, CompiledScreenPackage, Framework,
    GraphicsProfile, Rect, SystemEvent,
};
use renderer::{civil_from_unix, BoxError, InteractionSnapshot, VulkanRenderer};

pub use offscreen::{CapturedFrame, OffscreenRenderer};
pub use verify::LocaleSelection;
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
    /// Run the ADR-016 automated verification path instead of opening a window: render the
    /// screen offscreen per locale, run the `mdux-ui-verify` check suite, and write evidence
    /// reports under this directory. `None` means the flag was not passed (normal windowed run).
    pub verify_ui: Option<PathBuf>,
    /// Which locales to verify when `verify_ui` is set. Ignored otherwise.
    pub locales: LocaleSelection,
    /// Narrows verification to one registered scenario by id. `None` runs every scenario
    /// [`App::with_scenarios`] registered. Ignored when `verify_ui` is not set.
    pub scenario_filter: Option<String>,
}

/// Runs a compiled MedUI screen against a built [`Framework`] through a Vulkan-backed winit
/// window.
pub struct App {
    framework: Framework,
    screen: &'static CompiledScreenPackage,
    locale: String,
    realtime: Option<Box<dyn FnMut(&mut FrameInputs)>>,
    input: Option<Box<dyn FnMut(&mut FrameEvents, &mut FrameInputs)>>,
    scenarios: &'static [ScenarioScript],
    scenario_logic: Option<verify::ScenarioLogicFactory>,
}

impl App {
    pub fn new(framework: Framework, screen: &'static CompiledScreenPackage) -> Self {
        Self {
            framework,
            screen,
            locale: "en-US".to_string(),
            realtime: None,
            input: None,
            scenarios: &[],
            scenario_logic: None,
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

    /// Registers the behavior scripts the ADR-016 `--verify-ui` path can replay (ADR-016 §4).
    /// `logic_factory` must build a *fresh* pair of input/realtime closures every time it is
    /// called — one call per scenario replay, so state from one scenario (a typed patient id, a
    /// latched alert) never leaks into the next. This is the same pattern
    /// `examples/class_c_monitor/tests/scenarios.rs` uses to test scenarios with no GPU:
    /// `App::with_scenarios(verify_scenarios::SCENARIOS, || AppLogic::new().into_closures())`.
    /// Applications that register no scenarios still verify the screen's static rendering; only
    /// scenario captures and behavior traces are skipped.
    pub fn with_scenarios<I, R>(
        mut self,
        scenarios: &'static [ScenarioScript],
        logic_factory: impl Fn() -> (I, R) + 'static,
    ) -> Self
    where
        I: FnMut(&mut FrameEvents, &mut FrameInputs) + 'static,
        R: FnMut(&mut FrameInputs) + 'static,
    {
        self.scenarios = scenarios;
        self.scenario_logic = Some(Box::new(move || {
            let (input, realtime) = logic_factory();
            (
                Box::new(input) as Box<dyn FnMut(&mut FrameEvents, &mut FrameInputs)>,
                Box::new(realtime) as Box<dyn FnMut(&mut FrameInputs)>,
            )
        }));
        self
    }

    /// Parses `--headless-smoke`, `--auto-close-ms=<millis>`, `--verify-ui=<dir>`,
    /// `--locales=all|<comma-separated list>` and `--scenario=<id>` from `std::env::args()` and
    /// calls [`run`](Self::run).
    pub fn run_from_env(self) -> Result<(), BoxError> {
        let mut headless_smoke = false;
        let mut auto_close_after = None;
        let mut verify_ui = None;
        let mut locales = LocaleSelection::Default;
        let mut scenario_filter = None;

        for argument in std::env::args().skip(1) {
            if argument == "--headless-smoke" {
                headless_smoke = true;
            } else if let Some(value) = argument.strip_prefix("--auto-close-ms=") {
                let millis: u64 = value
                    .parse()
                    .map_err(|error| format!("invalid --auto-close-ms value '{value}': {error}"))?;
                auto_close_after = Some(Duration::from_millis(millis));
            } else if let Some(value) = argument.strip_prefix("--verify-ui=") {
                verify_ui = Some(PathBuf::from(value));
            } else if let Some(value) = argument.strip_prefix("--locales=") {
                locales = if value.trim() == "all" {
                    LocaleSelection::All
                } else {
                    let list: Vec<String> = value
                        .split(',')
                        .map(str::trim)
                        .filter(|entry| !entry.is_empty())
                        .map(str::to_string)
                        .collect();
                    if list.is_empty() {
                        return Err(format!("--locales={value} resolved to no locales").into());
                    }
                    LocaleSelection::List(list)
                };
            } else if let Some(value) = argument.strip_prefix("--scenario=") {
                scenario_filter = Some(value.to_string());
            }
        }

        self.run(RunOptions {
            headless_smoke,
            auto_close_after,
            verify_ui,
            locales,
            scenario_filter,
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
        if options.headless_smoke && options.verify_ui.is_some() {
            return Err(
                "--headless-smoke and --verify-ui are mutually exclusive run modes".into(),
            );
        }

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

        if let Some(dir) = options.verify_ui {
            return verify::run_verify(
                self,
                &dir,
                &options.locales,
                options.scenario_filter.as_deref(),
            );
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
    /// Predicted character count of the focused input's content, resynced to the echoed text
    /// every frame. Used only to stop enqueuing `CharTyped` events the application's model
    /// would refuse anyway (queue-pressure relief); the model stays authoritative.
    text_length: u16,
}

impl InteractionState {
    fn new() -> Self {
        Self {
            cursor: (-1.0, -1.0),
            armed: None,
            focused_input: None,
            caret: 0,
            text_length: 0,
        }
    }
}

/// Maps a pointer position from physical window coordinates into the authored UI surface —
/// the coordinate space of every compiled node's bounds — by the inverse of the ratio the
/// renderer uses to scale authored geometry to the swapchain extent.
fn authored_cursor(
    physical: (f64, f64),
    window_physical: (u32, u32),
    authored: (u32, u32),
) -> (f64, f64) {
    (
        physical.0 * f64::from(authored.0) / f64::from(window_physical.0.max(1)),
        physical.1 * f64::from(authored.1) / f64::from(window_physical.1.max(1)),
    )
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
                .map(|glyph_set| {
                    glyph_set.entries.iter().map(|entry| entry.character).collect::<Vec<char>>()
                })
                .ok_or_else(|| -> BoxError {
                    format!(
                        "text input {} references glyph set {} missing from the standard package (inconsistent screen bindings)",
                        binding.node_id, binding.glyph_set_id
                    )
                    .into()
                })
        })
        .collect::<Result<_, _>>()?;

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
                    // Hit-testing happens in the authored UI surface. The renderer scales the
                    // authored geometry to the swapchain extent (HiDPI, and windows clamped
                    // smaller than the authored surface by the window manager), so the pointer
                    // maps from physical window coordinates into authored coordinates by the
                    // inverse ratio — logical coordinates alone are NOT enough.
                    let size = window.inner_size();
                    interaction.cursor =
                        authored_cursor((position.x, position.y), (size.width, size.height), (
                            width, height,
                        ));
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
                            interaction.text_length = echoed_len(&frame_inputs, source);
                            interaction.caret = interaction.text_length;
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
                                interaction.text_length = echoed_len(&frame_inputs, source);
                                interaction.caret = interaction.text_length;
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
                            // The echoed text is authoritative (the application may transform
                            // or reject content): resync the predictions after the echo.
                            interaction.text_length =
                                echoed_len(&frame_inputs, input_targets[index].source);
                            interaction.caret = interaction.caret.min(interaction.text_length);
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
/// out-of-charset key never reaches the application's echo path. Caret movement clamps only to
/// `max_length` — never to the (possibly stale) echoed length, whose consumer-side clamp is
/// `CaretMoved`'s contract — and the per-frame resync against the fresh echo pulls the
/// presentation caret back to reality.
fn handle_editing_key(
    logical_key: &Key,
    binding: &TextInputBinding,
    allowed: &[char],
    interaction: &mut InteractionState,
    events: &mut FrameEvents,
) {
    let source = binding.source;
    match logical_key {
        Key::Named(NamedKey::Backspace) => {
            if interaction.caret > 0 {
                interaction.caret -= 1;
                interaction.text_length = interaction.text_length.saturating_sub(1);
            }
            events.push(WidgetEvent::Backspace { source });
        }
        Key::Named(NamedKey::Delete) => {
            if interaction.text_length > interaction.caret {
                interaction.text_length -= 1;
            }
            events.push(WidgetEvent::Delete { source });
        }
        Key::Named(NamedKey::ArrowLeft) => {
            interaction.caret = interaction.caret.saturating_sub(1);
            events.push(WidgetEvent::CaretMoved { source, position: interaction.caret });
        }
        Key::Named(NamedKey::ArrowRight) => {
            interaction.caret = interaction.caret.saturating_add(1).min(binding.max_length);
            events.push(WidgetEvent::CaretMoved { source, position: interaction.caret });
        }
        Key::Named(NamedKey::Home) => {
            interaction.caret = 0;
            events.push(WidgetEvent::CaretMoved { source, position: 0 });
        }
        Key::Named(NamedKey::End) => {
            interaction.caret = binding.max_length;
            events.push(WidgetEvent::CaretMoved { source, position: binding.max_length });
        }
        Key::Named(NamedKey::Enter) => {
            events.push(WidgetEvent::TextCommitted { source });
        }
        Key::Named(NamedKey::Space) => {
            push_character(' ', binding, allowed, interaction, events);
        }
        Key::Character(text) => {
            for character in text.chars() {
                push_character(character, binding, allowed, interaction, events);
            }
        }
        _ => {}
    }
}

/// Enqueues one typed character when it belongs to the baked charset AND the predicted content
/// still has room — a full field enqueues nothing, so bursts past `max_length` never pressure
/// the bounded queue with events the model would refuse.
fn push_character(
    character: char,
    binding: &TextInputBinding,
    allowed: &[char],
    interaction: &mut InteractionState,
    events: &mut FrameEvents,
) {
    if !allowed.contains(&character) {
        return;
    }
    if interaction.text_length >= binding.max_length {
        return;
    }
    interaction.text_length += 1;
    interaction.caret = interaction.caret.saturating_add(1).min(binding.max_length);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_maps_from_physical_window_into_the_authored_surface() {
        // Retina, window exactly the authored surface: 2x physical, authored 1920x1080.
        assert_eq!(
            authored_cursor((2784.0, 1440.0), (3840, 2160), (1920, 1080)),
            (1392.0, 720.0)
        );
        // Window clamped smaller than the authored surface (macOS laptop): the renderer
        // scales content down, so the pointer scales up by the same ratio.
        assert_eq!(
            authored_cursor((696.0, 360.0), (960, 540), (1920, 1080)),
            (1392.0, 720.0)
        );
        // 1:1 window, no scaling.
        assert_eq!(
            authored_cursor((100.0, 50.0), (1920, 1080), (1920, 1080)),
            (100.0, 50.0)
        );
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
