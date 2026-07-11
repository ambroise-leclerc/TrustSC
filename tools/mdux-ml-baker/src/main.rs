use std::env;
use std::path::Path;
use std::process;

use mdux_ml_baker::{CliInvocation, bake, import, verify};

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

    match command.as_str() {
        "bake" | "verify" => {
            let (Some(recipe_path), Some(package_output_path), Some(report_output_path)) =
                (args.next(), args.next(), args.next())
            else {
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

            if command == "bake" {
                let summary = bake(invocation).map_err(|error| error.to_string())?;
                println!(
                    "baked package_sha256={} layers={} tensors={} params={} golden_vectors={}",
                    summary.package_sha256,
                    summary.layer_count,
                    summary.tensor_count,
                    summary.param_count,
                    summary.golden_vector_count
                );
            } else {
                let summary = verify(invocation).map_err(|error| error.to_string())?;
                println!(
                    "verified package_sha256={} package_bytes={} report_bytes={}",
                    summary.package_sha256,
                    summary.package_bytes_verified,
                    summary.report_bytes_verified
                );
            }
            Ok(())
        }
        "import" => {
            let (Some(safetensors_path), Some(tensor_map_path)) = (args.next(), args.next())
            else {
                return Err(usage());
            };
            if args.next().is_some() {
                return Err(usage());
            }

            let fragment = import(Path::new(&safetensors_path), Path::new(&tensor_map_path))
                .map_err(|error| error.to_string())?;
            print!("{fragment}");
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: mdux-ml-baker <bake|verify> <recipe.toml> <package.json> <report.json>\n       mdux-ml-baker import <model.safetensors> <tensor-map.toml>   (offline authoring only, never CI)".to_string()
}
