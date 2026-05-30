use std::env;
use std::path::Path;
use std::process;

use mdux_font_baker::{bake, verify, CliInvocation};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Err(usage());
    };
    let Some(recipe_path) = args.next() else {
        return Err(usage());
    };
    let Some(package_output_path) = args.next() else {
        return Err(usage());
    };
    let Some(report_output_path) = args.next() else {
        return Err(usage());
    };
    if args.next().is_some() {
        return Err(usage());
    }

    let invocation = CliInvocation {
        recipe_path: Path::new(&recipe_path),
        package_output_path: Path::new(&package_output_path),
        report_output_path: Path::new(&report_output_path),
    };

    match command.as_str() {
        "bake" => {
            let summary = bake(invocation).map_err(|error| error.to_string())?;
            println!(
                "baked package_sha256={} atlas_sha256={} glyphs={} runs={} templates={}",
                summary.package_sha256,
                summary.atlas_sha256,
                summary.glyph_count,
                summary.run_count,
                summary.numeric_template_count
            );
            Ok(())
        }
        "verify" => {
            let summary = verify(invocation).map_err(|error| error.to_string())?;
            println!(
                "verified package_sha256={} atlas_sha256={} package_bytes={} report_bytes={}",
                summary.package_sha256,
                summary.atlas_sha256,
                summary.package_bytes_verified,
                summary.report_bytes_verified
            );
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: mdux-font-baker <bake|verify> <recipe.toml> <package.json> <report.json>".to_string()
}
