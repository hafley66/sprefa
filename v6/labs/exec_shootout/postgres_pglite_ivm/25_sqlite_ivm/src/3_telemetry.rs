use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Instant;
use tracing::Dispatch;

// A connection owns this dispatch. No global subscriber or SQLite callback.
// JSON goes directly to stderr; tracing's writer reports I/O failures to stderr
// without returning an SQL error. No event buffer or row/SQL payload is retained.
pub struct Telemetry {
    pub remaining: AtomicU64,
    pub sequence: AtomicU64,
    pub dispatch: Dispatch,
}
impl Telemetry {
    pub fn new() -> Arc<Self> {
        let limit = std::env::var("SQLITE_IVM_LOG_LIMIT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0)
            .min(10000);
        Arc::new(Self {
            remaining: AtomicU64::new(limit),
            sequence: AtomicU64::new(0),
            dispatch: Dispatch::new(
                tracing_subscriber::fmt()
                    .json()
                    .with_ansi(false)
                    .with_writer(std::io::stderr)
                    .finish(),
            ),
        })
    }
    pub fn event(&self, boundary: &str, operation: u64, view: &str, started: Instant, code: i32) {
        if self
            .remaining
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_sub(1))
            .is_err()
        {
            return;
        }
        tracing::dispatcher::with_default(&self.dispatch, || {
            tracing::info!(target:"sqlite_ivm", boundary,operation,view,extended_code=code,duration_us=started.elapsed().as_micros() as u64,algorithm="signed_join_count_sum",build=env!("CARGO_PKG_VERSION"),"boundary");
        });
    }
    pub fn next(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }
}
