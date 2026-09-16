//! Leg 1: every one-shot CLI entry applies the process CPU/IO budget (standing
//! law: nothing seizes the machine). Verified from OUTSIDE the process — the
//! test never applies the budget inside the shared test process (that would
//! deprioritize the whole suite); it spawns the built binary with
//! `DL_BUDGET_DEBUG=1` on a cheap `--parse-only` run and reads the `[budget]`
//! line off stderr.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("budget_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `DL_BUDGET_DEBUG=1` prints one verifiable `[budget]` line naming the QoS,
/// nice, iopol, and rayon thread count the run was capped at.
#[test]
fn budget_debug_line_reports_the_applied_cap() {
    let dir = sandbox("debug");
    let prog = dir.join("p.dl");
    fs::write(&prog, "rel seed(x: text).\nseed(\"ok\").\n? seed(x).\n").unwrap();
    let out = Command::new(DL)
        .arg(&prog)
        .args(["--parse-only", "--no-daemon"])
        .current_dir(&dir)
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .env("DL_NO_DAEMON", "1")
        .env("DL_BUDGET_DEBUG", "1")
        .output()
        .expect("run dl --parse-only under DL_BUDGET_DEBUG");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("[budget] qos="), "budget line printed:\n{err}");
    assert!(err.contains("nice="), "nice reported:\n{err}");
    assert!(
        err.contains("threads="),
        "rayon thread count reported:\n{err}"
    );
    #[cfg(target_os = "macos")]
    {
        assert!(err.contains("qos=utility"), "macos qos utility:\n{err}");
        assert!(err.contains("nice=10"), "macos nice 10:\n{err}");
        assert!(
            err.contains("iopol=throttle"),
            "macos iopol throttle:\n{err}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// `DL_NO_BUDGET=1` is the benchmarking escape hatch: no budget, and the debug
/// line is suppressed even when `DL_BUDGET_DEBUG=1` is also set.
#[test]
fn no_budget_env_prints_nothing() {
    let dir = sandbox("nobudget");
    let prog = dir.join("p.dl");
    fs::write(&prog, "rel seed(x: text).\nseed(\"ok\").\n? seed(x).\n").unwrap();
    let out = Command::new(DL)
        .arg(&prog)
        .args(["--parse-only", "--no-daemon"])
        .current_dir(&dir)
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .env("DL_NO_DAEMON", "1")
        .env("DL_NO_BUDGET", "1")
        .env("DL_BUDGET_DEBUG", "1")
        .output()
        .expect("run dl --parse-only under DL_NO_BUDGET");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        !err.contains("[budget]"),
        "escape hatch prints nothing:\n{err}"
    );
    let _ = fs::remove_dir_all(&dir);
}
