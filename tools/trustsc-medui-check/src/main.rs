//! `trustsc-medui-check` — a tiny CLI that validates a `.medui` file and prints compiler
//! diagnostics, for instant feedback while hand-editing a screen without building an example app.
//!
//! Host tooling only (ADR-005 trust zones): a workspace member under `tools/`, never linked into
//! any `crates/`/`adapters/` code that ships to a device.

use std::process::ExitCode;

use trustsc_ui_dsl_authoring::{
    CompileOptions, Diagnostic, ImagePackages, TextPackages, compile_screen_definition, parse_medui_source,
};

/// Matches the fallback every other tool in this repo uses for a screen with no `surface:` pin
/// (`tools/trustsc-medui-studio`'s `DEFAULT_SURFACE`, and `examples/hello_world`'s own build.rs
/// configuration) — the checker has no build.rs to ask for a real surface size, so it validates
/// against the same conventional default.
const DEFAULT_SURFACE: (u32, u32) = (800, 480);

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = match args.as_slice() {
        [path] => path,
        _ => {
            eprintln!("usage: trustsc-medui-check <path/to/screen.medui>");
            return ExitCode::from(2);
        }
    };

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("trustsc-medui-check: failed to read {path}: {error}");
            return ExitCode::FAILURE;
        }
    };

    let screen = match parse_medui_source(&source) {
        Ok(screen) => screen,
        Err(diagnostics) => {
            print_diagnostics(&diagnostics);
            return ExitCode::FAILURE;
        }
    };

    let standard_package = match trustsc::default_standard_text_package() {
        Ok(package) => package,
        Err(error) => {
            eprintln!("trustsc-medui-check: failed to load the standard text package: {error}");
            return ExitCode::FAILURE;
        }
    };
    let display_packages = match trustsc::default_display_text_packages() {
        Ok(packages) => packages,
        Err(error) => {
            eprintln!("trustsc-medui-check: failed to load the display text packages: {error}");
            return ExitCode::FAILURE;
        }
    };
    let display_refs = display_packages.iter().collect::<Vec<_>>();
    let image_packages = match trustsc::default_image_packages() {
        Ok(packages) => packages,
        Err(error) => {
            eprintln!("trustsc-medui-check: failed to load the image packages: {error}");
            return ExitCode::FAILURE;
        }
    };

    let (width, height) = screen.declared_surface.unwrap_or(DEFAULT_SURFACE);
    let screen_id = screen.id.clone();
    match compile_screen_definition(
        screen,
        &CompileOptions::new(width, height),
        TextPackages::with_displays(&standard_package, &display_refs),
        ImagePackages::new(&image_packages),
    ) {
        Ok(compiled) => {
            println!("OK {} ({} nodes)", screen_id, compiled.nodes.len());
            ExitCode::SUCCESS
        }
        Err(diagnostics) => {
            print_diagnostics(&diagnostics);
            ExitCode::FAILURE
        }
    }
}

fn print_diagnostics(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        match diagnostic.line {
            Some(line) => eprintln!("trustsc-medui-check: line {line}: {}", diagnostic.message),
            None => eprintln!("trustsc-medui-check: {}", diagnostic.message),
        }
    }
}
