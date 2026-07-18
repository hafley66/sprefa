//! End-to-end singleton-daemon lifecycle tests. Drives the compiled `dl` binary
//! through spawn, ping, query, diag, add_root/drop_root, shutdown.
//!
//! HERMETICITY: every test sets `XDG_STATE_HOME` to its own sandbox, so the
//! daemon's home (socket + pid + per-root dbs) lives under the temp dir and a
//! leaked/misbehaving test daemon can NEVER bind a developer's real socket (the
//! "disc2" class, pinned by `sandboxed_daemon_never_binds_default_home`). Each
//! RPC carries a `root` envelope naming which engine to address.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// A sandbox: an isolated XDG home for the singleton + a repo root to serve.
struct Sandbox {
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dl_singleton_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let home = base.join("xdg");
        let root = base.join("repo");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join(".dl")).unwrap();
        Sandbox { home, root }
    }

    /// The singleton socket for this sandbox's home (mirrors `socket_path_for`).
    fn sock(&self) -> PathBuf {
        sprefa_v5::daemon::socket_path_for(&self.home.join("sprefa"))
    }

    /// Spawn a foreground singleton serving `self.root`.
    fn spawn(&self, program: Option<&Path>, envs: &[(&str, &str)]) -> DaemonGuard {
        let mut cmd = Command::new(DL);
        cmd.args(["daemon", "start", "--foreground"]);
        if let Some(p) = program { cmd.arg(p); }
        cmd.current_dir(&self.root)
            .env("DL_DAEMON_ROOT", &self.root)
            .env("XDG_STATE_HOME", &self.home)
            // Hermetic: never pick up the developer's ~/.config/sprefa repos.
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .stdout(Stdio::null()).stderr(Stdio::null());
        for (k, v) in envs { cmd.env(k, v); }
        DaemonGuard(cmd.spawn().expect("spawn singleton daemon"))
    }

    fn wait_ready(&self) -> bool {
        let sock = self.sock();
        for _ in 0..200 {
            if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
                // Ping the ROOT so its engine is registered + ticked.
                if rpc_root(&sock, 1, "ping", &self.root, serde_json::json!({})).is_some() {
                    return true;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    /// Ping this root's engine, return its tick_count.
    fn tick(&self, id: u64) -> u64 {
        let v = rpc_root(&self.sock(), id, "ping", &self.root, serde_json::json!({}))
            .expect("ping response");
        v["result"]["tick_count"].as_u64().expect("tick_count")
    }

    /// One `?`-query row count for this root.
    fn query_rows(&self) -> usize {
        let v = rpc_root(&self.sock(), 99, "query", &self.root, serde_json::json!({}))
            .expect("query response");
        v["result"]["results"][0]["rows"].as_array().map(|a| a.len()).unwrap_or(0)
    }

    fn shutdown(&self) {
        // Process-level: no root envelope.
        let _ = rpc(&self.sock(), r#"{"jsonrpc":"2.0","id":9000,"method":"shutdown","params":{}}"#);
    }
}

/// Raw framed RPC: write the exact body, read one framed response.
fn rpc(sock: &Path, body: &str) -> Option<String> {
    use std::io::Write;
    let mut s = std::os::unix::net::UnixStream::connect(sock).ok()?;
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).ok()?;
    read_frame(&mut s)
}

/// RPC with the `root` envelope injected into params. Returns parsed JSON.
fn rpc_root(sock: &Path, id: u64, method: &str, root: &Path, mut params: serde_json::Value)
    -> Option<serde_json::Value>
{
    params["root"] = serde_json::json!(root.to_string_lossy());
    let body = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string();
    let resp = rpc(sock, &body)?;
    serde_json::from_str(&resp).ok()
}

#[test]
fn ping_query_diag_shutdown_round_trip() {
    let sb = Sandbox::new("roundtrip");
    fs::write(sb.root.join("p.dl"),
        "rel edge(a: text, b: text).\n\
         edge(\"a\", \"b\").\n\
         edge(\"b\", \"c\").\n\
         rel reach(a: text, b: text).\n\
         reach(x, y) <- edge(x, y).\n\
         reach(x, z) <- edge(x, y), reach(y, z).\n\
         ? reach(x, y).\n").unwrap();

    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "singleton not ready");

    let q = rpc_root(&sb.sock(), 2, "query", &sb.root, serde_json::json!({})).expect("query");
    let qs = q.to_string();
    assert!(qs.contains("reach"), "query response should mention reach: {qs}");
    assert!(qs.contains("columns"), "response should have columns: {qs}");

    let d = rpc_root(&sb.sock(), 3, "diag", &sb.root, serde_json::json!({})).expect("diag");
    assert!(d["result"]["rows"].is_array(), "diag response should have rows: {d}");

    sb.shutdown();
    let status = child.wait_timeout(std::time::Duration::from_secs(5))
        .expect("daemon did not exit after shutdown");
    assert!(status.success(), "daemon should exit 0 after shutdown");
}

/// Two sandbox roots served by ONE singleton over ONE socket, each with its own
/// program + isolated db: a query against root A never sees root B's rows.
#[test]
fn singleton_serves_two_roots_isolated() {
    let base = std::env::temp_dir().join(format!("dl_two_roots_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let home = base.join("xdg");
    let a = base.join("a");
    let b = base.join("b");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(a.join(".dl")).unwrap();
    fs::create_dir_all(b.join(".dl")).unwrap();
    // The daemon serves each root's discovered `.dl/*.dl` set (not a positional).
    fs::write(a.join(".dl").join("p.dl"), "rel a_only(x: text).\na_only(\"aaa\").\n? a_only(x).\n").unwrap();
    fs::write(b.join(".dl").join("p.dl"), "rel b_only(x: text).\nb_only(\"bbb\").\n? b_only(x).\n").unwrap();

    // Start the singleton rooted at A (discovery).
    let mut child = DaemonGuard(Command::new(DL)
        .args(["daemon", "start", "--foreground"])
        .current_dir(&a).env("DL_DAEMON_ROOT", &a).env("XDG_STATE_HOME", &home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().expect("spawn"));
    let sock = sprefa_v5::daemon::socket_path_for(&home.join("sprefa"));
    let mut up = false;
    for _ in 0..200 {
        if sock.exists() && rpc_root(&sock, 1, "ping", &a, serde_json::json!({})).is_some() { up = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(up, "singleton not ready");

    // Register B by naming it in a query (attach IS registration).
    let qa = rpc_root(&sock, 2, "query", &a, serde_json::json!({})).expect("query a").to_string();
    let qb = rpc_root(&sock, 3, "query", &b, serde_json::json!({})).expect("query b").to_string();
    assert!(qa.contains("aaa") && !qa.contains("bbb"), "root A sees only a_only: {qa}");
    assert!(qb.contains("bbb") && !qb.contains("aaa"), "root B sees only b_only: {qb}");

    // Both dbs live under the singleton home, distinct dirs.
    let roots_dir = home.join("sprefa").join("roots");
    let dbs: Vec<_> = fs::read_dir(&roots_dir).unwrap().flatten()
        .filter(|e| e.path().join("db.sqlite").exists()).collect();
    assert!(dbs.len() >= 2, "each root has its own db dir under {}; found {}", roots_dir.display(), dbs.len());

    let _ = rpc(&sock, r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#);
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// `add_root` is idempotent: registering the same root twice keeps ONE entry and
/// does not re-cold-tick a distinct engine.
#[test]
fn add_root_is_idempotent() {
    let sb = Sandbox::new("idem");
    fs::write(sb.root.join("p.dl"), "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();
    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");

    let first = rpc_root(&sb.sock(), 2, "add_root", &sb.root, serde_json::json!({})).expect("add_root");
    let key1 = first["result"]["key"].as_str().unwrap_or("").to_string();
    let second = rpc_root(&sb.sock(), 3, "add_root", &sb.root, serde_json::json!({})).expect("add_root2");
    let key2 = second["result"]["key"].as_str().unwrap_or("").to_string();
    assert_eq!(key1, key2, "same root -> same key");

    // The process summary lists exactly one registered root.
    let summary = rpc(&sb.sock(), r#"{"jsonrpc":"2.0","id":4,"method":"ping","params":{}}"#).expect("summary");
    let v: serde_json::Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(v["result"]["root_count"].as_u64(), Some(1), "one registered root: {v}");

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Registering a root nested inside an already-registered root is refused loudly
/// (the SCIP explosion-guard shape).
#[test]
fn nested_root_registration_refused() {
    let sb = Sandbox::new("nested");
    fs::write(sb.root.join("p.dl"), "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();
    let inner = sb.root.join("sub");
    fs::create_dir_all(inner.join(".dl")).unwrap();
    fs::write(inner.join("q.dl"), "rel u(a: text).\nu(\"y\").\n? u(a).\n").unwrap();

    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");

    let resp = rpc_root(&sb.sock(), 2, "add_root", &inner, serde_json::json!({})).expect("add_root inner");
    let msg = resp["error"]["message"].as_str().unwrap_or("");
    assert!(msg.contains("nested inside") && msg.contains(&sb.root.to_string_lossy().to_string()),
        "nested registration must be refused naming both paths: {resp}");

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// `dl daemon drop <root> --purge` deregisters and deletes the root's db dir.
#[test]
fn drop_purge_removes_root_db() {
    let sb = Sandbox::new("drop");
    fs::write(sb.root.join("p.dl"), "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();
    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");

    // The initial root is registered; its db dir exists.
    let roots_dir = sb.home.join("sprefa").join("roots");
    let key = fs::read_to_string(sb.home.join("sprefa").join("roots.json")).ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.as_array().and_then(|a| a.first().cloned()))
        .and_then(|r| r.get("key").and_then(|k| k.as_str()).map(String::from))
        .expect("registered root key in roots.json");
    assert!(roots_dir.join(&key).join("db.sqlite").exists(), "db exists before drop");

    // Drop via the CLI (targets DL_DAEMON_ROOT + XDG home).
    let out = Command::new(DL)
        .args(["daemon", "drop"]).arg(&sb.root).arg("--purge")
        .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output().expect("drop");
    assert!(out.status.success(), "drop failed: {}", String::from_utf8_lossy(&out.stderr));

    assert!(!roots_dir.join(&key).exists(), "--purge should remove roots/<key>/");

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Part 4 of the daemon CPU-hog fix (stale-root eviction): a registered
/// root whose directory was deleted while the daemon was down is evicted —
/// not silently re-skipped — at the NEXT boot's `roots.json` replay: the
/// entry is dropped from the served set AND removed from `roots.json`, one
/// log line, and its still-live sibling root replays normally.
#[test]
fn boot_replay_evicts_root_with_deleted_directory() {
    let base = std::env::temp_dir().join(format!("dl_stale_root_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let home = base.join("xdg");
    let gone = base.join("gone");
    let stays = base.join("stays");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(gone.join(".dl")).unwrap();
    fs::create_dir_all(stays.join(".dl")).unwrap();
    fs::write(gone.join(".dl").join("p.dl"), "rel g(x: text).\ng(\"g\").\n? g(x).\n").unwrap();
    fs::write(stays.join(".dl").join("p.dl"), "rel s(x: text).\ns(\"s\").\n? s(x).\n").unwrap();

    let sock = sprefa_v5::daemon::socket_path_for(&home.join("sprefa"));
    let spawn_at = |cwd: &Path| {
        DaemonGuard(Command::new(DL)
            .args(["daemon", "start", "--foreground"])
            .current_dir(cwd).env("DL_DAEMON_ROOT", cwd).env("XDG_STATE_HOME", &home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().expect("spawn"))
    };
    let wait_up = |root: &Path| -> bool {
        for _ in 0..200 {
            if sock.exists() && rpc_root(&sock, 1, "ping", root, serde_json::json!({})).is_some() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    };

    // First boot: register BOTH roots (the initial root, `gone`, via the
    // positional program arg; `stays` by naming it in a query — attach IS
    // registration, same as `singleton_serves_two_roots_isolated`).
    let mut child = spawn_at(&gone);
    assert!(wait_up(&gone), "first boot not ready");
    let qs = rpc_root(&sock, 2, "query", &stays, serde_json::json!({})).expect("query stays").to_string();
    assert!(qs.contains('s'), "stays root registered+queried: {qs}");
    let _ = rpc(&sock, r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#);
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));

    let roots_json_path = home.join("sprefa").join("roots.json");
    let before: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&roots_json_path).expect("roots.json after first boot")).unwrap();
    assert_eq!(before.as_array().map(|a| a.len()), Some(2), "both roots persisted: {before}");

    // Delete `gone`'s entire directory, then restart the daemon rooted at
    // the surviving `stays` root (so the process cwd stays valid).
    fs::remove_dir_all(&gone).unwrap();
    let mut child2 = spawn_at(&stays);
    assert!(wait_up(&stays), "second boot not ready");

    // `stays` replayed normally.
    let qs2 = rpc_root(&sock, 3, "query", &stays, serde_json::json!({})).expect("query stays 2").to_string();
    assert!(qs2.contains('s'), "stays root still served after replay: {qs2}");

    // `gone` was evicted: roots.json now lists only `stays`, and the
    // process-level summary counts one registered root, not two.
    let after: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&roots_json_path).expect("roots.json after second boot")).unwrap();
    let after_roots: Vec<String> = after.as_array().unwrap().iter()
        .filter_map(|r| r.get("root").and_then(|x| x.as_str()).map(String::from)).collect();
    assert_eq!(after_roots.len(), 1, "gone's entry evicted from roots.json: {after}");
    assert!(after_roots[0].contains("stays"), "surviving entry is `stays`: {after}");

    let summary = rpc(&sock, r#"{"jsonrpc":"2.0","id":4,"method":"ping","params":{}}"#).expect("summary");
    let v: serde_json::Value = serde_json::from_str(&summary).unwrap();
    assert_eq!(v["result"]["root_count"].as_u64(), Some(1),
        "only the surviving root is registered after replay: {v}");

    let _ = rpc(&sock, r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#);
    let _ = child2.wait_timeout(std::time::Duration::from_secs(5));
}

/// The disc2 class made structurally impossible: a daemon started with
/// `XDG_STATE_HOME` pointed at a sandbox binds ONLY the sandbox socket and never
/// creates a per-root `.dl/daemon.sock`, so it cannot bind a developer's real
/// socket regardless of cwd.
#[test]
fn sandboxed_daemon_never_binds_default_home() {
    let sb = Sandbox::new("disc2");
    fs::write(sb.root.join("p.dl"), "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();
    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");

    // The ONLY socket is under the sandbox home.
    assert!(sb.sock().starts_with(&sb.home), "socket must live under the sandbox XDG home: {}", sb.sock().display());
    assert!(sb.sock().exists(), "sandbox socket bound");
    // No per-root socket exists anymore (the leaked-socket mechanism is gone).
    assert!(!sb.root.join(".dl").join("daemon.sock").exists(),
        "the singleton must NOT create a per-root socket under the repo");

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// A detached `dl daemon start` (no `--foreground`) backgrounds the singleton and
/// returns; `dl daemon status` then lists the registered root.
#[test]
fn detached_start_and_status_lists_root() {
    let sb = Sandbox::new("detached");
    fs::write(sb.root.join("p.dl"), "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();

    // Detached start returns immediately; the background daemon keeps running.
    let out = Command::new(DL)
        .args(["daemon", "start"]).arg(sb.root.join("p.dl"))
        .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output().expect("detached start");
    assert!(out.status.success(), "detached start failed: {}", String::from_utf8_lossy(&out.stderr));

    // status lists the root.
    let mut listed = false;
    for _ in 0..100 {
        let st = Command::new(DL)
            .args(["daemon", "status"])
            .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .output().expect("status");
        let s = String::from_utf8_lossy(&st.stdout);
        if st.status.success() && s.contains(&sb.root.to_string_lossy().to_string()) && s.contains("roots") {
            listed = true; break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(listed, "status should list the registered root");

    // Clean up the detached daemon.
    let _ = Command::new(DL).args(["daemon", "stop"])
        .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output();
}

/// A bad `--load` (watched) must fail cleanly and NOT wedge the daemon: the bad
/// file rolls back, so a subsequent GOOD load still succeeds.
#[test]
fn bad_load_rolls_back_and_daemon_keeps_loading() {
    let sb = Sandbox::new("badload");
    fs::write(sb.root.join("p.dl"),
        "rel edge(a: text, b: text).\nedge(\"a\", \"b\").\n? edge(a, b).\n").unwrap();
    fs::write(sb.root.join("bad.dl"),
        "rel bad(a: text).\nbad(a) <- undeclared_rel(a).\n").unwrap();
    fs::write(sb.root.join("good.dl"),
        "rel good(a: text).\ngood(\"hi\").\n? good(a).\n").unwrap();

    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");

    let load = |id: u64, path: &Path| -> serde_json::Value {
        rpc_root(&sb.sock(), id, "load", &sb.root,
            serde_json::json!({"path": path.to_string_lossy(), "mode": "watched"})).expect("load resp")
    };
    let bad = load(1, &sb.root.join("bad.dl")).to_string();
    assert!(bad.contains("error"), "bad load must error: {bad}");
    let good = load(2, &sb.root.join("good.dl")).to_string();
    assert!(good.contains("result") && good.contains("loaded"),
        "a good load after a bad one must still succeed: {good}");

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// `dl daemon restart` replaces the running singleton (new pid) and answers again.
#[test]
fn restart_replaces_the_running_daemon() {
    let sb = Sandbox::new("restart");
    fs::write(sb.root.join("p.dl"), "rel e(a: text).\ne(\"x\").\n? e(a).\n").unwrap();
    let _child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");
    let pidfile = sb.home.join("sprefa").join("daemon.pid");
    let pid_before = fs::read_to_string(&pidfile).ok()
        .and_then(|t| t.lines().next().map(String::from)).unwrap_or_default();
    assert!(!pid_before.is_empty(), "pid file present before restart");

    let out = Command::new(DL)
        .args(["daemon", "restart"])
        .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output().expect("restart");
    assert!(out.status.success(), "restart failed: {}", String::from_utf8_lossy(&out.stderr));

    let sock = sb.sock();
    let mut pid_after = String::new();
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            pid_after = fs::read_to_string(&pidfile).ok()
                .and_then(|t| t.lines().next().map(String::from)).unwrap_or_default();
            if !pid_after.is_empty() && pid_after != pid_before { break; }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_ne!(pid_after, pid_before, "restart spawned a new daemon process");

    let _ = Command::new(DL).args(["daemon", "stop"])
        .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml").output();
}

/// `dl daemon rows REL` dumps a relation's live rows from the singleton (routed
/// to the cwd root via DL_DAEMON_ROOT).
#[test]
fn rows_verb_dumps_a_rel_from_the_daemon() {
    let sb = Sandbox::new("rowsflag");
    fs::write(sb.root.join("p.dl"),
        "rel edge(a: text, b: text).\nedge(\"a\", \"b\").\nedge(\"b\", \"c\").\n? edge(a, b).\n").unwrap();
    let _child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");

    let mut stdout = String::new();
    for _ in 0..100 {
        let out = Command::new(DL)
            .args(["daemon", "rows", "edge"])
            .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .output().expect("rows");
        stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if out.status.success() && stdout.contains("(2 rows)") { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(stdout.contains("a\tb"), "column header present: {stdout}");
    assert!(stdout.contains("(2 rows)"), "two edge rows dumped: {stdout}");
    sb.shutdown();
}

/// `ping` (with root) reports a non-empty build_id and an idle root does not spin
/// its tick counter.
#[test]
fn ping_reports_build_id_and_idle_is_quiet() {
    let sb = Sandbox::new("buildid");
    fs::write(sb.root.join(".dl").join("prog.dl"),
        "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();
    let mut child = sb.spawn(None, &[]); // discovery mode
    assert!(sb.wait_ready(), "not ready");

    let first = rpc_root(&sb.sock(), 1, "ping", &sb.root, serde_json::json!({})).expect("ping");
    let build_id = first["result"]["build_id"].as_str();
    assert!(build_id.is_some_and(|s| !s.is_empty()), "ping must report build_id: {first}");
    let t0 = sb.tick(2);
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let t1 = sb.tick(3);
    assert!(t1 - t0 <= 1, "idle root self-ticked {} times in 2s (expected 0-1)", t1 - t0);

    sb.shutdown();
    let status = child.wait_timeout(std::time::Duration::from_secs(5)).expect("exit");
    assert!(status.success());
}

/// P0: a deep XDG home whose natural socket path exceeds the sun_path cap must
/// relocate the socket to a short hashed path, and the daemon must bind there.
#[test]
fn deep_home_uses_short_socket() {
    use sprefa_v5::daemon::socket_path_for;
    let deep_home = std::env::temp_dir().join(format!(
        "dl_deephome_{}/{}",
        std::process::id(),
        "nested_component_padding_to_overflow_the_sun_path_limit_x".repeat(2),
    ));
    let sprefa_home = deep_home.join("sprefa");
    let natural = sprefa_home.join("daemon.sock");
    assert!(natural.as_os_str().len() >= 100, "fixture home not deep enough: {}", natural.display());
    let sock = socket_path_for(&sprefa_home);
    assert!(sock.as_os_str().len() < 100, "relocated socket still too long: {}", sock.display());
    assert!(sock.starts_with(std::env::temp_dir().join("dl-sock")),
        "relocated socket lives under the short base dir: {}", sock.display());
    assert_ne!(sock, natural, "deep home must not use the natural socket path");
    assert_eq!(sock, socket_path_for(&sprefa_home), "short socket path must be stable");

    let _ = fs::remove_file(&sock);
    let root = deep_home.join("repo");
    fs::create_dir_all(root.join(".dl")).unwrap();
    fs::write(root.join("p.dl"), "rel mark(t: text).\nmark(\"a\").\n? mark(t).\n").unwrap();
    let mut child = DaemonGuard(Command::new(DL)
        .args(["daemon", "start", "--foreground"]).arg(root.join("p.dl"))
        .current_dir(&root).env("DL_DAEMON_ROOT", &root).env("XDG_STATE_HOME", &deep_home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().expect("spawn deep-home daemon"));
    let mut ready = false;
    for _ in 0..200 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() { ready = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready, "deep-home daemon never bound the short socket {}", sock.display());
    let _ = rpc(&sock, r#"{"jsonrpc":"2.0","id":1,"method":"shutdown","params":{}}"#);
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
    let _ = fs::remove_dir_all(std::env::temp_dir().join(format!("dl_deephome_{}", std::process::id())));
}

/// `DL_NO_DAEMON=1` keeps the in-process path; no socket created.
#[test]
fn no_daemon_env_opts_out() {
    let sb = Sandbox::new("no_daemon");
    fs::write(sb.root.join("p.dl"), "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();
    let out = Command::new(DL)
        .arg(sb.root.join("p.dl"))
        .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .env("DL_NO_DAEMON", "1")
        .output().expect("run dl");
    assert!(out.status.success(), "dl should succeed; stderr={}", String::from_utf8_lossy(&out.stderr));
    assert!(!sb.sock().exists(), "no-daemon path must not create a socket");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("t =>"), "stdout should print query: {stdout}");
    assert!(stdout.contains("x"));
}

/// P1/P5: writes under a gitignored directory must not advance the tick count.
#[test]
fn gitignored_writes_do_not_tick() {
    let sb = Sandbox::new("noise");
    fs::create_dir_all(sb.root.join("junk")).unwrap();
    fs::write(sb.root.join(".gitignore"), "junk/\n").unwrap();
    fs::write(sb.root.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    fs::create_dir_all(sb.root.join("src")).unwrap();
    fs::write(sb.root.join("src/a.rs"), "fn a() {}\n").unwrap();

    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t0 = sb.tick(1);
    for i in 0..50 {
        fs::write(sb.root.join(format!("junk/f{i}.o")), "build output").unwrap();
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t1 = sb.tick(2);
    assert_eq!(t0, t1, "gitignored writes advanced the tick count ({t0} -> {t1})");
    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// P2: a burst of source edits coalesces into a single tick via the debounce.
#[test]
fn source_burst_coalesces_into_one_tick() {
    let sb = Sandbox::new("debounce");
    fs::create_dir_all(sb.root.join("src")).unwrap();
    fs::write(sb.root.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    fs::write(sb.root.join("src/a.rs"), "fn a() {}\n").unwrap();

    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t0 = sb.tick(1);
    for i in 0..5 {
        fs::write(sb.root.join(format!("src/b{i}.rs")), format!("fn b{i}() {{}}\n")).unwrap();
    }
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let t1 = sb.tick(2);
    let delta = t1 - t0;
    assert!(delta >= 1, "burst produced no tick ({t0} -> {t1})");
    assert!(delta <= 2, "burst of 5 edits produced {delta} ticks; debounce should coalesce to ~1");
    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Discovery-mode content edit hot-reloads (never exits — one exit would kill
/// every served root).
#[test]
fn discovery_mode_content_edit_hot_reloads() {
    let sb = Sandbox::new("dischot");
    fs::write(sb.root.join(".dl").join("panel.dl"),
        "rel mark(t: text).\nmark(\"a\").\n? mark(t).\n").unwrap();
    let mut child = sb.spawn(None, &[]); // discovery
    assert!(sb.wait_ready(), "not ready");

    let mut baseline_ok = false;
    for _ in 0..60 {
        if sb.query_rows() == 1 { baseline_ok = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(baseline_ok, "baseline should see 1 mark row; got {}", sb.query_rows());

    std::thread::sleep(std::time::Duration::from_millis(1500));
    {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new().append(true)
            .open(sb.root.join(".dl").join("panel.dl")).unwrap();
        writeln!(f, "mark(\"b\").").unwrap();
    }
    let mut reloaded = false;
    for _ in 0..100 {
        if sb.query_rows() == 2 { reloaded = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(reloaded, "content edit should hot-reload; got {} row(s)", sb.query_rows());
    assert!(child.try_wait().expect("try_wait").is_none(),
        "daemon should still be running after a discovered-program content edit");
    sb.shutdown();
    let status = child.wait_timeout(std::time::Duration::from_secs(5)).expect("exit");
    assert!(status.success());
}

/// P6: `--await-settle` blocks on the daemon until the program is quiescent.
#[test]
fn await_settle_blocks_until_effect_drains() {
    let sb = Sandbox::new("aws");
    fs::write(sb.root.join("p.dl"),
        "sh greet(name) -> (line: text) = `echo hi-{name}`.\n\
         rel want(name: text).\n\
         want(\"bob\").\n\
         rel greet(name: text, line: text).\n\
         greet(name, line) <- @async want(name).\n\
         rel done(line: text).\n\
         done(line) <- greet(_, line).\n\
         ? done(line).\n").unwrap();
    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[("DL_POLL_SECS", "1")]);
    assert!(sb.wait_ready(), "not ready");

    let out = Command::new(DL)
        .args(["daemon", "await-settle", "--ms", "20000"])
        .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output().expect("await-settle");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "await-settle should exit 0: {stdout} {}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("settled=true"), "reports settled: {stdout}");

    let q = rpc_root(&sb.sock(), 2, "query", &sb.root, serde_json::json!({})).expect("query").to_string();
    assert!(q.contains("hi-bob"), "the sh response drained into done: {q}");
    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Part 1 of the daemon CPU-hog fix: once a root with `@async` effects has
/// drained its queue and settled, further poll cycles must perform NO more
/// full ticks (the cheap idle probe skips them) — `tick_count` stays flat
/// across several idle poll cycles instead of climbing by one every
/// `DL_POLL_SECS`. This is the root cause of the reported CPU hog: a full
/// tick's corpus reconcile ran unconditionally on every poll cycle even
/// with nothing queued and nothing changed.
#[test]
fn idle_root_with_effects_gets_no_further_ticks_once_settled() {
    let sb = Sandbox::new("idlepoll");
    fs::write(sb.root.join("p.dl"),
        "sh greet(name) -> (line: text) = `echo hi-{name}`.\n\
         rel want(name: text).\n\
         want(\"bob\").\n\
         rel greet(name: text, line: text).\n\
         greet(name, line) <- @async want(name).\n\
         rel done(line: text).\n\
         done(line) <- greet(_, line).\n\
         ? done(line).\n").unwrap();
    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[("DL_POLL_SECS", "1")]);
    assert!(sb.wait_ready(), "not ready");

    // Block until the effect has drained and the program is quiescent (same
    // mechanism `await_settle_blocks_until_effect_drains` exercises).
    let out = Command::new(DL)
        .args(["daemon", "await-settle", "--ms", "20000"])
        .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home).env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output().expect("await-settle");
    assert_eq!(out.status.code(), Some(0), "await-settle should exit 0: {}", String::from_utf8_lossy(&out.stderr));

    // Settled means the queue is drained; give the poll loop one more cycle
    // to observe that quiet state, then sample `tick_count` as the baseline.
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let baseline = sb.tick(10);

    // Several more idle poll cycles (DL_POLL_SECS=1): nothing queued, nothing
    // changed, no `every`/`clock` cadence in this program, so `poll_idle`
    // should skip every one of them.
    std::thread::sleep(std::time::Duration::from_millis(4000));
    let after = sb.tick(11);

    assert_eq!(baseline, after,
        "an idle settled root must receive zero further full ticks from the poll loop \
         (tick_count {baseline} -> {after} over ~4 idle poll cycles)");

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Part 2 of the daemon CPU-hog fix: a `pending_effect` row whose kind has no
/// registered executor template (no `sh`/`sh!`/`sh*` decl, no `effect_cmd`
/// overlay fact) must not abort the whole drain — the observed live-daemon
/// symptom was `poll error: no effect command registered for kind 'ext_built'`
/// repeating on every poll cycle forever. It should instead be parked and
/// warned about ONCE (not once per poll), and the daemon must keep polling
/// normally (no `poll error` lines at all) afterward.
#[test]
fn orphan_effect_kind_warns_once_and_does_not_abort_poll() {
    let base = std::env::temp_dir().join(format!("dl_orphan_kind_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let home = base.join("xdg");
    let root = base.join("repo");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(root.join(".dl")).unwrap();
    // `orphan_kind` is a head-response `@async` rule with NO `sh` decl and NO
    // `effect_cmd(...)` fact overlay — its executor template can never
    // resolve, so every drain that reaches it hits the missing-template case.
    let program = root.join("p.dl");
    fs::write(&program,
        "rel want(name: text).\n\
         want(\"bob\").\n\
         rel orphan_kind(name: text, out: text).\n\
         orphan_kind(name, out) <- @async want(name).\n\
         ? orphan_kind(name, out).\n").unwrap();

    let sock = sprefa_v5::daemon::socket_path_for(&home.join("sprefa"));
    let mut child = DaemonGuard(Command::new(DL)
        .args(["daemon", "start", "--foreground"]).arg(&program)
        .current_dir(&root).env("DL_DAEMON_ROOT", &root).env("XDG_STATE_HOME", &home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .env("DL_POLL_SECS", "1")
        .stdout(Stdio::null()).stderr(Stdio::piped())
        .spawn().expect("spawn"));
    let mut up = false;
    for _ in 0..200 {
        if sock.exists() && rpc_root(&sock, 1, "ping", &root, serde_json::json!({})).is_some() { up = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(up, "daemon not ready");

    // Several poll cycles at DL_POLL_SECS=1 — a once-per-poll regression
    // would clearly repeat the warning/error many times over this window.
    std::thread::sleep(std::time::Duration::from_millis(4500));

    let _ = rpc(&sock, r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#);
    let status = child.wait_timeout(std::time::Duration::from_secs(5));
    assert!(status.is_some(), "daemon should exit cleanly, not hang, after an orphaned effect kind");

    let mut stderr_buf = String::new();
    {
        use std::io::Read as _;
        if let Some(mut s) = child.0.stderr.take() {
            let _ = s.read_to_string(&mut stderr_buf);
        }
    }
    let warn_lines = stderr_buf.lines()
        .filter(|l| l.contains("no effect command registered for kind `orphan_kind`"))
        .count();
    assert_eq!(warn_lines, 1,
        "orphan kind should warn exactly once across ~4 poll cycles, not once per cycle: {stderr_buf}");
    let abort_lines = stderr_buf.lines().filter(|l| l.contains("poll error")).count();
    assert_eq!(abort_lines, 0,
        "an orphaned effect kind must not abort the poll drain via `?`: {stderr_buf}");
}

/// `ping` (with root) reports an `activity` object with all fields.
#[test]
fn ping_reports_activity_object() {
    let sb = Sandbox::new("activity");
    fs::write(sb.root.join("p.dl"),
        "rel edge(a: text, b: text).\nedge(\"a\", \"b\").\n\
         rel reach(a: text, b: text).\n\
         reach(x, y) <- edge(x, y).\n\
         reach(x, z) <- edge(x, y), reach(y, z).\n\
         ? reach(x, y).\n").unwrap();
    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");

    let mut saw_idle = false;
    for i in 0..20 {
        let v = rpc_root(&sb.sock(), i, "ping", &sb.root, serde_json::json!({})).expect("ping");
        let activity = &v["result"]["activity"];
        assert!(activity.get("phase").and_then(|v| v.as_str()).is_some(), "phase: {activity}");
        assert!(activity.get("detail").and_then(|v| v.as_str()).is_some(), "detail: {activity}");
        assert!(activity.get("program").and_then(|v| v.as_str()).is_some(), "program: {activity}");
        assert!(activity.get("tick").and_then(|v| v.as_u64()).is_some(), "tick: {activity}");
        assert!(activity.get("elapsed_ms").and_then(|v| v.as_u64()).is_some(), "elapsed_ms: {activity}");
        if activity.get("phase").and_then(|v| v.as_str()) == Some("idle") { saw_idle = true; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(saw_idle, "activity.phase never read idle after the cold tick");
    sb.shutdown();
    let status = child.wait_timeout(std::time::Duration::from_secs(5)).expect("exit");
    assert!(status.success());
}

/// Scale test (macOS-runnable): a large gitignored subtree churning must produce
/// ZERO ticks, and a single real source edit buried in that churn must still tick.
#[test]
fn event_volume_gitignored_subtree_scale() {
    const DIRS: usize = 40;
    const FILES_PER_DIR: usize = 50; // 2000 gitignored files total
    let sb = Sandbox::new("scale");
    fs::write(sb.root.join(".gitignore"), "target/\n").unwrap();
    fs::write(sb.root.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    fs::create_dir_all(sb.root.join("src")).unwrap();
    fs::write(sb.root.join("src/a.rs"), "fn a() {}\n").unwrap();
    for d in 0..DIRS { fs::create_dir_all(sb.root.join(format!("target/build/{d}"))).unwrap(); }

    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t0 = sb.tick(1);

    let start = std::time::Instant::now();
    for d in 0..DIRS {
        for f in 0..FILES_PER_DIR {
            fs::write(sb.root.join(format!("target/build/{d}/obj{f}.o")), "build output").unwrap();
        }
    }
    let write_ms = start.elapsed().as_millis();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    let t1 = sb.tick(2);
    eprintln!("[scale] wrote {} gitignored files in {write_ms}ms; tick delta {} (want 0)",
        DIRS * FILES_PER_DIR, t1 - t0);
    assert_eq!(t0, t1, "{} gitignored writes advanced the tick count ({t0} -> {t1})", DIRS * FILES_PER_DIR);

    fs::write(sb.root.join("src/needle.rs"), "fn needle() {}\n").unwrap();
    let mut ticked = false;
    for _ in 0..40 {
        if sb.tick(3) > t1 { ticked = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(ticked, "a source edit buried in gitignored churn never ticked (gate over-dropped)");
    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// A `git checkout` rewrites worktree files AND moves `.git`; the git handler
/// diffs and feeds `tick_paths`. `#[ignore]`: macOS FSEvents unreliably delivers
/// git-checkout events (run manually / on Linux).
#[test]
#[ignore = "macOS FSEvents unreliably delivers git-checkout events; run manually / on Linux"]
fn git_checkout_rewrite_triggers_tick() {
    let sb = Sandbox::new("gitco");
    let g = |args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(&sb.root).args(args).output().expect("git");
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    g(&["init", "-q"]);
    g(&["config", "user.email", "t@t"]);
    g(&["config", "user.name", "t"]);
    g(&["config", "commit.gpgsign", "false"]);
    fs::create_dir_all(sb.root.join("src")).unwrap();
    fs::write(sb.root.join("src/a.rs"), "fn a() {}\nfn b() {}\n").unwrap();
    fs::write(sb.root.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    g(&["add", "."]);
    g(&["commit", "-q", "-m", "c2"]);
    let c2 = String::from_utf8_lossy(&Command::new("git").arg("-C").arg(&sb.root)
        .args(["rev-parse", "HEAD"]).output().unwrap().stdout).trim().to_string();
    fs::write(sb.root.join("src/a.rs"), "fn a() {}\n").unwrap();
    g(&["add", "."]);
    g(&["commit", "-q", "-m", "c1"]);
    let c1 = String::from_utf8_lossy(&Command::new("git").arg("-C").arg(&sb.root)
        .args(["rev-parse", "HEAD"]).output().unwrap().stdout).trim().to_string();
    g(&["checkout", "-q", &c2]);

    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");
    let mut ok = false;
    for _ in 0..60 {
        if sb.query_rows() == 2 { ok = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(ok, "baseline c2 should see 2 fns; got {}", sb.query_rows());
    g(&["checkout", "-q", &c1]);
    let mut reverted = false;
    for _ in 0..300 {
        if sb.query_rows() == 1 { reverted = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(reverted, "git checkout c2->c1 should drop a fn; got {}", sb.query_rows());
    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Scale test (Linux only, IGNORED until per-directory watch pruning lands): the
/// daemon's inotify watch count should track the UN-ignored directory count.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "fails until Linux per-directory watch pruning lands; this test is its spec"]
fn linux_inotify_watch_count_tracks_unignored() {
    const UNIGNORED_DIRS: usize = 20;
    const IGNORED_DIRS: usize = 2000;
    let sb = Sandbox::new("inotify_count");
    fs::write(sb.root.join(".gitignore"), "target/\n").unwrap();
    fs::write(sb.root.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    for d in 0..UNIGNORED_DIRS {
        let sub = sb.root.join(format!("src/mod{d}"));
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("f.rs"), "fn f() {}\n").unwrap();
    }
    for d in 0..IGNORED_DIRS { fs::create_dir_all(sb.root.join(format!("target/build/{d}"))).unwrap(); }

    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let pid = child.id();
    let watches = linux_inotify_watch_count(pid);
    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
    eprintln!("[inotify] {watches} watches for {UNIGNORED_DIRS} unignored + {IGNORED_DIRS} ignored dirs");
    assert!(watches < UNIGNORED_DIRS + 50,
        "inotify watch count {watches} scales with the ignored subtree; per-dir pruning not in effect");
}

#[cfg(target_os = "linux")]
fn linux_inotify_watch_count(pid: u32) -> usize {
    let fd_dir = format!("/proc/{pid}/fd");
    let mut total = 0;
    let entries = match fs::read_dir(&fd_dir) { Ok(e) => e, Err(_) => return 0 };
    for entry in entries.flatten() {
        let target = fs::read_link(entry.path()).unwrap_or_default();
        if target.to_string_lossy().contains("inotify") {
            let fd_name = entry.file_name();
            let fdinfo = format!("/proc/{pid}/fdinfo/{}", fd_name.to_string_lossy());
            if let Ok(text) = fs::read_to_string(&fdinfo) {
                total += text.lines().filter(|l| l.starts_with("inotify ")).count();
            }
        }
    }
    total
}

/// Rule B (hermetic daemon): a daemon-SERVED engine must NOT ingest the ambient
/// `SPREFA_CONFIG` repos; its corpus is its own root plus its program, so one
/// save wakes only that root instead of ticking every registered repo's engine.
/// `DL_AMBIENT_REPOS=1` restores the old cross-root behavior.
#[test]
fn served_engine_hermetic_to_ambient_config_repos() {
    // An UNRELATED repo the ambient config registers. The served root must not
    // see it under the hermetic default; DL_AMBIENT_REPOS=1 must pull it back in.
    let ext = std::env::temp_dir().join(format!("dl_ambient_ext_{}", std::process::id()));
    let _ = fs::remove_dir_all(&ext);
    fs::create_dir_all(ext.join("src")).unwrap();
    fs::write(ext.join("src/lib.rs"), "fn ext() {}\n").unwrap();
    let cfg = std::env::temp_dir().join(format!("dl_ambient_cfg_{}.toml", std::process::id()));
    fs::write(&cfg, format!("[[repos]]\nslug = \"ambient-ext\"\nroot = \"{}\"\n", ext.display())).unwrap();
    let cfg_str = cfg.to_string_lossy().into_owned();

    // The served root's `repo` relation, stringified from its query response.
    let repo_rel = |sb: &Sandbox| -> String {
        rpc_root(&sb.sock(), 2, "query", &sb.root, serde_json::json!({}))
            .expect("query response").to_string()
    };

    // --- Hermetic default: SPREFA_CONFIG set, no DL_AMBIENT_REPOS. ---
    let sb = Sandbox::new("hermetic");
    fs::write(sb.root.join("p.dl"), "? repo(slug, root, url).\n").unwrap();
    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[("SPREFA_CONFIG", &cfg_str)]);
    assert!(sb.wait_ready(), "not ready (hermetic)");
    let hermetic = repo_rel(&sb);
    assert!(!hermetic.contains("ambient-ext"),
        "served engine must NOT ingest the ambient config repo: {hermetic}");
    assert_eq!(sb.query_rows(), 1, "hermetic corpus is just the served root's own repo: {hermetic}");
    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));

    // --- Escape hatch: DL_AMBIENT_REPOS=1 restores the ambient corpus. ---
    let sb2 = Sandbox::new("ambient");
    fs::write(sb2.root.join("p.dl"), "? repo(slug, root, url).\n").unwrap();
    let mut child2 = sb2.spawn(Some(&sb2.root.join("p.dl")),
        &[("SPREFA_CONFIG", &cfg_str), ("DL_AMBIENT_REPOS", "1")]);
    assert!(sb2.wait_ready(), "not ready (ambient)");
    let ambient = repo_rel(&sb2);
    assert!(ambient.contains("ambient-ext"),
        "DL_AMBIENT_REPOS=1 restores the ambient config repo in the corpus: {ambient}");
    sb2.shutdown();
    let _ = child2.wait_timeout(std::time::Duration::from_secs(5));

    let _ = fs::remove_dir_all(&ext);
    let _ = fs::remove_file(&cfg);
}

/// J1: after a served-root source edit, the tick runs as a queued job and
/// `dl daemon jobs` lists a completed `tick:{root}` entry.
#[test]
fn daemon_jobs_lists_a_completed_tick_after_a_source_edit() {
    let sb = Sandbox::new("jobs");
    fs::create_dir_all(sb.root.join("src")).unwrap();
    fs::write(sb.root.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    fs::write(sb.root.join("src/a.rs"), "fn a() {}\n").unwrap();

    let mut child = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "not ready");
    // Past the watcher's startup grace, then edit a source file to enqueue a tick.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t0 = sb.tick(1);
    fs::write(sb.root.join("src/b.rs"), "fn b() {}\n").unwrap();

    // The tick job drains via a worker; wait for the tick to land.
    let mut ticked = false;
    for _ in 0..80 {
        if sb.tick(2) > t0 { ticked = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(ticked, "the queued tick never ran");

    // `dl daemon jobs` lists a completed tick job for this root.
    let mut listed = false;
    let mut last = String::new();
    for _ in 0..40 {
        let out = Command::new(DL)
            .args(["daemon", "jobs"])
            .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .output().expect("jobs");
        last = String::from_utf8_lossy(&out.stdout).into_owned();
        // Header + a tick row that has reached `done`.
        if out.status.success() && last.contains("KEY\tKIND\tSTATE")
            && last.lines().any(|l| l.starts_with("tick:") && l.contains("done")) {
            listed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(listed, "daemon jobs should list a completed tick job: {last}");

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Supervision arc (plans/2026-07-18-infra-library-adoption.md section 3.3):
/// the fd-lock single-instance witness. A second `run_daemon` against the SAME
/// home must fail fast — before touching the job queue or engine dbs — naming
/// the lock file, while the first instance keeps serving undisturbed. The old
/// bespoke pidfile was informational only and enforced nothing; this pins the
/// new behavior.
#[test]
fn second_daemon_in_same_home_refused_by_fd_lock() {
    let sb = Sandbox::new("fdlock");
    fs::write(sb.root.join("p.dl"), "rel marker(name: text).\nmarker(\"held\").\n").unwrap();
    let mut first = sb.spawn(Some(&sb.root.join("p.dl")), &[]);
    assert!(sb.wait_ready(), "first singleton not ready");

    // Second foreground daemon in the SAME sandbox home: must exit non-zero
    // with the fd-lock refusal instead of double-serving.
    let out = Command::new(DL)
        .args(["daemon", "start", "--foreground"])
        .current_dir(&sb.root).env("DL_DAEMON_ROOT", &sb.root).env("XDG_STATE_HOME", &sb.home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .output().expect("run second daemon");
    assert!(!out.status.success(),
        "second daemon in the same home must be refused (stdout: {} stderr: {})",
        String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("another dl daemon instance already holds"),
        "refusal must name the fd-lock witness: {stderr}");

    // The holder is untouched: still alive, still answering pings.
    assert!(matches!(first.try_wait(), Ok(None)), "first daemon must still be running");
    let ping = rpc_root(&sb.sock(), 5, "ping", &sb.root, serde_json::json!({}));
    assert!(ping.is_some(), "first daemon must still answer pings after the refused second start");

    sb.shutdown();
    let status = first.wait_timeout(std::time::Duration::from_secs(5))
        .expect("first daemon did not exit after shutdown");
    assert!(status.success(), "first daemon should exit 0 after shutdown");
}

// ---------- helpers ----------

fn read_frame(s: &mut std::os::unix::net::UnixStream) -> Option<String> {
    use std::io::Read;
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    let mut content_length: Option<usize> = None;
    let mut line = Vec::<u8>::new();
    loop {
        line.clear();
        loop {
            if s.read(&mut byte).ok()? == 0 { return None; }
            line.push(byte[0]);
            if byte[0] == b'\n' { break; }
        }
        let trimmed = trim_crlf(&line);
        if trimmed.is_empty() { break; }
        if let Some(rest) = trimmed.strip_prefix(b"Content-Length:") {
            let val = std::str::from_utf8(rest).ok()?.trim();
            content_length = Some(val.parse().ok()?);
        }
    }
    let len = content_length?;
    buf.resize(len, 0);
    s.read_exact(&mut buf).ok()?;
    String::from_utf8(buf).ok()
}

fn trim_crlf(b: &[u8]) -> &[u8] {
    let mut e = b.len();
    while e > 0 && (b[e-1] == b'\n' || b[e-1] == b'\r') { e -= 1; }
    &b[..e]
}

trait WaitTimeoutExt {
    fn wait_timeout(&mut self, dur: std::time::Duration) -> Option<std::process::ExitStatus>;
}
impl WaitTimeoutExt for std::process::Child {
    fn wait_timeout(&mut self, dur: std::time::Duration) -> Option<std::process::ExitStatus> {
        let start = std::time::Instant::now();
        loop {
            match self.try_wait() {
                Ok(Some(s)) => return Some(s),
                Ok(None) => {
                    if start.elapsed() > dur {
                        let _ = self.kill();
                        let _ = self.wait();
                        return None;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(_) => return None,
            }
        }
    }
}
