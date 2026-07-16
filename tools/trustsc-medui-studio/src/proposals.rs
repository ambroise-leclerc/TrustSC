//! The propose-change flow's git side (ADR-022 wave S15): turn a serialized `.medui` document
//! into a branch + commit in the served checkout's repository, push it, and (when the `gh` CLI
//! is available) open a pull request — the regulatory gate (CI `--verify-ui` + human review)
//! stays exactly the repo's normal one.
//!
//! Everything here shells out to the `git` CLI using *plumbing* commands (`hash-object`,
//! `read-tree` into a temporary index, `write-tree`, `commit-tree`, `update-ref`): the commit is
//! built against the base revision without ever touching the checkout's working tree or index,
//! so the running server keeps serving unmodified files throughout. No git library dependency —
//! `git` (and optionally `gh`) are invoked as external tools, documented in the crate README,
//! not linked SOUP components.

use std::path::Path;
use std::process::Command;

/// Proposal configuration, read once at startup from the environment (see the crate README):
/// `TRUSTSC_STUDIO_GIT_REMOTE`, `TRUSTSC_STUDIO_GIT_AUTHOR_NAME`/`_EMAIL`,
/// `TRUSTSC_STUDIO_GIT_TOKEN`.
pub struct ProposalConfig {
    /// Remote to fetch the base from and push the proposal branch to.
    pub remote: String,
    /// Bot identity used as both author and committer of proposal commits.
    pub author_name: String,
    pub author_email: String,
    /// Forwarded to `gh` as `GH_TOKEN` for PR creation; never logged.
    pub token: Option<String>,
}

impl ProposalConfig {
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        ProposalConfig {
            remote: var("TRUSTSC_STUDIO_GIT_REMOTE").unwrap_or_else(|| "origin".to_string()),
            author_name: var("TRUSTSC_STUDIO_GIT_AUTHOR_NAME")
                .unwrap_or_else(|| "MedUI Studio".to_string()),
            author_email: var("TRUSTSC_STUDIO_GIT_AUTHOR_EMAIL")
                .unwrap_or_else(|| "medui-studio@localhost".to_string()),
            token: var("TRUSTSC_STUDIO_GIT_TOKEN"),
        }
    }
}

pub struct ProposalOutcome {
    pub branch: String,
    pub commit: String,
    /// `None` when the branch was pushed but no PR could be opened (no `gh`, no GitHub remote);
    /// `warning` then says why, so the change is never silently half-delivered.
    pub pr_url: Option<String>,
    pub warning: Option<String>,
}

/// True when the source contains `//` comment lines — which the canonical serializer drops
/// (the AST has no trivia slots), so a proposal over such a file loses them and must be
/// explicitly acknowledged by the caller.
pub fn has_comment_lines(source: &str) -> bool {
    source.lines().any(|line| line.trim_start().starts_with("//"))
}

fn run_git(repo: &Path, envs: &[(&str, &str)], args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The base revision proposals build on: the remote's default branch tip when resolvable
/// (freshly fetched), otherwise the local `HEAD` — so a checkout with no remote still gets a
/// local proposal branch.
fn resolve_base(repo: &Path, remote: &str) -> Result<String, String> {
    let fetched = run_git(repo, &[], &["fetch", "--quiet", remote]).is_ok();
    if fetched {
        for candidate in [format!("{remote}/main"), format!("{remote}/master")] {
            if let Ok(rev) = run_git(repo, &[], &["rev-parse", "--verify", "--quiet", &candidate]) {
                return Ok(rev);
            }
        }
    }
    run_git(repo, &[], &["rev-parse", "--verify", "HEAD"])
        .map_err(|error| format!("cannot resolve a base revision: {error}"))
}

/// `yyyymmdd-hhmmss` in UTC from the system clock, via the standard civil-from-days algorithm —
/// not worth a chrono dependency for one branch-name timestamp.
fn utc_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let (hh, mm, ss) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    // Howard Hinnant's civil_from_days, days since 1970-01-01.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}{month:02}{day:02}-{hh:02}{mm:02}{ss:02}")
}

/// Creates the proposal: a commit on a fresh `medui-studio/<stem>-<timestamp>` branch whose only
/// change against the base revision is `screen_rel_path` replaced by `serialized`, pushed to the
/// configured remote, with a PR opened via `gh` when possible.
pub fn create_proposal(
    repo: &Path,
    screen_rel_path: &str,
    serialized: &str,
    title: &str,
    description: &str,
    config: &ProposalConfig,
) -> Result<ProposalOutcome, String> {
    let base = resolve_base(repo, &config.remote)?;

    let stem = Path::new(screen_rel_path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| "screen".to_string());
    let branch = format!("medui-studio/{stem}-{}", utc_timestamp());

    // Build the commit with a throwaway index so the working tree/index of the serving checkout
    // are never touched. The counter makes concurrent proposals (same pid, same second) use
    // disjoint scratch dirs — pid+timestamp alone raced.
    static SCRATCH_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let scratch = std::env::temp_dir().join(format!(
        "trustsc-medui-studio-proposal-{}-{}",
        std::process::id(),
        SCRATCH_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&scratch).map_err(|error| format!("cannot create scratch dir: {error}"))?;
    let index_path = scratch.join("index");
    let index = index_path.to_string_lossy().to_string();
    let index_env: [(&str, &str); 1] = [("GIT_INDEX_FILE", index.as_str())];

    let result = (|| {
        let blob_source = scratch.join("proposal.medui");
        std::fs::write(&blob_source, serialized)
            .map_err(|error| format!("cannot write scratch blob: {error}"))?;
        let blob = run_git(
            repo,
            &[],
            &["hash-object", "-w", "--", &blob_source.to_string_lossy()],
        )?;

        run_git(repo, &index_env, &["read-tree", &base])?;
        run_git(
            repo,
            &index_env,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("100644,{blob},{screen_rel_path}"),
            ],
        )?;
        let tree = run_git(repo, &index_env, &["write-tree"])?;

        let base_tree = run_git(repo, &[], &["rev-parse", &format!("{base}^{{tree}}")])?;
        if tree == base_tree {
            return Err("the serialized screen is identical to the committed file; nothing to propose".to_string());
        }

        let message = if description.trim().is_empty() {
            title.to_string()
        } else {
            format!("{title}\n\n{description}")
        };
        let identity: [(&str, &str); 4] = [
            ("GIT_AUTHOR_NAME", config.author_name.as_str()),
            ("GIT_AUTHOR_EMAIL", config.author_email.as_str()),
            ("GIT_COMMITTER_NAME", config.author_name.as_str()),
            ("GIT_COMMITTER_EMAIL", config.author_email.as_str()),
        ];
        let commit = run_git(repo, &identity, &["commit-tree", &tree, "-p", &base, "-m", &message])?;
        run_git(repo, &[], &["update-ref", &format!("refs/heads/{branch}"), &commit])?;
        Ok((commit, message))
    })();
    let _ = std::fs::remove_dir_all(&scratch);
    let (commit, _message) = result?;

    if let Err(error) = run_git(
        repo,
        &[],
        &["push", "--quiet", &config.remote, &format!("refs/heads/{branch}:refs/heads/{branch}")],
    ) {
        return Ok(ProposalOutcome {
            branch: branch.clone(),
            commit,
            pr_url: None,
            warning: Some(format!(
                "branch {branch} was created locally but could not be pushed to `{}`: {error}",
                config.remote
            )),
        });
    }

    match open_pull_request(repo, &branch, title, description, config) {
        Ok(url) => Ok(ProposalOutcome { branch, commit, pr_url: Some(url), warning: None }),
        Err(error) => Ok(ProposalOutcome {
            branch: branch.clone(),
            commit,
            pr_url: None,
            warning: Some(format!("branch {branch} was pushed, but no PR was opened: {error}")),
        }),
    }
}

/// Opens the PR with the GitHub CLI. `gh` was chosen over an in-process client (`octocrab`)
/// deliberately: it keeps the whole GitHub API surface out of this crate's dependency tree (no
/// new SOUP entries) and inherits whatever auth the host already has, with
/// `TRUSTSC_STUDIO_GIT_TOKEN` overriding via `GH_TOKEN` when set.
fn open_pull_request(
    repo: &Path,
    branch: &str,
    title: &str,
    description: &str,
    config: &ProposalConfig,
) -> Result<String, String> {
    let url = run_git(repo, &[], &["remote", "get-url", &config.remote])?;
    if !url.contains("github.com") {
        return Err(format!(
            "remote `{}` ({url}) is not a github.com URL; open the pull request manually from the pushed branch",
            config.remote
        ));
    }
    let mut command = Command::new("gh");
    command
        .arg("-R")
        .arg(slug_from_url(&url)?)
        .args(["pr", "create", "--head", branch, "--title", title, "--body", description]);
    command.current_dir(repo);
    if let Some(token) = &config.token {
        command.env("GH_TOKEN", token);
    }
    let output = command
        .output()
        .map_err(|error| format!("failed to run gh: {error}"))?;
    if !output.status.success() {
        return Err(format!("gh pr create failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .rev()
        .find(|line| line.starts_with("https://"))
        .map(str::to_string)
        .ok_or_else(|| "gh pr create printed no PR URL".to_string())
}

/// Extracts the `owner/repo` slug for `gh -R` — `gh` alone would guess from the working
/// directory, which is exactly the ambiguity we don't want on a server. Handles the two common
/// git URL shapes: scp-like ssh
/// (`git@github.com:owner/repo.git`) and scheme URLs (`https://github.com/owner/repo.git`).
fn slug_from_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let tail = if let Some(after_scheme) = trimmed.find("://").map(|idx| &trimmed[idx + 3..]) {
        after_scheme.split_once('/').map(|(_, path)| path).unwrap_or("")
    } else if let Some((head, after_colon)) = trimmed.split_once(':') {
        if head.contains('/') { trimmed } else { after_colon }
    } else {
        trimmed
    };
    let segments = tail.split('/').filter(|segment| !segment.is_empty()).collect::<Vec<_>>();
    match segments.as_slice() {
        [.., owner, name] => Ok(format!("{owner}/{name}")),
        _ => Err(format!("cannot parse an owner/repo slug from remote url: {url}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_comment_lines_detects_full_line_comments_only() {
        assert!(has_comment_lines("Screen X {\n    // a note\n}\n"));
        assert!(has_comment_lines("// leading\nScreen X {\n}\n"));
        assert!(!has_comment_lines("Screen X {\n    source: \"NOT//COMMENT\";\n}\n"));
    }

    #[test]
    fn slug_from_url_handles_ssh_and_https_shapes() {
        for (url, expected) in [
            ("git@github.com:owner/repo.git", "owner/repo"),
            ("https://github.com/owner/repo.git", "owner/repo"),
            ("https://github.com/owner/repo", "owner/repo"),
            ("ssh://git@github.com/owner/repo.git", "owner/repo"),
        ] {
            assert_eq!(slug_from_url(url).as_deref(), Ok(expected), "url: {url}");
        }
        assert!(slug_from_url("https://github.com/").is_err());
    }

    #[test]
    fn utc_timestamp_has_the_documented_shape() {
        let stamp = utc_timestamp();
        assert_eq!(stamp.len(), 15, "was: {stamp}");
        assert_eq!(stamp.as_bytes()[8], b'-', "was: {stamp}");
        assert!(stamp[..8].chars().all(|c| c.is_ascii_digit()), "was: {stamp}");
        assert!(stamp[9..].chars().all(|c| c.is_ascii_digit()), "was: {stamp}");
        // Sanity: the year is in a plausible range, catching an off-by-era bug.
        let year: u32 = stamp[..4].parse().unwrap();
        assert!((2024..2100).contains(&year), "was: {stamp}");
    }
}
