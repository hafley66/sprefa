//! Placeholder boundary for irreversible output after SQLite commit.

use super::CommittedGeneration;

/// Capability required by future query printing, generated-file writes, audit
/// output, and perf emission. Holding this token proves semantic commit already
/// succeeded; this module performs no output until the tick split is wired.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PostCommit {
    committed: CommittedGeneration,
}

impl PostCommit {
    pub(crate) fn new(committed: CommittedGeneration) -> Self {
        Self { committed }
    }

    pub(crate) fn finish(self) {
        let _ = self.committed.into_intent();
    }
}
