//! Structured parse/compile diagnostics for tools (MedUI Studio, ADR-022) that need a line
//! number and a severity instead of a bare error string.

use trustsc_core::ValidationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub line: Option<u32>,
    pub severity: Severity,
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
            message,
            line,
            severity: Severity::Error,
        }
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
