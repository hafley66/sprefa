//! The exit codes a spawn-and-join depends on, taken from the real binary:
//! `--wait` on `lane create` returns through this same verb.

use std::path::PathBuf;
use std::process::Command;

fn mail_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("boop-wait-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// One result row in the shape the on-exit epilogue hails: from the lane, to
/// the parent that spawned it.
fn seed_result(dir: &std::path::Path, lane: &str, rc: i32) {
    let row = serde_json::json!({
        "id": "m-seed",
        "from": lane,
        "to": "sprefa-coordinator",
        "from_timestamp": "2026-08-10T00:00:00.000Z",
        "to_timestamp": null,
        "kind": "result",
        "reply_to": null,
        "body": format!("lane {lane} done rc={rc}"),
        "ref": null,
    });
    std::fs::write(dir.join("bus.ndjson"), format!("{row}\n")).unwrap();
}

fn wait_exit(dir: &std::path::Path, lane: &str, timeout: &str) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_boop"))
        .args(["beep", "lane", "wait", lane, "--timeout", timeout])
        .arg("--mail-dir")
        .arg(dir)
        .status()
        .unwrap()
        .code()
        .unwrap()
}

/// RECEIPT. A lane's rc reaches the caller's shell unchanged, which is what
/// makes `lane create --wait` a spawn-and-join.
#[test]
fn a_wait_exits_with_the_lanes_own_rc() {
    let dir = mail_dir("rc");
    seed_result(&dir, "feature-schema-emit", 0);
    assert_eq!(wait_exit(&dir, "feature-schema-emit", "5"), 0);

    let dir = mail_dir("rc-fail");
    seed_result(&dir, "feature-schema-emit", 17);
    assert_eq!(wait_exit(&dir, "feature-schema-emit", "5"), 17);
    let _ = std::fs::remove_dir_all(&dir);
}

/// RECEIPT. A lane that never reports exits 124, the timeout code, so a
/// wedged lane cannot look like a success.
#[test]
fn a_wait_that_times_out_exits_124() {
    let dir = mail_dir("timeout");
    assert_eq!(wait_exit(&dir, "feature-never-lands", "1"), 124);
    let _ = std::fs::remove_dir_all(&dir);
}
