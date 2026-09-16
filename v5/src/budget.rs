//! Hard CPU ceiling for background work: a duty-cycle governor.
//!
//! The OS background tiers (QoS UTILITY, nice 10, darwin BG, IOPOL_THROTTLE —
//! all applied in `daemon::apply_process_budget` / `apply_daemon_budget`) are
//! SCHEDULING advice: they yield to foreground load but let a background
//! process run hot whenever cores look idle. Receipt 2026-07-18: the daemon
//! held 278% CPU through a full source re-extract with every tier applied, on
//! a machine already 7GB into swap. Advice is not a ceiling.
//!
//! This module is the ceiling. A governor thread samples this process's own
//! cumulative CPU (via `why::read_self_usage`, the same counters `dl daemon
//! why` reports) every `INTERVAL_MS`, and when the last window ran over the
//! configured percentage it computes how long the workers must idle to
//! amortize the debt and parks them: hot loops call `throttle_point()` at
//! their natural work boundaries (per source-extract job, per family file
//! batch, per jobq claim) and sleep there until the debt clears.
//!
//! Build-vs-buy: this is the in-process form of what `cpulimit` does from the
//! outside (SIGSTOP/SIGCONT duty cycling); cpulimit itself would add an
//! external binary dependency and stop threads mid-SQLite-write (lock hold
//! across the stop — a class-8 hazard). The `governor` crate rate-limits
//! request admission, not CPU time; cgroups do not exist on macOS. ~80 lines
//! against `why`'s existing counters is the whole cost.
//!
//! Env: `DL_MAX_CPU_PCT` — percentage of ONE core (100 = one full core,
//! 200 = two). 0 or unset = no governor. The daemon defaults to 150 unless
//! the env overrides; one-shot CLI runs default to off (interactive latency)
//! and opt in via the same env. `DL_NO_BUDGET=1` disables the governor along
//! with the rest of the budget.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Epoch-ms deadline the workers must sleep until. 0 = gate open.
static PAUSE_UNTIL_MS: AtomicU64 = AtomicU64::new(0);
/// Cheap fast-path check so `throttle_point` costs one relaxed load when no
/// governor runs (the common one-shot case).
static GOVERNOR_ON: AtomicBool = AtomicBool::new(false);

const INTERVAL_MS: u64 = 250;
/// Never park the workers longer than this per window; keeps the daemon
/// responsive to shutdown and bounds the damage of a counter glitch.
const MAX_PAUSE_MS: u64 = 2_000;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// The ceiling from the environment, with a per-caller default.
/// `DL_MAX_CPU_PCT=0` explicitly disables.
pub fn ceiling_from_env(default_pct: u32) -> u32 {
    match std::env::var("DL_MAX_CPU_PCT") {
        Ok(v) => v.trim().parse::<u32>().unwrap_or(default_pct),
        Err(_) => default_pct,
    }
}

/// How long the workers must idle so that `used_ms` of CPU over `wall_ms` of
/// wall time amortizes down to `max_pct`. Pure, unit-tested.
fn pause_for(used_ms: u64, wall_ms: u64, max_pct: u32) -> u64 {
    let allowed = wall_ms.saturating_mul(max_pct as u64) / 100;
    if used_ms <= allowed || max_pct == 0 {
        return 0;
    }
    // Idle long enough that (used) / (wall + pause) == max_pct.
    let pause = used_ms.saturating_mul(100) / max_pct as u64 - wall_ms;
    pause.min(MAX_PAUSE_MS)
}

/// Park here until the governor's debt clears. Call at natural work
/// boundaries only (between files, between jobs) — never while holding the
/// engine lock or an open transaction (the cpulimit hazard this in-process
/// design exists to avoid).
pub fn throttle_point() {
    if !GOVERNOR_ON.load(Ordering::Relaxed) {
        return;
    }
    loop {
        let until = PAUSE_UNTIL_MS.load(Ordering::Relaxed);
        let now = now_ms();
        if now >= until {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis((until - now).min(200)));
    }
}

/// Spawn the governor thread. Idempotent per process (second call is a no-op);
/// `max_pct == 0` means "no ceiling" and spawns nothing.
pub fn start_governor(max_pct: u32) {
    if max_pct == 0 || GOVERNOR_ON.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("dl-governor".into())
        .spawn(move || {
            let mut last_cpu = crate::why::read_self_usage().cpu_ms;
            let mut last = std::time::Instant::now();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(INTERVAL_MS));
                let cpu = crate::why::read_self_usage().cpu_ms;
                let wall = last.elapsed().as_millis() as u64;
                let used = cpu.saturating_sub(last_cpu);
                let pause = pause_for(used, wall, max_pct);
                if pause > 0 {
                    let until = now_ms() + pause;
                    PAUSE_UNTIL_MS.fetch_max(until, Ordering::Relaxed);
                }
                last_cpu = cpu;
                last = std::time::Instant::now();
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pause_math_holds_the_ceiling() {
        // Under the ceiling: no pause.
        assert_eq!(pause_for(200, 250, 100), 0);
        assert_eq!(pause_for(0, 250, 100), 0);
        // 250ms window burned 625ms cpu (2.5 cores) at a 100% ceiling:
        // needs 625/1.0 - 250 = 375ms of idle.
        assert_eq!(pause_for(625, 250, 100), 375);
        // Same burn at 150%: 625/1.5 - 250 = 166ms.
        assert_eq!(pause_for(625, 250, 150), 166);
        // Ceiling 0 = disabled, never pauses.
        assert_eq!(pause_for(10_000, 250, 0), 0);
        // Pause is clamped.
        assert_eq!(pause_for(1_000_000, 250, 1), MAX_PAUSE_MS);
    }

    #[test]
    fn throttle_point_is_free_when_off() {
        // GOVERNOR_ON is false in a fresh test process unless start_governor
        // ran; throttle_point must return immediately.
        let t = std::time::Instant::now();
        throttle_point();
        assert!(t.elapsed().as_millis() < 50);
    }
}
