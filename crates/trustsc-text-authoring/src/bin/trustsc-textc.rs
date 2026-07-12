use std::env;
use std::process;

use trustsc_text_authoring::{fingerprint_font_file, pipeline_description};

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
        "hash-font" => {
            let Some(path) = args.next() else {
                return Err("missing <path> argument for hash-font".to_string());
            };
            let fingerprint = fingerprint_font_file(&path).map_err(|error| error.to_string())?;
            println!(
                "path={} bytes={} sha256={}",
                fingerprint.path.display(),
                fingerprint.byte_len,
                fingerprint.sha256
            );
            Ok(())
        }
        "describe-pipeline" => {
            println!("{}", pipeline_description());
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: trustsc-textc <hash-font <path> | describe-pipeline>".to_string()
}
