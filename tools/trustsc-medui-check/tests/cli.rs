//! Drives the built `trustsc-medui-check` binary end to end (issue #25's acceptance criteria):
//! a valid screen prints `OK` and exits `0`; a broken one prints the diagnostic and exits
//! nonzero — with a line number when the parser produced one (only *parse*-time diagnostics
//! carry one; a semantic/compile-time error like an unknown color token does not, see the two
//! tests below for both cases).

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

fn run_json(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_trustsc-medui-check"))
        .arg("--format=json")
        .arg(path)
        .output()
        .expect("trustsc-medui-check should run")
}

#[test]
fn json_mode_envelopes_source_read_failures() {
    let missing = std::env::temp_dir().join(format!(
        "trustsc-medui-check-missing-{}-{:?}.medui",
        std::process::id(),
        std::thread::current().id()
    ));
    let output = run_json(&missing);

    assert!(!output.status.success());
    assert!(output.stderr.is_empty(), "stderr was: {}", String::from_utf8_lossy(&output.stderr));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("--format=json should emit an envelope for an unreadable source");
    assert_eq!(envelope["findings"][0]["code"], "MEDUI-E003");
    assert_eq!(envelope["findings"][0]["file"], missing.to_string_lossy().as_ref());
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
        // pid + thread id + nanos: the test harness runs tests in parallel threads, so pid alone
        // (constant for the whole run) or pid+nanos (coarse clock resolution can tie two threads
        // started together) can collide and silently truncate another test's fixture via
        // File::create. `duration_since` uses unwrap_or_default rather than unwrap: a clock set
        // before the epoch would otherwise panic here for no reason relevant to what's tested.
        let path = std::env::temp_dir().join(format!(
            "trustsc-medui-check-test-{}-{:?}-{}.medui",
            std::process::id(),
            std::thread::current().id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
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


/// Checks `--format=json` against the finding envelope in the *pinned* contract rather than
/// against a copy of it here, so a required member added upstream fails this test instead of
/// silently making the output non-conformant. Skipped without the checkout, and a hard failure in
/// CI, for the same reason the shared conformance suite is (`MEDUI-DEC-005`).
#[test]
fn json_output_satisfies_the_pinned_diagnostic_schema() {
    let Some(root) = std::env::var_os("MEDUI_CONFORMANCE_DIR") else {
        assert!(
            std::env::var_os("CI").is_none(),
            "MEDUI_CONFORMANCE_DIR is unset in CI, so the emitted envelope is unchecked against \
             the pinned schema"
        );
        eprintln!("diagnostic-schema check: SKIPPED — set MEDUI_CONFORMANCE_DIR to run it.");
        return;
    };
    let schema: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(PathBuf::from(&root).join("schemas/diagnostic.schema.json"))
            .expect("the pinned schema should read"),
    )
    .expect("the pinned schema should parse");

    // A screen with a genuine syntax error, so there is a finding to inspect.
    let original = std::fs::read_to_string(repo_root().join("examples/hello_world/hello_world.medui"))
        .expect("hello_world.medui should read");
    let broken = original.replace("hello-world-label", "hello world label");
    assert_ne!(
        broken, original,
        "the fixture anchor `hello-world-label` is gone from hello_world.medui; pick a new one so \
         this test still produces a syntax error"
    );
    let file = TempMeduiFile::new(&broken);
    let output = run_json(file.path());
    assert!(
        !output.status.success(),
        "the broken screen should be rejected; stderr was {:?}",
        String::from_utf8_lossy(&output.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_str(&String::from_utf8_lossy(&output.stdout))
        .expect("--format=json should emit valid JSON");

    assert_object_matches(&envelope, &schema, "envelope");

    let findings = envelope["findings"]
        .as_array()
        .expect("findings should be an array");
    assert!(!findings.is_empty(), "the broken screen should report a finding");
    let item_schema = &schema["properties"]["findings"]["items"];
    for finding in findings {
        assert_object_matches(finding, item_schema, "finding");

        // The pattern and enum are asserted literally: if the contract changes either, this fails
        // and a human decides what the new shape means rather than a loose check quietly passing.
        assert_eq!(
            item_schema["properties"]["code"]["pattern"], "^MEDUI-E[0-9]{3}$",
            "the pinned code pattern changed; update this check deliberately"
        );
        let code = finding["code"].as_str().expect("code should be a string");
        assert!(
            code.len() == 10
                && code.starts_with("MEDUI-E")
                && code[7..].bytes().all(|b| b.is_ascii_digit()),
            "code {code:?} does not match the pinned pattern"
        );

        let severities = item_schema["properties"]["severity"]["enum"]
            .as_array()
            .expect("severity should declare an enum");
        assert!(
            severities.contains(&finding["severity"]),
            "severity {:?} is not one of {severities:?}",
            finding["severity"]
        );

        for key in ["line", "column"] {
            let value = finding[key].as_i64().unwrap_or_else(|| {
                panic!("{key} should be an integer, got {:?}", finding[key])
            });
            assert!(value >= 0, "{key} should be non-negative, got {value}");
        }
        assert!(
            !finding["file"].as_str().unwrap_or_default().is_empty(),
            "file must be non-empty; the schema sets minLength 1"
        );
    }
}

/// Asserts every member the schema requires is present, and that nothing undeclared is emitted —
/// both schemas set `additionalProperties: false`.
fn assert_object_matches(value: &serde_json::Value, schema: &serde_json::Value, what: &str) {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{what} should be an object, got {value:?}"));
    for required in schema["required"]
        .as_array()
        .expect("the schema should list required members")
    {
        let key = required.as_str().expect("a required member names a string");
        assert!(object.contains_key(key), "{what} is missing required member {key:?}");
    }
    let declared = schema["properties"]
        .as_object()
        .expect("the schema should declare properties");
    for key in object.keys() {
        assert!(
            declared.contains_key(key),
            "{what} emits {key:?}, which the schema does not declare (additionalProperties: false)"
        );
    }
}
