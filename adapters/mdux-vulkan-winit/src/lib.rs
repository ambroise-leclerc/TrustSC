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
    mem,
    rc::Rc,
    time::{Duration, Instant},
};

use mdux::{CompiledScreenPackage, Framework, screen_text::ScreenTextLayout};
use renderer::{BoxError, VulkanRenderer};
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
}

impl App {
    pub fn new(framework: Framework, screen: &'static CompiledScreenPackage) -> Self {
        Self {
            framework,
            screen,
            locale: "en-US".to_string(),
        }
    }

    /// Overrides the locale used to resolve the screen's approved text (default `en-US`).
    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = locale.into();
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
                let millis: u64 = value.parse().map_err(|error| {
                    format!("invalid --auto-close-ms value '{value}': {error}")
                })?;
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
    pub fn run(self, options: RunOptions) -> Result<(), BoxError> {
        println!(
            "screen id={} nodes={} golden_refs={}",
            self.screen.screen_id,
            self.screen.nodes.len(),
            self.screen.golden_references.len()
        );
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

        let package = mdux::default_standard_text_package()?;
        let layout = ScreenTextLayout::from_screen(self.screen, package, &self.locale)?;
        let app_name = self.framework.identity().name.clone();
        let config = self.framework.ui_runtime().config().clone();

        run_windowed(&app_name, config.width, config.height, layout, options.auto_close_after)
    }
}

fn run_windowed(
    app_name: &str,
    width: u32,
    height: u32,
    layout: ScreenTextLayout,
    auto_close_after: Option<Duration>,
) -> Result<(), BoxError> {
    let title = format!("{app_name} - Vulkan");
    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title(&title)
        .with_inner_size(LogicalSize::new(width as f64, height as f64))
        .build(&event_loop)?;

    let mut renderer = Some(VulkanRenderer::new(&window, app_name, layout)?);
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
                        if let Err(error) = active_renderer.draw_frame(&window) {
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

/// Takes `renderer` out of the `Option` and intentionally leaks it via [`mem::forget`] instead
/// of letting it `Drop`, on every shutdown path (close request, render error, auto-close).
///
/// `VulkanRenderer::drop` calls `vkDestroyDevice` after an already-presented swapchain, and on at
/// least one real (non-virtualized) desktop this segfaults inside the NVIDIA driver 100% of the
/// time — reproducible regardless of *when* the drop runs (mid-loop, in `LoopExiting`, or after
/// `event_loop.run` returns) and even with zero frames rendered, so it is not a drop-ordering bug
/// in this crate. `App::run`/`run_from_env` take `self` by value, so a single `App` can only run
/// once, but nothing stops a caller from constructing and running several `App`s in one
/// long-lived process (e.g. reopening a window after the user closes it); each run leaks its
/// Vulkan instance/device/surface rather than releasing them, and the leaks accumulate for as
/// long as the process stays alive. For a process that runs one `App` and then exits (the only
/// usage this crate currently exercises), this has the same practical effect as the old
/// `process::exit` — the OS reclaims everything on exit — just without hard-exiting the process.
/// See <https://github.com/ambroise-leclerc/MduX-rust/issues/28> for root-causing the underlying
/// driver/teardown interaction and removing this workaround.
fn shutdown_renderer(renderer: &mut Option<VulkanRenderer>) {
    if let Some(active_renderer) = renderer.take() {
        mem::forget(active_renderer);
    }
}
