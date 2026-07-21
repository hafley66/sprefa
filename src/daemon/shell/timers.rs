//! The poll (`@async` clock) + idle (self-reap) timer tasks. Both are
//! `tokio::time::interval` tasks that select on the cancellation token; the poll
//! scan's per-root probes take the prog/eng locks, so the whole scan runs on the
//! blocking pool.

use std::sync::Arc;
use std::time::Duration;

use super::ShellCtx;
use crate::daemon::{lock, shutdown_cleanup, Daemon, IDLE_TICK_SECS};

/// Exit when EVERY served root has been idle past the threshold (keep engines
/// warm while any is active). Only reads `last_activity` (a quick mutex, no
/// engine lock) so it runs inline on a shell worker.
pub(crate) async fn idle_task(d: Arc<Daemon>, ctx: ShellCtx, idle_secs: u64) {
    let mut ticker = tokio::time::interval(Duration::from_secs(IDLE_TICK_SECS));
    loop {
        tokio::select! {
            _ = ctx.cancel.cancelled() => return,
            _ = ticker.tick() => {}
        }
        // Job-queue maintenance backstop: the shell reconciler runs this on
        // its own doorbell cadence; the idle tick repeats it so retention and
        // cold promotion survive even a wedged reconciler task. Abandoned
        // leases are apalis's heartbeat `reenqueue_orphaned` now. A quick
        // own-db write, no engine lock — fine inline on the shell worker.
        if let Err(e) = d.jobs.reconcile() {
            tracing::warn!("[daemon] job reconcile: {e}");
        }
        // Class-28 rail: re-check the externally-redirected log files every
        // idle tick (30s), so a long-lived run stays bounded, not just the
        // one-time sweep `run_daemon` does at boot. Three `stat` calls, no
        // lock — see `daemon::logcap`'s module doc for the mechanism.
        crate::daemon::logcap::sweep(&d.home);
        let roots = d.all_roots();
        let all_idle = roots.iter().all(|sr| {
            lock(&sr.last_activity).elapsed() > Duration::from_secs(idle_secs)
        });
        if all_idle {
            tracing::info!("[daemon] all roots idle {}min, exiting", idle_secs / 60);
            shutdown_cleanup(&d);
            std::process::exit(0);
        }
    }
}

/// The poll task no longer DRAINS inline; it ENQUEUES a `sink:{root}` job for
/// each effect-bearing root that has drainable work, and a dispatcher task runs
/// `poll_tick`. The idle-gate (`has_effects` + `poll_idle`) is the enqueue
/// CONDITION — unchanged. Failure backoff is NOT re-implemented here: the queue
/// owns it. The whole per-cycle scan runs on the blocking pool (`poll_scan`
/// takes the prog/eng locks); the async task only paces it on a `tokio::time`
/// interval and stops on cancellation.
pub(crate) async fn poll_task(d: Arc<Daemon>, ctx: ShellCtx, secs: u64) {
    tracing::info!("[daemon] poll loop every {secs}s (@async drain via job queue)");
    let mut ticker = tokio::time::interval(Duration::from_secs(secs));
    // Consume the immediate first tick so the first scan waits a full `secs`,
    // matching the old `sleep(secs)`-first loop.
    ticker.tick().await;
    loop {
        tokio::select! {
            _ = ctx.cancel.cancelled() => return,
            _ = ticker.tick() => {}
        }
        let d2 = d.clone();
        let _ = tokio::task::spawn_blocking(move || poll_scan(&d2)).await;
    }
}

/// One poll-cycle scan (blocking): evict vanished roots, then for each
/// effect-bearing root that passes the idle-gate, enqueue a `sink:{root}` drain
/// job (which also rings the tokio doorbell via `ServedRoot::enqueue_job`).
fn poll_scan(d: &Arc<Daemon>) {
    for sr in d.all_roots() {
        // Part 4 (stale-root eviction): a registered root whose directory
        // vanished out from under the daemon is deregistered here instead of
        // being served — and error-looped — forever. The config view
        // (`key == None`) has no directory of its own to vanish.
        if sr.key.is_some() && !sr.root.exists() {
            tracing::warn!("[daemon] root {} no longer exists; deregistering", sr.root.display());
            let _ = d.drop_root(&sr.root, false);
            continue;
        }
        // An effect-free root still owes ONE confirming full tick before
        // `settled` can flip true (`poll_idle`: quiescence is only confirmed
        // by a tick that sees nothing move). Skipping on `has_effects` alone
        // froze such roots at settled=false forever and `await-settle` hung
        // (incident: smashy root, 2026-07-18 redeploy receipt run).
        if !sr.has_effects() && sr.settled.load(std::sync::atomic::Ordering::Relaxed) {
            continue;
        }
        // Idle-gate (the CPU-hog fix, preserved): nothing queued, settled, no
        // source motion since the last full tick, no `every`/`clock` cadence ->
        // nothing to drain, so do not enqueue.
        match sr.poll_idle() {
            Ok(true) => continue,
            Ok(false) => {}
            Err(e) => { tracing::warn!("[{}] poll idle probe: {e}", sr.root_label()); continue; }
        }
        if let Err(e) = sr.enqueue_job(crate::jobq::JobRow::sink_drain(&sr.job_root_id())) {
            tracing::warn!("[{}] enqueue sink drain: {e}", sr.root_label());
        }
    }
}
