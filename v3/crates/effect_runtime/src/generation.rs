//! Process-wide monotonic clock for sprefa.
//!
//! One `GenCounter` per `RtCtx`. Reachable via `ctx.current_gen()` from
//! every op and batcher. Bumped at exactly the points that re-trigger
//! pipeline evaluation: LSP did_open / did_change / did_save, fs-watcher
//! events, cold-start, manual reload. Read at every staged-effect
//! enqueue site so downstream actors can correlate effect rows back to
//! the round that produced them.
//!
//! See human-goals.md item 13 ("ID all input events") and
//! sprf-dd-purity-and-durability for the broader durability contract:
//! gen IS DD's `Time`, the same value drives `advance_to(T)` and the
//! `StagedRow.gen` capture.

use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic round identifier. Process-local. Not wall-clock.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Generation(pub u64);

impl Generation {
    pub const ZERO: Self = Self(0);
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for Generation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "g{}", self.0)
    }
}

/// Atomic counter behind `RtCtx::gen`. Shared across every actor that
/// reaches the same RtCtx (or holds a clone of the `Arc<GenCounter>`).
pub struct GenCounter {
    inner: AtomicU64,
}

impl GenCounter {
    pub fn new() -> Self {
        Self {
            inner: AtomicU64::new(0),
        }
    }

    /// Sample the current round. Acquire load — sees every value
    /// previously written by any thread before this load returned.
    pub fn current(&self) -> Generation {
        Generation(self.inner.load(Ordering::Acquire))
    }

    /// Advance to the next round. Returns the new value (post-increment).
    /// AcqRel so writers and readers across threads agree on order.
    pub fn bump(&self) -> Generation {
        let prev = self.inner.fetch_add(1, Ordering::AcqRel);
        Generation(prev + 1)
    }
}

impl Default for GenCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn starts_at_zero_and_increments() {
        let g = GenCounter::new();
        assert_eq!(g.current(), Generation::ZERO);
        assert_eq!(g.bump(), Generation(1));
        assert_eq!(g.bump(), Generation(2));
        assert_eq!(g.current(), Generation(2));
    }

    #[test]
    fn shared_counter_via_arc_is_single_source_of_truth() {
        let g = Arc::new(GenCounter::new());
        let g2 = Arc::clone(&g);
        g.bump();
        assert_eq!(g2.current(), Generation(1));
        g2.bump();
        assert_eq!(g.current(), Generation(2));
    }

    #[test]
    fn parallel_bump_is_monotonic_total_count() {
        use std::thread;
        let g = Arc::new(GenCounter::new());
        let mut handles = Vec::new();
        for _ in 0..8 {
            let g = Arc::clone(&g);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    g.bump();
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(g.current(), Generation(8 * 1000));
    }

    #[test]
    fn display_form() {
        assert_eq!(format!("{}", Generation(42)), "g42");
    }
}
