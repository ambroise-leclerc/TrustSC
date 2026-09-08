//! Structured parse/compile diagnostics for tools (MedUI Studio, ADR-022) that need a line
//! number and a severity instead of a bare error string.

use trustsc_core::ValidationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// Stable implementation-neutral identity from `Compliatory/MedUI`.
    pub code: &'static str,
    /// Source path supplied by the caller; empty for in-memory input.
    pub file: String,
    pub message: String,
    pub line: Option<u32>,
    /// 1-based UTF-8 byte column, or `None` when the legacy parser cannot locate it precisely.
    pub column: Option<u32>,
    pub severity: Severity,
    pub fix_hint: String,
}

impl Diagnostic {
    /// Builds the shared diagnostic envelope from an error raised inside this crate.
    ///
    /// The code is read from the error itself — assigned at the point the condition was
    /// detected — never re-derived from the message. `spec/diagnostics.md` states that a message
    /// "may be reworded" while a code's meaning is stable, so inferring one from the other would
    /// make a stable identity depend on prose.
    ///
    /// Source positions are carried independently of the human-readable message. The current
    /// line-oriented parser supplies a line but no column; compilation failures without a
    /// source anchor leave both absent.
    pub fn from_validation_error(error: &ValidationError) -> Diagnostic {
        let message = error.to_string();
        Diagnostic {
            // An untagged error means a raise site in this crate forgot its code, or an error
            // crossed in from another crate. `assigns_a_code_to_every_raise_site` covers the
            // first; the fallback keeps the envelope well-formed for the second.
            code: error.code().unwrap_or(code::UNEXPECTED_TOKEN),
            file: String::new(),
            message,
            line: error.line(),
            column: error.column(),
            severity: Severity::Error,
            fix_hint: String::new(),
        }
    }
}

/// The `MEDUI-E###` identities from the pinned contract's `spec/diagnostics.md`, named by meaning
/// so a raise site reads as the condition it detects.
///
/// A code's meaning never changes and a retired number is never reused (`MEDUI-DEC-005`), so
/// these constants are append-only. This is an implementation subset of the `0.3.0-rc.1` registry,
/// not a declaration that every registered diagnostic is implemented. Missing here are:
///
/// - E000/E001/E002: recipe diagnostics (this crate has no recipe reader).
/// - E017: hardcoded product string; the legacy parser instead reports E010.
/// - E033/E034/E035: semantic kind, closed-set member and resource identity; older local
///   diagnostic mappings remain instead of these dedicated identities.
/// - E053: dynamic text escaping its charset.
/// - E054: positioned node without fixed dimensions; the legacy parser reports E051.
/// - E070: a safety-critical annotation without a requirement. In particular, an annotated
///   Button or TextInput with no requirement is currently accepted; this is a behavioral gap,
///   not simply an unnamed error. Semantics, layout and safety remain unclaimed by the harness.
///
/// See `docs/dsl/conformance.md` for the coverage limits at the pinned contract.
pub mod code {
    /// `MEDUI-E003` — the `.medui` source could not be read.
    pub const SOURCE_UNREADABLE: &str = "MEDUI-E003";
    /// `MEDUI-E004` — the source bytes are not valid UTF-8.
    pub const SOURCE_NOT_UTF8: &str = "MEDUI-E004";
    /// `MEDUI-E010` — the source does not parse.
    pub const UNEXPECTED_TOKEN: &str = "MEDUI-E010";
    /// `MEDUI-E011` — the component name is not in the closed dictionary.
    pub const UNKNOWN_COMPONENT: &str = "MEDUI-E011";
    /// `MEDUI-E012` — a construct omits a field its schema requires.
    pub const MISSING_FIELD: &str = "MEDUI-E012";
    /// `MEDUI-E013` — a construct declares a field its schema does not define.
    pub const UNKNOWN_FIELD: &str = "MEDUI-E013";
    /// `MEDUI-E014` — two nodes resolve to the same id.
    pub const DUPLICATE_NODE_ID: &str = "MEDUI-E014";
    /// `MEDUI-E015` — a `Row` nests inside another `Row`.
    pub const NESTED_ROW: &str = "MEDUI-E015";
    /// `MEDUI-E016` — a construct the language forbids in this position.
    pub const FORBIDDEN_CONSTRUCT: &str = "MEDUI-E016";
    /// `MEDUI-E030` — the theme colour token is not approved.
    pub const UNKNOWN_COLOR_TOKEN: &str = "MEDUI-E030";
    /// `MEDUI-E031` — the text key does not exist in the approved package.
    pub const UNKNOWN_TEXT_KEY: &str = "MEDUI-E031";
    /// `MEDUI-E032` — the text key exists but an approved locale has no run for it.
    pub const TEXT_KEY_MISSING_LOCALE: &str = "MEDUI-E032";
    /// `MEDUI-E050` — rendered text does not fit its box in the worst approved case.
    pub const TEXT_BUDGET_EXCEEDED: &str = "MEDUI-E050";
    /// `MEDUI-E051` — a node escapes its container or collides with another.
    pub const LAYOUT_OVERFLOW: &str = "MEDUI-E051";
    /// `MEDUI-E052` — resolved geometry leaves the screen surface.
    pub const SURFACE_EXCEEDED: &str = "MEDUI-E052";
    /// `MEDUI-E071` — a CV check is not one the contract defines for this node.
    pub const UNKNOWN_CV_CHECK: &str = "MEDUI-E071";

    /// Legacy fallback for conditions without a dedicated raise-site identity in this crate.
    ///
    /// Some now have registered identities in the pinned contract (notably E034 for clock/event
    /// members and E035 for image/template IDs); others still lack one. This name records a
    /// local implementation gap, not a claim that the shared registry has no matching code.
    pub const UNREGISTERED: &str = UNEXPECTED_TOKEN;
}

/// Raises an error carrying the stable identity of the condition that produced it.
pub(crate) fn coded(code: &'static str, message: impl Into<String>) -> ValidationError {
    ValidationError::with_code(code, message)
}

/// Raises a parser error with its structured 1-based line position.
pub(crate) fn coded_at(
    line_number: usize,
    code: &'static str,
    message: impl Into<String>,
) -> ValidationError {
    coded(code, message).with_position(u32::try_from(line_number).ok(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_uses_structured_position_instead_of_message_text() {
        let positioned = ValidationError::with_code(code::UNEXPECTED_TOKEN, "no line in prose")
            .with_position(Some(12), None);
        let unpositioned =
            ValidationError::with_code(code::UNKNOWN_COLOR_TOKEN, "untrusted token says line 99");

        assert_eq!(
            Diagnostic::from_validation_error(&positioned).line,
            Some(12)
        );
        assert_eq!(Diagnostic::from_validation_error(&unpositioned).line, None);
    }
}
