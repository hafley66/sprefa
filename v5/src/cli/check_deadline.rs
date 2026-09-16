//! Bounded `--check` runner. A deadline is advisory: on expiry the process
//! reports an explicitly partial result and exits successfully so hooks do not
//! turn a slow repository into an opaque write-blocker.

use anyhow::Result;
use std::time::Duration;

/// Run `work` until `secs` elapse. `Some` is a completed check result; `None`
/// means the caller must keep the exit status at zero after the loud warning.
/// The thread+channel wait itself lives in [`crate::watchdog::run_with_deadline`]
/// (shared with the `--hook` self-timeout).
pub(crate) fn run<T: Send + 'static>(
    secs: u64,
    work: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<Option<T>> {
    if secs == 0 {
        timed_out(secs);
        return Ok(None);
    }
    match crate::watchdog::run_with_deadline(Duration::from_secs(secs), "dl-check-work", work)? {
        Some(result) => Ok(Some(result)),
        None => {
            timed_out(secs);
            Ok(None)
        }
    }
}

fn timed_out(secs: u64) {
    eprintln!("[CHECK TIMED OUT] partial report only — --max-wall {secs}s elapsed; results may be incomplete"); // @eprintln-ok: command's output contract for a human at a TTY
    eprintln!("(check):1: warning[check-timed-out]: check exceeded its {secs}s wall deadline; partial report only, exiting 0");
    // @eprintln-ok: command's output contract for a human at a TTY
}
