//! Discovery-mode `dl --check` daemon-first regression tests.
//!
//! Before this fix, `--check` in discovery mode (no positional program) always
//! defaulted the db to `<root>/.dl/cache.db` and passed `Some(path)` into
//! `run_check`, whose daemon gate only checked `db_path.is_none()` — so it
//! could never attach to a warm singleton daemon serving the same root, and
//! paid a full cold extraction on every hook/check invocation instead.
//!
//! HERMETICITY: every test sets `XDG_STATE_HOME` to its own sandbox (mirrors
//! `tests/it/daemon.rs`), so a leaked/misbehaving daemon can never touch a
//! developer's real socket or state.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

struct Sandbox {
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dl_check_daemon_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("xdg");
        let root = base.join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join(".dl")).unwrap();
        Sandbox { home, root }
    }

    fn sock(&self) -> PathBuf {
        sprefa_v5::daemon::socket_path_for(&self.home.join("sprefa"))
    }

    fn write_program(&self) {
        // Clean program: zero diag rows, but the `diag` rel must exist and be
        // queryable so the daemon-vs-in-process paths are distinguishable by
        // side effects (cache.db presence), not by output content.
        fs::write(self.root.join(".dl").join("p.dl"),
            "rel edge(a: text, b: text).\n\
             edge(\"a\", \"b\").\n").unwrap();
    }

    fn spawn(&self) -> DaemonGuard {
        let mut cmd = Command::new(DL);
        cmd.args(["daemon", "start", "--foreground"])
            .current_dir(&self.root)
            .env("DL_DAEMON_ROOT", &self.root)
            .env("XDG_STATE_HOME", &self.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .stdout(Stdio::null()).stderr(Stdio::null());
        DaemonGuard(cmd.spawn().expect("spawn singleton daemon"))
    }

    fn wait_ready(&self) -> bool {
        let sock = self.sock();
        for _ in 0..200 {
            if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
                if rpc_root(&sock, 1, "ping", &self.root, serde_json::json!({})).is_some() {
                    return true;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    fn shutdown(&self) {
        let _ = rpc(&self.sock(), r#"{"jsonrpc":"2.0","id":9000,"method":"shutdown","params":{}}"#);
    }

    /// Run `dl --check` in this sandbox's root, same env as a real hook/CI
    /// invocation would use (no explicit --db, no positional program).
    fn run_check(&self) -> std::process::Output {
        Command::new(DL)
            .arg("--check")
            .current_dir(&self.root)
            .env("DL_DAEMON_ROOT", &self.root)
            .env("XDG_STATE_HOME", &self.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .output()
            .expect("run dl --check")
    }

    fn cache_db_path(&self) -> PathBuf {
        self.root.join(".dl").join("cache.db")
    }
}

fn rpc(sock: &std::path::Path, body: &str) -> Option<String> {
    use std::io::Write;
    let mut s = std::os::unix::net::UnixStream::connect(sock).ok()?;
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).ok()?;
    read_frame(&mut s)
}

fn rpc_root(sock: &std::path::Path, id: u64, method: &str, root: &std::path::Path, mut params: serde_json::Value)
    -> Option<serde_json::Value>
{
    params["root"] = serde_json::json!(root.to_string_lossy());
    let body = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string();
    let resp = rpc(sock, &body)?;
    serde_json::from_str(&resp).ok()
}

fn read_frame(s: &mut std::os::unix::net::UnixStream) -> Option<String> {
    use std::io::{BufRead, BufReader, Read};
    let mut reader = BufReader::new(s);
    let mut len: usize = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        if line == "\r\n" || line.is_empty() { break; }
        if let Some(v) = line.trim().strip_prefix("Content-Length:") {
            len = v.trim().parse().ok()?;
        }
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

/// (a) A sandboxed daemon serving the fixture root answers `--check` in
/// discovery mode, and no fresh in-process extraction happens: `cache.db`
/// is never created (the daemon's engine lives at
/// $XDG_STATE_HOME/sprefa/roots/<hash>/db.sqlite, not `<root>/.dl/cache.db`).
#[test]
fn discovery_check_prefers_warm_daemon_no_cache_db() {
    let sb = Sandbox::new("warm");
    sb.write_program();
    let _daemon = sb.spawn();
    assert!(sb.wait_ready(), "singleton daemon not ready");

    let out = sb.run_check();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "clean program should exit 0: {stderr}");
    assert!(!stderr.contains("no daemon serving this root"),
        "a warm daemon must be used, not the loud cold-fallback path: {stderr}");
    assert!(!stderr.contains("check attach failed"),
        "daemon attach should succeed cleanly: {stderr}");
    assert!(!sb.cache_db_path().exists(),
        "discovery-mode --check against a warm daemon must NOT extract into .dl/cache.db");

    sb.shutdown();
}

/// (b) Without any daemon reachable, `--check` still works in-process AND
/// prints the loud cold-fallback line (never a silent cold extraction).
#[test]
fn discovery_check_cold_fallback_is_loud() {
    let sb = Sandbox::new("cold");
    sb.write_program();
    // No daemon spawned. DL_NO_DAEMON makes this hermetic against any stray
    // singleton on the machine (belt + suspenders with the isolated XDG home).
    let out = Command::new(DL)
        .arg("--check")
        .current_dir(&sb.root)
        .env("DL_NO_DAEMON", "1")
        .env("XDG_STATE_HOME", &sb.home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output()
        .expect("run dl --check");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "clean program should exit 0: {stderr}");
    assert!(stderr.contains("no daemon serving this root"),
        "a cold one-shot check must name itself loudly: {stderr}");
    assert!(sb.cache_db_path().exists(),
        "the cold fallback DOES use .dl/cache.db as its one-shot cache");
}

/// (c) An explicit program + `--db` keeps the old in-process path exactly:
/// no daemon attach attempted even when a daemon is running for the root.
#[test]
fn explicit_program_and_db_stays_inprocess() {
    let sb = Sandbox::new("explicit");
    sb.write_program();
    let _daemon = sb.spawn();
    assert!(sb.wait_ready(), "singleton daemon not ready");

    let scratch_db = sb.home.join("explicit.sqlite");
    let out = Command::new(DL)
        .arg("--check")
        .arg(sb.root.join(".dl").join("p.dl"))
        .arg("--db").arg(&scratch_db)
        .current_dir(&sb.root)
        .env("DL_DAEMON_ROOT", &sb.root)
        .env("XDG_STATE_HOME", &sb.home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output()
        .expect("run dl --check --db");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "clean program should exit 0: {stderr}");
    // Positive evidence of the in-process path: the [db] verdict names the
    // explicit db as the one actually opened. (A bare `contains("daemon")`
    // assert false-positived on this test's own sandbox dir name appearing
    // in that verdict's path.)
    assert!(stderr.contains("explicit.sqlite"),
        "the [db] open verdict must name the explicit --db path: {stderr}");
    assert!(!stderr.contains("no daemon serving"),
        "an explicit --db run must not even consult daemon routing: {stderr}");
    assert!(scratch_db.exists(), "the explicit --db path is what gets used");
    assert!(!sb.cache_db_path().exists(), "no cache.db side effect from an explicit --db run");

    sb.shutdown();
}
