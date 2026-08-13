//! Candidate shared-contract checks. CI supplies the exact pinned checkout through
//! `MEDUI_CONFORMANCE_DIR`; local test runs skip these cases when it is absent.

use std::{fs, path::Path};

use trustsc_ui_dsl_authoring::parse_medui_source;

fn source(root: &Path, relative: &str) -> String {
    fs::read_to_string(root.join(relative)).expect("shared conformance source should read")
}

#[test]
fn pinned_shared_syntax_cases() {
    let Some(root) = std::env::var_os("MEDUI_CONFORMANCE_DIR") else {
        return;
    };
    let root = Path::new(&root);

    let accepted = source(root, "conformance/syntax/accepted-comments/source.medui");
    parse_medui_source(&accepted).expect("shared comments case should parse");

    let nested = source(root, "conformance/syntax/rejected-nested-row/source.medui");
    let diagnostics = parse_medui_source(&nested).expect_err("nested Row should be rejected");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].code, "MEDUI-E015");
    assert_eq!(diagnostics[0].line, Some(6));
}
