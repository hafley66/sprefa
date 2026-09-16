//! Leg 2: the one-shot wall-clock watchdog hard-exits 124 when a run overruns
//! `DL_MAX_WALL_SECS`, and the timeout message names what the engine was doing
//! (the self-diagnosis law) so the user needs no second command. Driven as a
//! subprocess with a short deadline and the hidden `DL_TEST_HANG_SECS` seam so
//! the run overruns without a genuinely slow repository.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("watchdog_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A run that overruns its 1s deadline exits 124 and the message names the
/// phase, detail, and the raise/disable knob.
#[test]
fn overrun_exits_124_and_names_the_phase() {
    let dir = sandbox("trip");
    let prog = dir.join("p.dl");
    fs::write(&prog, "rel seed(x: text).\nseed(\"ok\").\n? seed(x).\n").unwrap();
    let out = Command::new(DL)
        .arg(&prog)
        .args(["--no-daemon", "--db", dir.join("db").to_str().unwrap()])
        .current_dir(&dir)
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .env("DL_NO_DAEMON", "1")
        .env("DL_MAX_WALL_SECS", "1")
        .env("DL_TEST_HANG_SECS", "10")
        .output()
        .expect("run dl with a tripped wall watchdog");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(124),
        "hard-exit 124 on overrun:\n{err}"
    );
    assert!(err.contains("WALL TIMEOUT"), "loud timeout banner:\n{err}");
    assert!(
        err.contains("phase=extract"),
        "message names the phase:\n{err}"
    );
    assert!(
        err.contains("test-hang"),
        "message carries the phase detail:\n{err}"
    );
    assert!(
        err.contains("DL_MAX_WALL_SECS"),
        "message names the raise/disable knob:\n{err}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// `DL_MAX_WALL_SECS=0` disables the watchdog: the same hang runs to completion
/// (here the hang is short so the test stays fast) and exits normally, never
/// 124.
#[test]
fn zero_deadline_disables_the_watchdog() {
    let dir = sandbox("disabled");
    let prog = dir.join("p.dl");
    fs::write(&prog, "rel seed(x: text).\nseed(\"ok\").\n? seed(x).\n").unwrap();
    let out = Command::new(DL)
        .arg(&prog)
        .args(["--no-daemon", "--db", dir.join("db").to_str().unwrap()])
        .current_dir(&dir)
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .env("DL_NO_DAEMON", "1")
        .env("DL_MAX_WALL_SECS", "0")
        .env("DL_TEST_HANG_SECS", "1")
        .output()
        .expect("run dl with the watchdog disabled");
    assert_ne!(out.status.code(), Some(124), "0 disables the 124 hard-exit");
    let _ = fs::remove_dir_all(&dir);
}
