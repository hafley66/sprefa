//! `--check --max-wall` must leave hooks with a loud, successful partial
//! result instead of an opaque timeout or a blocking error exit.

use std::fs;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

#[test]
fn zero_wall_deadline_prints_warning_and_exits_zero() {
    let dir = std::env::temp_dir().join(format!("dl_check_deadline_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let program = dir.join("p.dl");
    fs::write(&program, "rel seed(x: text).\nseed(\"ok\").\n").unwrap();
    let out = Command::new(DL)
        .arg(&program)
        .args(["--check", "--max-wall", "0", "--no-daemon", "--db", dir.join("db").to_str().unwrap()])
        .current_dir(&dir)
        .env("SPREFA_CONFIG", "/nonexistent/x.toml")
        .env("DL_NO_DAEMON", "1")
        .output().expect("run timed check");
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "deadline must not block hooks: {err}");
    assert!(err.contains("CHECK TIMED OUT"), "loud partial report: {err}");
    assert!(err.contains("warning[check-timed-out]"), "warning diagnostic: {err}");
    assert!(err.contains("partial report"), "incompleteness is explicit: {err}");
}
