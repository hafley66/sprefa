// @comment-ok: module contract, the rev-pinned file-surface family's doc site.
// `repo_files_at(root, rev, glob) -> (path, digest)` and
// `repo_grep_at(root, rev, glob, pattern) -> (path, line, g1, g2, g3)`.
//
// THE ROOT IS AN INPUT COLUMN, which is the whole difference from the shipped
// single-repository `files` host: crossing repositories needs the checkout to
// be part of what the demand is about, after which each repository is one
// soopy call and asking twice for one (root, rev, glob) is free.
//
// THE REV IS WHY THESE EXIST. A host witness is content-addressed over its
// input columns, so a worktree-only host caches its first answer for the life
// of the db. A rev-pinned host is asked once per rev and each commit is a new
// witness by construction.
//
// REGEX DIALECT IS THE RUST `regex` CRATE, which is v5's own and NOT the
// python `re` the paused tsv2 leg ran through `parity-grep.py`. The row shape
// is that script's contract verbatim: `find_iter` per LINE, so a line with N
// matches emits N rows, and an absent capture group renders `""` and never
// null, because a null column drops the whole row at `select_columns`.

use std::collections::BTreeMap;

use crate::hosts::{host_row, required_input, HostError, IHostExecutor};
use crate::types::HostRow;

use super::{checkout_root, revision_of, stop};

pub const REPO_FILES_AT_COLUMNS: &[&str] = &["path", "digest"];
pub const REPO_GREP_AT_COLUMNS: &[&str] = &["path", "line", "g1", "g2", "g3"];

/// The three capture groups the row shape declares, beyond group 0.
const DECLARED_GROUPS: usize = 3;

#[derive(Default)]
pub struct RepoAtExecutor;

impl RepoAtExecutor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IHostExecutor for RepoAtExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let root = checkout_root(host, &required_input(host, env, "root")?)?;
        let rev = required_input(host, env, "rev")?;
        let glob = required_input(host, env, "glob")?;
        match host {
            "repo_files_at" => listing_rows(host, &root, &rev, &glob),
            "repo_grep_at" => {
                let pattern = required_input(host, env, "pattern")?;
                grep_rows(host, &root, &rev, &glob, &pattern)
            }
            _ => Err(stop(
                host,
                "not a rev-pinned file host; the roster is repo_files_at and repo_grep_at"
                    .to_string(),
            )),
        }
    }
}

fn entries_at(
    host: &str,
    root: &std::path::Path,
    rev: &str,
    glob: &str,
) -> Result<(soopy::SourceTree, Vec<soopy::SourceEntry>), HostError> {
    let repository = soopy::discover(root.to_path_buf()).map_err(|error| {
        stop(
            host,
            format!("open a repository at {}: {error}", root.display()),
        )
    })?;
    let mut tree = soopy::SourceTree::open(repository);
    let pathspecs = if glob.is_empty() {
        Vec::new()
    } else {
        vec![glob.to_string()]
    };
    let entries = tree
        .git_files(&soopy::GitFilesQuery {
            revision: revision_of(rev),
            pathspecs,
        })
        .map_err(|error| {
            stop(
                host,
                format!("enumerate `{glob}` at {rev} in {}: {error}", root.display()),
            )
        })?;
    Ok((tree, entries))
}

fn digest_of(host: &str, entry: &soopy::SourceEntry) -> Result<String, HostError> {
    match &entry.content {
        soopy::ContentId::GitBlob(oid) => Ok(oid.0.to_string()),
        // Only a worktree revision yields a Blake3 address, and nothing
        // downstream can resolve one back to bytes through the blob batch.
        other => Err(stop(
            host,
            format!(
                "{} carries {other:?}, not a git blob",
                entry.source.path.0.as_ref()
            ),
        )),
    }
}

fn listing_rows(
    host: &str,
    root: &std::path::Path,
    rev: &str,
    glob: &str,
) -> Result<Vec<HostRow>, HostError> {
    let span = tracing::info_span!("repo_files_at", repo = %root.display(), rev, glob);
    let _entered = span.enter();
    let (_tree, entries) = entries_at(host, root, rev, glob)?;
    let mut rows = Vec::with_capacity(entries.len());
    for entry in &entries {
        rows.push(host_row([
            ("path", serde_json::json!(entry.source.path.0.as_ref())),
            ("digest", serde_json::json!(digest_of(host, entry)?)),
        ]));
    }
    Ok(rows)
}

fn grep_rows(
    host: &str,
    root: &std::path::Path,
    rev: &str,
    glob: &str,
    pattern: &str,
) -> Result<Vec<HostRow>, HostError> {
    let span = tracing::info_span!("repo_grep_at", repo = %root.display(), rev, glob);
    let _entered = span.enter();
    let matcher = regex::Regex::new(pattern)
        .map_err(|error| stop(host, format!("compile `{pattern}`: {error}")))?;
    let (mut tree, entries) = entries_at(host, root, rev, glob)?;
    let requests: Vec<soopy::ReadRequest> = entries
        .into_iter()
        .map(|entry| soopy::ReadRequest {
            source: entry.source,
            expected: Some(entry.content),
        })
        .collect();
    let mut rows: Vec<HostRow> = Vec::new();
    let mut buffer = Vec::new();
    // Bytes are visited through one reusable buffer and dropped, so a wide
    // pathspec over a large checkout never holds the corpus in memory.
    tree.read_each(&requests, &mut buffer, |answer| {
        let path = answer.source.path.0.to_string();
        // A stray non-UTF8 byte must not delete a whole file's rows.
        let text = String::from_utf8_lossy(answer.bytes);
        for (index, line) in text.lines().enumerate() {
            for captures in matcher.captures_iter(line) {
                let mut columns = vec![
                    ("path".to_string(), serde_json::json!(path)),
                    ("line".to_string(), serde_json::json!(index + 1)),
                ];
                for group in 1..=DECLARED_GROUPS {
                    let text = captures.get(group).map_or("", |found| found.as_str());
                    columns.push((format!("g{group}"), serde_json::json!(text)));
                }
                rows.push(columns.into_iter().collect());
            }
        }
        Ok(())
    })
    .map_err(|error| stop(host, format!("read `{glob}` at {rev}: {error:#}")))?;
    Ok(rows)
}
