//! Failure-modes class 28 (docs/failure-modes.md): the external-log-cap sweep
//! (`src/daemon/logcap.rs`) actually runs inside a REAL daemon process and
//! truncates REAL oversized files at the exact `launchd-stdout.log` /
//! `launchd-stderr.log` / `daemon.log` paths `crate::supervise::plist_contents`
//! and `crate::daemon::client::spawn_detached` compute — not just the
//! unit-level truncate-in-place proof already in `logcap.rs` itself.
//!
//! HERMETICITY: a sandboxed `XDG_STATE_HOME` per test (mirrors
//! `tests/it/daemon_http.rs`), so this can never touch a developer's real
//! `~/.local/state/sprefa/` logs — the exact thing this arc is forbidden from
//! deleting/truncating.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");
/// Past `daemon::logcap::EXTERNAL_LOG_CAP_BYTES` (8MB).
const OVER_CAP_BYTES: usize = 9 * 1024 * 1024;

struct Sandbox {
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dl_logcap_it_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("xdg");
        let root = base.join("repo");
        fs::create_dir_all(home.join("sprefa")).unwrap();
        fs::create_dir_all(root.join(".dl")).unwrap();
        Sandbox { home, root }
    }

    /// `$XDG_STATE_HOME/sprefa` — the daemon home this sandbox's env points at.
    fn sprefa_home(&self) -> PathBuf {
        self.home.join("sprefa")
    }

    fn sock(&self) -> PathBuf {
        sprefa_v5::daemon::socket_path_for(&self.sprefa_home())
    }

    /// Pre-seed all three governed paths past the cap, BEFORE the daemon ever
    /// boots — simulates a prior run (or a fresh adoption of this fix)
    /// leaving oversized files behind.
    fn seed_oversized(&self) {
        for name in ["launchd-stdout.log", "launchd-stderr.log", "daemon.log"] {
            fs::write(self.sprefa_home().join(name), vec![b'x'; OVER_CAP_BYTES]).unwrap();
        }
    }

    /// Spawn a foreground singleton registering `self.root` (no program
    /// positional needed: empty = discover `<root>/.dl/*.dl`, which is empty
    /// here — this test only cares about the daemon's boot-time log-cap
    /// sweep, not any served program).
    fn spawn(&self) -> DaemonGuard {
        let mut cmd = Command::new(DL);
        cmd.args(["daemon", "start", "--foreground"])
            .current_dir(&self.root)
            .env("DL_DAEMON_ROOT", &self.root)
            .env("XDG_STATE_HOME", &self.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        DaemonGuard(cmd.spawn().expect("spawn foreground daemon"))
    }

    fn wait_ready(&self) -> bool {
        for _ in 0..200 {
            if self.sock().exists()
                && crate::util::uds_rpc(
                    &self.sock(),
                    r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#,
                )
                .is_some()
            {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    fn shutdown(&self) {
        let _ = crate::util::uds_rpc(
            &self.sock(),
            r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#,
        );
    }
}

/// The class-28 rail end to end: pre-seed all three governed paths past the
/// cap, boot a real (foreground) daemon in a sandboxed home, and observe every
/// one truncated to 0 by the time the daemon answers its first ping — proving
/// `run_daemon`'s boot-time `logcap::sweep` call actually runs, against the
/// real path helpers, inside a real process — not merely the in-crate unit
/// mechanism.
#[test]
fn boot_sweep_truncates_preexisting_oversized_external_logs() {
    let sb = Sandbox::new("boot");
    sb.seed_oversized();
    for name in ["launchd-stdout.log", "launchd-stderr.log", "daemon.log"] {
        let path = sb.sprefa_home().join(name);
        assert!(
            fs::metadata(&path).unwrap().len() as usize >= OVER_CAP_BYTES,
            "{name} must start oversized: {}",
            path.display()
        );
    }

    let _child = sb.spawn();
    assert!(sb.wait_ready(), "daemon never answered ping — boot-time sweep test cannot proceed");

    for name in ["launchd-stdout.log", "launchd-stderr.log", "daemon.log"] {
        let path = sb.sprefa_home().join(name);
        let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(u64::MAX);
        assert_eq!(
            len, 0,
            "{name} must be truncated to 0 by the boot-time logcap sweep, got {len} bytes: {}",
            path.display()
        );
    }

    sb.shutdown();
    // `_child` (a `DaemonGuard`) kills-on-drop if `shutdown` did not land in
    // time, so no explicit wait is required for this test's own assertions.
}

/// A file that never crossed the cap must be left untouched by the same real
/// boot path (not just by the unit-level `cap_in_place`) — the sweep must not
/// be a blanket "always truncate on boot" that would silently eat a small,
/// legitimately-in-progress log.
#[test]
fn boot_sweep_leaves_under_cap_logs_alone() {
    let sb = Sandbox::new("undercap");
    fs::write(sb.sprefa_home().join("launchd-stderr.log"), b"a few bytes only\n").unwrap();

    let _child = sb.spawn();
    assert!(sb.wait_ready(), "daemon never answered ping");

    let content = fs::read_to_string(sb.sprefa_home().join("launchd-stderr.log")).unwrap();
    assert!(
        content.starts_with("a few bytes only"),
        "under-cap pre-existing content must survive the boot sweep untouched: {content:?}"
    );

    sb.shutdown();
}
