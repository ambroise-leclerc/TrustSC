//! `trustsc-medui-check` — a tiny CLI that validates a `.medui` file and prints compiler
//! diagnostics, for instant feedback while hand-editing a screen without building an example app.
//!
//! Host tooling only (ADR-005 trust zones): a workspace member under `tools/`, never linked into
//! any `crates/`/`adapters/` code that ships to a device.
//!
//! `--format=json` emits the shared finding envelope from the pinned `Compliatory/MedUI` contract
//! (`schemas/diagnostic.schema.json`) so an agent or CI step keys off the stable `MEDUI-E###`
//! identity rather than parsing the human text, which the contract says may be reworded.

use std::process::ExitCode;

use trustsc_ui_dsl_authoring::{
    CompileOptions, Diagnostic, ImagePackages, Severity, TextPackages, code,
    compile_screen_definition, parse_medui_source,
};

/// Matches the fallback every other tool in this repo uses for a screen with no `surface:` pin
/// (`tools/trustsc-medui-studio`'s `DEFAULT_SURFACE`, and `examples/hello_world`'s own build.rs
/// configuration) — the checker has no build.rs to ask for a real surface size, so it validates
/// against the same conventional default.
const DEFAULT_SURFACE: (u32, u32) = (800, 480);

/// How diagnostics are rendered. `Text` is for a human at a terminal; `Json` is the shared
/// envelope, which is what a machine should read.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Format {
    Text,
    Json,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut format = Format::Text;
    let mut positional = Vec::new();
    let mut argument_error = None;
    for arg in &args {
        match arg.as_str() {
            "--format=text" => format = Format::Text,
            "--format=json" => format = Format::Json,
            other if other.starts_with("--") => {
                argument_error.get_or_insert_with(|| format!("unknown option {other}"));
            }
            other => positional.push(other),
        }
    }
    if let Some(message) = argument_error {
        report_early_failure(&message, "<arguments>", code::UNEXPECTED_TOKEN, format);
        return ExitCode::from(2);
    }
    let path = match positional.as_slice() {
        [path] => *path,
        _ => {
            let message = "usage: trustsc-medui-check [--format=text|json] <path/to/screen.medui>";
            match format {
                Format::Text => eprintln!("{message}"),
                Format::Json => {
                    report_early_failure(message, "<arguments>", code::UNEXPECTED_TOKEN, format)
                }
            }
            return ExitCode::from(2);
        }
    };

    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            report_early_failure(
                &format!("failed to read {path}: {error}"),
                path,
                code::SOURCE_UNREADABLE,
                format,
            );
            return ExitCode::FAILURE;
        }
    };

    let screen = match parse_medui_source(&source) {
        Ok(screen) => screen,
        Err(diagnostics) => {
            report(&diagnostics, path, format);
            return ExitCode::FAILURE;
        }
    };

    let standard_package = match trustsc::default_standard_text_package() {
        Ok(package) => package,
        Err(error) => {
            report_early_failure(
                &format!("failed to load the standard text package: {error}"),
                path,
                code::UNREGISTERED,
                format,
            );
            return ExitCode::FAILURE;
        }
    };
    let display_packages = match trustsc::default_display_text_packages() {
        Ok(packages) => packages,
        Err(error) => {
            report_early_failure(
                &format!("failed to load the display text packages: {error}"),
                path,
                code::UNREGISTERED,
                format,
            );
            return ExitCode::FAILURE;
        }
    };
    let display_refs = display_packages.iter().collect::<Vec<_>>();
    let image_packages = match trustsc::default_image_packages() {
        Ok(packages) => packages,
        Err(error) => {
            report_early_failure(
                &format!("failed to load the image packages: {error}"),
                path,
                code::UNREGISTERED,
                format,
            );
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
            match format {
                Format::Text => println!("OK {} ({} nodes)", screen_id, compiled.nodes.len()),
                // A clean run still emits the envelope, so a consumer parses one shape either way
                // rather than having to special-case success.
                Format::Json => report(&[], path, format),
            }
            ExitCode::SUCCESS
        }
        Err(diagnostics) => {
            report(&diagnostics, path, format);
            ExitCode::FAILURE
        }
    }
}

/// Reports failures that occur before the parser or compiler can return a diagnostic. JSON mode
/// still emits the standard envelope, while text mode preserves the CLI's concise stderr form.
fn report_early_failure(message: &str, path: &str, error_code: &'static str, format: Format) {
    match format {
        Format::Text => eprintln!("trustsc-medui-check: {message}"),
        Format::Json => report(
            &[Diagnostic {
                code: error_code,
                file: String::new(),
                message: message.to_string(),
                line: None,
                column: None,
                severity: Severity::Error,
                fix_hint: String::new(),
            }],
            path,
            format,
        ),
    }
}

fn report(diagnostics: &[Diagnostic], path: &str, format: Format) {
    match format {
        Format::Text => print_diagnostics(diagnostics),
        Format::Json => println!("{}", envelope(diagnostics, path)),
    }
}

/// The shared envelope. Positions are emitted as `0` for "unknown" rather than as `null`, which is
/// what `schemas/diagnostic.schema.json` requires and what `spec/diagnostics.md` defines `0` to
/// mean; `file` is the path this run was given, so no finding carries the empty string the schema
/// forbids.
fn envelope(diagnostics: &[Diagnostic], path: &str) -> String {
    let findings = diagnostics
        .iter()
        .map(|diagnostic| {
            serde_json::json!({
                "file": path,
                "line": diagnostic.line.unwrap_or(0),
                "column": diagnostic.column.unwrap_or(0),
                "code": diagnostic.code,
                "severity": match diagnostic.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Note => "note",
                },
                "message": diagnostic.message,
                "fixHint": diagnostic.fix_hint,
            })
        })
        .collect::<Vec<_>>();

    serde_json::json!({
        "tool": "trustsc-medui-check",
        "filesChecked": 1,
        "findings": findings,
    })
    .to_string()
}

fn print_diagnostics(diagnostics: &[Diagnostic]) {
    for diagnostic in diagnostics {
        match (diagnostic.line, diagnostic.column) {
            (Some(line), Some(column)) => eprintln!(
                "trustsc-medui-check: [{}] {line}:{column}: {}",
                diagnostic.code, diagnostic.message
            ),
            (Some(line), None) => eprintln!(
                "trustsc-medui-check: [{}] line {line}: {}",
                diagnostic.code, diagnostic.message
            ),
            (None, _) => eprintln!(
                "trustsc-medui-check: [{}] {}",
                diagnostic.code, diagnostic.message
            ),
        }
    }
}
