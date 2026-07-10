//! Build-script helper compiling authored scenario TOML (ADR-016 §4) into the static Rust data
//! `mdux::include_scenarios!()` brings into scope — the same authored-source-in, static-data-out
//! doctrine `MeduiScreen` applies to `.medui` files (ADR-008).
//!
//! ```no_run
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     mdux_build::ScenarioSet::new("verify/scenarios").compile()
//! }
//! ```
//!
//! Pair this with `mdux::include_scenarios!()` in the crate's `src/` to bring the generated
//! `verify_scenarios` module into scope, exposing `verify_scenarios::SCENARIOS`.

use std::{
    env, fmt, fs,
    path::{Path, PathBuf},
};
use std::fmt::Write as _;

use serde::Deserialize;

use crate::DynError;

/// Builder for compiling every `*.toml` scenario script under a directory into the generated
/// Rust module consumed by `mdux::include_scenarios!()`.
pub struct ScenarioSet {
    dir: PathBuf,
}

impl ScenarioSet {
    /// `dir` is resolved relative to `CARGO_MANIFEST_DIR` of the calling build script. The
    /// directory may not exist or may be empty — that compiles to an empty `SCENARIOS` slice.
    pub fn new(dir: impl AsRef<Path>) -> Self {
        Self {
            dir: dir.as_ref().to_path_buf(),
        }
    }

    /// Parses, validates, and compiles every `*.toml` file directly under the configured
    /// directory (sorted by filename, for a deterministic `SCENARIOS` order) into
    /// `$OUT_DIR/mdux_scenarios.rs`, and emits `cargo:rerun-if-changed` for the directory and
    /// every scenario file found.
    pub fn compile(self) -> Result<(), DynError> {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
        let scenarios_dir = manifest_dir.join(&self.dir);
        let out_dir = PathBuf::from(env::var("OUT_DIR")?);
        let generated_path = out_dir.join("mdux_scenarios.rs");

        println!("cargo:rerun-if-changed={}", scenarios_dir.display());

        let mut toml_files = Vec::new();
        if scenarios_dir.is_dir() {
            for entry in fs::read_dir(&scenarios_dir)? {
                let path = entry?.path();
                if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
                    toml_files.push(path);
                }
            }
        }
        // Sorted by filename so a directory listing's incidental order never changes the
        // compiled SCENARIOS array — determinism the same way MeduiScreen output is determinism.
        toml_files.sort();

        let mut scenarios = Vec::with_capacity(toml_files.len());
        for path in &toml_files {
            println!("cargo:rerun-if-changed={}", path.display());
            let source = fs::read_to_string(path).map_err(|error| {
                format!("failed to read scenario file {}: {error}", path.display())
            })?;
            let file: ScenarioFile = toml::from_str(&source)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            scenarios.push(compile_scenario(path, file)?);
        }

        let mut seen_ids = std::collections::HashSet::with_capacity(scenarios.len());
        for scenario in &scenarios {
            if !seen_ids.insert(scenario.id.clone()) {
                return Err(format!("duplicate scenario id {:?} in {}", scenario.id, scenarios_dir.display()).into());
            }
        }

        fs::create_dir_all(&out_dir)?;
        fs::write(&generated_path, emit_scenarios_module(&scenarios)).map_err(|error| {
            format!(
                "failed to write generated scenarios module {}: {error}",
                generated_path.display()
            )
        })?;

        Ok(())
    }
}

/// Deserialized shape of one `*.toml` scenario file, before validation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScenarioFile {
    id: String,
    #[serde(default)]
    requirement_ids: Vec<String>,
    clock: String,
    #[serde(rename = "step", default)]
    steps: Vec<StepTable>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StepTable {
    event: Option<EventTable>,
    advance: Option<u32>,
    expect_text: Option<ExpectTextTable>,
    expect_status: Option<ExpectStatusTable>,
    expect_number: Option<ExpectNumberTable>,
    capture: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventTable {
    kind: String,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    character: Option<String>,
    #[serde(default)]
    position: Option<u16>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectTextTable {
    source: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectStatusTable {
    source: String,
    value: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectNumberTable {
    source: String,
    value: i64,
}

/// A validated scenario script, ready for codegen. Mirrors
/// `mdux::verify_scenario::ScenarioScript`, one field at a time, as owned data.
struct CompiledScenario {
    id: String,
    requirement_ids: Vec<String>,
    clock: CompiledClock,
    steps: Vec<CompiledStep>,
}

struct CompiledClock {
    year: u16,
    month: u8,
    day: u8,
    hours: u8,
    minutes: u8,
    seconds: u8,
}

#[derive(Debug)]
enum CompiledStep {
    Event(CompiledEvent),
    Advance { frames: u32 },
    ExpectText { source: String, value: String },
    ExpectStatus { source: String, value: u8 },
    ExpectNumber { source: String, value: i64 },
    Capture { label: String },
}

/// Mirrors `mdux::WidgetEvent` exactly (ADR-015): every variant that can arrive from scripted
/// interaction, none that the framework dispatches itself (`CriticalButtonPressed` is
/// framework-governed and has no scenario authoring surface).
#[derive(Debug)]
enum CompiledEvent {
    ButtonPressed { source: String },
    CharTyped { source: String, character: char },
    Backspace { source: String },
    Delete { source: String },
    CaretMoved { source: String, position: u16 },
    TextCommitted { source: String },
    FocusChanged { source: Option<String> },
}

fn compile_scenario(path: &Path, file: ScenarioFile) -> Result<CompiledScenario, DynError> {
    if file.id.trim().is_empty() {
        return Err(format!("{}: scenario id must not be empty", path.display()).into());
    }

    let clock = compile_clock(path, &file.clock)?;

    let mut steps = Vec::with_capacity(file.steps.len());
    for (index, step) in file.steps.iter().enumerate() {
        steps.push(compile_step(path, index, step)?);
    }

    Ok(CompiledScenario {
        id: file.id,
        requirement_ids: file.requirement_ids,
        clock,
        steps,
    })
}

/// Parses the exact RFC3339-UTC shape `YYYY-MM-DDTHH:MM:SSZ` by hand (no new SOUP dependency for
/// a single fixed-width timestamp format); anything else — fractional seconds, an explicit
/// offset, a lowercase `z`, missing punctuation — is rejected rather than loosely interpreted.
fn compile_clock(path: &Path, value: &str) -> Result<CompiledClock, DynError> {
    let bytes = value.as_bytes();
    let shape_ok = bytes.len() == 20
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
        && bytes[10] == b'T'
        && bytes[11..13].iter().all(u8::is_ascii_digit)
        && bytes[13] == b':'
        && bytes[14..16].iter().all(u8::is_ascii_digit)
        && bytes[16] == b':'
        && bytes[17..19].iter().all(u8::is_ascii_digit)
        && bytes[19] == b'Z';

    let invalid = || -> DynError {
        format!(
            "{}: clock must be an RFC3339 UTC timestamp shaped YYYY-MM-DDTHH:MM:SSZ, got {value:?}",
            path.display()
        )
        .into()
    };

    if !shape_ok {
        return Err(invalid());
    }

    let year: u16 = value[0..4].parse().map_err(|_| invalid())?;
    let month: u8 = value[5..7].parse().map_err(|_| invalid())?;
    let day: u8 = value[8..10].parse().map_err(|_| invalid())?;
    let hours: u8 = value[11..13].parse().map_err(|_| invalid())?;
    let minutes: u8 = value[14..16].parse().map_err(|_| invalid())?;
    let seconds: u8 = value[17..19].parse().map_err(|_| invalid())?;

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hours > 23
        || minutes > 59
        || seconds > 59
    {
        return Err(invalid());
    }

    Ok(CompiledClock { year, month, day, hours, minutes, seconds })
}

fn compile_step(path: &Path, index: usize, step: &StepTable) -> Result<CompiledStep, DynError> {
    let present = [
        step.event.is_some(),
        step.advance.is_some(),
        step.expect_text.is_some(),
        step.expect_status.is_some(),
        step.expect_number.is_some(),
        step.capture.is_some(),
    ];
    let present_count = present.iter().filter(|set| **set).count();
    if present_count != 1 {
        return Err(format!(
            "{}: step {index} must set exactly one of event/advance/expect_text/expect_status/expect_number/capture, found {present_count}",
            path.display()
        )
        .into());
    }

    if let Some(event) = &step.event {
        return Ok(CompiledStep::Event(compile_event(path, index, event)?));
    }
    if let Some(&frames) = step.advance.as_ref() {
        if frames == 0 {
            return Err(format!(
                "{}: step {index} advance must be positive (use a later step instead of advance=0)",
                path.display()
            )
            .into());
        }
        return Ok(CompiledStep::Advance { frames });
    }
    if let Some(expect_text) = &step.expect_text {
        return Ok(CompiledStep::ExpectText {
            source: require_non_empty_source(path, index, "expect_text", &expect_text.source)?,
            value: expect_text.value.clone(),
        });
    }
    if let Some(expect_status) = &step.expect_status {
        return Ok(CompiledStep::ExpectStatus {
            source: require_non_empty_source(path, index, "expect_status", &expect_status.source)?,
            value: expect_status.value,
        });
    }
    if let Some(expect_number) = &step.expect_number {
        return Ok(CompiledStep::ExpectNumber {
            source: require_non_empty_source(path, index, "expect_number", &expect_number.source)?,
            value: expect_number.value,
        });
    }

    let label = step
        .capture
        .clone()
        .expect("exactly one field present, checked above");
    if label.trim().is_empty() {
        return Err(format!("{}: step {index} capture label must not be empty", path.display()).into());
    }
    Ok(CompiledStep::Capture { label })
}

/// Rejects an empty or whitespace-only `source` for an `expect_*` step, naming the file, step
/// index and field so an authoring mistake fails at build time with a precise pointer rather
/// than as a scenario that can never match any real widget source key.
fn require_non_empty_source(
    path: &Path,
    index: usize,
    step_kind: &str,
    source: &str,
) -> Result<String, DynError> {
    if source.trim().is_empty() {
        return Err(format!(
            "{}: step {index} {step_kind} source must not be empty",
            path.display()
        )
        .into());
    }
    Ok(source.to_string())
}

/// Fields an event table may legally set beyond `kind`/`source`, per event kind — anything else
/// present is an authoring mistake (e.g. a stray `position` on `CharTyped`) that must fail at
/// build time instead of being silently accepted and discarded.
struct AllowedEventFields {
    character: bool,
    position: bool,
}

fn compile_event(path: &Path, index: usize, table: &EventTable) -> Result<CompiledEvent, DynError> {
    let require_source = |kind: &str| -> Result<String, DynError> {
        table
            .source
            .clone()
            .filter(|source| !source.trim().is_empty())
            .ok_or_else(|| {
                format!("{}: step {index} event kind {kind} requires a non-empty source", path.display()).into()
            })
    };

    let reject_unused_fields = |kind: &str, allowed: AllowedEventFields| -> Result<(), DynError> {
        if !allowed.character && table.character.is_some() {
            return Err(format!(
                "{}: step {index} event kind {kind} does not accept `character`",
                path.display()
            )
            .into());
        }
        if !allowed.position && table.position.is_some() {
            return Err(format!(
                "{}: step {index} event kind {kind} does not accept `position`",
                path.display()
            )
            .into());
        }
        Ok(())
    };

    match table.kind.as_str() {
        "ButtonPressed" => {
            reject_unused_fields("ButtonPressed", AllowedEventFields { character: false, position: false })?;
            Ok(CompiledEvent::ButtonPressed { source: require_source("ButtonPressed")? })
        }
        "CharTyped" => {
            reject_unused_fields("CharTyped", AllowedEventFields { character: true, position: false })?;
            let source = require_source("CharTyped")?;
            let character_value = table.character.as_deref().ok_or_else(|| -> DynError {
                format!("{}: step {index} event kind CharTyped requires a character", path.display()).into()
            })?;
            let mut characters = character_value.chars();
            let character = characters.next().ok_or_else(|| -> DynError {
                format!("{}: step {index} event kind CharTyped character must not be empty", path.display())
                    .into()
            })?;
            if characters.next().is_some() {
                return Err(format!(
                    "{}: step {index} event kind CharTyped character must be exactly one character, got {character_value:?}",
                    path.display()
                )
                .into());
            }
            Ok(CompiledEvent::CharTyped { source, character })
        }
        "Backspace" => {
            reject_unused_fields("Backspace", AllowedEventFields { character: false, position: false })?;
            Ok(CompiledEvent::Backspace { source: require_source("Backspace")? })
        }
        "Delete" => {
            reject_unused_fields("Delete", AllowedEventFields { character: false, position: false })?;
            Ok(CompiledEvent::Delete { source: require_source("Delete")? })
        }
        "CaretMoved" => {
            reject_unused_fields("CaretMoved", AllowedEventFields { character: false, position: true })?;
            let source = require_source("CaretMoved")?;
            let position = table.position.ok_or_else(|| -> DynError {
                format!("{}: step {index} event kind CaretMoved requires a position", path.display()).into()
            })?;
            Ok(CompiledEvent::CaretMoved { source, position })
        }
        "TextCommitted" => {
            reject_unused_fields("TextCommitted", AllowedEventFields { character: false, position: false })?;
            Ok(CompiledEvent::TextCommitted { source: require_source("TextCommitted")? })
        }
        "FocusChanged" => {
            reject_unused_fields("FocusChanged", AllowedEventFields { character: false, position: false })?;
            Ok(CompiledEvent::FocusChanged {
                source: table.source.clone().filter(|source| !source.trim().is_empty()),
            })
        }
        other => Err(format!("{}: step {index} has unknown event kind {other:?}", path.display()).into()),
    }
}

fn emit_scenarios_module(scenarios: &[CompiledScenario]) -> String {
    let mut output = String::new();
    let _ = writeln!(
        output,
        "pub static SCENARIOS: &[::mdux::verify_scenario::ScenarioScript] = &["
    );
    for scenario in scenarios {
        emit_scenario(&mut output, scenario);
    }
    let _ = writeln!(output, "];");
    output
}

fn emit_scenario(output: &mut impl fmt::Write, scenario: &CompiledScenario) {
    let _ = writeln!(output, "    ::mdux::verify_scenario::ScenarioScript {{");
    let _ = writeln!(output, "        id: {:?},", scenario.id);
    let _ = writeln!(output, "        requirement_ids: {},", emit_str_slice(&scenario.requirement_ids));
    let _ = writeln!(
        output,
        "        clock: ::mdux::verify_scenario::ScenarioClock {{ year: {}, month: {}, day: {}, hours: {}, minutes: {}, seconds: {} }},",
        scenario.clock.year,
        scenario.clock.month,
        scenario.clock.day,
        scenario.clock.hours,
        scenario.clock.minutes,
        scenario.clock.seconds
    );
    let _ = writeln!(output, "        steps: &[");
    for step in &scenario.steps {
        let _ = writeln!(output, "            {},", emit_step(step));
    }
    let _ = writeln!(output, "        ],");
    let _ = writeln!(output, "    }},");
}

fn emit_str_slice(values: &[String]) -> String {
    format!(
        "&[{}]",
        values.iter().map(|value| format!("{value:?}")).collect::<Vec<_>>().join(", ")
    )
}

fn emit_step(step: &CompiledStep) -> String {
    match step {
        CompiledStep::Event(event) => {
            format!("::mdux::verify_scenario::ScenarioStep::Event({})", emit_event(event))
        }
        CompiledStep::Advance { frames } => {
            format!("::mdux::verify_scenario::ScenarioStep::Advance {{ frames: {frames} }}")
        }
        CompiledStep::ExpectText { source, value } => format!(
            "::mdux::verify_scenario::ScenarioStep::ExpectText {{ source: {source:?}, value: {value:?} }}"
        ),
        CompiledStep::ExpectStatus { source, value } => format!(
            "::mdux::verify_scenario::ScenarioStep::ExpectStatus {{ source: {source:?}, value: {value} }}"
        ),
        CompiledStep::ExpectNumber { source, value } => format!(
            "::mdux::verify_scenario::ScenarioStep::ExpectNumber {{ source: {source:?}, value: {value} }}"
        ),
        CompiledStep::Capture { label } => {
            format!("::mdux::verify_scenario::ScenarioStep::Capture {{ label: {label:?} }}")
        }
    }
}

fn emit_event(event: &CompiledEvent) -> String {
    match event {
        CompiledEvent::ButtonPressed { source } => {
            format!("::mdux::WidgetEvent::ButtonPressed {{ source: {source:?} }}")
        }
        CompiledEvent::CharTyped { source, character } => format!(
            "::mdux::WidgetEvent::CharTyped {{ source: {source:?}, character: {character:?} }}"
        ),
        CompiledEvent::Backspace { source } => {
            format!("::mdux::WidgetEvent::Backspace {{ source: {source:?} }}")
        }
        CompiledEvent::Delete { source } => {
            format!("::mdux::WidgetEvent::Delete {{ source: {source:?} }}")
        }
        CompiledEvent::CaretMoved { source, position } => format!(
            "::mdux::WidgetEvent::CaretMoved {{ source: {source:?}, position: {position} }}"
        ),
        CompiledEvent::TextCommitted { source } => {
            format!("::mdux::WidgetEvent::TextCommitted {{ source: {source:?} }}")
        }
        CompiledEvent::FocusChanged { source } => format!(
            "::mdux::WidgetEvent::FocusChanged {{ source: {} }}",
            emit_optional_string(source.as_deref())
        ),
    }
}

fn emit_optional_string(value: Option<&str>) -> String {
    value.map(|entry| format!("Some({entry:?})")).unwrap_or_else(|| "None".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path() -> PathBuf {
        PathBuf::from("scenario.toml")
    }

    fn event_table(kind: &str) -> EventTable {
        EventTable {
            kind: kind.to_string(),
            source: Some("ACK_BUTTON".to_string()),
            character: None,
            position: None,
        }
    }

    #[test]
    fn rejects_whitespace_only_source_on_an_expect_step() {
        let step = StepTable {
            event: None,
            advance: None,
            expect_text: Some(ExpectTextTable {
                source: "   ".to_string(),
                value: "A".to_string(),
            }),
            expect_status: None,
            expect_number: None,
            capture: None,
        };

        let error = compile_step(&sample_path(), 0, &step)
            .expect_err("a whitespace-only expect_text source must be rejected");
        assert!(
            error.to_string().contains("expect_text source must not be empty"),
            "{error}"
        );
    }

    #[test]
    fn rejects_whitespace_only_source_on_an_event() {
        let mut table = event_table("ButtonPressed");
        table.source = Some("   ".to_string());

        let error = compile_event(&sample_path(), 0, &table)
            .expect_err("a whitespace-only event source must be rejected");
        assert!(error.to_string().contains("requires a non-empty source"), "{error}");
    }

    #[test]
    fn rejects_a_character_field_on_an_event_kind_that_does_not_accept_one() {
        let mut table = event_table("ButtonPressed");
        table.character = Some("A".to_string());

        let error = compile_event(&sample_path(), 0, &table)
            .expect_err("ButtonPressed must not accept a character field");
        assert!(
            error.to_string().contains("ButtonPressed does not accept `character`"),
            "{error}"
        );
    }

    #[test]
    fn rejects_a_position_field_on_an_event_kind_that_does_not_accept_one() {
        let mut table = event_table("CharTyped");
        table.character = Some("A".to_string());
        table.position = Some(3);

        let error = compile_event(&sample_path(), 0, &table)
            .expect_err("CharTyped must not accept a position field");
        assert!(
            error.to_string().contains("CharTyped does not accept `position`"),
            "{error}"
        );
    }

    #[test]
    fn accepts_the_fields_each_event_kind_declares() {
        let mut char_typed = event_table("CharTyped");
        char_typed.character = Some("A".to_string());
        assert!(compile_event(&sample_path(), 0, &char_typed).is_ok());

        let mut caret_moved = event_table("CaretMoved");
        caret_moved.position = Some(4);
        assert!(compile_event(&sample_path(), 0, &caret_moved).is_ok());

        assert!(compile_event(&sample_path(), 0, &event_table("ButtonPressed")).is_ok());
    }
}
