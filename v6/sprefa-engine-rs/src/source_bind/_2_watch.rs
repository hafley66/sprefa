use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use sprefa_rust_runtime_host::{
    ClockedSourceHostRequest, ReadRequestWire, SourceHostDemand, SourceIdentityDemand,
};

use super::GitSourceRegistration;

/// The watcher owns one debounced Soopy source stream per explicitly
/// registered worktree. The worktree key is part of the map key so linked
/// checkouts in one repository cannot feed each other's ticks.
#[derive(Default)]
pub(crate) struct SourceBindWatchers {
    streams: BTreeMap<soopy::WorktreeId, soopy::SourceWatcher>,
}

impl SourceBindWatchers {
    pub(crate) fn register(
        &mut self,
        root: impl AsRef<Path>,
        query: soopy::SourceQuery,
    ) -> Result<GitSourceRegistration> {
        let repository = soopy::discover(root.as_ref())?;
        let registration = GitSourceRegistration {
            repository: repository.identity.clone(),
            worktree: repository.worktree.clone(),
        };
        let watcher = soopy::SourceTree::open(repository).watch(query)?;
        self.streams.insert(registration.worktree.clone(), watcher);
        Ok(registration)
    }

    pub(crate) fn recv(
        &mut self,
        worktree: &soopy::WorktreeId,
        timeout: Duration,
    ) -> Result<Option<Vec<soopy::SourceDelta>>> {
        let watcher = self
            .streams
            .get_mut(worktree)
            .ok_or_else(|| anyhow::anyhow!("worktree {} has no watcher", worktree.0))?;
        watcher.recv_timeout(timeout)
    }
}

/// Convert one Soopy logical batch into the source-host identity request that
/// the normal SourceBind path already knows how to execute. A replacement or
/// rename stays one request and therefore one engine tick.
pub(crate) fn request_for_deltas(
    clock: u64,
    deltas: &[soopy::SourceDelta],
) -> Option<ClockedSourceHostRequest> {
    let mut reads = BTreeMap::<soopy::SourceRef, ReadRequestWire>::new();
    let mut retire_sources = BTreeSet::<soopy::SourceRef>::new();
    for delta in deltas {
        match delta {
            soopy::SourceDelta::Added(entry) => {
                reads.insert(
                    entry.source.clone(),
                    ReadRequestWire {
                        source: entry.source.clone(),
                        expected: Some(entry.content.clone()),
                    },
                );
            }
            soopy::SourceDelta::Changed { before: _, after } => {
                reads.insert(
                    after.source.clone(),
                    ReadRequestWire {
                        source: after.source.clone(),
                        expected: Some(after.content.clone()),
                    },
                );
            }
            soopy::SourceDelta::Removed(source) => {
                retire_sources.insert(source.clone());
            }
            soopy::SourceDelta::RevisionChanged { .. } | soopy::SourceDelta::RescanRequired => {}
        }
    }
    if reads.is_empty() && retire_sources.is_empty() {
        return None;
    }
    Some(ClockedSourceHostRequest {
        clock,
        demand: SourceHostDemand::Identity(SourceIdentityDemand {
            reads: reads.into_values().collect(),
            spans: Vec::new(),
            retire_sources: retire_sources.into_iter().collect(),
            store_git_bytes: false,
        }),
    })
}
