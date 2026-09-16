//! Hermetic state isolation (failure-modes class 29 + 30, 2026-07-21).
//!
//! Two live defects these tests pin:
//!   30. `DL_STATE_DIR` was told to be the sandbox knob but was read nowhere;
//!       every "sandboxed" run wrote the real `~/.local/state/sprefa`.
//!   29. A file-scoped `--check`/`--diag-json`/`--lsp` silently retargeted its
//!       db to the real per-root db (and the daemon that serves it), narrowing
//!       an ~860MB analysis cache from a partial worktree scan.
//!
//! HERMETICITY: every `dl` here runs under `util::hermetic_env` — DL_STATE_DIR
//! + XDG_STATE_HOME + HOME all pinned to a per-test temp dir — so nothing can
//! reach the developer's real state home. The tests assert that positively:
//! writes land under DL_STATE_DIR, and the XDG fallback dir stays empty.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::{hermetic_env, DaemonGuard};

const DL: &str = env!("CARGO_BIN_EXE_dl");

struct Sandbox {
    base: PathBuf,
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    /// A root carrying a `.dl/` rail that fires exactly one `error` diag off a
    /// scanned fact, plus a `src/` file for the scan to see. The rail firing is
    /// the observable both the daemon and in-process paths must agree on.
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dl_hermetic_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("home");
        let root = base.join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join(".dl")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src").join("x.rs"), "fn tripwire_here() {}\n").unwrap();
        fs::write(root.join(".dl").join("p.dl"), concat!(
            "rel hit(path: file, line: int).\n",
            "hit(path, line) <- scan(\"WORK\", \"src/**/*.rs\", path, rev), match(path, rev, /tripwire_here/, line).\n",
            "diag(path: path, line: line, severity: \"error\", code: \"tripwire\", msg: \"tripwire found\") <- hit(path, line).\n",
        )).unwrap();
        Sandbox { base, home, root }
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(DL);
        cmd.current_dir(&self.root);
        hermetic_env(&mut cmd, &self.home);
        cmd
    }

    /// The DL_STATE_DIR the hermetic env pins (`<home>/sprefa`) — where every
    /// write MUST land.
    fn state_dir(&self) -> PathBuf {
        self.home.join("sprefa")
    }

    /// Count `roots/<key>/db.sqlite` files under a given state dir.
    fn root_dbs_under(dir: &Path) -> usize {
        let roots = dir.join("roots");
        fs::read_dir(&roots)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().join("db.sqlite").is_file())
            .count()
    }

    fn spawn_daemon(&self) -> DaemonGuard {
        let mut cmd = self.cmd();
        cmd.args(["daemon", "start", "--foreground"])
            .env("DL_DAEMON_ROOT", &self.root)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        DaemonGuard(cmd.spawn().expect("spawn daemon"))
    }

    fn daemon_ready(&self) -> bool {
        let sock = sprefa_v5::daemon::socket_path_for(&self.state_dir());
        for _ in 0..200 {
            if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }
}

/// (3) `DL_STATE_DIR` actually redirects: a discovery-mode run whose db defaults
/// to the per-root db writes it under DL_STATE_DIR, and the XDG_STATE_HOME
/// fallback dir (a DIFFERENT temp path) gains nothing. Proves DL_STATE_DIR
/// outranks XDG_STATE_HOME (class 30 — before the fix DL_STATE_DIR was inert
/// and the write went to XDG's `sprefa`, or the real home).
#[test]
fn dl_state_dir_outranks_xdg_and_receives_the_write() {
    let sb = Sandbox::new("redirect");
    // Point XDG at a SEPARATE dir so we can tell which knob won.
    let xdg = sb.base.join("xdg-separate");
    fs::create_dir_all(&xdg).unwrap();
    let mut cmd = sb.cmd();
    cmd.arg("--check") // discovery mode (no positional) => defaults db
        .env("XDG_STATE_HOME", &xdg) // distinct from DL_STATE_DIR (<home>/sprefa)
        .env("DL_NO_DAEMON", "1");
    let out = cmd.output().expect("run dl --check discovery");
    let stderr = String::from_utf8_lossy(&out.stderr);
    // The rail fires -> exit 2.
    assert_eq!(
        out.status.code(),
        Some(2),
        "discovery --check should trip the rail: {stderr}"
    );
    assert_eq!(
        Sandbox::root_dbs_under(&sb.state_dir()),
        1,
        "DL_STATE_DIR must receive the defaulted per-root db: {stderr}"
    );
    assert_eq!(
        Sandbox::root_dbs_under(&xdg.join("sprefa")),
        0,
        "XDG_STATE_HOME must NOT receive the write when DL_STATE_DIR is set: {stderr}"
    );
}

/// (1) A file-scoped `--check` (positional program) writes ONLY the scratch
/// state home, and crucially mints NO per-root db at all — it runs on an
/// ephemeral in-memory db (class 29). Contrast with discovery mode above, which
/// legitimately builds one.
#[test]
fn file_scoped_check_is_ephemeral_no_root_db() {
    let sb = Sandbox::new("filescoped");
    let mut cmd = sb.cmd();
    cmd.arg(sb.root.join(".dl").join("p.dl")) // POSITIONAL => file-scoped
        .arg("--check")
        .env("DL_NO_DAEMON", "1");
    let out = cmd.output().expect("run file-scoped dl --check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(2),
        "the rail still fires in-process: {stderr}"
    );
    assert!(stderr.contains("tripwire"), "the diag renders: {stderr}");
    // The whole point: no roots/<key>/db.sqlite anywhere under the state home.
    let roots = sb.state_dir().join("roots");
    let minted = fs::read_dir(&roots).map(|d| d.count()).unwrap_or(0);
    assert_eq!(minted, 0,
        "a file-scoped --check must not cold-build ANY per-root db (ephemeral :memory:); found {minted} under {}: {stderr}",
        roots.display());
}

/// (2) Same file-scoped `--check` yields the SAME diagnostic verdict whether a
/// daemon is running or `--no-daemon` is forced — and stays hermetic in both:
/// with the daemon up it must still NOT narrow/attach the served root (class 29
/// is exactly "a check ran against the daemon's real db"). Parity of the
/// exit code + rendered diag is the assertion.
#[test]
fn file_scoped_check_parity_daemon_vs_no_daemon() {
    let sb = Sandbox::new("parity");

    // No-daemon mode.
    let mut nd = sb.cmd();
    nd.arg(sb.root.join(".dl").join("p.dl"))
        .arg("--check")
        .env("DL_NO_DAEMON", "1");
    let out_nd = nd.output().expect("no-daemon file-scoped check");
    let code_nd = out_nd.status.code();
    let err_nd = String::from_utf8_lossy(&out_nd.stderr).into_owned();

    // Daemon mode: a real singleton serving this root is up.
    let daemon = sb.spawn_daemon();
    assert!(sb.daemon_ready(), "daemon did not become ready");
    let mut dm = sb.cmd();
    dm.arg(sb.root.join(".dl").join("p.dl")).arg("--check"); // no DL_NO_DAEMON
    let out_dm = dm.output().expect("daemon-mode file-scoped check");
    let code_dm = out_dm.status.code();
    let err_dm = String::from_utf8_lossy(&out_dm.stderr).into_owned();
    drop(daemon);

    assert_eq!(code_nd, Some(2), "no-daemon check trips the rail: {err_nd}");
    assert_eq!(
        code_dm,
        Some(2),
        "daemon-mode check trips the same rail: {err_dm}"
    );
    assert_eq!(
        err_nd.lines().filter(|l| l.contains("tripwire")).count(),
        err_dm.lines().filter(|l| l.contains("tripwire")).count(),
        "same diagnostic count in both modes\nno-daemon:\n{err_nd}\ndaemon:\n{err_dm}",
    );
}

/// The rail itself fires when a NEW state-home env read lands outside the
/// resolver (`.dl/state-home-single-source.dl`, class-30 guard). A fixture with
/// a fresh `env::var_os("DL_STATE_DIR")` read must warn.
#[test]
fn state_home_rail_flags_a_new_reader() {
    let sb = Sandbox::new("rail");
    // The shipped rail file, run against a fixture tree that contains a
    // forbidden read in a file with no baseline row.
    fs::write(
        sb.root.join("src").join("leak.rs"),
        "pub fn h() -> Option<std::ffi::OsString> { std::env::var_os(\"DL_STATE_DIR\") }\n",
    )
    .unwrap();
    let rail = format!(
        "{}/.dl/state-home-single-source.dl",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut cmd = sb.cmd();
    cmd.arg(&rail).arg("--check").env("DL_NO_DAEMON", "1");
    let out = cmd.output().expect("run the state-home rail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("state-home-read-new-file"),
        "a new state-home env read outside the resolver must warn: {stderr}"
    );
}
