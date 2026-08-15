//! Runs the pinned `Compliatory/MedUI` conformance cases against this implementation.
//!
//! Every expectation is read from the pinned checkout's `case.json` — never from a hand-written
//! copy here — so a change to the shared contract shows up as a failure rather than as a silent
//! divergence between the two repositories.
//!
//! `MEDUI-DEC-005` requires that a consumer "must not silently skip a claimed phase". Two rules
//! follow from that and are enforced below:
//!
//! - CI, which does have the pinned checkout, *fails* when it is missing rather than skipping.
//!   A developer without the checkout gets an explicit skip notice; `CI=1` turns that into a
//!   hard error, so the evidence-producing run can never be vacuous.
//! - Every capability claimed by `medui-conformance.toml` must be backed by at least one pinned
//!   case that actually executed, and the claim must name a phase this harness knows how to
//!   observe. Claiming a phase with no adapter fails the suite instead of passing silently.
//! - A pinned position is checked against the precision the manifest declares, per
//!   `spec/diagnostics.md`. A phase claimed at reduced precision is still claimed, and reporting
//!   more position than declared is a failure, so precision cannot move without a manifest edit.
//!
//! Only the parse phase is observable here, which is why `capabilities` claims `syntax` alone.
//! Claiming `semantics`, `layout`, or `safety` additionally requires driving
//! `compile_medui_source` with the text and image packages a case declares in `inputs`, and this
//! harness deliberately refuses such a claim until that adapter exists.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use trustsc_ui_dsl_authoring::{Diagnostic, parse_medui_source};

/// Phases this harness can genuinely observe. Anything else claimed in `medui-conformance.toml`
/// is a claim without an adapter behind it.
const RUNNABLE_CAPABILITIES: &[&str] = &["syntax"];

#[test]
fn pinned_shared_conformance_cases() {
    let manifest = Manifest::read(&repo_root().join("medui-conformance.toml"));

    for capability in &manifest.capabilities {
        assert!(
            RUNNABLE_CAPABILITIES.contains(&capability.as_str()),
            "medui-conformance.toml claims `{capability}`, but this harness can only observe \
             {RUNNABLE_CAPABILITIES:?}. Add the adapter that checks that phase's normalized \
             observations before claiming it."
        );
    }

    let Some(root) = conformance_root() else {
        return;
    };

    let cases = load_cases(&root);
    assert!(
        !cases.is_empty(),
        "no conformance cases found under {}; is MEDUI_CONFORMANCE_DIR pointing at a \
         Compliatory/MedUI checkout?",
        root.display()
    );

    let mut executed = BTreeSet::new();
    let mut skipped = Vec::new();
    for case in &cases {
        if manifest.capabilities.contains(&case.phase) {
            run_case(case, &manifest);
            executed.insert(case.phase.clone());
        } else {
            skipped.push(format!("{} ({})", case.id, case.phase));
        }
    }

    for capability in &manifest.capabilities {
        assert!(
            executed.contains(capability),
            "capability `{capability}` is claimed, but no case in the pinned contract exercised \
             it. Either the pin is wrong or the claim is unsupported."
        );
    }

    eprintln!(
        "MedUI conformance: {} case(s) at {}; claimed {:?}; unclaimed and not asserted: {}",
        cases.len(),
        manifest.commit,
        manifest.capabilities,
        if skipped.is_empty() {
            "none".to_string()
        } else {
            skipped.join(", ")
        }
    );
}

fn run_case(case: &Case, manifest: &Manifest) {
    let source = fs::read_to_string(&case.source).unwrap_or_else(|error| {
        panic!("{}: source {:?} should read: {error}", case.id, case.source)
    });

    match parse_medui_source(&source) {
        Ok(_) => assert!(
            case.expect_valid,
            "{}: this implementation accepted a source the pinned contract rejects",
            case.id
        ),
        Err(diagnostics) => {
            assert!(
                !case.expect_valid,
                "{}: this implementation rejected a source the pinned contract accepts: {:?}",
                case.id, diagnostics
            );
            assert_diagnostics(case, &diagnostics, manifest);
        }
    }
}

fn assert_diagnostics(case: &Case, actual: &[Diagnostic], manifest: &Manifest) {
    assert_eq!(
        actual.len(),
        case.expected.len(),
        "{}: expected {} diagnostic(s), got {:?}",
        case.id,
        case.expected.len(),
        actual
    );

    for (expected, actual) in case.expected.iter().zip(actual) {
        assert_eq!(
            actual.code, expected.code,
            "{}: diagnostic code (message was {:?})",
            case.id, actual.message
        );
        // `spec/diagnostics.md`, "Positions in conformance cases": a pinned position is matched
        // as far as the declared precision goes, and reporting more than was declared fails. The
        // declaration is checked rather than tolerated, so precision cannot move silently in
        // either direction. `0` means unknown, which is how `None` maps.
        match manifest.positions {
            Positions::Full => {
                assert_eq!(
                    i64::from(actual.line.unwrap_or(0)),
                    expected.line,
                    "{}: diagnostic line",
                    case.id
                );
                assert_eq!(
                    i64::from(actual.column.unwrap_or(0)),
                    expected.column,
                    "{}: diagnostic column",
                    case.id
                );
            }
            Positions::LineOnly => {
                assert_eq!(
                    i64::from(actual.line.unwrap_or(0)),
                    expected.line,
                    "{}: diagnostic line",
                    case.id
                );
                assert!(
                    actual.column.is_none(),
                    "{}: medui-conformance.toml declares positions = \"line-only\", but the parser \
                     reported column {:?}. Raise the declaration to \"full\" so the pinned column \
                     {} is checked.",
                    case.id,
                    actual.column,
                    expected.column
                );
            }
            Positions::None => assert!(
                actual.line.is_none() && actual.column.is_none(),
                "{}: medui-conformance.toml declares positions = \"none\", but the parser \
                 reported {:?}:{:?}. Raise the declaration so the pinned position {}:{} is \
                 checked.",
                case.id,
                actual.line,
                actual.column,
                expected.line,
                expected.column
            ),
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The pinned checkout, or `None` when a developer is running without one. Absent in CI this is a
/// hard failure: a claimed phase is never silently skipped where the evidence is produced.
fn conformance_root() -> Option<PathBuf> {
    match std::env::var_os("MEDUI_CONFORMANCE_DIR") {
        Some(value) => Some(PathBuf::from(value)),
        None if std::env::var_os("CI").is_some() => panic!(
            "MEDUI_CONFORMANCE_DIR is unset in CI. The pinned Compliatory/MedUI checkout is \
             required to substantiate the capabilities claimed in medui-conformance.toml."
        ),
        None => {
            eprintln!(
                "MedUI conformance: SKIPPED — set MEDUI_CONFORMANCE_DIR to a checkout of the \
                 commit pinned in medui-conformance.toml to run the shared cases locally."
            );
            None
        }
    }
}

// -------------------------------------------------------------------------------------------
// medui-conformance.toml
// -------------------------------------------------------------------------------------------

/// The declared diagnostic position precision from `spec/diagnostics.md`.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum Positions {
    /// Line and exact 1-based UTF-8 byte column; both pinned positions are checked.
    Full,
    /// Line only; the pinned line is checked and every column is required to be absent.
    LineOnly,
    /// Neither; both are required to be absent.
    None,
}

#[derive(Debug)]
struct Manifest {
    commit: String,
    capabilities: Vec<String>,
    positions: Positions,
}

impl Manifest {
    /// A deliberately strict reader for this repository's own six-key manifest rather than a
    /// general TOML parser: `crates/` takes no third-party dependencies, and anything this does
    /// not recognise is an error instead of a silently ignored line.
    fn read(path: &Path) -> Manifest {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("{} should read: {error}", path.display()));

        let mut commit = None;
        let mut capabilities = None;
        let mut positions = None;

        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once(" = ").unwrap_or_else(|| {
                panic!(
                    "{}:{}: expected `key = value`, got {line:?}",
                    path.display(),
                    index + 1
                )
            });
            match key {
                "commit" => commit = Some(unquote(path, index, value).to_string()),
                "capabilities" => capabilities = Some(string_array(path, index, value)),
                "positions" => {
                    positions = Some(match unquote(path, index, value) {
                        "full" => Positions::Full,
                        "line-only" => Positions::LineOnly,
                        "none" => Positions::None,
                        other => panic!(
                            "{}:{}: positions must be \"full\", \"line-only\" or \"none\", got \
                             {other:?}",
                            path.display(),
                            index + 1
                        ),
                    })
                }
                "repository" | "version" => {}
                other => panic!(
                    "{}:{}: unknown key {other:?}; update this harness when the manifest grows a key",
                    path.display(),
                    index + 1
                ),
            }
        }

        let commit = commit.unwrap_or_else(|| panic!("{} must pin a `commit`", path.display()));
        assert!(
            commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "{} must pin a 40-character commit SHA, got {commit:?}",
            path.display()
        );
        let capabilities = capabilities
            .unwrap_or_else(|| panic!("{} must declare `capabilities`", path.display()));
        assert!(
            !capabilities.is_empty(),
            "{} declares no capabilities, so this suite would assert nothing. Remove the pin or \
             claim a phase.",
            path.display()
        );

        Manifest {
            commit,
            capabilities,
            positions: positions
                .unwrap_or_else(|| panic!("{} must declare `positions`", path.display())),
        }
    }
}

fn unquote<'a>(path: &Path, index: usize, value: &'a str) -> &'a str {
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| {
            panic!(
                "{}:{}: expected a quoted string, got {value:?}",
                path.display(),
                index + 1
            )
        })
}

fn string_array(path: &Path, index: usize, value: &str) -> Vec<String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or_else(|| {
            panic!(
                "{}:{}: expected an array, got {value:?}",
                path.display(),
                index + 1
            )
        });
    inner
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| unquote(path, index, entry).to_string())
        .collect()
}

// -------------------------------------------------------------------------------------------
// Conformance cases
// -------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Case {
    id: String,
    phase: String,
    source: PathBuf,
    expect_valid: bool,
    expected: Vec<ExpectedDiagnostic>,
}

#[derive(Debug)]
struct ExpectedDiagnostic {
    code: String,
    line: i64,
    column: i64,
}

fn load_cases(root: &Path) -> Vec<Case> {
    let mut paths = Vec::new();
    collect_case_files(&root.join("conformance"), &mut paths);
    paths.sort();
    paths.iter().map(|path| Case::read(path)).collect()
}

fn collect_case_files(directory: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("directory entry should read").path();
        if path.is_dir() {
            collect_case_files(&path, out);
        } else if path.file_name().is_some_and(|name| name == "case.json") {
            out.push(path);
        }
    }
}

impl Case {
    fn read(path: &Path) -> Case {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("{} should read: {error}", path.display()));
        let json = Json::parse(&text, path);

        let id = json.get(path, "id").string(path).to_string();
        let phase = json.get(path, "phase").string(path).to_string();
        let source = json.get(path, "source").string(path).to_string();
        let expected = json.get(path, "expected");

        assert!(
            !source.contains('/') && source.ends_with(".medui"),
            "{}: source must be a sibling `.medui` file, got {source:?}",
            path.display()
        );
        // `case.schema.json` binds the phase to the directory that contains the case.
        let directory_phase = path
            .parent()
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        assert_eq!(
            phase,
            directory_phase,
            "{}: declared phase does not match its directory",
            path.display()
        );

        Case {
            id,
            phase,
            source: path.parent().expect("case has a parent").join(source),
            expect_valid: expected.get(path, "valid").boolean(path),
            expected: expected
                .get(path, "diagnostics")
                .array(path)
                .iter()
                .map(|entry| ExpectedDiagnostic {
                    code: entry.get(path, "code").string(path).to_string(),
                    line: entry.get(path, "line").integer(path),
                    column: entry.get(path, "column").integer(path),
                })
                .collect(),
        }
    }
}

// -------------------------------------------------------------------------------------------
// A strict reader for the JSON subset `schemas/case.schema.json` permits
// -------------------------------------------------------------------------------------------
//
// `crates/` takes no third-party dependencies (AGENTS.md trust zones), so rather than pull
// `serde_json` into a governed crate this reads the small, schema-constrained shape the pinned
// cases use. It is strict on purpose: escapes, floats, and `null` are rejected outright, so a
// case this cannot represent fails loudly instead of being silently misread into a vacuous pass.

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Bool(bool),
    Int(i64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn parse(text: &str, path: &Path) -> Json {
        let bytes = text.as_bytes();
        let mut at = 0usize;
        let value = parse_value(bytes, &mut at, path);
        skip_whitespace(bytes, &mut at);
        assert_eq!(
            at,
            bytes.len(),
            "{}: trailing content after the top-level JSON value",
            path.display()
        );
        value
    }

    fn get(&self, path: &Path, key: &str) -> &Json {
        match self {
            Json::Obj(members) => members
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value)
                .unwrap_or_else(|| panic!("{}: missing required member {key:?}", path.display())),
            other => panic!("{}: expected an object, got {other:?}", path.display()),
        }
    }

    fn string(&self, path: &Path) -> &str {
        match self {
            Json::Str(value) => value,
            other => panic!("{}: expected a string, got {other:?}", path.display()),
        }
    }

    fn integer(&self, path: &Path) -> i64 {
        match self {
            Json::Int(value) => *value,
            other => panic!("{}: expected an integer, got {other:?}", path.display()),
        }
    }

    fn boolean(&self, path: &Path) -> bool {
        match self {
            Json::Bool(value) => *value,
            other => panic!("{}: expected a boolean, got {other:?}", path.display()),
        }
    }

    fn array(&self, path: &Path) -> &[Json] {
        match self {
            Json::Arr(values) => values,
            other => panic!("{}: expected an array, got {other:?}", path.display()),
        }
    }
}

fn skip_whitespace(bytes: &[u8], at: &mut usize) {
    while *at < bytes.len() && bytes[*at].is_ascii_whitespace() {
        *at += 1;
    }
}

fn expect_byte(bytes: &[u8], at: &mut usize, byte: u8, path: &Path) {
    skip_whitespace(bytes, at);
    assert!(
        *at < bytes.len() && bytes[*at] == byte,
        "{}: expected {:?} at byte {at}",
        path.display(),
        byte as char
    );
    *at += 1;
}

fn parse_value(bytes: &[u8], at: &mut usize, path: &Path) -> Json {
    skip_whitespace(bytes, at);
    assert!(
        *at < bytes.len(),
        "{}: unexpected end of input",
        path.display()
    );
    match bytes[*at] {
        b'{' => parse_object(bytes, at, path),
        b'[' => parse_array(bytes, at, path),
        b'"' => Json::Str(parse_string(bytes, at, path)),
        b't' | b'f' => parse_bool(bytes, at, path),
        b'-' | b'0'..=b'9' => parse_integer(bytes, at, path),
        other => panic!(
            "{}: unsupported JSON value starting with {:?} at byte {at}",
            path.display(),
            other as char
        ),
    }
}

fn parse_object(bytes: &[u8], at: &mut usize, path: &Path) -> Json {
    expect_byte(bytes, at, b'{', path);
    let mut members = Vec::new();
    skip_whitespace(bytes, at);
    if *at < bytes.len() && bytes[*at] == b'}' {
        *at += 1;
        return Json::Obj(members);
    }
    loop {
        skip_whitespace(bytes, at);
        let key = parse_string(bytes, at, path);
        expect_byte(bytes, at, b':', path);
        let value = parse_value(bytes, at, path);
        assert!(
            !members
                .iter()
                .any(|(name, _): &(String, Json)| name == &key),
            "{}: duplicate member {key:?}",
            path.display()
        );
        members.push((key, value));
        skip_whitespace(bytes, at);
        match bytes.get(*at) {
            Some(b',') => *at += 1,
            Some(b'}') => {
                *at += 1;
                return Json::Obj(members);
            }
            other => panic!(
                "{}: expected ',' or '}}' in object, got {other:?} at byte {at}",
                path.display()
            ),
        }
    }
}

fn parse_array(bytes: &[u8], at: &mut usize, path: &Path) -> Json {
    expect_byte(bytes, at, b'[', path);
    let mut values = Vec::new();
    skip_whitespace(bytes, at);
    if *at < bytes.len() && bytes[*at] == b']' {
        *at += 1;
        return Json::Arr(values);
    }
    loop {
        values.push(parse_value(bytes, at, path));
        skip_whitespace(bytes, at);
        match bytes.get(*at) {
            Some(b',') => *at += 1,
            Some(b']') => {
                *at += 1;
                return Json::Arr(values);
            }
            other => panic!(
                "{}: expected ',' or ']' in array, got {other:?} at byte {at}",
                path.display()
            ),
        }
    }
}

fn parse_string(bytes: &[u8], at: &mut usize, path: &Path) -> String {
    expect_byte(bytes, at, b'"', path);
    let start = *at;
    while *at < bytes.len() && bytes[*at] != b'"' {
        assert_ne!(
            bytes[*at],
            b'\\',
            "{}: escape sequences are not supported by this reader",
            path.display()
        );
        *at += 1;
    }
    assert!(*at < bytes.len(), "{}: unterminated string", path.display());
    let value = std::str::from_utf8(&bytes[start..*at])
        .unwrap_or_else(|error| panic!("{}: string is not UTF-8: {error}", path.display()))
        .to_string();
    *at += 1;
    value
}

fn parse_bool(bytes: &[u8], at: &mut usize, path: &Path) -> Json {
    if bytes[*at..].starts_with(b"true") {
        *at += 4;
        Json::Bool(true)
    } else if bytes[*at..].starts_with(b"false") {
        *at += 5;
        Json::Bool(false)
    } else {
        panic!("{}: invalid literal at byte {at}", path.display())
    }
}

fn parse_integer(bytes: &[u8], at: &mut usize, path: &Path) -> Json {
    let start = *at;
    if bytes[*at] == b'-' {
        *at += 1;
    }
    while *at < bytes.len() && bytes[*at].is_ascii_digit() {
        *at += 1;
    }
    // `case.schema.json` uses integers only; a float here means the case says something this
    // reader would otherwise round away.
    assert!(
        !matches!(bytes.get(*at), Some(b'.') | Some(b'e') | Some(b'E')),
        "{}: non-integer numbers are not supported by this reader",
        path.display()
    );
    let text = std::str::from_utf8(&bytes[start..*at]).expect("digits are ASCII");
    Json::Int(
        text.parse().unwrap_or_else(|error| {
            panic!("{}: invalid integer {text:?}: {error}", path.display())
        }),
    )
}
