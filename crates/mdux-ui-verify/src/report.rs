//! `VerificationReport` (ADR-016 §5, schema v1) and its hand-rolled, deterministic JSON emitter.
//! No serde in governed code: fixed key order, integers only (coverage as parts-per-million, not
//! floats), escaped strings, LF line endings, no trailing whitespace — byte-reproducible given
//! identical inputs, so a report can be committed and byte-compared like the lavapipe `ColorHash`
//! baselines it partly exists to carry.

use crate::{CheckKind, CheckOutcome, CheckPayload, CheckResult};
use mdux_ui::Rect;

const SCHEMA_VERSION: u32 = 1;
const REPORT_KIND: &str = "mdux-ui-verification";

/// One scripted-scenario step's trace (ADR-016 §4/§5): the event injected, what was expected,
/// what was observed, and whether it matched. `mdux-ui-verify` only defines the shape here — the
/// scenario runner (a later wave) is the producer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioTraceRow {
    pub scenario_id: String,
    pub step_index: u32,
    pub description: String,
    pub expected: String,
    pub observed: String,
    pub passed: bool,
}

/// One row of the requirement -> verification -> check trace join (ADR-016 §5): built from
/// `CompiledNodeKind::requirement_id()` and a `ComplianceProgram::trace_rows()` accessor by the
/// caller assembling the report.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceRow {
    pub requirement_id: String,
    pub verification_ids: Vec<String>,
    pub check_ids: Vec<String>,
}

/// The full evidence artifact for one `(application, locale)` capture (ADR-016 §5). Every field
/// here is a concrete, already-resolved value: assembling one is the caller's job (it knows the
/// device context, the backend identity and the capture file paths), `mdux-ui-verify` only knows
/// how to fill in `checks` via [`crate::verify_frame`] and to render the whole thing
/// deterministically via [`emit_report_json`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerificationReport {
    pub tool_name: String,
    pub tool_version: String,
    pub software_item: String,
    pub safety_class: String,
    pub screen_id: String,
    pub locale: String,
    pub surface_width: u32,
    pub surface_height: u32,
    pub backend_id: String,
    pub device_name: String,
    pub pixel_format: String,
    pub clock: String,
    pub screenshot_file: String,
    pub screenshot_sha256: String,
    pub checks: Vec<CheckResult>,
    pub scenario_traces: Vec<ScenarioTraceRow>,
    pub trace_rows: Vec<TraceRow>,
}

fn indent(level: usize) -> String {
    "  ".repeat(level)
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", escape(value))
}

fn str_value(value: &str) -> String {
    quoted(value)
}

fn opt_str_value(value: &Option<String>) -> String {
    match value {
        Some(inner) => quoted(inner),
        None => "null".to_string(),
    }
}

fn int_value(value: impl std::fmt::Display) -> String {
    value.to_string()
}

fn bool_value(value: bool) -> String {
    value.to_string()
}

fn rgba_value(rgba: [u8; 4]) -> String {
    format!("[{}, {}, {}, {}]", rgba[0], rgba[1], rgba[2], rgba[3])
}

/// One `"key": value` line at `level`, without a trailing comma — callers join sibling fields
/// with `",\n"`.
fn field(level: usize, key: &str, raw_value: String) -> String {
    format!("{}{}: {}", indent(level), quoted(key), raw_value)
}

/// Renders an object whose opening brace sits on the line at `closing_level` and whose fields
/// (already rendered at `closing_level + 1`) close back at `closing_level`. `{}` for no fields.
fn render_object(closing_level: usize, fields: Vec<String>) -> String {
    if fields.is_empty() {
        return "{}".to_string();
    }
    format!("{{\n{}\n{}}}", fields.join(",\n"), indent(closing_level))
}

/// Renders an array whose opening bracket sits on the line at `closing_level`; each item is
/// prefixed with `closing_level + 1` indentation on its first line (later lines, for
/// object/array items, already carry their own absolute indentation). `[]` for no items.
fn render_array(closing_level: usize, items: Vec<String>) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    let item_indent = indent(closing_level + 1);
    let prefixed = items
        .iter()
        .map(|item| format!("{item_indent}{item}"))
        .collect::<Vec<_>>()
        .join(",\n");
    format!("[\n{}\n{}]", prefixed, indent(closing_level))
}

fn render_rect(level: usize, rect: Rect) -> String {
    render_object(
        level,
        vec![
            field(level + 1, "x", int_value(rect.x)),
            field(level + 1, "y", int_value(rect.y)),
            field(level + 1, "width", int_value(rect.width)),
            field(level + 1, "height", int_value(rect.height)),
        ],
    )
}

fn render_payload(level: usize, payload: &CheckPayload) -> String {
    match payload {
        CheckPayload::ChromeColor {
            expected_token,
            expected_rgba,
            measured_rgba,
            max_channel_delta,
            sample_count,
        } => render_object(
            level,
            vec![
                field(level + 1, "expected_token", str_value(expected_token)),
                field(level + 1, "expected_rgba", rgba_value(*expected_rgba)),
                field(level + 1, "measured_rgba", rgba_value(*measured_rgba)),
                field(level + 1, "max_channel_delta", int_value(*max_channel_delta)),
                field(level + 1, "sample_count", int_value(*sample_count)),
            ],
        ),
        CheckPayload::GoldenBounds {
            expected_bounds,
            measured_ink_bounds,
            contained,
        } => render_object(
            level,
            vec![
                field(level + 1, "expected_bounds", render_rect(level + 1, *expected_bounds)),
                field(
                    level + 1,
                    "measured_ink_bounds",
                    match measured_ink_bounds {
                        Some(rect) => render_rect(level + 1, *rect),
                        None => "null".to_string(),
                    },
                ),
                field(level + 1, "contained", bool_value(*contained)),
            ],
        ),
        CheckPayload::TextPresence {
            coverage_ppm,
            expected_min_ppm,
            expected_max_ppm,
        } => render_object(
            level,
            vec![
                field(level + 1, "coverage_ppm", int_value(*coverage_ppm)),
                field(level + 1, "expected_min_ppm", int_value(*expected_min_ppm)),
                field(level + 1, "expected_max_ppm", int_value(*expected_max_ppm)),
            ],
        ),
        CheckPayload::InkContainment { outside_ink_pixels } => render_object(
            level,
            vec![field(level + 1, "outside_ink_pixels", int_value(*outside_ink_pixels))],
        ),
        CheckPayload::ColorHash {
            expected_hex,
            measured_hex,
        } => render_object(
            level,
            vec![
                field(level + 1, "expected_hex", opt_str_value(expected_hex)),
                field(level + 1, "measured_hex", str_value(measured_hex)),
            ],
        ),
    }
}

fn check_kind_str(kind: CheckKind) -> &'static str {
    kind.as_str()
}

fn check_outcome_str(outcome: CheckOutcome) -> &'static str {
    outcome.as_str()
}

fn render_check(level: usize, check: &CheckResult) -> String {
    render_object(
        level,
        vec![
            field(level + 1, "check_id", str_value(&check.check_id)),
            field(level + 1, "node_id", str_value(&check.node_id)),
            field(level + 1, "kind", str_value(check_kind_str(check.kind))),
            field(level + 1, "requirement_id", opt_str_value(&check.requirement_id)),
            field(level + 1, "outcome", str_value(check_outcome_str(check.outcome))),
            field(level + 1, "payload", render_payload(level + 1, &check.payload)),
        ],
    )
}

fn render_scenario_trace_row(level: usize, row: &ScenarioTraceRow) -> String {
    render_object(
        level,
        vec![
            field(level + 1, "scenario_id", str_value(&row.scenario_id)),
            field(level + 1, "step_index", int_value(row.step_index)),
            field(level + 1, "description", str_value(&row.description)),
            field(level + 1, "expected", str_value(&row.expected)),
            field(level + 1, "observed", str_value(&row.observed)),
            field(level + 1, "passed", bool_value(row.passed)),
        ],
    )
}

fn render_trace_row(level: usize, row: &TraceRow) -> String {
    let verification_ids = row
        .verification_ids
        .iter()
        .map(|id| str_value(id))
        .collect::<Vec<_>>();
    let check_ids = row.check_ids.iter().map(|id| str_value(id)).collect::<Vec<_>>();
    render_object(
        level,
        vec![
            field(level + 1, "requirement_id", str_value(&row.requirement_id)),
            field(level + 1, "verification_ids", render_array(level + 1, verification_ids)),
            field(level + 1, "check_ids", render_array(level + 1, check_ids)),
        ],
    )
}

/// Renders `report` as deterministic JSON: fixed key order, integers only, LF endings, no
/// trailing spaces, a trailing newline at end of file. Byte-reproducible given an identical
/// `report` value.
pub fn emit_report_json(report: &VerificationReport) -> String {
    let check_items = report.checks.iter().map(|check| render_check(2, check)).collect();
    let scenario_items = report
        .scenario_traces
        .iter()
        .map(|row| render_scenario_trace_row(2, row))
        .collect();
    let trace_items = report.trace_rows.iter().map(|row| render_trace_row(2, row)).collect();

    let fields = vec![
        field(1, "schema_version", int_value(SCHEMA_VERSION)),
        field(1, "report_kind", str_value(REPORT_KIND)),
        field(1, "tool_name", str_value(&report.tool_name)),
        field(1, "tool_version", str_value(&report.tool_version)),
        field(1, "software_item", str_value(&report.software_item)),
        field(1, "safety_class", str_value(&report.safety_class)),
        field(1, "screen_id", str_value(&report.screen_id)),
        field(1, "locale", str_value(&report.locale)),
        field(1, "surface_width", int_value(report.surface_width)),
        field(1, "surface_height", int_value(report.surface_height)),
        field(1, "backend_id", str_value(&report.backend_id)),
        field(1, "device_name", str_value(&report.device_name)),
        field(1, "pixel_format", str_value(&report.pixel_format)),
        field(1, "clock", str_value(&report.clock)),
        field(1, "screenshot_file", str_value(&report.screenshot_file)),
        field(1, "screenshot_sha256", str_value(&report.screenshot_sha256)),
        field(1, "checks", render_array(1, check_items)),
        field(1, "scenario_traces", render_array(1, scenario_items)),
        field(1, "trace_rows", render_array(1, trace_items)),
    ];

    let mut json = render_object(0, fields);
    json.push('\n');
    json
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CheckKind, CheckOutcome, CheckPayload, CheckResult};

    #[test]
    fn emits_a_minimal_report_byte_for_byte() {
        let report = VerificationReport {
            tool_name: "mdux-ui-verify".to_string(),
            tool_version: "0.1.0".to_string(),
            software_item: "ui".to_string(),
            safety_class: "Class C".to_string(),
            screen_id: "NeuroSense".to_string(),
            locale: "en-US".to_string(),
            surface_width: 1920,
            surface_height: 1080,
            backend_id: "lavapipe".to_string(),
            device_name: "llvmpipe (LLVM 17.0.0, 256 bits)".to_string(),
            pixel_format: "R8G8B8A8_UNORM".to_string(),
            clock: "2026-01-01T00:00:00Z".to_string(),
            screenshot_file: "screenshot.ppm".to_string(),
            screenshot_sha256: "abc123".to_string(),
            checks: vec![CheckResult {
                check_id: "ack-button::chrome_color".to_string(),
                node_id: "ack-button".to_string(),
                kind: CheckKind::ChromeColor,
                requirement_id: Some("REQ-NS-004".to_string()),
                outcome: CheckOutcome::Pass,
                payload: CheckPayload::ChromeColor {
                    expected_token: "Theme.Colors.PrimaryAction".to_string(),
                    expected_rgba: [41, 112, 219, 255],
                    measured_rgba: [41, 112, 219, 255],
                    max_channel_delta: 0,
                    sample_count: 96,
                },
            }],
            scenario_traces: Vec::new(),
            trace_rows: vec![TraceRow {
                requirement_id: "REQ-NS-004".to_string(),
                verification_ids: vec!["VER-004".to_string()],
                check_ids: vec!["ack-button::chrome_color".to_string()],
            }],
        };

        // Built line-by-line (rather than backslash-continued) because a `\`-continuation at
        // end of line strips *all* following whitespace, including the next line's leading
        // indentation — exactly the bytes this test needs to pin.
        let expected_lines = [
            "{",
            "  \"schema_version\": 1,",
            "  \"report_kind\": \"mdux-ui-verification\",",
            "  \"tool_name\": \"mdux-ui-verify\",",
            "  \"tool_version\": \"0.1.0\",",
            "  \"software_item\": \"ui\",",
            "  \"safety_class\": \"Class C\",",
            "  \"screen_id\": \"NeuroSense\",",
            "  \"locale\": \"en-US\",",
            "  \"surface_width\": 1920,",
            "  \"surface_height\": 1080,",
            "  \"backend_id\": \"lavapipe\",",
            "  \"device_name\": \"llvmpipe (LLVM 17.0.0, 256 bits)\",",
            "  \"pixel_format\": \"R8G8B8A8_UNORM\",",
            "  \"clock\": \"2026-01-01T00:00:00Z\",",
            "  \"screenshot_file\": \"screenshot.ppm\",",
            "  \"screenshot_sha256\": \"abc123\",",
            "  \"checks\": [",
            "    {",
            "      \"check_id\": \"ack-button::chrome_color\",",
            "      \"node_id\": \"ack-button\",",
            "      \"kind\": \"chrome_color\",",
            "      \"requirement_id\": \"REQ-NS-004\",",
            "      \"outcome\": \"pass\",",
            "      \"payload\": {",
            "        \"expected_token\": \"Theme.Colors.PrimaryAction\",",
            "        \"expected_rgba\": [41, 112, 219, 255],",
            "        \"measured_rgba\": [41, 112, 219, 255],",
            "        \"max_channel_delta\": 0,",
            "        \"sample_count\": 96",
            "      }",
            "    }",
            "  ],",
            "  \"scenario_traces\": [],",
            "  \"trace_rows\": [",
            "    {",
            "      \"requirement_id\": \"REQ-NS-004\",",
            "      \"verification_ids\": [",
            "        \"VER-004\"",
            "      ],",
            "      \"check_ids\": [",
            "        \"ack-button::chrome_color\"",
            "      ]",
            "    }",
            "  ]",
            "}",
            "",
        ];
        let expected = expected_lines.join("\n");

        assert_eq!(emit_report_json(&report), expected);
        assert!(!emit_report_json(&report).contains('\r'), "must be LF-only");
        for line in emit_report_json(&report).lines() {
            assert_eq!(line, line.trim_end(), "no trailing whitespace: {line:?}");
        }
    }

    #[test]
    fn escapes_control_and_quote_characters() {
        let escaped = escape("line1\nline2\t\"quoted\"\\backslash");
        assert_eq!(escaped, "line1\\nline2\\t\\\"quoted\\\"\\\\backslash");
    }

    #[test]
    fn empty_arrays_render_as_empty_brackets_not_dangling_commas() {
        let report = VerificationReport {
            tool_name: "mdux-ui-verify".to_string(),
            tool_version: "0.1.0".to_string(),
            software_item: "ui".to_string(),
            safety_class: "Class B".to_string(),
            screen_id: "Hello".to_string(),
            locale: "en-US".to_string(),
            surface_width: 800,
            surface_height: 480,
            backend_id: "lavapipe".to_string(),
            device_name: "llvmpipe (LLVM 17.0.0, 256 bits)".to_string(),
            pixel_format: "R8G8B8A8_UNORM".to_string(),
            clock: "2026-01-01T00:00:00Z".to_string(),
            screenshot_file: "screenshot.ppm".to_string(),
            screenshot_sha256: "deadbeef".to_string(),
            checks: Vec::new(),
            scenario_traces: Vec::new(),
            trace_rows: Vec::new(),
        };

        let json = emit_report_json(&report);
        assert!(json.contains("\"checks\": [],"));
        assert!(json.contains("\"scenario_traces\": [],"));
        assert!(json.contains("\"trace_rows\": []\n"));
    }
}
