//! Structured tracing (`tracing` crate). Off by default; enable with
//! `RUST_LOG=<level>` or `DL_TRACE=<level>` (e.g. `DL_TRACE=debug dl …`).
//!
//! Span CLOSE events carry durations, so the tick phases and reactivity
//! decisions surface as timed lines without recompiling or scattering
//! `eprintln!`s. Keep span levels graded: `info` for whole ticks, `debug` for
//! phases (reconcile/refresh/rebuild/gens/pulls), `trace` for per-file work.
//!
//! Tests never call `init`, so they stay silent (no subscriber => no overhead
//! beyond a relaxed-atomic load per span).

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init() {
    // Only claim the global subscriber when a filter is explicitly requested via
    // RUST_LOG or DL_TRACE. With neither set the effective level is `off` (nothing
    // would print anyway), so skip the install and leave the global slot unclaimed:
    // the daemon installs its OWN stderr `fmt` subscriber at the top of `run_daemon`
    // and must win that `try_init` race, which it can only do if this CLI-entry init
    // has not already taken the slot.
    if std::env::var_os("RUST_LOG").is_none() && std::env::var_os("DL_TRACE").is_none() {
        return;
    }
    // DL_TRACE seeds the filter when RUST_LOG is unset, so the project keeps its
    // own knob without squatting on RUST_LOG for deps.
    let filter = EnvFilter::try_from_env("RUST_LOG")
        .or_else(|_| EnvFilter::try_new(&std::env::var("DL_TRACE").unwrap_or_else(|_| "off".into())))
        .unwrap_or_else(|_| EnvFilter::new("off"));
    let _ = tracing_subscriber::registry()
        .with(fmt::layer()
            .with_target(false)
            .with_span_events(fmt::format::FmtSpan::CLOSE)
            .compact())
        .with(filter)
        .try_init();
}
