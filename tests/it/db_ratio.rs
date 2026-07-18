//! e2e: the db-ratio verdict (failure-modes class 17 promotion,
//! docs/failure-modes.md:407-441): `db bytes / corpus bytes` printed once at
//! a cold `--no-daemon --check` completion (`src/lib.rs::run_check_inproc`,
//! via `src/db_ratio.rs`) and once per root at daemon boot
//! (`src/daemon.rs::ServedRoot::open`). Both call the same `emit_verdict`, so
//! the `--no-daemon` path below is the discriminating fixture for the
//! verdict's shape (fields present, threshold honored) and the daemon-boot
//! test only needs to prove that surface fires at all — the shared unit is
//! already exercised.
//!
//! HERMETICITY: every run sets `SPREFA_CONFIG` to a nonexistent path (no
//! ambient `~/.config/sprefa/config.toml` ingestion) and, for the daemon
//! case, its own `XDG_STATE_HOME` sandbox (mirrors `tests/it/verdict.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dl_db_ratio_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    dir
}

const PROG: &str = r#"
rel seen(path: file).
seen(p) <- scan("WORK", "src/**/*.rs", p, rev).
"#;

fn run_check(dir: &Path, db: &Path, extra_envs: &[(&str, &str)]) -> String {
    fs::write(dir.join("p.dl"), PROG).unwrap();
    let mut cmd = Command::new(DL);
    cmd.arg(dir.join("p.dl"))
        .args(["--db", db.to_str().unwrap(), "--no-daemon", "--check"])
        .current_dir(dir)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml");
    for (key, value) in extra_envs { cmd.env(key, value); }
    let out = cmd.output().expect("run dl --check");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Cold `--no-daemon --check` completion always prints the `[db-ratio]` info
/// line with real db/corpus/ratio fields — this is the "boot" moment named
/// in the class-17 rail for a one-shot run.
#[test]
fn no_daemon_check_completion_emits_db_ratio_fields() {
    let dir = sandbox("fields");
    fs::write(dir.join("src/a.rs"), "pub struct Alpha { pub n: u32 }\n").unwrap();
    let db = dir.join("db");

    let stderr = run_check(&dir, &db, &[]);
    let line = stderr
        .lines()
        .find(|l| l.starts_with("[db-ratio]"))
        .unwrap_or_else(|| panic!("no [db-ratio] verdict line, got:\n{stderr}"));
    assert!(line.contains("db="), "missing db= field: {line}");
    assert!(line.contains("corpus="), "missing corpus= field: {line}");
    assert!(line.contains("ratio="), "missing ratio= field: {line}");
    assert!(!line.contains("WARNING"), "default threshold should not warn: {line}");
}

/// `DL_DB_RATIO_WARN` lowered below the fixture's actual ratio flips the
/// verdict to a loud WARNING line naming the threshold — proven fail-pre-fix
/// by the absence of any `[db-ratio]` line at all before this rail landed
/// (grep for the literal tag against the pre-fix binary finds nothing).
#[test]
fn ratio_warn_env_lowers_the_ceiling_and_fires() {
    let dir = sandbox("warn");
    fs::write(dir.join("src/a.rs"), "pub struct Alpha { pub n: u32 }\n").unwrap();
    let db = dir.join("db");

    // A near-empty SQLite db already carries a fixed page-size floor well
    // past a two-line source file's byte count, so a near-zero ceiling is
    // guaranteed to trip regardless of the exact bytes on this machine.
    let stderr = run_check(&dir, &db, &[("DL_DB_RATIO_WARN", "0.0001")]);
    let warn_line = stderr
        .lines()
        .find(|l| l.starts_with("[db-ratio]") && l.contains("WARNING"))
        .unwrap_or_else(|| panic!("no [db-ratio] WARNING line at a near-zero ceiling, got:\n{stderr}"));
    assert!(warn_line.contains("exceeds 0.0x"), "threshold not named in warn line: {warn_line}");
    assert!(warn_line.contains("class 17"), "warn line should point at the failure-modes rail: {warn_line}");

    // Same fixture, a ceiling no real ratio will cross: no WARNING line.
    let quiet_stderr = run_check(&dir, &dir.join("db2"), &[("DL_DB_RATIO_WARN", "1000000")]);
    assert!(
        !quiet_stderr.lines().any(|l| l.starts_with("[db-ratio]") && l.contains("WARNING")),
        "a million-x ceiling must not warn: {quiet_stderr}"
    );
}

/// `DL_NO_STALE_WARN`'s sibling surface, same rail family: the db-ratio
/// verdict also fires once per root at daemon boot, not only on the
/// `--no-daemon` one-shot path. This only proves the surface fires (the
/// field/threshold shape is already pinned above via the shared
/// `db_ratio::emit_verdict` unit both call sites route through).
#[test]
fn daemon_boot_emits_db_ratio_for_the_served_root() {
    let base = std::env::temp_dir().join(format!("dl_db_ratio_daemon_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let home = base.join("xdg");
    let root = base.join("repo");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(root.join(".dl")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/a.rs"), "pub struct Alpha { pub n: u32 }\n").unwrap();
    fs::write(root.join(".dl/p.dl"), PROG).unwrap();

    let mut cmd = Command::new(DL);
    cmd.args(["daemon", "start", "--foreground"])
        .current_dir(&root)
        .env("DL_DAEMON_ROOT", &root)
        .env("XDG_STATE_HOME", &home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = DaemonGuard(cmd.spawn().expect("spawn foreground daemon"));
    let mut stderr = child.0.stderr.take().expect("captured stderr");

    use std::io::Read;
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let _ = child.0.kill();
    let mut buf = String::new();
    let _ = stderr.read_to_string(&mut buf);

    assert!(
        buf.lines().any(|l| l.starts_with("[db-ratio]")),
        "daemon boot should print a [db-ratio] verdict for the served root, got:\n{buf}"
    );
}
