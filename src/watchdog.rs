//! Wall-clock watchdog for one-shot in-process `dl` runs. Standing law: nothing
//! seizes the machine, and the system answers "what was it doing" itself. Arm it
//! at the top of a one-shot entry (`run_file_inproc`, `warm_engine`,
//! `run_settle`/`run_changed`/`run_verify`); a detached thread sleeps
//! `DL_MAX_WALL_SECS` (default 300, `0` disables) and, if the run has not
//! finished, prints the current activity phase/detail/root from the
//! process-global slot (`crate::activity`) and hard-exits 124. SQLite is
//! WAL-crash-safe, so a hard exit here is acceptable — the next run recovers.
//!
//! Long-lived modes (`--watch`, `--lsp`, `--mcp`, `--hook`, any daemon path)
//! are deliberately NOT armed; they are meant to run unbounded.

use std::time::Duration;

/// Default wall budget for a one-shot run, in seconds.
const DEFAULT_MAX_WALL_SECS: u64 = 300;

/// The wall budget in seconds: `DL_MAX_WALL_SECS` (default 300, `0` = disabled).
/// A malformed value falls back to the default rather than disabling the guard.
pub fn max_wall_secs() -> u64 {
    match std::env::var("DL_MAX_WALL_SECS") {
        Ok(s) => s.trim().parse().unwrap_or(DEFAULT_MAX_WALL_SECS),
        Err(_) => DEFAULT_MAX_WALL_SECS,
    }
}

/// Arm a one-shot wall-clock watchdog under `label` (the entry point's name).
/// Spawns ONE detached thread that hard-exits the process with code 124 after
/// the deadline, naming what the engine was doing so the user answers "what was
/// it stuck on" without a second command. No-op when the budget is 0.
pub fn arm_wall_watchdog(label: &str) {
    let secs = max_wall_secs();
    if secs == 0 {
        return;
    }
    let label = label.to_string();
    let _ = std::thread::Builder::new()
        .name("dl-watchdog".into())
        .spawn(move || {
            std::thread::sleep(Duration::from_secs(secs));
            let a = crate::activity::snapshot();
            eprintln!("[WALL TIMEOUT] {secs}s elapsed in `{label}` — hard-exiting (124)"); // @eprintln-ok: final user-facing timeout report
            eprintln!(
                "  phase={} detail={} root={}",
                a.phase.as_str(),
                if a.detail.is_empty() { "-" } else { &a.detail },
                if a.root.is_empty() { "-" } else { &a.root },
            ); // @eprintln-ok: final user-facing timeout report
            eprintln!("  tick={} phase-elapsed={}ms", a.tick, a.elapsed_ms); // @eprintln-ok: final user-facing timeout report
            eprintln!("  raise with DL_MAX_WALL_SECS=<n> or 0 to disable"); // @eprintln-ok: final user-facing timeout report
            std::process::exit(124);
        });
}

/// Run `work` on a named worker thread and wait at most `deadline` for its
/// result — the thread+channel deadline shape shared by `--check --max-wall`
/// (`cli::check_deadline`) and the `--hook` self-timeout. `Ok(Some(_))` is the
/// finished result; `Ok(None)` means the deadline expired and the worker was
/// abandoned (it dies with the process — the caller is expected to exit soon,
/// and SQLite is WAL-crash-safe); `Err` means the worker stopped without
/// reporting (a panic). Give-up reporting (stderr/tracing/exit code) is the
/// caller's job: each surface has its own contract.
pub fn run_with_deadline<T: Send + 'static>(
    deadline: Duration,
    thread_name: &str,
    work: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> anyhow::Result<Option<T>> {
    use std::sync::mpsc;
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name(thread_name.to_string())
        .spawn(move || {
            let _ = tx.send(work());
        })?;
    tracing::debug!(
        deadline_ms = deadline.as_millis() as u64,
        thread = thread_name,
        "deadline wait begin"
    );
    match rx.recv_timeout(deadline) {
        Ok(result) => result.map(Some),
        Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            anyhow::bail!("deadline worker `{thread_name}` stopped before reporting a result")
        }
    }
}

/// Test-only seam: block the calling (work) thread long enough for the armed
/// watchdog thread to fire, stamping an activity phase so the timeout message
/// has a real phase to name. Does nothing unless `DL_TEST_HANG_SECS` is set, so
/// it is inert in every real run. Placed on the armed one-shot paths so the
/// watchdog can be exercised without a genuinely slow repository.
#[doc(hidden)]
pub fn test_hang_hook() {
    if let Ok(s) = std::env::var("DL_TEST_HANG_SECS") {
        if let Ok(n) = s.trim().parse::<u64>() {
            crate::activity::set(crate::activity::Phase::ParseExtract, "test-hang");
            std::thread::sleep(Duration::from_secs(n));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The one property that matters and is testable in-process without exiting:
    // the env parse. The 124 hard-exit + message shape is exercised end-to-end
    // by the subprocess it-test (tests/it/wall_watchdog.rs), which is the only
    // place `std::process::exit` is safe to observe.
    #[test]
    fn budget_env_parse() {
        std::env::remove_var("DL_MAX_WALL_SECS");
        assert_eq!(max_wall_secs(), DEFAULT_MAX_WALL_SECS);
        std::env::set_var("DL_MAX_WALL_SECS", "0");
        assert_eq!(max_wall_secs(), 0, "0 disables");
        std::env::set_var("DL_MAX_WALL_SECS", "7");
        assert_eq!(max_wall_secs(), 7);
        std::env::set_var("DL_MAX_WALL_SECS", "garbage");
        assert_eq!(max_wall_secs(), DEFAULT_MAX_WALL_SECS, "malformed falls back to default");
        std::env::remove_var("DL_MAX_WALL_SECS");
    }
}
