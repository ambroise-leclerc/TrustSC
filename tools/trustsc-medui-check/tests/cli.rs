//! Drives the built `trustsc-medui-check` binary end to end (issue #25's acceptance criteria):
//! a valid screen prints `OK` and exits `0`; a broken one prints the diagnostic with its line
//! and exits nonzero.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trustsc-medui-check"))
        .arg(path)
        .output()
        .expect("trustsc-medui-check should run")
}

#[test]
fn a_valid_screen_prints_ok_and_exits_zero() {
    let path = repo_root().join("examples/hello_world/hello_world.medui");
    let output = run(&path);
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("OK HelloWorld ("), "stdout was: {stdout}");
}

#[test]
fn an_unknown_color_token_prints_the_diagnostic_and_exits_nonzero() {
    // A semantic error caught during compilation (post-parse), not a syntax error — its
    // diagnostic has no line number (only *parse*-time diagnostics carry one; see
    // `Diagnostic::from_validation_error`'s doc comment), so this test only checks the message.
    // The next test covers the line-number path with a genuine syntax error.
    let original = std::fs::read_to_string(repo_root().join("examples/hello_world/hello_world.medui"))
        .expect("hello_world.medui should read");
    let broken = original.replace("Theme.Colors.PrimaryAction", "Theme.Colors.NotARealToken");
    assert_ne!(original, broken, "the replacement should have matched something");

    let file = TempMeduiFile::new(&broken);
    let output = run(file.path());

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("NotARealToken"), "stderr was: {stderr}");
}

#[test]
fn a_syntax_error_prints_the_diagnostic_with_its_line_and_exits_nonzero() {
    let original = std::fs::read_to_string(repo_root().join("examples/hello_world/hello_world.medui"))
        .expect("hello_world.medui should read");
    // An id containing a space is a parse-time error (`parse_identifier` rejects it), which
    // — unlike the semantic color-token error above — does carry a line number.
    let broken = original.replace("hello-world-label", "hello world label");
    assert_ne!(original, broken, "the replacement should have matched something");

    let file = TempMeduiFile::new(&broken);
    let output = run(file.path());

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unsupported characters"), "stderr was: {stderr}");
    assert!(stderr.contains("line "), "stderr should carry a line number, was: {stderr}");
}

/// A named temp file the child process can open by path — `tempfile`/`NamedTempFile` isn't a
/// dependency here, so this is the small amount of manual plumbing that buys us instead.
struct TempMeduiFile {
    path: PathBuf,
}

impl TempMeduiFile {
    fn new(contents: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "trustsc-medui-check-test-{}-{}.medui",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut file = std::fs::File::create(&path).expect("temp file should create");
        file.write_all(contents.as_bytes()).expect("temp file should write");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempMeduiFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
