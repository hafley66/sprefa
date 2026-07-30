//! Background scheduling budget (the nothing-seizes-the-machine law:
//! QoS/nice/IOPOL + thread caps in `apply_daemon_budget`) plus the
//! idle/mem/exe-stamp/poll environment knobs (relocated from `daemon.rs`;
//! decomposition plan step 6).
use super::*;

// ---------- background scheduling budget ----------

/// Positive nice value for the daemon process on unix. Higher values mean lower
/// CPU priority, keeping the daemon from competing with the user's foreground.
const DAEMON_NICE: libc::c_int = 10;

/// macOS QoS class for utility/background work. Values come from `<sys/qos.h>`;
/// UTILITY is `0x11`.
#[cfg(target_os = "macos")]
const QOS_CLASS_UTILITY: u32 = 0x11;

/// Compute the rayon thread-count budget for the daemon. Never claim more than a
/// quarter of the machine's cores, but keep at least 2 threads so small laptops
/// don't collapse to serial operation. `DL_DAEMON_THREADS` overrides the
/// heuristic.
pub(crate) fn daemon_thread_count(cores: usize, env: Option<&str>) -> usize {
    env.and_then(|s| s.parse::<usize>().ok())
        .filter(|&n| n > 0)
        .unwrap_or_else(|| std::cmp::max(2, cores / 4))
}

/// Apply the daemon's background scheduling budget once, at the top of the serve
/// path. macOS threads inherit the QoS class of the spawning thread, so setting
/// this on the main thread before spawning any workers keeps the whole daemon in
/// the utility tier where the platform supports it. On all unix systems the
/// process also gets a positive nice value as a portable fallback. The rayon
/// global pool is bounded here; if it was already initialized earlier in this
/// process (e.g. by the CLI dispatch path) the call is a no-op and we report the
/// current pool size.
/// Apply the CPU + disk-I/O scheduling budget to THIS process, minus the rayon
/// thread cap. macOS QoS UTILITY on the calling thread (spawned workers inherit
/// it), a positive nice value on every unix, and a process-scope IOPOL_THROTTLE
/// on macOS so bulk db writes land in the background disk tier. Split out of
/// `apply_daemon_budget` so the one-shot CLI entry can apply the same cap
/// (standing law: nothing seizes the machine). Every syscall here is idempotent,
/// so applying it twice — CLI entry then the daemon serve path — is harmless.
pub(crate) fn apply_process_budget() {
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn pthread_set_qos_class_self_np(class: u32, priority: libc::c_int) -> libc::c_int;
        }
        // SAFETY: libc call with a constant, valid QoS class; ignore errors.
        unsafe { pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0) };
    }

    #[cfg(unix)]
    {
        // SAFETY: setpriority on this process is always valid; ignore EPERM.
        unsafe { libc::setpriority(libc::PRIO_PROCESS, 0 as libc::id_t, DAEMON_NICE) };
    }

    #[cfg(target_os = "macos")]
    {
        // Demote every disk read/write this process issues to the background
        // I/O tier (Time Machine's). QoS/nice cap CPU only; the first-run
        // rebuild's bulk db writes are what beachball the machine, and only
        // this throttle reaches them. IOPOL_TYPE_DISK=0, IOPOL_SCOPE_PROCESS=0,
        // IOPOL_THROTTLE=3 per <sys/resource.h>.
        extern "C" {
            fn setiopolicy_np(
                iotype: libc::c_int,
                scope: libc::c_int,
                policy: libc::c_int,
            ) -> libc::c_int;
        }
        // SAFETY: constant, documented arguments; scope is this process.
        unsafe { setiopolicy_np(0, 0, 3) };
    }
}

pub(crate) fn apply_daemon_budget() -> (&'static str, i32, usize) {
    // Supervision arc (plans/2026-07-18-infra-library-adoption.md section 3;
    // standing law a1f049ff "infra is bought, never built"): `launchd` injects
    // `XPC_SERVICE_NAME` into every process it spawns (confirmed via
    // `launchctl print` during the 2026-07-18 gate-2 probe). Its presence
    // means the LaunchAgent's `Nice`/`LowPriorityIO` plist keys
    // (`crate::supervise::plist_contents`) already govern this process's
    // CPU/disk scheduling class, so re-applying `nice()`/`setiopolicy_np()`/
    // the QoS class here — and the extra PRIO_DARWIN_BG tier below — would be
    // redundant self-throttling stacked on top of what supervision now owns.
    // Gate 2 (plan section 5.2 q2) measured the PRIO_DARWIN_BG-equivalent
    // plist key (`ProcessType=Background`) at ~45% of baseline CPU throughput
    // (two 5s fixed-duration spinner trials); the plist deliberately does NOT
    // set that key, so re-imposing the same clamp in-process here would
    // silently reinstate exactly what that gate decided against.
    //
    // Absent `XPC_SERVICE_NAME` — `dl daemon serve` launched via the bespoke
    // `spawn_detached` fallback (plan section 3.4: no service manager
    // installed, CI, `cargo test`) — nothing else applies these caps, so the
    // full nice/IOPOL/QoS/PRIO_DARWIN_BG stack stays exactly as it was before
    // this arc: the 2026-07-17 incident that motivated PRIO_DARWIN_BG ran
    // under this same un-supervised shape.
    //
    // Presence alone is NOT proof of launchd management: Terminal/iTerm app
    // contexts leave `XPC_SERVICE_NAME=0` (or their bundle id) in every shell
    // child, so a fallback-spawned daemon would skip the whole budget with no
    // plist covering it — the 2026-07-18 evening unbudgeted cold rebuild
    // (pid 4424, NI=0, XPC_SERVICE_NAME=0, failure-modes class 19/20 receipt).
    // Only our own label, which launchd sets when it spawns the LaunchAgent,
    // counts.
    let launchd_managed = std::env::var_os("XPC_SERVICE_NAME").is_some_and(|name| {
        name.to_string_lossy() == crate::supervise::service_label().to_string()
    });
    if !launchd_managed {
        apply_process_budget();

        #[cfg(target_os = "macos")]
        {
            // The daemon additionally drops into the darwin BACKGROUND resource
            // tier — the OS switch that keeps a background process from competing
            // with the user at all (CPU to leftover cycles, disk to the throttled
            // tier with deeper backoff, network deprioritized). QoS UTILITY +
            // nice + IOPOL_THROTTLE demonstrably did NOT prevent a 4.2GB/min boot
            // rebuild from freezing the foreground (2026-07-17 receipts); this
            // tier is what Time Machine and Spotlight reindex actually run under.
            // Daemon-only: one-shot CLI runs keep UTILITY so an interactive
            // `dl` answer is not starved behind foreground load.
            // PRIO_DARWIN_PROCESS=4, PRIO_DARWIN_BG=0x1000 per <sys/resource.h>.
            if std::env::var("DL_NO_BUDGET").ok().as_deref() != Some("1") {
                const PRIO_DARWIN_PROCESS: libc::c_int = 4;
                const PRIO_DARWIN_BG: libc::c_int = 0x1000;
                // SAFETY: setpriority on this process with documented darwin
                // constants; ignore EPERM.
                unsafe { libc::setpriority(PRIO_DARWIN_PROCESS, 0 as libc::id_t, PRIO_DARWIN_BG) };
            }
        }
    }

    // The tiers above are scheduling advice; the governor is the ceiling
    // (receipt 2026-07-18: 278% cpu through a full re-extract with every tier
    // applied). Daemon default 100% of one core; DL_MAX_CPU_PCT overrides,
    // 0 disables. Stays bespoke regardless of supervision (plan section 3.5
    // point 3): no OS mechanism gives a CPU-percent cap on macOS.
    if std::env::var("DL_NO_BUDGET").ok().as_deref() != Some("1") {
        crate::budget::start_governor(crate::budget::ceiling_from_env(100));
    }

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    let desired = daemon_thread_count(cores, std::env::var("DL_DAEMON_THREADS").ok().as_deref());

    // Bound the global rayon pool. This may already have been configured by the
    // CLI entry path; build_global returns an error in that case rather than
    // panicking, so the daemon stays safe to run both foreground and detached.
    // Thread caps stay bespoke regardless of supervision (plan section 3.5
    // point 3): rayon/tokio pool sizing has no OS-level equivalent.
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(desired)
        .build_global();
    let threads = rayon::current_num_threads();

    let qos_label = if launchd_managed {
        "launchd(nice+lowprioio)"
    } else if cfg!(target_os = "macos") {
        "utility+iothrottle"
    } else {
        "none"
    };
    let priority = if launchd_managed {
        0
    } else if cfg!(unix) {
        DAEMON_NICE
    } else {
        0
    };
    (qos_label, priority, threads)
}

pub(crate) fn idle_timeout_secs() -> u64 {
    std::env::var("DL_DAEMON_IDLE_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_IDLE_SECS)
}

/// Hard RSS ceiling for the daemon process, in MB. Over it, the serve loop
/// exits (a runaway extract loop once grew the served image past a gigabyte and
/// pinned the machine). `DL_DAEMON_MEM_MB=0` disables the guard. The default is
/// generous: a multi-repo daemon legitimately holds a few hundred MB, so the
/// ceiling catches a genuine leak/storm, not steady-state serving.
pub(crate) const DEFAULT_MEM_LIMIT_MB: u64 = 4096;
/// Exit code the memory guard uses, distinct from a clean shutdown so a
/// supervisor can tell a self-kill from an orderly stop.
const MEM_GUARD_EXIT_CODE: i32 = 137;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExeStamp {
    pub(crate) len: u64,
    pub(crate) mtime: SystemTime,
}

/// Return the current executable's stat identity. A missing stat is unknown,
/// not evidence that the daemon's binary was replaced.
pub(crate) fn current_exe_stamp() -> Option<ExeStamp> {
    if std::env::var_os("DL_EXE_STAMP").is_some() {
        return None;
    }
    let path = std::env::current_exe().ok()?;
    let metadata = std::fs::metadata(path).ok()?;
    Some(ExeStamp {
        len: metadata.len(),
        mtime: metadata.modified().ok()?,
    })
}

pub(crate) fn should_exit_for_binary_change(
    launch: Option<ExeStamp>,
    current: Option<ExeStamp>,
) -> bool {
    matches!((launch, current), (Some(before), Some(after)) if before != after)
}

pub(crate) fn enforce_fresh_binary(launch_exe_stamp: Option<ExeStamp>) {
    if launch_exe_stamp.is_none() {
        return;
    }
    if should_exit_for_binary_change(launch_exe_stamp, current_exe_stamp()) {
        tracing::info!(
            "[daemon] binary replaced on disk, exiting so the next call runs the new version"
        );
        std::process::exit(0);
    }
}

fn mem_limit_mb() -> Option<u64> {
    match std::env::var("DL_DAEMON_MEM_MB") {
        Ok(s) => s.parse::<u64>().ok().filter(|&n| n > 0),
        Err(_) => Some(DEFAULT_MEM_LIMIT_MB),
    }
}

/// Current resident set size of this process in MB, via `ps` (portable across
/// macOS and Linux, no extra dependency; `ps` reports RSS in KB). `None` when
/// the read fails, so the guard simply skips that check rather than guessing.
pub(crate) fn self_rss_mb() -> Option<u64> {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p"])
        .arg(std::process::id().to_string())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<u64>()
        .ok()
        .map(|kb| kb / 1024)
}

/// Exit the process if RSS has crossed the ceiling. Called on the serve loop's
/// 1s heartbeat and after each tick, so both an idle leak and a busy-tick storm
/// trip it. Logs the numbers before exiting so the cause is in the daemon log.
pub(crate) fn enforce_mem_limit(label: &str) {
    let Some(limit) = mem_limit_mb() else { return };
    let Some(rss) = self_rss_mb() else { return };
    if rss > limit {
        tracing::warn!(
            "[{label}] RSS {rss}MB exceeded limit {limit}MB — exiting (memory guard; \
                   set DL_DAEMON_MEM_MB to adjust, 0 to disable)"
        );
        std::process::exit(MEM_GUARD_EXIT_CODE);
    }
}

// ---------- poll task (the @async clock source, all roots) ----------

pub(crate) fn poll_interval_secs() -> Option<u64> {
    match std::env::var("DL_POLL_SECS") {
        Ok(s) => s.parse::<u64>().ok().filter(|&n| n > 0),
        Err(_) => Some(DEFAULT_POLL_SECS),
    }
}
