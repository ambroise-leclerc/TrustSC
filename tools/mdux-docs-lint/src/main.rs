//! `mdux-docs-lint check` — machine-checks the regulatory citation contract (ADR-020).
//!
//! Scans every `*.md` file under `docs/` and `software_development_file/` and fails on:
//! - a citation-key-shaped string (`<Standard>:<year> §<clause> ...`) whose standard id +
//!   edition year is not one of the five pinned identifiers from
//!   `docs/governance/citation-convention.md`;
//! - a cited clause number absent from that standard's `AI-Reference.md` index (prefix- and
//!   range-aware, since the indexes list rows like `§5.1.1-§5.1.3` and citations may be one
//!   level coarser or finer than an index row);
//! - a fenced ```json `Justification` block violating the structural rules of
//!   `docs/iec62304/schemas/justification.schema.json`, a duplicate `justification_id`, or an
//!   `evidence_refs` path that does not exist in the repository.
//!
//! Placeholder text is tolerated where the corpus deliberately uses it: clause placeholders
//! without digits (`§N.M`, `§...`) are skipped everywhere, and `software_development_file/
//! templates/` may use the literal `JUS-NNN` id and `[ ... ]` evidence placeholders.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// (`standard` enum value, pinned citation-key prefix, corpus folder) — the only valid ids.
const PINNED_STANDARDS: [(&str, &str, &str); 5] = [
    ("IEC 62304", "IEC 62304:2006", "docs/iec62304"),
    ("ISO 13485", "ISO 13485:2016", "docs/iso13485"),
    ("ISO 14971", "ISO 14971:2019", "docs/iso14971"),
    ("IEC 62366-1", "IEC 62366-1:2015", "docs/iec62366"),
    ("IEC 81001-5-1", "IEC 81001-5-1:2021", "docs/iec81001"),
];

const SCANNED_DIRS: [&str; 2] = ["docs", "software_development_file"];

/// The convention document's own illustrative `Justification` example is exempt from the
/// corpus-wide `justification_id` uniqueness rule (it intentionally mirrors a real entry).
const UNIQUENESS_EXEMPT: &str = "docs/governance/citation-convention.md";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = match args.as_slice() {
        [cmd] if cmd == "check" => repo_root(),
        [cmd, root] if cmd == "check" => PathBuf::from(root),
        _ => {
            eprintln!("usage: mdux-docs-lint check [repo-root]");
            return ExitCode::from(2);
        }
    };

    let mut linter = match Linter::new(&root) {
        Ok(linter) => linter,
        Err(message) => {
            eprintln!("mdux-docs-lint: {message}");
            return ExitCode::FAILURE;
        }
    };
    let stats = linter.check_corpus();

    println!(
        "mdux-docs-lint: {} files, {} citation keys, {} justification blocks checked",
        stats.files, stats.citations, stats.justifications
    );
    if linter.violations.is_empty() {
        println!("mdux-docs-lint: no violations");
        ExitCode::SUCCESS
    } else {
        for violation in &linter.violations {
            eprintln!("{violation}");
        }
        eprintln!("mdux-docs-lint: {} violation(s)", linter.violations.len());
        ExitCode::FAILURE
    }
}

/// The workspace root, two levels above this crate's manifest — keeps `cargo run -p
/// mdux-docs-lint` working from any current directory, matching the baker tools' spirit.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").into()
}

#[derive(Default)]
struct Stats {
    files: usize,
    citations: usize,
    justifications: usize,
}

struct Linter {
    root: PathBuf,
    /// Pinned citation-key prefix -> every clause number its `AI-Reference.md` mentions.
    clause_index: BTreeMap<String, BTreeSet<String>>,
    /// `justification_id` -> first location seen, for the corpus-wide uniqueness rule.
    seen_ids: BTreeMap<String, String>,
    violations: Vec<String>,
}

impl Linter {
    fn new(root: &Path) -> Result<Self, String> {
        let mut clause_index = BTreeMap::new();
        for (_, pinned, folder) in PINNED_STANDARDS {
            let index_path = root.join(folder).join("AI-Reference.md");
            let text = std::fs::read_to_string(&index_path)
                .map_err(|err| format!("cannot read {}: {err}", index_path.display()))?;
            clause_index.insert(pinned.to_string(), index_clauses(&text));
        }
        Ok(Self {
            root: root.to_path_buf(),
            clause_index,
            seen_ids: BTreeMap::new(),
            violations: Vec::new(),
        })
    }

    fn check_corpus(&mut self) -> Stats {
        let mut stats = Stats::default();
        let mut files = Vec::new();
        for dir in SCANNED_DIRS {
            collect_markdown(&self.root.join(dir), &mut files);
        }
        files.sort();
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else {
                self.violations.push(format!("{}: unreadable file", file.display()));
                continue;
            };
            let rel = file
                .strip_prefix(&self.root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            stats.files += 1;
            self.check_file(&rel, &text, &mut stats);
        }
        stats
    }

    fn check_file(&mut self, rel: &str, text: &str, stats: &mut Stats) {
        for (line_index, line) in text.lines().enumerate() {
            for citation in citation_keys(line) {
                stats.citations += 1;
                self.check_citation(rel, line_index + 1, &citation);
            }
        }
        for block in json_blocks(text) {
            if block.body.contains("\"justification_id\"") {
                stats.justifications += 1;
                self.check_justification(rel, block.line, &block.body);
            }
        }
    }

    fn check_citation(&mut self, rel: &str, line: usize, citation: &CitationKey) {
        let Some(clauses) = self.clause_index.get(&citation.standard) else {
            self.violations.push(format!(
                "{rel}:{line}: `{}` is not one of the pinned standard identifiers \
                 (wrong edition year or unknown standard — see docs/governance/citation-convention.md)",
                citation.standard
            ));
            return;
        };
        if let Some(clause) = &citation.clause {
            if !clause_known(clauses, clause) {
                self.violations.push(format!(
                    "{rel}:{line}: clause §{clause} is not in {}'s AI-Reference.md index",
                    citation.standard
                ));
            }
        }
    }

    fn check_justification(&mut self, rel: &str, line: usize, body: &str) {
        let is_template = rel.starts_with("software_development_file/templates/");
        for issue in
            justification_issues(body, is_template, &self.clause_index, &self.root)
        {
            self.violations.push(format!("{rel}:{line}: justification: {issue}"));
        }
        // Corpus-wide uniqueness (templates' JUS-NNN placeholder and the convention document's
        // illustrative example are exempt).
        if is_template || rel == UNIQUENESS_EXEMPT {
            return;
        }
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(body) {
            if let Some(serde_json::Value::String(id)) = map.get("justification_id") {
                let location = format!("{rel}:{line}");
                if let Some(first) = self.seen_ids.get(id) {
                    self.violations.push(format!(
                        "{location}: justification: duplicate justification_id `{id}` (first used at {first})"
                    ));
                } else {
                    self.seen_ids.insert(id.clone(), location);
                }
            }
        }
    }
}

fn collect_markdown(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_markdown(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(path);
        }
    }
}

/// A citation-key-shaped occurrence: a standard id with edition year followed by ` §`.
/// `clause` is `None` for digit-free placeholders (`§N.M`, `§...`), which are tolerated.
#[derive(Debug, PartialEq)]
struct CitationKey {
    standard: String,
    clause: Option<String>,
}

/// Finds every citation-key-shaped string in a line. Bare standard mentions without
/// `:year §...` (e.g. "ISO 14971 and IEC 62366-1 apply") are not citation keys and are ignored.
fn citation_keys(line: &str) -> Vec<CitationKey> {
    let bytes = line.as_bytes();
    let mut keys = Vec::new();
    for (start, _) in line.match_indices("IEC ").chain(line.match_indices("ISO ")) {
        let mut pos = start + 4;
        let number_start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos == number_start {
            continue;
        }
        // Optional part suffixes: `-5-1` in `IEC 81001-5-1`.
        while pos < bytes.len() && bytes[pos] == b'-' {
            let digits_start = pos + 1;
            let mut digits_end = digits_start;
            while digits_end < bytes.len() && bytes[digits_end].is_ascii_digit() {
                digits_end += 1;
            }
            if digits_end == digits_start {
                break;
            }
            pos = digits_end;
        }
        // Edition year.
        if bytes.get(pos) != Some(&b':') {
            continue;
        }
        pos += 1;
        let year_start = pos;
        while pos < bytes.len() && bytes[pos].is_ascii_digit() {
            pos += 1;
        }
        if pos - year_start != 4 {
            continue;
        }
        let standard = line[start..pos].to_string();
        // The ` §` marker is what makes this a citation key rather than a bare mention
        // (so `IEC 62304:2006+AMD1:2015` in running prose is not matched).
        let rest = &line[pos..];
        let Some(after_marker) = rest.strip_prefix(" §") else {
            continue;
        };
        keys.push(CitationKey { standard, clause: leading_clause(after_marker) });
    }
    keys
}

/// Extracts a leading clause number (`5.2.4`) from text following a `§`. Returns `None` when
/// the text is a digit-free placeholder.
fn leading_clause(text: &str) -> Option<String> {
    let end = text
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(text.len());
    let clause = text[..end].trim_matches('.');
    (!clause.is_empty()).then(|| clause.to_string())
}

/// Every clause number an `AI-Reference.md` mentions, with ranges like `§5.1.1-§5.1.3` and
/// `§1-§4` expanded to their individual clauses.
fn index_clauses(text: &str) -> BTreeSet<String> {
    let mut clauses = BTreeSet::new();
    let mut rest = text;
    while let Some(marker) = rest.find('§') {
        rest = &rest[marker + '§'.len_utf8()..];
        let Some(first) = leading_clause(rest) else {
            continue;
        };
        let after_first = &rest[rest.find(&first).unwrap_or(0) + first.len()..];
        if let Some(range_rest) = after_first.strip_prefix("-§") {
            if let Some(second) = leading_clause(range_rest) {
                for clause in expand_range(&first, &second) {
                    clauses.insert(clause);
                }
                continue;
            }
        }
        clauses.insert(first);
    }
    clauses
}

/// Expands `5.1.1..5.1.3` (shared stem, increasing final component) to every clause in the
/// range; falls back to just the two endpoints for anything irregular.
fn expand_range(first: &str, second: &str) -> Vec<String> {
    let first_parts: Vec<&str> = first.split('.').collect();
    let second_parts: Vec<&str> = second.split('.').collect();
    if first_parts.len() == second_parts.len()
        && first_parts[..first_parts.len() - 1] == second_parts[..second_parts.len() - 1]
    {
        if let (Ok(low), Ok(high)) = (
            first_parts[first_parts.len() - 1].parse::<u32>(),
            second_parts[second_parts.len() - 1].parse::<u32>(),
        ) {
            if low <= high && high - low <= 100 {
                let stem = &first_parts[..first_parts.len() - 1];
                return (low..=high)
                    .map(|n| {
                        let mut parts: Vec<String> =
                            stem.iter().map(|s| s.to_string()).collect();
                        parts.push(n.to_string());
                        parts.join(".")
                    })
                    .collect();
            }
        }
    }
    vec![first.to_string(), second.to_string()]
}

/// A cited clause is known if the index lists it exactly, lists something more specific
/// (`§5.2` is covered by an index row for `§5.2.4`), or lists something coarser
/// (`§7.1.2` is covered by an index row for `§7.1`).
fn clause_known(index: &BTreeSet<String>, clause: &str) -> bool {
    index.iter().any(|indexed| {
        indexed == clause
            || indexed.starts_with(&format!("{clause}."))
            || clause.starts_with(&format!("{indexed}."))
    })
}

struct JsonBlock {
    /// 1-based line number of the opening fence.
    line: usize,
    body: String,
}

/// Extracts fenced ```json blocks (the corpus convention for `Justification` objects).
fn json_blocks(text: &str) -> Vec<JsonBlock> {
    let mut blocks = Vec::new();
    let mut current: Option<JsonBlock> = None;
    for (line_index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        match current.as_mut() {
            None => {
                if trimmed == "```json" {
                    current = Some(JsonBlock { line: line_index + 1, body: String::new() });
                }
            }
            Some(block) => {
                if trimmed.starts_with("```") {
                    blocks.push(current.take().unwrap());
                } else {
                    block.body.push_str(line);
                    block.body.push('\n');
                }
            }
        }
    }
    // A fence left unterminated at end-of-file still yields its block, so a
    // malformed-but-present Justification object cannot escape validation by dropping the
    // closing fence.
    blocks.extend(current);
    blocks
}

/// Structural validation of one `Justification` block against
/// `docs/iec62304/schemas/justification.schema.json` plus the repo-grounding rules the schema
/// itself cannot express (pinned `clause_ref` prefix, clause existence, evidence paths).
fn justification_issues(
    body: &str,
    is_template: bool,
    clause_index: &BTreeMap<String, BTreeSet<String>>,
    root: &Path,
) -> Vec<String> {
    let mut issues = Vec::new();
    let value: serde_json::Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(err) => return vec![format!("invalid JSON ({err})")],
    };
    let serde_json::Value::Object(map) = &value else {
        return vec!["not a JSON object".to_string()];
    };

    const ALLOWED: [&str; 6] = [
        "justification_id",
        "standard",
        "clause_ref",
        "rationale",
        "requirement_id",
        "evidence_refs",
    ];
    for key in map.keys() {
        if !ALLOWED.contains(&key.as_str()) {
            issues.push(format!("unknown field `{key}` (additionalProperties: false)"));
        }
    }

    let string_field = |name: &str, issues: &mut Vec<String>| -> Option<String> {
        match map.get(name) {
            Some(serde_json::Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
            Some(_) => {
                issues.push(format!("`{name}` must be a non-empty string"));
                None
            }
            None => {
                issues.push(format!("missing required field `{name}`"));
                None
            }
        }
    };

    if let Some(id) = string_field("justification_id", &mut issues) {
        let digits = id.strip_prefix("JUS-").unwrap_or("");
        let well_formed =
            (3..=6).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit());
        let template_placeholder = is_template && id == "JUS-NNN";
        if !well_formed && !template_placeholder {
            issues.push(format!(
                "`justification_id` `{id}` does not match JUS-<3..6 digits>"
            ));
        }
    }

    let standard = string_field("standard", &mut issues);
    let clause_ref = string_field("clause_ref", &mut issues);
    string_field("rationale", &mut issues);

    if let (Some(standard), Some(clause_ref)) = (standard, clause_ref) {
        match PINNED_STANDARDS.iter().find(|(name, _, _)| *name == standard) {
            None => issues.push(format!(
                "`standard` `{standard}` is not one of the five pinned standards"
            )),
            Some((_, pinned, _)) => {
                let expected = format!("{pinned} §");
                match clause_ref.strip_prefix(&expected) {
                    None => issues.push(format!(
                        "`clause_ref` `{clause_ref}` does not start with `{expected}`"
                    )),
                    Some(after_marker) => {
                        if let Some(clause) = leading_clause(after_marker) {
                            if !clause_known(&clause_index[*pinned], &clause) {
                                issues.push(format!(
                                    "`clause_ref` cites §{clause}, not in {pinned}'s AI-Reference.md index"
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(refs) = map.get("evidence_refs") {
        match refs {
            serde_json::Value::Array(items) => {
                for item in items {
                    match item {
                        serde_json::Value::String(evidence) => {
                            // `[ ... ]`-style placeholders are the templates' fill-in markers.
                            if evidence.starts_with('[') {
                                if !is_template {
                                    issues.push(format!(
                                        "placeholder evidence_ref `{evidence}` outside templates/"
                                    ));
                                }
                            } else if !root.join(evidence).exists() {
                                issues.push(format!(
                                    "evidence_ref `{evidence}` does not exist in the repository"
                                ));
                            }
                        }
                        _ => issues.push("`evidence_refs` items must be strings".to_string()),
                    }
                }
            }
            _ => issues.push("`evidence_refs` must be an array of strings".to_string()),
        }
    }

    if let Some(requirement) = map.get("requirement_id") {
        match requirement {
            serde_json::Value::String(s) if !s.trim().is_empty() => {}
            _ => issues.push("`requirement_id` must be a non-empty string".to_string()),
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(clauses: &[&str]) -> BTreeSet<String> {
        clauses.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recognizes_citation_keys_and_ignores_bare_mentions() {
        let line = "See `IEC 62304:2006 §5.2 Software development planning` and ISO 14971 generally.";
        let keys = citation_keys(line);
        assert_eq!(
            keys,
            vec![CitationKey {
                standard: "IEC 62304:2006".to_string(),
                clause: Some("5.2".to_string())
            }]
        );
    }

    #[test]
    fn amendment_suffix_is_not_a_citation_key() {
        assert!(citation_keys("IEC 62304:2006+AMD1:2015 — AI-Reference index").is_empty());
    }

    #[test]
    fn part_numbers_and_placeholders_parse() {
        let keys = citation_keys("`IEC 81001-5-1:2021 §4-§5 (approx.) Scope` and `ISO 14971:2019 §N.M Title`");
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].standard, "IEC 81001-5-1:2021");
        assert_eq!(keys[0].clause.as_deref(), Some("4"));
        assert_eq!(keys[1].clause, None); // digit-free placeholder: tolerated
    }

    #[test]
    fn wrong_edition_year_is_flagged_via_pinned_lookup() {
        let keys = citation_keys("`ISO 14971:2007 §4.1 Risk analysis`");
        assert_eq!(keys[0].standard, "ISO 14971:2007"); // not pinned -> Linter flags it
    }

    #[test]
    fn index_ranges_expand() {
        let clauses = index_clauses("- **§5.1.1-§5.1.3 Planning** — x\n## §1-§4 — general\n- **§7.1 Analysis** — y");
        for expected in ["5.1.1", "5.1.2", "5.1.3", "1", "2", "3", "4", "7.1"] {
            assert!(clauses.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn clause_matching_is_prefix_and_range_aware() {
        let idx = index(&["5.2.1", "5.2.4", "7.1"]);
        assert!(clause_known(&idx, "5.2")); // coarser than index rows
        assert!(clause_known(&idx, "5.2.4")); // exact
        assert!(clause_known(&idx, "7.1.2")); // finer than index row
        assert!(!clause_known(&idx, "5.3"));
        assert!(!clause_known(&idx, "5.2.40")); // no accidental string-prefix match
    }

    fn demo_index() -> BTreeMap<String, BTreeSet<String>> {
        PINNED_STANDARDS
            .iter()
            .map(|(_, pinned, _)| (pinned.to_string(), index(&["5.2", "5.3.3", "7.1"])))
            .collect()
    }

    #[test]
    fn valid_justification_passes() {
        let body = r#"{
            "justification_id": "JUS-001",
            "standard": "IEC 62304",
            "clause_ref": "IEC 62304:2006 §5.3.3 Identify segregation necessary for risk control",
            "rationale": "Governed crates forbid unsafe code.",
            "evidence_refs": ["Cargo.toml"]
        }"#;
        let issues =
            justification_issues(body, false, &demo_index(), &repo_root());
        assert!(issues.is_empty(), "unexpected issues: {issues:?}");
    }

    #[test]
    fn structural_violations_are_each_reported() {
        let body = r#"{
            "justification_id": "JUS-1",
            "standard": "IEC 62304",
            "clause_ref": "IEC 62304:2019 §5.3.3 Wrong year",
            "rationale": "x",
            "surprise": true,
            "evidence_refs": ["does/not/exist.rs"]
        }"#;
        let issues =
            justification_issues(body, false, &demo_index(), &repo_root());
        assert!(issues.iter().any(|i| i.contains("JUS-1")), "{issues:?}");
        assert!(issues.iter().any(|i| i.contains("does not start with")), "{issues:?}");
        assert!(issues.iter().any(|i| i.contains("unknown field `surprise`")), "{issues:?}");
        assert!(issues.iter().any(|i| i.contains("does/not/exist.rs")), "{issues:?}");
    }

    #[test]
    fn unknown_clause_is_reported() {
        let body = r#"{
            "justification_id": "JUS-002",
            "standard": "IEC 62304",
            "clause_ref": "IEC 62304:2006 §9.9.9 Imaginary clause",
            "rationale": "x"
        }"#;
        let issues =
            justification_issues(body, false, &demo_index(), &repo_root());
        assert!(issues.iter().any(|i| i.contains("§9.9.9")), "{issues:?}");
    }

    #[test]
    fn template_placeholders_are_tolerated_only_in_templates() {
        let body = r#"{
            "justification_id": "JUS-NNN",
            "standard": "IEC 62304",
            "clause_ref": "IEC 62304:2006 §5.3.3 Identify segregation necessary for risk control",
            "rationale": "[ ... ]",
            "evidence_refs": ["[ ... ]"]
        }"#;
        let index = demo_index();
        assert!(justification_issues(body, true, &index, &repo_root()).is_empty());
        let issues = justification_issues(body, false, &index, &repo_root());
        assert!(issues.iter().any(|i| i.contains("JUS-NNN")), "{issues:?}");
        assert!(issues.iter().any(|i| i.contains("placeholder evidence_ref")), "{issues:?}");
    }

    #[test]
    fn json_blocks_are_extracted_with_line_numbers() {
        let text = "a\n```json\n{\"justification_id\": \"JUS-001\"}\n```\nb\n";
        let blocks = json_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].line, 2);
        assert!(blocks[0].body.contains("JUS-001"));
    }

    #[test]
    fn unterminated_fence_still_yields_its_block() {
        let text = "a\n```json\n{\"justification_id\": \"JUS-001\",\n";
        let blocks = json_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].body.contains("JUS-001"));
        // The truncated body then fails JSON parsing in validation rather than being skipped.
        assert!(serde_json::from_str::<serde_json::Value>(&blocks[0].body).is_err());
    }
}
