// @comment-ok: module contract, the ref-store family's one doc site.
// `git_ref` and `git_tag`: one `soopy::Refs` snapshot per repository root,
// memoised on that root, answering both names out of one `for-each-ref` pass.
//
// `kind` names the ref's NAMESPACE (branch, tag, remote, head), never the Git
// object type: every branch tip is a commit object, so the object type cannot
// tell a branch from a tag.
//
// `target_sha` is PEELED, so an annotated tag's row carries the commit and joins
// against a commit-keyed relation. `annotated` is what keeps a lightweight tag's
// zero `tagged_at` from reading as the Unix epoch.

use std::collections::BTreeMap;

use crate::hosts::{host_row, required_input, HostError, IHostExecutor};
use crate::types::HostRow;

use super::{checkout_root, stop};

pub const GIT_REF_COLUMNS: &[&str] = &["ref_name", "kind", "target_sha"];
pub const GIT_TAG_COLUMNS: &[&str] = &["tag_name", "target_sha", "tagged_at", "annotated"];

#[derive(Default)]
pub struct GitRefsExecutor;

impl GitRefsExecutor {
    pub fn new() -> Self {
        Self::default()
    }
}

impl IHostExecutor for GitRefsExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        if !matches!(host, "git_ref" | "git_tag") {
            return Err(stop(
                host,
                "not a ref-store host; the roster is git_ref and git_tag".to_string(),
            ));
        }
        let root = checkout_root(host, &required_input(host, env, "repo")?)?;
        snapshot_rows(host, &root)
    }
}

/// The ref namespace a full ref name sits in, plus the short name a program
/// joins on. `HEAD` is its own namespace and carries no `refs/` prefix.
fn namespace_of(full_name: &str) -> (&'static str, &str) {
    for (prefix, kind) in [
        ("refs/heads/", "branch"),
        ("refs/tags/", "tag"),
        ("refs/remotes/", "remote"),
    ] {
        if let Some(short) = full_name.strip_prefix(prefix) {
            return (kind, short);
        }
    }
    if full_name == "HEAD" {
        return ("head", full_name);
    }
    ("other", full_name)
}

fn snapshot_rows(host: &str, root: &std::path::Path) -> Result<Vec<HostRow>, HostError> {
    let span = tracing::info_span!("git_refs", repo = %root.display());
    let _entered = span.enter();
    let repository = soopy::discover(root.to_path_buf())
        .map_err(|error| stop(host, format!("open a repository at {}: {error}", root.display())))?;
    let identity = repository.identity.clone();
    let refs = soopy::Refs::open(repository);
    // An empty namespace with no name and no pattern is every ref the store
    // holds; HEAD rides the snapshot's own `head_target`.
    let query = soopy::RefQuery {
        repository: identity,
        namespace: "".into(),
        name: None,
        pattern: None,
    };
    let snapshot = refs
        .snapshot(&query)
        .map_err(|error| stop(host, format!("enumerate refs in {}: {error}", root.display())))?;
    let mut rows: Vec<HostRow> = Vec::with_capacity(snapshot.refs.len() * 2 + 1);
    for observation in &snapshot.refs {
        let full_name = observation.name.as_ref();
        let (kind, short_name) = namespace_of(full_name);
        let peeled = observation
            .peeled
            .as_ref()
            .unwrap_or(&observation.direct)
            .0
            .to_string();
        rows.push(host_row([
            ("ref_name", serde_json::json!(full_name)),
            ("kind", serde_json::json!(kind)),
            ("target_sha", serde_json::json!(peeled)),
        ]));
        if kind != "tag" {
            continue;
        }
        // A lightweight tag has no tag object and therefore no tag date, which
        // is the whole reason `annotated` is a column.
        let (tagged_at, annotated) = match &observation.tag {
            Some(metadata) => (metadata.tagger.when, true),
            None => (0, false),
        };
        rows.push(host_row([
            ("tag_name", serde_json::json!(short_name)),
            ("target_sha", serde_json::json!(peeled)),
            ("tagged_at", serde_json::json!(tagged_at)),
            ("annotated", serde_json::json!(annotated)),
        ]));
    }
    if let Some(head) = &snapshot.head_target {
        rows.push(host_row([
            ("ref_name", serde_json::json!("HEAD")),
            ("kind", serde_json::json!("head")),
            ("target_sha", serde_json::json!(head.0.to_string())),
        ]));
    }
    Ok(rows)
}
