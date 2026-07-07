//! End-to-end daemon lifecycle tests. Drives the compiled `dl` binary through
//! spawn-if-missing, ping, query, diag, shutdown. Each test uses a fresh tempdir
//! so the daemon socket never collides across files.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_daemon_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn run_daemon_explicit(dir: &PathBuf) -> DaemonGuard {
    // Spawn the daemon in foreground; tests communicate only over the unix
    // socket, so stdout/stderr are nulled — a leaked or slow-exiting daemon
    // must never write over the suite summary or hold a pipe open. The guard
    // kills the child on drop if a test panics before its clean shutdown.
    DaemonGuard(Command::new(DL)
        .args(["--daemon"])
        .arg(dir.join("p.dl"))
        .current_dir(dir).env("DL_DAEMON_ROOT", &dir)
        .stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().expect("spawn dl --daemon"))
}

#[test]
fn ping_query_diag_shutdown_round_trip() {
    let dir = sandbox("roundtrip");
    fs::write(dir.join("p.dl"),
        "rel edge(a: text, b: text).\n\
         edge(\"a\", \"b\").\n\
         edge(\"b\", \"c\").\n\
         rel reach(a: text, b: text).\n\
         reach(x, y) <- edge(x, y).\n\
         reach(x, z) <- edge(x, y), reach(y, z).\n\
         ? reach(x, y).\n").unwrap();
    fs::create_dir_all(dir.join(".dl")).unwrap();

    // Cold start: spawn daemon.
    let mut child = run_daemon_explicit(&dir);
    // Wait for socket to come up.
    let sock = dir.join(".dl").join("daemon.sock");
    let mut ready = false;
    for _ in 0..100 {
        if sock.exists() {
            // Confirm via ping RPC.
            if let Ok(mut s) = std::os::unix::net::UnixStream::connect(&sock) {
                let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#;
                use std::io::Write;
                if write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).is_ok() {
                    ready = true; break;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready, "daemon socket not ready");

    // Query RPC.
    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let body = r#"{"jsonrpc":"2.0","id":2,"method":"query","params":{}}"#;
    use std::io::Write;
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    let resp = read_frame(&mut s).expect("response");
    assert!(resp.contains("reach"), "query response should mention reach: {resp}");
    assert!(resp.contains(r#""columns"#), "response should have columns: {resp}");

    // Diag RPC.
    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let body = r#"{"jsonrpc":"2.0","id":3,"method":"diag","params":{}}"#;
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    let resp = read_frame(&mut s).expect("response");
    assert!(resp.contains(r#""rows""#), "diag response should have rows: {resp}");

    // Shutdown.
    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let body = r#"{"jsonrpc":"2.0","id":4,"method":"shutdown","params":{}}"#;
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    let _ = read_frame(&mut s).expect("shutdown ack");

    // Process should exit cleanly.
    let status = child.wait_timeout(std::time::Duration::from_secs(5))
        .expect("daemon did not exit after shutdown");
    assert!(status.success(), "daemon should exit 0 after shutdown");
}

/// A bad `--load` (watched) must fail cleanly and NOT wedge the daemon: the bad
/// file is rolled back out of the program set, so a subsequent GOOD load still
/// succeeds. Before the fix, the bad file lingered in `program_files` and every
/// future reload re-parsed it → failed → the daemon could never reload again.
#[test]
fn bad_load_rolls_back_and_daemon_keeps_loading() {
    use std::io::Write;
    let dir = sandbox("badload");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::write(dir.join("p.dl"),
        "rel edge(a: text, b: text).\nedge(\"a\", \"b\").\n? edge(a, b).\n").unwrap();
    // A file with a type error (undeclared rel in the body): reload_program
    // parses it, counts an error diag, and bails — the failure path under test.
    fs::write(dir.join("bad.dl"),
        "rel bad(a: text).\nbad(a) <- undeclared_rel(a).\n").unwrap();
    fs::write(dir.join("good.dl"),
        "rel good(a: text).\ngood(\"hi\").\n? good(a).\n").unwrap();

    let mut child = run_daemon_explicit(&dir);
    let sock = dir.join(".dl").join("daemon.sock");
    let mut ready = false;
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() { ready = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready, "daemon socket not ready");

    let rpc = |id: u32, path: &str| -> String {
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"load","params":{{"path":"{path}","mode":"watched"}}}}"#);
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        read_frame(&mut s).expect("load response")
    };

    // Bad load → error response, file rolled back.
    let bad = rpc(1, dir.join("bad.dl").to_str().unwrap());
    assert!(bad.contains("\"error\""), "bad load must return an error: {bad}");

    // Good load → success, proving the bad file did NOT poison the program set.
    let good = rpc(2, dir.join("good.dl").to_str().unwrap());
    assert!(good.contains("\"result\"") && good.contains("loaded"),
        "a good load after a bad one must still succeed (rollback worked): {good}");

    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let sd = r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#;
    write!(s, "Content-Length: {}\r\n\r\n{}", sd.len(), sd).unwrap();
    let _ = read_frame(&mut s);
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Every `query_sql` RPC appends one row to the query-history log
/// (`_query_log`, projected by the built-in `query_log` relation). Sends two
/// `query_sql` requests over the same daemon session (no file changes, no
/// intervening tick) and asserts the second one — reading the built-in's
/// backing table directly — sees the first request's SQL text. The fixture
/// program references `query_log` so the relation is declared+opted-in.
#[test]
fn query_log_records_request_history() {
    let dir = sandbox("querylog");
    fs::write(dir.join("p.dl"),
        "rel edge(a: text, b: text).\n\
         edge(\"a\", \"b\").\n\
         ? query_log(ts, source, method, body, params).\n").unwrap();
    fs::create_dir_all(dir.join(".dl")).unwrap();

    let mut child = run_daemon_explicit(&dir);
    let sock = dir.join(".dl").join("daemon.sock");
    let mut ready = false;
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            ready = true; break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready, "daemon socket not ready");

    use std::io::Write;

    // First query_sql RPC: an arbitrary SQL read over a declared rel.
    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let body = r#"{"jsonrpc":"2.0","id":10,"method":"query_sql","params":{"sql":"SELECT a, b FROM rel_edge"}}"#;
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    let resp = read_frame(&mut s).expect("first query_sql response");
    assert!(resp.contains(r#""rows""#), "first query_sql should return rows: {resp}");

    // Second query_sql RPC: read the history back through query_log's backing
    // table. No tick happened between the two requests — the handler's own
    // inline refresh (not the tick-scoped one) is what makes this visible.
    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let body2 = r#"{"jsonrpc":"2.0","id":11,"method":"query_sql","params":{"sql":"SELECT method, body FROM rel_query_log ORDER BY ts"}}"#;
    write!(s, "Content-Length: {}\r\n\r\n{}", body2.len(), body2).unwrap();
    let resp2 = read_frame(&mut s).expect("second query_sql response");
    assert!(resp2.contains("SELECT a, b FROM rel_edge"),
        "query_log history should carry the first request's SQL body: {resp2}");
    assert!(resp2.contains("query_sql"), "method column should be recorded: {resp2}");

    // Shutdown.
    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let body3 = r#"{"jsonrpc":"2.0","id":12,"method":"shutdown","params":{}}"#;
    write!(s, "Content-Length: {}\r\n\r\n{}", body3.len(), body3).unwrap();
    let _ = read_frame(&mut s);
    let status = child.wait_timeout(std::time::Duration::from_secs(5)).expect("daemon exit");
    assert!(status.success());
}

#[test]
fn stop_flag_sends_shutdown() {
    let dir = sandbox("stopflag");
    fs::write(dir.join("p.dl"), "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();
    fs::create_dir_all(dir.join(".dl")).unwrap();

    let mut child = run_daemon_explicit(&dir);
    // Wait for socket.
    let sock = dir.join(".dl").join("daemon.sock");
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // dl --stop should retire the daemon.
    let out = Command::new(DL).arg("--stop").current_dir(&dir).env("DL_DAEMON_ROOT", &dir)
        .output().expect("run dl --stop");
    assert!(out.status.success(),
        "dl --stop should exit 0; stderr={}",
        String::from_utf8_lossy(&out.stderr));

    let status = child.wait_timeout(std::time::Duration::from_secs(5))
        .expect("daemon did not exit after dl --stop");
    assert!(status.success());
}

#[test]
fn no_daemon_env_opts_out() {
    // Sanity: DL_NO_DAEMON=1 keeps the in-process path; no socket created.
    let dir = sandbox("no_daemon");
    fs::write(dir.join("p.dl"), "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();
    fs::create_dir_all(dir.join(".dl")).unwrap();

    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .current_dir(&dir).env("DL_DAEMON_ROOT", &dir)
        .env("DL_NO_DAEMON", "1")
        .output().expect("run dl");
    assert!(out.status.success(),
        "dl should succeed; stderr={}", String::from_utf8_lossy(&out.stderr));
    assert!(!dir.join(".dl").join("daemon.sock").exists(),
        "no-daemon path must not create a socket file");
    // Stdout should carry the query row.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("t =>"), "stdout should print query: {stdout}");
    assert!(stdout.contains("x"));
}

/// A `git checkout`/`pull` rewrites worktree files AND moves `.git`. The
/// git-ref handler computes the changed files from the deterministic
/// `files_changed_between` diff and feeds them to `tick_paths` (FSEvents does
/// not reliably co-deliver a checkout's rewritten files with the `.git` event).
///
/// NOTE: `#[ignore]` — on macOS, FSEvents frequently drops/latencies the events
/// for a `git checkout` (temp+rename writes + `.git` churn coalesce), so the
/// `.git` watcher trigger itself is unreliable here. The diff-driven fix this
/// guards is correct and works on Linux (inotify); run manually on a quiet box
/// or on Linux. A poll-based ref watcher (timer thread checking HEAD oids) is
/// the robust fix for macOS and is tracked as a follow-up.
#[test]
#[ignore = "macOS FSEvents unreliably delivers git-checkout events; run manually / on Linux"]
fn git_checkout_rewrite_triggers_tick() {
    let dir = sandbox("gitco");
    let g = |args: &[&str]| {
        let out = Command::new("git").arg("-C").arg(&dir).args(args).output().expect("git");
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    };
    g(&["init", "-q"]);
    g(&["config", "user.email", "t@t"]);
    g(&["config", "user.name", "t"]);
    g(&["config", "commit.gpgsign", "false"]);
    fs::create_dir_all(dir.join("src")).unwrap();
    // c2 (two fns) committed first; c1 (one fn) on top. Daemon starts at c2.
    fs::write(dir.join("src/a.rs"), "fn a() {}\nfn b() {}\n").unwrap();
    fs::write(dir.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    g(&["add", "."]);
    g(&["commit", "-q", "-m", "c2"]);
    let c2 = String::from_utf8_lossy(&Command::new("git").arg("-C").arg(&dir)
        .args(["rev-parse", "HEAD"]).output().unwrap().stdout)
        .trim().to_string();
    fs::write(dir.join("src/a.rs"), "fn a() {}\n").unwrap();
    g(&["add", "."]);
    g(&["commit", "-q", "-m", "c1"]);
    let c1 = String::from_utf8_lossy(&Command::new("git").arg("-C").arg(&dir)
        .args(["rev-parse", "HEAD"]).output().unwrap().stdout)
        .trim().to_string();
    g(&["checkout", "-q", &c2]); // HEAD=c2, worktree=2 fns (daemon baseline)

    fs::create_dir_all(dir.join(".dl")).unwrap();
    let mut child = run_daemon_explicit(&dir);
    let sock = dir.join(".dl").join("daemon.sock");
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let query_rows = || -> usize {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":99,"method":"query","params":{}}"#;
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let resp = read_frame(&mut s).expect("query response");
        let v: serde_json::Value = serde_json::from_str(&resp).expect("parse query resp");
        v["result"]["results"][0]["rows"].as_array().map(|a| a.len()).unwrap_or(0)
    };

    // Baseline at c2: two fns -> two rows. Poll (cold tick + query settle).
    let mut ok = false;
    for _ in 0..60 {
        if query_rows() == 2 { ok = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(ok, "baseline c2 should see 2 fns; got {}", query_rows());

    // Checkout c1: rewrites src/a.rs (one fn) AND moves .git in one batch.
    // Before the fix the git-ref handler `continue`d and the worktree tick was
    // lost (query stayed at 2). Now it falls through to tick_paths.
    g(&["checkout", "-q", &c1]);
    let mut reverted = false;
    for _ in 0..300 {
        if query_rows() == 1 { reverted = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(reverted, "git checkout c2->c1 should drop a fn via a worktree tick; got {}", query_rows());

    {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":5,"method":"shutdown","params":{}}"#;
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let _ = read_frame(&mut s);
    }
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// The bug this guards: a discovery-mode daemon (no positional program args,
/// programs come from `<root>/.dl/*.dl`) used to treat ANY edit to an
/// already-discovered file as `touches_program` and exit-for-respawn — but a
/// discovery daemon has no positional args to respawn FROM, so nothing brings
/// it back until some unrelated `dl` client happens to run. Concretely: the
/// VS Code panel's mark command appends one fact line to `.dl/marks.dl`,
/// which is exactly this case. The fix hot-reloads instead. Content edit to a
/// discovered file must re-tick with the new facts and leave the daemon
/// process alive.
#[test]
fn discovery_mode_content_edit_hot_reloads() {
    let dir = sandbox("dischot");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::write(dir.join(".dl").join("panel.dl"),
        "rel mark(t: text).\n\
         mark(\"a\").\n\
         ? mark(t).\n").unwrap();

    // Discovery mode: no positional program file, root comes from cwd.
    let mut child = DaemonGuard(Command::new(DL)
        .args(["--daemon"])
        .current_dir(&dir).env("DL_DAEMON_ROOT", &dir)
        .stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().expect("spawn dl --daemon (discovery)"));

    let sock = dir.join(".dl").join("daemon.sock");
    let mut ready = false;
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            ready = true; break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready, "discovery daemon socket not ready");

    let query_rows = |sock: &PathBuf| -> usize {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(sock).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"query","params":{}}"#;
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let resp = read_frame(&mut s).expect("query response");
        let v: serde_json::Value = serde_json::from_str(&resp).expect("parse query resp");
        v["result"]["results"][0]["rows"].as_array().map(|a| a.len()).unwrap_or(0)
    };

    // Baseline: one mark row.
    let mut baseline_ok = false;
    for _ in 0..60 {
        if query_rows(&sock) == 1 { baseline_ok = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(baseline_ok, "baseline should see 1 mark row; got {}", query_rows(&sock));

    // Past STARTUP_GRACE (1s) plus margin, append a fact — the marks.dl append
    // shape from the VS Code panel's mark command.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    {
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(dir.join(".dl").join("panel.dl"))
            .unwrap();
        writeln!(f, "mark(\"b\").").unwrap();
    }

    // Poll until the daemon has hot-reloaded and re-ticked with 2 rows.
    let mut reloaded = false;
    for _ in 0..100 {
        if query_rows(&sock) == 2 { reloaded = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(reloaded, "content edit to a discovered program file should hot-reload, not exit; got {} row(s)", query_rows(&sock));

    // The daemon process must still be alive (the bug: it would have exited).
    assert!(child.try_wait().expect("try_wait").is_none(),
        "daemon should still be running after a discovered-program content edit");

    // Clean shutdown.
    {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":{}}"#;
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let _ = read_frame(&mut s);
    }
    let status = child.wait_timeout(std::time::Duration::from_secs(5))
        .expect("daemon did not exit after shutdown");
    assert!(status.success());
}

/// `ping` must report the `build_id` the client uses to decide whether to
/// attach or respawn (a rebuilt/reinstalled `dl` reports a different id and
/// replaces the stale daemon instead of attaching to it). Also a light smoke
/// check that an idle discovery daemon — db defaulted to `.dl/cache.db` under
/// the watched root, stderr redirected to `.dl/daemon.log` under it too — does
/// not spin the tick counter. NOTE: this does not reproduce the production
/// self-write loop, which is db-scale/timing dependent (a 173 MB WAL write
/// lands as a fresh FSEvent after the debounce closes; a tiny synthetic db
/// coalesces it away). The watcher's self-write DISCRIMINATION is pinned by the
/// `is_daemon_internal` unit test in the lib crate; this is the RPC-surface half.
#[test]
fn ping_reports_build_id_and_idle_is_quiet() {
    use std::io::Write;
    let dir = sandbox("buildid");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::write(dir.join(".dl").join("prog.dl"),
        "rel t(a: text).\nt(\"x\").\n? t(a).\n").unwrap();

    let logf = fs::File::create(dir.join(".dl").join("daemon.log")).unwrap();
    let mut child = DaemonGuard(Command::new(DL)
        .args(["--daemon"])
        .current_dir(&dir).env("DL_DAEMON_ROOT", &dir)
        .stdout(Stdio::null()).stderr(Stdio::from(logf))
        .spawn().expect("spawn dl --daemon"));
    let sock = dir.join(".dl").join("daemon.sock");
    let mut ready = false;
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            ready = true; break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready, "daemon socket not ready");

    let ping = |id: u64| -> serde_json::Value {
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping","params":{{}}}}"#);
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let resp = read_frame(&mut s).expect("ping response");
        serde_json::from_str(&resp).expect("ping json")
    };

    let first = ping(1);
    let result = first.get("result").expect("ping result");
    let build_id = result.get("build_id").and_then(|v| v.as_str());
    assert!(build_id.is_some_and(|s| !s.is_empty()),
        "ping must report a non-empty build_id for the rebuild handshake: {first}");
    let t0 = result.get("tick_count").and_then(|v| v.as_u64()).expect("tick_count");

    std::thread::sleep(std::time::Duration::from_millis(2000));

    let t1 = ping(2).get("result").and_then(|r| r.get("tick_count"))
        .and_then(|v| v.as_u64()).expect("tick_count");
    assert!(t1 - t0 <= 1,
        "idle daemon self-ticked {} times in 2s (expected 0-1)", t1 - t0);

    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let body = r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#;
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    let _ = read_frame(&mut s);
    let status = child.wait_timeout(std::time::Duration::from_secs(5))
        .expect("daemon exit after shutdown");
    assert!(status.success());
}

/// P0: a root whose natural `.dl/daemon.sock` path exceeds the `sun_path` cap
/// must relocate the socket to a short hashed path, and the daemon must bind
/// there. Deterministic on the path derivation; the spawn confirms the bind.
#[test]
fn deep_root_uses_short_socket() {
    use sprefa_v5::daemon::socket_path;
    // A deeply nested root whose `<root>/.dl/daemon.sock` overruns 100 bytes.
    let deep = std::env::temp_dir().join(format!(
        "dl_deep_{}/{}",
        std::process::id(),
        "nested_component_padding_to_overflow_the_sun_path_limit_x".repeat(2),
    ));
    fs::create_dir_all(&deep).unwrap();
    let natural = deep.join(".dl").join("daemon.sock");
    assert!(natural.as_os_str().len() >= 100, "fixture root not deep enough: {}", natural.display());

    let sock = socket_path(Some(&deep));
    assert!(sock.as_os_str().len() < 100, "relocated socket still too long: {}", sock.display());
    assert!(sock.starts_with(std::env::temp_dir().join("dl-sock")),
        "relocated socket should live under the short base dir: {}", sock.display());
    assert_ne!(sock, natural, "deep root must not use the natural socket path");
    // Determinism: same root -> same short path (bind and connect must agree).
    assert_eq!(sock, socket_path(Some(&deep)), "short socket path must be stable");

    // The daemon binds the short socket and answers a ping.
    let _ = fs::remove_file(&sock);
    fs::write(deep.join("p.dl"),
        "rel mark(t: text).\nmark(\"a\").\n? mark(t).\n").unwrap();
    fs::create_dir_all(deep.join(".dl")).unwrap();
    let mut child = DaemonGuard(Command::new(DL)
        .args(["--daemon"])
        .arg(deep.join("p.dl"))
        .current_dir(&deep).env("DL_DAEMON_ROOT", &deep)
        .stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().expect("spawn dl --daemon on deep root"));
    let mut ready = false;
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            ready = true; break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready, "deep-root daemon never bound the short socket {}", sock.display());
    {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"shutdown","params":{}}"#;
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let _ = read_frame(&mut s);
    }
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
    let _ = fs::remove_dir_all(std::env::temp_dir().join(format!("dl_deep_{}", std::process::id())));
}

/// P1/P5: writes under a gitignored directory must not advance the tick count.
/// The gate mirrors the scan corpus, so `target/`-style build output the engine
/// would never scan produces no tick (and, by P5, no idle-timer reset).
#[test]
fn gitignored_writes_do_not_tick() {
    let dir = sandbox("noise");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::create_dir_all(dir.join("junk")).unwrap();
    fs::write(dir.join(".gitignore"), "junk/\n").unwrap();
    fs::write(dir.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.rs"), "fn a() {}\n").unwrap();

    let mut child = run_daemon_explicit(&dir);
    let sock = dir.join(".dl").join("daemon.sock");
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let tick = |id: u64| -> u64 {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping","params":{{}}}}"#);
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let resp = read_frame(&mut s).expect("ping response");
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        v["result"]["tick_count"].as_u64().expect("tick_count")
    };
    // Settle past the 1s startup grace and any cold tick.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t0 = tick(1);
    // 50 writes under the gitignored dir.
    for i in 0..50 {
        fs::write(dir.join(format!("junk/f{i}.o")), "build output").unwrap();
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t1 = tick(2);
    assert_eq!(t0, t1, "gitignored writes advanced the tick count ({t0} -> {t1})");

    {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#;
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let _ = read_frame(&mut s);
    }
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// P2: a burst of source edits coalesces into a single tick via the quiet-period
/// debounce, rather than one tick per file.
#[test]
fn source_burst_coalesces_into_one_tick() {
    let dir = sandbox("debounce");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    fs::write(dir.join("src/a.rs"), "fn a() {}\n").unwrap();

    let mut child = run_daemon_explicit(&dir);
    let sock = dir.join(".dl").join("daemon.sock");
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let tick = |id: u64| -> u64 {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping","params":{{}}}}"#);
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let resp = read_frame(&mut s).expect("ping response");
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        v["result"]["tick_count"].as_u64().expect("tick_count")
    };
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t0 = tick(1);
    // Five source files written in a tight loop (< the 120ms quiet period).
    for i in 0..5 {
        fs::write(dir.join(format!("src/b{i}.rs")), format!("fn b{i}() {{}}\n")).unwrap();
    }
    // Wait well past debounce + tick.
    std::thread::sleep(std::time::Duration::from_millis(2000));
    let t1 = tick(2);
    let delta = t1 - t0;
    assert!(delta >= 1, "burst produced no tick ({t0} -> {t1})");
    assert!(delta <= 2, "burst of 5 edits produced {delta} ticks; debounce should coalesce to 1 (~2 max)");

    {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#;
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let _ = read_frame(&mut s);
    }
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Scale test (macOS-runnable): a large gitignored subtree churning must
/// produce ZERO ticks, and a single real source edit buried in that churn must
/// still tick — proving the gate scales as a filter, not just as a blanket
/// suppressor. This is `gitignored_writes_do_not_tick` blown up ~40x with a
/// needle-in-haystack follow-up and timing output. It does NOT measure watcher
/// *count* — on macOS FSEvents is one stream per root regardless of tree size;
/// the per-directory inotify-watch explosion is the Linux test below.
#[test]
fn event_volume_gitignored_subtree_scale() {
    const DIRS: usize = 40;
    const FILES_PER_DIR: usize = 50; // 2000 gitignored files total

    let dir = sandbox("scale");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::write(dir.join(".gitignore"), "target/\n").unwrap();
    fs::write(dir.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.rs"), "fn a() {}\n").unwrap();
    // Pre-create the gitignored subtree dirs (dir creation itself is an event;
    // do it before the daemon starts so the burst below is pure file writes).
    for d in 0..DIRS {
        fs::create_dir_all(dir.join(format!("target/build/{d}"))).unwrap();
    }

    let mut child = run_daemon_explicit(&dir);
    let sock = dir.join(".dl").join("daemon.sock");
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let tick = |id: u64| -> u64 {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = format!(r#"{{"jsonrpc":"2.0","id":{id},"method":"ping","params":{{}}}}"#);
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let resp = read_frame(&mut s).expect("ping response");
        let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
        v["result"]["tick_count"].as_u64().expect("tick_count")
    };
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let t0 = tick(1);

    // Burst: 2000 writes across the gitignored subtree, timed.
    let start = std::time::Instant::now();
    for d in 0..DIRS {
        for f in 0..FILES_PER_DIR {
            fs::write(dir.join(format!("target/build/{d}/obj{f}.o")), "build output").unwrap();
        }
    }
    let write_ms = start.elapsed().as_millis();
    // Let every FSEvents batch drain through the gate.
    std::thread::sleep(std::time::Duration::from_millis(2500));
    let t1 = tick(2);
    eprintln!("[scale] wrote {} gitignored files in {write_ms}ms; tick delta {} (want 0)",
        DIRS * FILES_PER_DIR, t1 - t0);
    assert_eq!(t0, t1, "{} gitignored writes advanced the tick count ({t0} -> {t1})",
        DIRS * FILES_PER_DIR);

    // Needle in the haystack: one real source edit must still tick, proving the
    // gate KEEPS a scanned file rather than dropping the whole batch.
    fs::write(dir.join("src/needle.rs"), "fn needle() {}\n").unwrap();
    let mut ticked = false;
    for _ in 0..40 {
        if tick(3) > t1 { ticked = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(ticked, "a source edit buried in gitignored churn never ticked (gate over-dropped)");
    // Daemon still responsive after the storm.
    assert!(tick(4) >= t1, "daemon unresponsive after the churn storm");

    {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#;
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let _ = read_frame(&mut s);
    }
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// Scale test (Linux only, IGNORED until per-directory watch pruning lands):
/// the real "large amounts of watchers" problem. notify 6 recursive on Linux
/// installs one `inotify_add_watch` per subdirectory with no exclude hook
/// (`ignore` is not consulted), so a gitignored `target/` of N subdirs costs N
/// kernel watches the engine will never scan. This asserts the daemon's inotify
/// watch count tracks the UN-ignored directory count, not the whole tree.
///
/// It FAILS today by design (notify watches every dir) — that failure is the
/// spec for the deferred Linux per-dir pruning (walk with `ignore::WalkBuilder`,
/// `watch(dir, NonRecursive)` per surviving dir). Remove `#[ignore]` when that
/// lands. Reads `/proc/<pid>/fdinfo` to count `inotify wd:` lines.
#[cfg(target_os = "linux")]
#[test]
#[ignore = "fails until Linux per-directory watch pruning lands; this test is its spec"]
fn linux_inotify_watch_count_tracks_unignored() {
    const UNIGNORED_DIRS: usize = 20;
    const IGNORED_DIRS: usize = 2000;

    let dir = sandbox("inotify_count");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::write(dir.join(".gitignore"), "target/\n").unwrap();
    fs::write(dir.join("p.dl"), r#"rel seen(path: file, line: int).
seen(path, line) <- scan("WORK", "src/**/*.rs", path, rev),
  match(path, rev, /fn \w+/, line).
? seen(path, line).
"#).unwrap();
    for d in 0..UNIGNORED_DIRS {
        let sub = dir.join(format!("src/mod{d}"));
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("f.rs"), "fn f() {}\n").unwrap();
    }
    for d in 0..IGNORED_DIRS {
        fs::create_dir_all(dir.join(format!("target/build/{d}"))).unwrap();
    }

    let mut child = run_daemon_explicit(&dir);
    let sock = dir.join(".dl").join("daemon.sock");
    for _ in 0..100 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    // Give the recursive walk time to install every watch.
    std::thread::sleep(std::time::Duration::from_millis(2000));

    let pid = child.id();
    let watches = linux_inotify_watch_count(pid);

    {
        use std::io::Write;
        let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
        let body = r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#;
        write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        let _ = read_frame(&mut s);
    }
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));

    eprintln!("[inotify] {watches} watches for {UNIGNORED_DIRS} unignored + {IGNORED_DIRS} ignored dirs");
    // The gitignored subtree must NOT cost kernel watches. Allow generous slack
    // for root/.dl/.git/src parents; the point is it is O(unignored), not
    // O(total). Today's un-pruned watcher installs ~IGNORED_DIRS+ and trips this.
    assert!(watches < UNIGNORED_DIRS + 50,
        "inotify watch count {watches} scales with the ignored subtree; \
         per-dir pruning not in effect");
}

/// Count the inotify watches held by `pid` via `/proc/<pid>/fdinfo`. An inotify
/// fd's fdinfo lists one `inotify wd:...` line per installed watch; sum across
/// every inotify fd the process holds.
#[cfg(target_os = "linux")]
fn linux_inotify_watch_count(pid: u32) -> usize {
    let fd_dir = format!("/proc/{pid}/fd");
    let mut total = 0;
    let entries = match fs::read_dir(&fd_dir) { Ok(e) => e, Err(_) => return 0 };
    for entry in entries.flatten() {
        // An inotify fd's symlink target is "anon_inode:inotify".
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

/// P6: `--await-settle` blocks on the daemon until the program is quiescent. The
/// program emits an `sh` effect; the daemon drains it off-tick (poll loop), the
/// response lands, and the tick that observes no further motion flips `settled`.
/// The client returns `settled=true` and the response is queryable — proving the
/// daemon drove every cascade to a fixpoint (the daemon-side `--settle`).
#[test]
fn await_settle_blocks_until_effect_drains() {
    use std::io::Write;
    // Short tag: keep the canonical socket path under the sun_path cap so it
    // stays at `<root>/.dl/daemon.sock` (relocation is covered by the deep-root
    // test); this test's raw-socket query needs the natural path.
    let dir = sandbox("aws");
    fs::create_dir_all(dir.join(".dl")).unwrap();
    fs::write(dir.join("p.dl"),
        "sh greet(name) -> (line: text) = `echo hi-{name}`.\n\
         rel want(name: text).\n\
         want(\"bob\").\n\
         rel greet(name: text, line: text).\n\
         greet(name, line) <- @async want(name).\n\
         rel done(line: text).\n\
         done(line) <- greet(_, line).\n\
         ? done(line).\n").unwrap();

    // Fast poll so the off-tick drain happens within the test window.
    let logf = fs::File::create(dir.join(".dl").join("daemon.log")).unwrap();
    let mut child = DaemonGuard(Command::new(DL)
        .args(["--daemon"])
        .arg(dir.join("p.dl"))
        .current_dir(&dir).env("DL_DAEMON_ROOT", &dir)
        .env("DL_POLL_SECS", "1")
        .stdout(Stdio::null()).stderr(Stdio::from(logf))
        .spawn().expect("spawn dl --daemon"));
    let sock = dir.join(".dl").join("daemon.sock");
    let mut ready = false;
    for _ in 0..200 {
        if sock.exists() && std::os::unix::net::UnixStream::connect(&sock).is_ok() {
            ready = true; break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(ready, "daemon socket not ready; log:\n{}",
        fs::read_to_string(dir.join(".dl").join("daemon.log")).unwrap_or_default());

    // Block on quiescence via the CLI front-end (its own process, connecting to
    // the running daemon — no spawn).
    let out = Command::new(DL)
        .args(["--await-settle", "--await-settle-ms", "20000"])
        .current_dir(&dir).env("DL_DAEMON_ROOT", &dir)
        .output().expect("run dl --await-settle");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(0),
        "await-settle should exit 0 when quiescent: {stdout} {}",
        String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("settled=true"), "reports settled: {stdout}");

    // The effect response reached the derived rel — the cascade actually ran.
    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let body = r#"{"jsonrpc":"2.0","id":2,"method":"query","params":{}}"#;
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    let resp = read_frame(&mut s).expect("query response");
    assert!(resp.contains("hi-bob"), "the sh response drained into done: {resp}");

    let mut s = std::os::unix::net::UnixStream::connect(&sock).unwrap();
    let body = r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#;
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    let _ = read_frame(&mut s);
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

// ---------- helpers ----------

fn read_frame(s: &mut std::os::unix::net::UnixStream) -> Option<String> {
    use std::io::Read;
    let mut buf = Vec::new();
    // Read headers.
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

// wait_timeout extension (std doesn't ship one).
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
