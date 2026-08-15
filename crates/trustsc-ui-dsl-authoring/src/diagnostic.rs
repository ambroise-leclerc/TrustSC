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
    /// Repository-relative source file when the caller knows it; empty for in-memory input.
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
    /// The line number is still recovered from the message: every parser error embeds it as
    /// `line {N}` somewhere in the text (not always at the end — e.g.
    /// `"component at line {n} must declare \`id\`"`). That stays message-coupled until the
    /// line-oriented parser is replaced by a positioned lexer, which is also what would supply
    /// the column the shared cases pin.
    pub fn from_validation_error(error: &ValidationError) -> Diagnostic {
        let message = error.to_string();
        let line = extract_line_number(&message);
        Diagnostic {
            // An untagged error means a raise site in this crate forgot its code, or an error
            // crossed in from another crate. `assigns_a_code_to_every_raise_site` covers the
            // first; the fallback keeps the envelope well-formed for the second.
            code: error.code().unwrap_or(code::UNEXPECTED_TOKEN),
            file: String::new(),
            message,
            line,
            column: None,
            severity: Severity::Error,
            fix_hint: String::new(),
        }
    }
}

/// The `MEDUI-E###` identities from the pinned contract's `spec/diagnostics.md`, named by meaning
/// so a raise site reads as the condition it detects.
///
/// A code's meaning never changes and a retired number is never reused (`MEDUI-DEC-005`), so
/// these constants are append-only. Codes the contract registers but this implementation never
/// raises — `MEDUI-E004` (source is not UTF-8), `MEDUI-E017` (hardcoded product string),
/// `MEDUI-E053` (dynamic text escapes its charset) — are deliberately absent rather than
/// declared and unused.
pub mod code {
    /// `MEDUI-E003` — the `.medui` source could not be read.
    pub const SOURCE_UNREADABLE: &str = "MEDUI-E003";
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

    /// A condition the pinned registry has no code for, reported as `MEDUI-E010` until one
    /// exists.
    ///
    /// `spec/component-model.md` makes unknown image IDs, templates, charsets, clock formats,
    /// system events, and layout kinds fatal, but registers a code only for colour tokens
    /// (`MEDUI-E030`) and CV checks (`MEDUI-E071`). Marking these sites explicitly keeps them
    /// greppable as registry gaps to raise upstream, instead of letting them fall through a
    /// catch-all unnoticed.
    pub const UNREGISTERED: &str = UNEXPECTED_TOKEN;
}

/// Raises an error carrying the stable identity of the condition that produced it.
pub(crate) fn coded(code: &'static str, message: impl Into<String>) -> ValidationError {
    ValidationError::with_code(code, message)
}

fn extract_line_number(message: &str) -> Option<u32> {
    let mut search = message;
    let mut found = None;
    while let Some(index) = search.find("line ") {
        let after = &search[index + "line ".len()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !digits.is_empty() {
            found = digits.parse().ok();
        }
        search = &search[index + "line ".len()..];
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_line_number_mid_sentence() {
        assert_eq!(
            extract_line_number("component at line 12 must declare `id`"),
            Some(12)
        );
    }

    #[test]
    fn extracts_line_number_at_end() {
        assert_eq!(
            extract_line_number("unexpected content after screen closing brace at line 40"),
            Some(40)
        );
    }

    #[test]
    fn returns_none_without_line_number() {
        assert_eq!(extract_line_number("id must not be empty"), None);
    }
}
