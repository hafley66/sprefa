//! `/soopy/watch(glob) -> (path, digest)`: the tracked worktree files a glob
//! names, re-called by run.rs's resident loop on the watcher's own wakeup.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::hosts::{ExecutorCadence, HostError, IHostExecutor};
use crate::types::HostRow;

use super::{required_input, row};

pub struct SoopyWatchExecutor;

impl SoopyWatchExecutor {
    pub fn new() -> SoopyWatchExecutor {
        SoopyWatchExecutor
    }

    /// Every tracked file the glob names, worktree revision, blob digest.
    pub fn enumerate_glob(
        root: &std::path::Path,
        glob: &str,
    ) -> Result<Vec<(String, String)>, String> {
        let repository = soopy::discover(root)
            .map_err(|error| format!("open a repository at {}: {error}", root.display()))?;
        let entries = soopy::enumerate(
            &repository,
            &soopy::GitFilesQuery {
                revision: soopy::Revision::Worktree,
                pathspecs: vec![glob.to_string()],
            },
        )
        .map_err(|error| format!("enumerate `{glob}`: {error}"))?;
        entries
            .into_iter()
            .map(|entry| match &entry.content {
                soopy::ContentId::GitBlob(oid) => {
                    Ok((entry.source.path.0.to_string(), oid.0.to_string()))
                }
                other => Err(format!(
                    "tracked file {} carries {other:?}, not a git blob",
                    entry.source.path.0
                )),
            })
            .collect()
    }
}

impl IHostExecutor for SoopyWatchExecutor {
    fn cadence(&self) -> ExecutorCadence {
        ExecutorCadence::Continuing
    }

    fn run(
        &self,
        host: &str,
        _command_line: &str,
        env: &BTreeMap<String, String>,
    ) -> Result<Vec<HostRow>, HostError> {
        let glob = required_input(host, env, &["glob"])?.to_string();
        let root = env
            .get("repo")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let entries = SoopyWatchExecutor::enumerate_glob(&root, &glob)
            .map_err(|message| super::host_error(host, message))?;
        Ok(entries
            .into_iter()
            .map(|(path, digest)| {
                row([
                    ("path", serde_json::json!(path)),
                    ("digest", serde_json::json!(digest)),
                ])
            })
            .collect())
    }
}
