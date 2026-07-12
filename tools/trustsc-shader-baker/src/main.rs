use std::env;
use std::path::Path;
use std::process;

use trustsc_shader_baker::{CliInvocation, bake, verify};

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
    let Some(manifest_path) = args.next() else {
        return Err(usage());
    };
    let Some(output_dir) = args.next() else {
        return Err(usage());
    };
    let Some(report_path) = args.next() else {
        return Err(usage());
    };
    if args.next().is_some() {
        return Err(usage());
    }

    let invocation = CliInvocation {
        manifest_path: Path::new(&manifest_path),
        output_dir: Path::new(&output_dir),
        report_path: Path::new(&report_path),
    };

    match command.as_str() {
        "bake" => {
            let summary = bake(invocation).map_err(|error| error.to_string())?;
            println!("baked artifacts={}", summary.artifact_count);
            Ok(())
        }
        "verify" => {
            let summary = verify(invocation).map_err(|error| error.to_string())?;
            println!("verified artifacts={}", summary.artifact_count);
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: trustsc-shader-baker <bake|verify> <manifest.toml> <output_dir> <report.json>".to_string()
}
