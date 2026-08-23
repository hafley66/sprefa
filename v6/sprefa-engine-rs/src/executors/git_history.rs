// @comment-ok: module contract, the two commit-history families' one doc site.
// TWO FAMILIES, TWO MEMO KEYS, both keyed on the whole demand triple.
//
// The PAIRWISE family (`git_merge_base`, `git_ancestor`, `git_ahead_behind`) is
// one `soopy::RevisionGraph` answer per (repo, rev_a, rev_b): Git has no batch
// form for the pairwise relations, so three declarations would otherwise cost
// six processes.
//
// The DIFF family (`git_change`, `git_rename`, `git_changed_line`) is one
// `SoopyRevisionDiffer` answer per (repo, rev_base, rev_head): two tracked-file
// listings and one `cat-file --batch` over the blobs the line diff reads.
//
// ABSENCE IS A ROW COUNT, NEVER A NULL. Two revisions with no shared history
// produce zero `git_merge_base` rows, two diverged tips zero `git_ancestor`
// rows, and an unchanged path no row in any diff relation.

use std::collections::BTreeMap;

use crate::change_facts::{IRevisionDiffer, RevisionDiff, SoopyRevisionDiffer};
use crate::hosts::{host_row, required_input, HostError, IHostExecutor};
use crate::types::HostRow;

use super::{checkout_root, revision_of, stop};

pub const GIT_MERGE_BASE_COLUMNS: &[&str] = &["base_sha"];
pub const GIT_AHEAD_BEHIND_COLUMNS: &[&str] = &["ahead_count", "behind_count"];
pub const GIT_ANCESTOR_COLUMNS: &[&str] = &["ancestor_sha", "descendant_sha"];

#[derive(Copy, Clone, PartialEq, Eq)]
enum Family {
    Pair,
    Diff,
}

fn family_of(host: &str) -> Option<Family> {
    match host {
        "git_merge_base" | "git_ancestor" | "git_ahead_behind" => Some(Family::Pair),
        "git_change" | "git_rename" | "git_changed_line" => Some(Family::Diff),
        _ => None,
    }
}

#[derive(Default)]
pub struct GitHistoryExecutor;

impl GitHistoryExecutor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IHostExecutor for GitHistoryExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let Some(family) = family_of(host) else {
            return Err(stop(
                host,
                "not a commit-history host; the roster is git_merge_base, git_ancestor, \
                 git_ahead_behind, git_change, git_rename and git_changed_line"
                    .to_string(),
            ));
        };
        let root = checkout_root(host, &required_input(host, env, "repo")?)?;
        match family {
            Family::Pair => {
                let rev_a = required_input(host, env, "rev_a")?;
                let rev_b = required_input(host, env, "rev_b")?;
                pair_rows(host, &root, &rev_a, &rev_b)
            }
            Family::Diff => {
                let base = required_input(host, env, "rev_base")?;
                let head = required_input(host, env, "rev_head")?;
                diff_rows(host, &root, &base, &head)
            }
        }
    }
}

fn pair_rows(
    host: &str,
    root: &std::path::Path,
    rev_a: &str,
    rev_b: &str,
) -> Result<Vec<HostRow>, HostError> {
    let span = tracing::info_span!("git_pair", repo = %root.display(), rev_a, rev_b);
    let _entered = span.enter();
    let repository = soopy::discover(root.to_path_buf()).map_err(|error| {
        stop(
            host,
            format!("open a repository at {}: {error}", root.display()),
        )
    })?;
    let identity = repository.identity.clone();
    let graph = soopy::RevisionGraph::open(repository);
    let resolved = graph
        .query(&soopy::RevisionGraphQuery {
            repository: identity.clone(),
            resolve: vec![revision_of(rev_a), revision_of(rev_b)],
            parents: Vec::new(),
            ancestry: Vec::new(),
            merge_bases: Vec::new(),
            ahead_behind: Vec::new(),
            walks: Vec::new(),
        })
        .map_err(|error| stop(host, format!("resolve {rev_a} and {rev_b}: {error}")))?;
    // A revision this repository cannot resolve answers ZERO rows: the pair was
    // never a fact about this checkout, and a stop would red the whole fold.
    let mut commits = resolved
        .resolutions
        .iter()
        .map(|resolution| match resolution {
            soopy::RevisionResolution::Present(oid)
            | soopy::RevisionResolution::ShallowBoundary(oid) => Some(oid.clone()),
            soopy::RevisionResolution::Absent | soopy::RevisionResolution::CorruptObject => None,
        });
    let (Some(Some(left)), Some(Some(right))) = (commits.next(), commits.next()) else {
        return Ok(Vec::new());
    };
    let answer = graph
        .query(&soopy::RevisionGraphQuery {
            repository: identity,
            resolve: Vec::new(),
            parents: Vec::new(),
            ancestry: vec![(left.clone(), right.clone()), (right.clone(), left.clone())],
            merge_bases: vec![(left.clone(), right.clone())],
            ahead_behind: vec![(left, right)],
            walks: Vec::new(),
        })
        .map_err(|error| {
            stop(
                host,
                format!("walk the graph for {rev_a} and {rev_b}: {error}"),
            )
        })?;
    let mut rows: Vec<HostRow> = Vec::new();
    for base in answer.merge_bases.iter().flat_map(|pair| &pair.bases) {
        rows.push(host_row([(
            "base_sha",
            serde_json::json!(base.0.to_string()),
        )]));
    }
    for counts in &answer.ahead_behind {
        rows.push(host_row([
            ("ahead_count", serde_json::json!(counts.ahead)),
            ("behind_count", serde_json::json!(counts.behind)),
        ]));
    }
    for edge in answer.ancestry.iter().filter(|edge| edge.is_ancestor) {
        rows.push(host_row([
            (
                "ancestor_sha",
                serde_json::json!(edge.ancestor.0.to_string()),
            ),
            (
                "descendant_sha",
                serde_json::json!(edge.descendant.0.to_string()),
            ),
        ]));
    }
    Ok(rows)
}

fn diff_rows(
    host: &str,
    root: &std::path::Path,
    rev_base: &str,
    rev_head: &str,
) -> Result<Vec<HostRow>, HostError> {
    let span = tracing::info_span!("git_diff", repo = %root.display(), rev_base, rev_head);
    let _entered = span.enter();
    let diff: RevisionDiff = SoopyRevisionDiffer
        .diff(
            &root.display().to_string(),
            &revision_of(rev_base),
            &revision_of(rev_head),
        )
        .map_err(|error| {
            stop(
                host,
                format!(
                    "diff {rev_base}..{rev_head} in {}: {error:#}",
                    root.display()
                ),
            )
        })?;
    let mut rows: Vec<HostRow> =
        Vec::with_capacity(diff.changes.len() + diff.renames.len() + diff.changed_lines.len());
    for change in &diff.changes {
        rows.push(host_row([
            ("change_kind", serde_json::json!(change.kind.as_str())),
            ("path", serde_json::json!(change.path)),
        ]));
    }
    for rename in &diff.renames {
        rows.push(host_row([
            ("path_from", serde_json::json!(rename.path_from)),
            ("path_to", serde_json::json!(rename.path_to)),
        ]));
    }
    for line in &diff.changed_lines {
        rows.push(host_row([
            ("path", serde_json::json!(line.path)),
            ("line_number", serde_json::json!(line.line_number)),
        ]));
    }
    Ok(rows)
}
