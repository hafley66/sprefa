//! Durable self-diagnosis trail: the daemon appends one JSON line every few
//! seconds to `<daemon_home>/why.jsonl` — what it is doing (tick, phase,
//! detail, running job) joined with what it costs the machine (CPU ms, disk
//! read/write bytes, resident memory). `dl daemon why` reads ONLY this file:
//! no RPC, no socket, no engine lock — so it answers while the daemon is
//! wedged mid-rebuild and after a SIGKILL, when the in-memory activity slot is
//! gone. Each append is its own open/write/close, so a hard kill loses at most
//! one sample.
//!
//! Line kinds:
//!   {"kind":"boot","ts":..,"pid":..,"build":".."}
//!   {"kind":"sample","ts":..,"pid":..,"tick":..,"phase":"..","detail":"..",
//!    "program":"..","job":"..","cpu_ms":..,"io_read_mb":..,"io_write_mb":..,
//!    "rss_mb":..}   (cpu/io are CUMULATIVE since process start)
//!   {"kind":"shutdown","ts":..,"pid":..}
//!
//! A boot line with no matching shutdown line and a dead pid = the run was
//! killed or crashed; the last sample says what it was doing and the cumulative
//! counters say what it had consumed.

use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Sample cadence while a tick is running. Idle heartbeats stretch to one
/// every `IDLE_EVERY` samples so a quiet day doesn't churn the file.
const SAMPLE_SECS: u64 = 2;
const IDLE_EVERY: u32 = 15;

/// Rotate at ~4MB: rename to `why.jsonl.1` (one generation). Keeps the report
/// path allowed to read the whole file.
const ROTATE_BYTES: u64 = 4 * 1024 * 1024;

fn trail_path(home: &Path) -> PathBuf {
    home.join("why.jsonl")
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn mb(bytes: u64) -> f64 {
    (bytes as f64 / 1e6 * 10.0).round() / 10.0
}

// ---------- OS counters ----------

/// Cumulative-since-boot resource counters for THIS process.
#[derive(Clone, Copy, Debug, Default)]
pub struct SelfUsage {
    pub cpu_ms: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub rss_bytes: u64,
}

/// CPU from `getrusage(RUSAGE_SELF)` (portable); disk I/O + memory footprint
/// from `proc_pid_rusage` on macOS (`ri_diskio_*` is the number Activity
/// Monitor's Disk tab shows) or `/proc/self/{io,statm}` on Linux.
pub fn read_self_usage() -> SelfUsage {
    let mut u = SelfUsage::default();
    #[cfg(unix)]
    {
        // SAFETY: zeroed out-param, RUSAGE_SELF is always valid.
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } == 0 {
            let ms = |tv: libc::timeval| tv.tv_sec as u64 * 1000 + tv.tv_usec as u64 / 1000;
            u.cpu_ms = ms(ru.ru_utime) + ms(ru.ru_stime);
        }
    }
    #[cfg(target_os = "macos")]
    {
        // `rusage_info_v2` prefix: 16-byte uuid then 18 u64 fields; the three
        // we need sit at fixed indices (17=diskio_read, 18=diskio_written are
        // v2's last two; 8=phys_footprint). Layout is ABI-frozen per
        // <libproc.h>; RUSAGE_INFO_V2 = 2.
        #[repr(C)]
        struct RusageInfoV2 {
            ri_uuid: [u8; 16],
            fields: [u64; 18],
        }
        extern "C" {
            fn proc_pid_rusage(
                pid: libc::c_int,
                flavor: libc::c_int,
                buffer: *mut RusageInfoV2,
            ) -> libc::c_int;
        }
        let mut info = RusageInfoV2 { ri_uuid: [0; 16], fields: [0; 18] };
        // SAFETY: out-param sized for flavor 2, own pid.
        if unsafe { proc_pid_rusage(std::process::id() as libc::c_int, 2, &mut info) } == 0 {
            u.rss_bytes = info.fields[7]; // ri_phys_footprint
            u.io_read_bytes = info.fields[16]; // ri_diskio_bytesread
            u.io_write_bytes = info.fields[17]; // ri_diskio_byteswritten
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/self/io") {
            for line in s.lines() {
                if let Some(v) = line.strip_prefix("read_bytes: ") {
                    u.io_read_bytes = v.trim().parse().unwrap_or(0);
                } else if let Some(v) = line.strip_prefix("write_bytes: ") {
                    u.io_write_bytes = v.trim().parse().unwrap_or(0);
                }
            }
        }
        if let Ok(s) = std::fs::read_to_string("/proc/self/statm") {
            let pages: u64 = s.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            u.rss_bytes = pages * 4096;
        }
    }
    u
}

// ---------- writer (daemon side) ----------

fn append_line(home: &Path, line: &Value) {
    let path = trail_path(home);
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > ROTATE_BYTES {
            let _ = std::fs::rename(&path, home.join("why.jsonl.1"));
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

/// Boot / shutdown markers. `boot` is written before the sampler starts so a
/// crash in the very first tick still leaves a bracket to diagnose against.
pub fn mark_boot(home: &Path, build: &str) {
    append_line(home, &json!({"kind":"boot","ts":now_secs(),"pid":std::process::id(),"build":build}));
}

pub fn mark_shutdown(home: &Path) {
    append_line(home, &json!({"kind":"shutdown","ts":now_secs(),"pid":std::process::id()}));
}

/// One sample line: activity slot + running job + OS counters.
fn sample_line(jobs: &crate::jobq::JobQueue) -> Value {
    let a = crate::activity::snapshot();
    let job = jobs
        .list()
        .ok()
        .and_then(|rows| rows.into_iter().find(|r| r.state == "running"))
        .map(|r| r.key)
        .unwrap_or_default();
    let u = read_self_usage();
    json!({
        "kind": "sample", "ts": now_secs(), "pid": std::process::id(),
        "tick": a.tick, "phase": a.phase.as_str(), "detail": a.detail,
        "program": a.program, "job": job,
        "cpu_ms": u.cpu_ms, "io_read_mb": mb(u.io_read_bytes),
        "io_write_mb": mb(u.io_write_bytes), "rss_mb": mb(u.rss_bytes),
    })
}

/// Spawn the `dl-why` sampler thread. Never touches the engine lock: the
/// activity slot has its own mutex and the job list rides a read-only
/// connection, so the sampler keeps writing while a tick holds the engine —
/// that wedged-mid-rebuild window is exactly when the trail matters.
pub fn start_sampler(home: PathBuf, jobs: std::sync::Arc<crate::jobq::JobQueue>) {
    let _ = std::thread::Builder::new().name("dl-why".into()).spawn(move || {
        let mut n: u32 = 0;
        loop {
            std::thread::sleep(Duration::from_secs(SAMPLE_SECS));
            let line = sample_line(&jobs);
            let idle = line["phase"] == "idle";
            n = n.wrapping_add(1);
            if !idle || n % IDLE_EVERY == 0 {
                append_line(&home, &line);
            }
        }
    });
}

// ---------- reader (`dl daemon why`) ----------

fn pid_alive(pid: u32) -> bool {
    // SAFETY: signal 0 = existence probe, no signal delivered.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

fn fmt_ago(secs: i64) -> String {
    match secs {
        s if s < 0 => "in the future(?)".into(),
        s if s < 120 => format!("{s}s ago"),
        s if s < 7200 => format!("{}m ago", s / 60),
        s => format!("{}h ago", s / 3600),
    }
}

/// Render the report from the on-disk trail alone. Works with the daemon
/// alive, hung, or dead.
pub fn report(home: &Path) -> String {
    let path = trail_path(home);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return format!(
            "no trail at {} — the daemon has not run with self-diagnosis yet\n\
             (reinstall the binary, then the next daemon run writes it)\n",
            path.display()
        );
    };
    let lines: Vec<Value> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let Some(boot_at) = lines.iter().rposition(|l| l["kind"] == "boot") else {
        return format!("{}: no boot marker found ({} lines)\n", path.display(), lines.len());
    };
    let run = &lines[boot_at..];
    let pid = run[0]["pid"].as_u64().unwrap_or(0) as u32;
    let shutdown = run.iter().any(|l| l["kind"] == "shutdown");
    let samples: Vec<&Value> = run.iter().filter(|l| l["kind"] == "sample").collect();
    let now = now_secs();

    let mut out = String::new();
    let state = if shutdown {
        "exited clean"
    } else if pid_alive(pid) {
        "RUNNING"
    } else {
        "DEAD without a shutdown marker — killed or crashed"
    };
    out += &format!("daemon pid {pid}: {state}\n");

    let Some(last) = samples.last() else {
        out += "no samples in this run (died within the first sample interval)\n";
        return out;
    };
    let last_ts = last["ts"].as_i64().unwrap_or(0);
    let job = last["job"].as_str().unwrap_or("");
    out += &format!(
        "last sample: {} | tick {} phase={} {}{}\n",
        fmt_ago(now - last_ts),
        last["tick"],
        last["phase"].as_str().unwrap_or("?"),
        last["detail"].as_str().unwrap_or(""),
        if job.is_empty() { String::new() } else { format!(" | job {job}") },
    );

    // Deltas over the trailing 60s window of THIS run (counters cumulative).
    let first = samples
        .iter()
        .find(|s| s["ts"].as_i64().unwrap_or(0) >= last_ts - 60)
        .unwrap_or(last);
    let d = |k: &str| last[k].as_f64().unwrap_or(0.0) - first[k].as_f64().unwrap_or(0.0);
    let span = (last_ts - first["ts"].as_i64().unwrap_or(last_ts)).max(1);
    out += &format!(
        "last {span}s: cpu {:.0}ms | disk read {:.1}MB write {:.1}MB | rss {:.0}MB\n",
        d("cpu_ms"),
        d("io_read_mb"),
        d("io_write_mb"),
        last["rss_mb"].as_f64().unwrap_or(0.0),
    );
    out += &format!(
        "run total: cpu {:.1}s | disk read {:.1}MB write {:.1}MB\n",
        last["cpu_ms"].as_f64().unwrap_or(0.0) / 1000.0,
        last["io_read_mb"].as_f64().unwrap_or(0.0),
        last["io_write_mb"].as_f64().unwrap_or(0.0),
    );

    // Where the window's time went, by phase (consecutive-sample attribution).
    let windowed: Vec<&&Value> = samples
        .iter()
        .filter(|s| s["ts"].as_i64().unwrap_or(0) >= last_ts - 60)
        .collect();
    let mut phase_secs: Vec<(String, i64)> = Vec::new();
    for pair in windowed.windows(2) {
        let dt = pair[1]["ts"].as_i64().unwrap_or(0) - pair[0]["ts"].as_i64().unwrap_or(0);
        let phase = pair[1]["phase"].as_str().unwrap_or("?").to_string();
        match phase_secs.iter_mut().find(|(p, _)| *p == phase) {
            Some((_, t)) => *t += dt,
            None => phase_secs.push((phase, dt)),
        }
    }
    phase_secs.sort_by_key(|(_, t)| -*t);
    if !phase_secs.is_empty() {
        let parts: Vec<String> =
            phase_secs.iter().map(|(phase, t)| format!("{phase} {t}s")).collect();
        out += &format!("phase time (window): {}\n", parts.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_names_the_phase_and_io_of_a_killed_run() {
        // The ABI-riskiest piece first: the hand-declared rusage struct must
        // yield real numbers, or every report is garbage.
        let u = read_self_usage();
        assert!(u.cpu_ms > 0, "cpu_ms never read");
        #[cfg(target_os = "macos")]
        assert!(u.rss_bytes > 1024 * 1024, "phys footprint never read");

        let dir = std::env::temp_dir().join(format!("dl_why_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.as_path();
        // Synthetic trail: boot, ramping extract samples, NO shutdown, dead pid.
        mark_boot(home, "test-build");
        let dead_pid = 4_000_000u32; // beyond macOS/Linux pid ranges
        let base = now_secs() - 70;
        for (i, ts) in [(0i64, base), (1, base + 30), (2, base + 60)] {
            append_line(home, &json!({
                "kind":"sample","ts":ts,"pid":dead_pid,
                "tick":3,"phase":"extract","detail":"repo=smashy",
                "program":"self.dl","job":"tick:/x/smashy",
                "cpu_ms": 1000 * (i + 1), "io_read_mb": 50.0 * (i + 1) as f64,
                "io_write_mb": 200.0 * (i + 1) as f64, "rss_mb": 900.0,
            }));
        }
        // Overwrite the boot pid with the dead one so liveness reads false.
        let path = trail_path(home);
        let fixed = std::fs::read_to_string(&path)
            .unwrap()
            .replace(&format!("\"pid\":{}", std::process::id()), &format!("\"pid\":{dead_pid}"));
        std::fs::write(&path, fixed).unwrap();

        let r = report(home);
        assert!(r.contains("DEAD"), "not flagged dead:\n{r}");
        assert!(r.contains("phase=extract"), "phase missing:\n{r}");
        assert!(r.contains("repo=smashy"), "detail missing:\n{r}");
        assert!(r.contains("write 400.0MB"), "60s io delta wrong:\n{r}");
        assert!(r.contains("job tick:/x/smashy"), "job missing:\n{r}");
    }

}
