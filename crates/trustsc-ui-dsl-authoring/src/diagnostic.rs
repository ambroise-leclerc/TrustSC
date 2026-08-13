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
    /// Extracts the line number from a [`ValidationError`]'s message, if present. Every parser
    /// error in this crate embeds the source line as `line {N}` somewhere in the message (not
    /// always at the end — e.g. `"component at line {n} must declare \`id\`"`), so this scans for
    /// the last `line ` occurrence followed by a run of ASCII digits rather than assuming a fixed
    /// position.
    pub fn from_validation_error(error: &ValidationError) -> Diagnostic {
        let message = error.to_string();
        let line = extract_line_number(&message);
        Diagnostic {
            code: classify(&message),
            file: String::new(),
            message,
            line,
            column: None,
            severity: Severity::Error,
            fix_hint: String::new(),
        }
    }
}

/// Transitional classification for the line-oriented parser. Codes are canonical now; replacing
/// message-based classification with positioned lexer errors is tracked by the shared syntax
/// conformance migration and does not change this public envelope.
fn classify(message: &str) -> &'static str {
    if message.contains("nested Row") {
        "MEDUI-E015"
    } else if message.contains("unsupported component") {
        "MEDUI-E011"
    } else if message.contains("unsupported property")
        || message.contains("unsupported Row property")
    {
        "MEDUI-E013"
    } else if message.contains("must declare") {
        "MEDUI-E012"
    } else if message.contains("unknown color") || message.contains("unknown theme") {
        "MEDUI-E030"
    } else if message.contains("unknown text") || message.contains("unknown string") {
        "MEDUI-E031"
    } else if message.contains("widest") || message.contains("budget") || message.contains("fit") {
        "MEDUI-E050"
    } else if message.contains("outside") || message.contains("exceeds the available surface") {
        "MEDUI-E052"
    } else if message.contains("safety-critical") && message.contains("requirement") {
        "MEDUI-E070"
    } else if message.contains("CV check") {
        "MEDUI-E071"
    } else {
        "MEDUI-E010"
    }
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
