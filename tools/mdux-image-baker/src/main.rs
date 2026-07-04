use std::env;
use std::path::Path;
use std::process;

use mdux_image_baker::{CliInvocation, bake, placeholder_logo_ppm, verify};

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

    if command == "generate-placeholder" {
        let Some(output_path) = args.next() else {
            return Err(usage());
        };
        if args.next().is_some() {
            return Err(usage());
        }
        std::fs::write(&output_path, placeholder_logo_ppm())
            .map_err(|error| format!("failed to write {output_path}: {error}"))?;
        println!("generated deterministic placeholder logo at {output_path}");
        return Ok(());
    }

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
                "baked package_sha256={} source_sha256={} size={}x{}",
                summary.package_sha256, summary.source_sha256, summary.width, summary.height
            );
            Ok(())
        }
        "verify" => {
            let summary = verify(invocation).map_err(|error| error.to_string())?;
            println!(
                "verified package_sha256={} package_bytes={} report_bytes={}",
                summary.package_sha256,
                summary.package_bytes_verified,
                summary.report_bytes_verified
            );
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: mdux-image-baker <bake|verify> <recipe.toml> <package.json> <report.json>\n       mdux-image-baker generate-placeholder <output.ppm>".to_string()
}
