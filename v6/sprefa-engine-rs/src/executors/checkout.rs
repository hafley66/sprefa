//! `repo_checkout(repo_slug, dest_root, want_sha) -> (checkout_path, head_sha)`
//! through soopy. No `git` reaches a shell and no `sh -c` is spawned.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::hosts::{HostError, IHostExecutor};
use crate::types::HostRow;

use super::{first_input, host_error, required_input, resolve_home, row};

/// Soopy exports no clone and no checkout: `Acquisition` covers FetchRef,
/// FetchTag, Deepen, Unshallow only, so an absent directory is a named stop.
const CLONE_GAP: &str =
    "soopy exports no clone/checkout call (Acquisition covers FetchRef, FetchTag, Deepen, \
     Unshallow only; _4_worktree exports `enumerate` alone), so this executor observes an \
     existing checkout and cannot create one";

pub struct SoopyCheckoutExecutor;

impl IHostExecutor for SoopyCheckoutExecutor {
    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let repo_slug = required_input(host, env, &["repo_slug", "slug"])?;
        let dest_root = required_input(host, env, &["dest_root", "root"])?;
        let want_sha = first_input(env, &["want_sha", "sha"]).unwrap_or("");
        let checkout_path = resolve_home(dest_root).join(repo_slug);
        let span = tracing::info_span!("soopy_checkout", host, repo_slug,
            path = %checkout_path.display(), head_sha = tracing::field::Empty);
        let _entered = span.enter();
        if !checkout_path.is_dir() {
            return Err(host_error(
                host,
                format!(
                    "soopy_clone_missing: {} is absent and {CLONE_GAP}",
                    checkout_path.display()
                ),
            ));
        }
        let repository = soopy::discover(&checkout_path).map_err(|failure| {
            host_error(host, format!("open {}: {failure}", checkout_path.display()))
        })?;
        let query = soopy::RefQuery {
            repository: repository.identity.clone(),
            namespace: Arc::from("refs/heads"),
            name: None,
            pattern: None,
        };
        let snapshot = soopy::Refs::open(repository)
            .snapshot(&query)
            .map_err(|failure| host_error(host, format!("read HEAD of {repo_slug}: {failure}")))?;
        let head_sha = match snapshot.head_target {
            Some(object) => object.0.to_string(),
            None => return Err(host_error(host, format!("{repo_slug} has an unborn HEAD"))),
        };
        span.record("head_sha", &head_sha);
        if !want_sha.is_empty() && want_sha != head_sha {
            tracing::warn!(host, repo_slug, want_sha, head_sha, "checkout is stale");
        }
        Ok(vec![row([
            ("repo_slug", serde_json::json!(repo_slug)),
            ("dest_root", serde_json::json!(dest_root)),
            (
                "checkout_path",
                serde_json::json!(checkout_path.to_string_lossy()),
            ),
            ("head_sha", serde_json::json!(head_sha)),
        ])])
    }
}
