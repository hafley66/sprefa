//! The CPU governor's before/after receipt (failure-modes class 18): the same
//! extraction workload, run twice, uncapped vs `DL_MAX_CPU_PCT=60`. CPU
//! concurrency (cpu_time / wall_time via /usr/bin/time) must drop under the
//! toggle. The OS background tiers alone demonstrably do not do this: the
//! 2026-07-18 receipt was 278% cpu through a full re-extract with QoS UTILITY,
//! nice 10, darwin BG, and IOPOL_THROTTLE all applied.
#![cfg(target_os = "macos")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_budget_cpu_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A corpus with enough tree-sitter work that parallel parse shows up in the
/// cpu/wall ratio: many files, each with distinct fns and call sites.
fn write_corpus(dir: &Path, files: usize) {
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    for file_idx in 0..files {
        let mut body = String::new();
        for fn_idx in 0..60 {
            // Let-chains give the dataflow family real per-fn emit work; the
            // call gives the call family an edge.
            body.push_str(&format!(
                "pub fn unit_{file_idx}_{fn_idx}(input: u64) -> u64 {{\n    \
                 let doubled = input.wrapping_mul(2);\n    \
                 let shifted = doubled.rotate_left(3);\n    \
                 let mixed = shifted ^ doubled;\n    \
                 let summed = mixed.wrapping_add(shifted);\n    \
                 let folded = summed.wrapping_mul(mixed);\n    \
                 helper_{file_idx}(folded.wrapping_add({fn_idx}))\n}}\n"
            ));
        }
        body.push_str(&format!(
            "pub fn helper_{file_idx}(value: u64) -> u64 {{ value ^ {file_idx} }}\n"
        ));
        fs::write(src.join(format!("gen_{file_idx}.rs")), body).unwrap();
    }
}

/// Run the workload once; returns (wall_secs, cpu_secs) parsed from
/// `/usr/bin/time -p` (POSIX format: real/user/sys lines on stderr).
fn timed_run(dir: &Path, state: &Path, db_tag: &str, cap: Option<&str>) -> (f64, f64) {
    let prog = dir.join("p.dl");
    // The scan rule defines the corpus (a program without one ingests no
    // files); call_edge then forces the call family's tree-sitter parse over
    // all of it.
    fs::write(
        &prog,
        "rel rust_file(path: file).\n\
         rel busy(total: int).\n\
         rel flowing(total: int).\n\
         rust_file(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev).\n\
         busy(count(caller)) <- rust_file(_), call_edge(caller, callee, _).\n\
         flowing(count(src)) <- rust_file(_), flow_edge(src, dst).\n\
         ? busy(total).\n\
         ? flowing(total).\n",
    )
    .unwrap();
    let mut cmd = Command::new("/usr/bin/time");
    cmd.arg("-p")
        .arg(DL)
        .arg(&prog)
        .args(["--no-daemon", "--db"])
        .arg(dir.join(db_tag))
        .current_dir(dir)
        .env("SPREFA_CONFIG", "/nonexistent/hermetic.toml")
        .env("XDG_STATE_HOME", state)
        .env("DL_RAYON_THREADS", "4")
        .env_remove("DL_MAX_CPU_PCT");
    if let Some(pct) = cap {
        cmd.env("DL_MAX_CPU_PCT", pct);
    }
    let out = cmd.output().expect("run dl under /usr/bin/time");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let grab = |key: &str| -> f64 {
        stderr
            .lines()
            .rev()
            .find_map(|line| line.trim().strip_prefix(key).map(|v| v.trim().to_string()))
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| panic!("no `{key}` line in time output:\n{stderr}"))
    };
    let real = grab("real");
    let cpu = grab("user") + grab("sys");
    (real, cpu)
}

/// 1-minute load average divided by core count. The toggle receipt compares
/// cpu/wall ratios, and a busy machine (fleet builds, parallel suites) starves
/// the uncapped run of cores — ratio_off collapses toward 1x and the relative
/// assertion flakes. The receipt only means something on a quiet machine.
fn load_per_core() -> f64 {
    let out = Command::new("sysctl")
        .args(["-n", "vm.loadavg"])
        .output()
        .expect("sysctl vm.loadavg");
    let text = String::from_utf8_lossy(&out.stdout);
    // Format: `{ 1.86 2.06 2.21 }` — the first number is the 1-minute average.
    let load_1min: f64 = text
        .split_whitespace()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| panic!("unparseable vm.loadavg: {text}"));
    let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
    load_1min / cores as f64
}

#[test]
#[ignore = "load-sensitive; passes in isolation, flakes under full-suite contention"]
fn governor_toggle_caps_cpu_concurrency() {
    let load = load_per_core();
    assert!(load <= 0.5,
        "machine load {load:.2}/core too high for this timing receipt — needs a quiet machine \
         (see this test's #[ignore] reason)");
    let dir = sandbox("toggle");
    let state = dir.join("state");
    fs::create_dir_all(&state).unwrap();
    write_corpus(&dir, 260);

    let (wall_off, cpu_off) = timed_run(&dir, &state, "db_off", None);
    let (wall_capped, cpu_capped) = timed_run(&dir, &state, "db_capped", Some("60"));

    let ratio_off = cpu_off / wall_off;
    let ratio_capped = cpu_capped / wall_capped;
    eprintln!(
        "[receipt] uncapped: {cpu_off:.2}s cpu / {wall_off:.2}s wall = {ratio_off:.2}x | \
         capped@60%: {cpu_capped:.2}s cpu / {wall_capped:.2}s wall = {ratio_capped:.2}x"
    );

    // The workload must be big enough to mean something.
    assert!(
        wall_off > 0.4,
        "workload too small to measure ({wall_off:.2}s) — grow the corpus"
    );
    // Uncapped extraction parallelizes; if this ever fails the corpus stopped
    // exercising the parallel parse path and the test needs a new workload.
    assert!(
        ratio_off > 1.05,
        "uncapped run was not parallel ({ratio_off:.2}x) — workload no longer discriminates"
    );
    // The toggle's effect, relative first (robust to machine load), then an
    // absolute ceiling with tolerance for the governor's 250ms window grain
    // and the non-throttleable SQL segments.
    assert!(
        ratio_capped < ratio_off * 0.85,
        "governor did not reduce concurrency: {ratio_capped:.2}x vs uncapped {ratio_off:.2}x"
    );
    assert!(
        ratio_capped < 1.25,
        "governor ceiling not held: {ratio_capped:.2}x cpu concurrency at DL_MAX_CPU_PCT=60"
    );
}
