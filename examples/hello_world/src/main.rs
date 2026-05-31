pub(crate) mod hello_text;
mod medui_screen;
mod vulkan_window;

use std::error::Error;

use mdux::{HelloWorldDemoConfig, run_hello_world_demo};

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let headless_smoke = arguments
        .iter()
        .any(|argument| argument == "--headless-smoke");
    let auto_close_after = arguments.iter().find_map(|argument| {
        argument
            .strip_prefix("--auto-close-ms=")
            .and_then(|value| value.parse::<u64>().ok())
            .map(std::time::Duration::from_millis)
    });
    let greeting = hello_text::hello_world_greeting_from_dsl()?;
    let screen = medui_screen::hello_world_screen_package();
    let run = run_hello_world_demo(HelloWorldDemoConfig {
        greeting,
        ..HelloWorldDemoConfig::default()
    })?;

    let greeting = run
        .framework
        .ui_runtime()
        .components()
        .first()
        .map(|component| component.label.as_str())
        .unwrap_or("Hello world");

    println!("{greeting} from MduX-rust");
    println!(
        "medui_screen id={} nodes={} golden_refs={}",
        screen.screen_id,
        screen.nodes.len(),
        screen.golden_references.len()
    );
    println!("{}", run.framework.release_summary());
    println!(
        "preview_frame index={} draw_calls={} frame_time_ms={} dynamic_allocations={}",
        run.frame.frame_index,
        run.frame.draw_calls,
        run.frame.frame_time_ms,
        run.frame.dynamic_allocations
    );
    println!("trace_matrix\n{}", run.framework.trace_matrix_export());
    println!("audit_log\n{}", run.framework.audit_export());

    if headless_smoke {
        println!("headless_smoke=ok");
        return Ok(());
    }

    vulkan_window::run(run, auto_close_after)
}
