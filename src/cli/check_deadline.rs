//! Bounded `--check` runner. A deadline is advisory: on expiry the process
//! reports an explicitly partial result and exits successfully so hooks do not
//! turn a slow repository into an opaque write-blocker.

use anyhow::Result;
use std::sync::mpsc;
use std::time::Duration;

/// Run `work` until `secs` elapse. `Some` is a completed check result; `None`
/// means the caller must keep the exit status at zero after the loud warning.
pub(crate) fn run<T: Send + 'static>(secs: u64, work: impl FnOnce() -> Result<T> + Send + 'static)
    -> Result<Option<T>>
{
    if secs == 0 {
        timed_out(secs);
        return Ok(None);
    }
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || { let _ = tx.send(work()); });
    match rx.recv_timeout(Duration::from_secs(secs)) {
        Ok(result) => result.map(Some),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            timed_out(secs);
            Ok(None)
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            anyhow::bail!("check worker stopped before reporting a result")
        }
    }
}

fn timed_out(secs: u64) {
    eprintln!("[CHECK TIMED OUT] partial report only — --max-wall {secs}s elapsed; results may be incomplete");
    eprintln!("(check):1: warning[check-timed-out]: check exceeded its {secs}s wall deadline; partial report only, exiting 0");
}
