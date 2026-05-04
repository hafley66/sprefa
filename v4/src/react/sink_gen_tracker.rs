// SinkGenTracker — per-sink generation counters for SinkGen wake.
//
// One Vec<Gen> indexed by SinkId. `advance(sink)` bumps the counter and
// returns the new gen. `snapshot()` borrows a slice for `pull_runnable`.
//
// Replaces the legacy global AtomicU64 GEN. Each sink is its own
// timeline; cross-sink ordering joins at the queue row's drive_tick.

use std::sync::RwLock;

use crate::Gen;
use super::wake::SinkId;

pub struct SinkGenTracker {
    gens: RwLock<Vec<Gen>>,
}

impl SinkGenTracker {
    pub fn new() -> Self {
        Self { gens: RwLock::new(Vec::new()) }
    }

    /// Ensure the tracker has at least `n_sinks` entries. Existing
    /// counters are preserved; new slots start at 0.
    pub fn reserve(&self, n_sinks: usize) {
        let mut g = self.gens.write().unwrap();
        if g.len() < n_sinks { g.resize(n_sinks, 0); }
    }

    /// Bump the counter for `sink` and return the new value. Grows
    /// the vec if needed.
    pub fn advance(&self, sink: SinkId) -> Gen {
        let mut g = self.gens.write().unwrap();
        let idx = sink as usize;
        if g.len() <= idx { g.resize(idx + 1, 0); }
        g[idx] += 1;
        g[idx]
    }

    /// Read the current gen for `sink`. 0 if never advanced.
    pub fn current(&self, sink: SinkId) -> Gen {
        let g = self.gens.read().unwrap();
        g.get(sink as usize).copied().unwrap_or(0)
    }

    /// Clone the gens vec for passing into `pull_runnable`. Cheap; the
    /// vec is small (one entry per registered sink).
    pub fn snapshot(&self) -> Vec<Gen> {
        self.gens.read().unwrap().clone()
    }
}

impl Default for SinkGenTracker {
    fn default() -> Self { Self::new() }
}
