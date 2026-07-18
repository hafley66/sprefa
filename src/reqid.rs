//! Per-request id: the otel "trace id" concept without an otel dependency.
//! Every inbound RPC on either daemon transport (`daemon_shell::uds`,
//! `daemon_shell::http`) mints one via [`next`], logs an access-log line
//! tagged with it, then holds it in a thread-local for the duration of
//! `daemon::handle_request` via [`scope`]. Anything running SYNCHRONOUSLY on
//! that same thread while the scope guard is alive — an inline tick from
//! `run_eval` (the `eval`/`load mode=once` RPC path), a `JobRow` built and
//! enqueued from inside dispatch — can read it back with [`current`] and tag
//! its own log line / row with it, closing the gap CLAUDE.md's ledger calls
//! out ("root attribution residual... job-context plumbing is the true
//! fix").
//!
//! What this does NOT do: cross an `await` onto a different OS thread (tokio
//! does not propagate thread-locals across `spawn`/`spawn_blocking`
//! boundaries — this crate's engine dispatch is one synchronous call inside a
//! single `spawn_blocking` closure, so the scope guard is entered inside that
//! closure, not before it), and it does not survive a job's coalesced rerun
//! (a `dirty` reopen runs later, on a jobq worker thread, outside any live
//! request's scope — the `req_id` stored on the row is a snapshot of who last
//! touched it, not a durable causal link across reruns).
//!
//! Shape: a counter + pid, not a UUID — short, unique enough within one
//! process's lifetime for grepping a log file, no new dependency (the
//! project already declines to add otel; see the plan doc for why a real
//! trace-id library was not evaluated for this narrow a need).

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static CURRENT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Mint a new request id: `<pid-hex>-<counter-hex>`. Monotonic within this
/// process; combined with pid, unique enough across the whole machine's `dl`
/// processes for the purpose of grepping a shared log file for one request's
/// lines.
pub fn next() -> String {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}", std::process::id(), n)
}

/// The request id active on THIS thread, if a [`scope`] guard is currently
/// held here. `None` on a watcher/poll/sampler thread that never entered one
/// — correctly so, since that work was not caused by any client request.
pub fn current() -> Option<String> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Enter a request's scope on this thread for the guard's lifetime; restores
/// whatever was active before on drop (so a nested scope — none exist today,
/// but the guard is nesting-safe) unwinds cleanly.
pub fn scope(id: &str) -> ScopeGuard {
    let previous = CURRENT.with(|c| c.replace(Some(id.to_string())));
    ScopeGuard { previous }
}

pub struct ScopeGuard {
    previous: Option<String>,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        CURRENT.with(|c| *c.borrow_mut() = self.previous.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_sets_and_restores_current() {
        assert_eq!(current(), None);
        {
            let _g = scope("abc-1");
            assert_eq!(current(), Some("abc-1".to_string()));
            {
                let _g2 = scope("abc-2");
                assert_eq!(current(), Some("abc-2".to_string()));
            }
            assert_eq!(current(), Some("abc-1".to_string()), "restores outer scope on inner drop");
        }
        assert_eq!(current(), None, "restores None after outer drop");
    }

    #[test]
    fn next_is_unique_and_carries_pid() {
        let a = next();
        let b = next();
        assert_ne!(a, b);
        let pid_hex = format!("{:x}", std::process::id());
        assert!(a.starts_with(&pid_hex), "id should be prefixed by pid hex: {a}");
    }
}
