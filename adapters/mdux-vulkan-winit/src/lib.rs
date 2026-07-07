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

use mdux::realtime::{FrameInputs, ScreenBindings};
use mdux::{screen_text::ScreenTextLayout, CompiledScreenPackage, Framework, GraphicsProfile};
use renderer::{civil_from_unix, BoxError, VulkanRenderer};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
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
}

impl App {
    pub fn new(framework: Framework, screen: &'static CompiledScreenPackage) -> Self {
        Self {
            framework,
            screen,
            locale: "en-US".to_string(),
            realtime: None,
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

        run_windowed(
            &app_name,
            config.width,
            config.height,
            layout,
            bindings,
            frame_inputs,
            self.realtime,
            options.auto_close_after,
        )
    }
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
    auto_close_after: Option<Duration>,
) -> Result<(), BoxError> {
    let title = format!("{app_name} - Vulkan");
    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title(&title)
        .with_inner_size(LogicalSize::new(width as f64, height as f64))
        .build(&event_loop)?;

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

    event_loop.run(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent {
                window_id: id,
                event,
            } if id == window_id => match event {
                WindowEvent::CloseRequested => {
                    shutdown_renderer(&mut renderer);
                    event_loop_window_target.exit();
                }
                WindowEvent::Resized(_) => {}
                WindowEvent::RedrawRequested => {
                    if let Some(active_renderer) = renderer.as_mut() {
                        if let Some(realtime) = realtime.as_mut() {
                            realtime(&mut frame_inputs);
                        }
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|elapsed| elapsed.as_secs() as i64)
                            .unwrap_or(0);
                        let clock = civil_from_unix(now);
                        if let Err(error) =
                            active_renderer.draw_frame(&window, &frame_inputs, clock)
                        {
                            eprintln!("failed to render frame: {error}");
                            *render_error_for_closure.borrow_mut() = Some(error);
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
