//! Shared serialization lock for tests that inject wall-clock time via the
//! process-global `DL_NOW_SECS` env var (read by `now_secs`, reached only when a
//! program uses `every` or `clock`). Any test that sets `DL_NOW_SECS` must hold
//! this lock for its duration so the injected time of one test never leaks into
//! another running in parallel within the single `it` binary.

use std::sync::Mutex;

pub static CLOCK_LOCK: Mutex<()> = Mutex::new(());

/// Set the injected wall-clock seconds. Hold `CLOCK_LOCK` across the whole test.
pub fn set_now(secs: i64) {
    std::env::set_var("DL_NOW_SECS", secs.to_string());
}

/// Restore real time. Call at the end of a clock test.
pub fn clear_now() {
    std::env::remove_var("DL_NOW_SECS");
}
