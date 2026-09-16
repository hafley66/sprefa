//! End-to-end tests for the daemon's HTTP/JSON transport
//! (`src/daemon_shell/http.rs`): ONE axum router served over both the
//! localhost TCP door and the UDS socket (which carries HTTP too since the
//! axum adoption arc).
//!
//! HERMETICITY: same sandbox contract as `daemon.rs` — every test points
//! `XDG_STATE_HOME` at its own temp dir so the singleton's home (socket, pid,
//! http.json, per-root dbs) can never touch a developer's real daemon, and
//! `SPREFA_CONFIG` points at a nonexistent file so no ambient config repos load.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

/// An isolated XDG home + a repo root to serve (mirrors `daemon.rs::Sandbox`).
struct Sandbox {
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dl_http_{tag}_{}", std::process::id()));
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

    fn http_json(&self) -> PathBuf {
        self.home.join("sprefa").join("http.json")
    }

    /// Spawn a foreground singleton serving `self.root`.
    fn spawn(&self, program: &Path) -> DaemonGuard {
        let mut cmd = Command::new(DL);
        cmd.args(["daemon", "start", "--foreground"])
            .arg(program)
            .current_dir(&self.root)
            .env("DL_DAEMON_ROOT", &self.root)
            .env("XDG_STATE_HOME", &self.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        DaemonGuard(cmd.spawn().expect("spawn singleton daemon"))
    }

    /// Wait until the UDS socket answers a root ping AND http.json is published.
    fn wait_ready(&self) -> bool {
        for _ in 0..200 {
            if self.sock().exists()
                && rpc_root(&self.sock(), 1, "ping", &self.root, serde_json::json!({})).is_some()
                && self.http_json().exists()
            {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }

    /// `http://127.0.0.1:<port>` read from the published http.json.
    fn base_url(&self) -> String {
        let info = read_http_json(&self.http_json()).expect("http.json present");
        let port = info["port"].as_u64().expect("port in http.json");
        format!("http://127.0.0.1:{port}")
    }

    fn shutdown(&self) {
        let _ = rpc(
            &self.sock(),
            r#"{"jsonrpc":"2.0","id":9000,"method":"shutdown","params":{}}"#,
        );
    }
}

fn read_http_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

// ---------- minimal HTTP/1.1 client (no dep; Connection: close) ----------

/// One request/response over a fresh TCP connection. Returns (status, body).
fn http_request(base: &str, method: &str, path: &str, body: Option<&str>) -> Option<(u16, String)> {
    let addr = base.strip_prefix("http://")?;
    let mut stream = TcpStream::connect(addr).ok()?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .ok()?;
    let body = body.unwrap_or("");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(req.as_bytes()).ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let text = String::from_utf8_lossy(&raw);
    let status: u16 = text.split_whitespace().nth(1)?.parse().ok()?;
    let body_start = text.find("\r\n\r\n")? + 4;
    Some((status, text[body_start..].to_string()))
}

fn http_get(base: &str, path: &str) -> Option<(u16, String)> {
    http_request(base, "GET", path, None)
}
fn http_post(base: &str, path: &str, body: &str) -> Option<(u16, String)> {
    http_request(base, "POST", path, Some(body))
}

// ---------- UDS HTTP RPC (mirrors daemon.rs test helpers) ----------

fn rpc(sock: &Path, body: &str) -> Option<String> {
    crate::util::uds_rpc(sock, body)
}

fn rpc_root(
    sock: &Path,
    id: u64,
    method: &str,
    root: &Path,
    mut params: serde_json::Value,
) -> Option<serde_json::Value> {
    params["root"] = serde_json::json!(root.to_string_lossy());
    let body =
        serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string();
    let resp = rpc(sock, &body)?;
    serde_json::from_str(&resp).ok()
}

const EDGE_PROGRAM: &str =
    "rel edge(a: text, b: text).\nedge(\"a\", \"b\").\nedge(\"b\", \"c\").\n? edge(a, b).\n";

/// (a) http.json is published with a LIVE port while the daemon serves, and is
/// deleted once the daemon stops.
#[test]
fn http_json_published_then_removed_on_stop() {
    let sb = Sandbox::new("lifecycle");
    fs::write(sb.root.join("p.dl"), EDGE_PROGRAM).unwrap();
    let mut child = sb.spawn(&sb.root.join("p.dl"));
    assert!(
        sb.wait_ready(),
        "daemon not ready / http.json never published"
    );

    let info = read_http_json(&sb.http_json()).expect("http.json present");
    assert!(
        info["port"].as_u64().is_some_and(|p| p > 0),
        "http.json has a live port: {info}"
    );
    assert_eq!(
        info["pid"].as_u64(),
        Some(child.id() as u64),
        "http.json pid = daemon pid: {info}"
    );
    assert!(
        info["build_id"].as_str().is_some_and(|s| !s.is_empty()),
        "http.json build_id: {info}"
    );

    // The published port actually answers.
    let (status, _) = http_get(&sb.base_url(), "/health").expect("health over the published port");
    assert_eq!(status, 200, "published port must serve /health");

    sb.shutdown();
    let st = child
        .wait_timeout(std::time::Duration::from_secs(5))
        .expect("daemon exit");
    assert!(st.success(), "daemon exits 0 on shutdown");
    assert!(
        !sb.http_json().exists(),
        "http.json removed by shutdown_cleanup"
    );
}

/// (b) POST /rpc round-trips one existing request and its result equals the UDS
/// path's result for the same request.
#[test]
fn rpc_over_http_equals_uds_result() {
    let sb = Sandbox::new("parity");
    fs::write(sb.root.join("p.dl"), EDGE_PROGRAM).unwrap();
    let mut child = sb.spawn(&sb.root.join("p.dl"));
    assert!(sb.wait_ready(), "not ready");

    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 5, "method": "query",
        "params": { "root": sb.root.to_string_lossy() },
    })
    .to_string();

    let uds = rpc(&sb.sock(), &body).expect("uds query");
    let uds_v: serde_json::Value = serde_json::from_str(&uds).unwrap();

    let (status, http_raw) = http_post(&sb.base_url(), "/rpc", &body).expect("http query");
    assert_eq!(status, 200, "executed RPC returns 200: {http_raw}");
    let http_v: serde_json::Value = serde_json::from_str(&http_raw).expect("http body is json");

    assert!(
        http_v["result"]["results"][0]["rows"]
            .as_array()
            .is_some_and(|r| r.len() == 2),
        "query returned the two edge rows over http: {http_v}"
    );
    assert_eq!(uds_v["result"], http_v["result"],
        "http /rpc result must equal the uds result for the same request\nuds={uds_v}\nhttp={http_v}");

    // A body that does not deserialize to a request is a 400 with a JSON error.
    let (bad_status, bad_body) = http_post(&sb.base_url(), "/rpc", "not json").expect("bad post");
    assert_eq!(bad_status, 400, "undeserializable body is 400: {bad_body}");
    assert!(
        serde_json::from_str::<serde_json::Value>(&bad_body).is_ok(),
        "400 body is json: {bad_body}"
    );

    // The retired `subscribe` method: a clean JSON error naming `GET /watch`,
    // the SSE stream that replaced the kept-open-socket push.
    let sub =
        serde_json::json!({"jsonrpc":"2.0","id":6,"method":"subscribe","params":{}}).to_string();
    let (sub_status, sub_body) = http_post(&sb.base_url(), "/rpc", &sub).expect("subscribe post");
    assert_eq!(
        sub_status, 400,
        "the retired subscribe method is rejected: {sub_body}"
    );
    let sub_v: serde_json::Value = serde_json::from_str(&sub_body).expect("json error");
    let msg = sub_v["error"]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("/watch"),
        "subscribe rejection must point at GET /watch: {sub_body}"
    );

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// (c) GET /health answers while another thread keeps the engine mutex busy.
/// A background loop hammers `query` (each acquisition takes `lock_eng`); the
/// main thread fires many /health calls in that window and every one must
/// succeed — /health only touches the roots-map lock, never `lock_eng`.
#[test]
fn health_answers_under_engine_lock_contention() {
    let sb = Sandbox::new("lockfree");
    // A recursive closure so each `query` does real work under the engine lock.
    fs::write(
        sb.root.join("p.dl"),
        "rel edge(a: text, b: text).\n\
         edge(\"a\", \"b\").\nedge(\"b\", \"c\").\nedge(\"c\", \"d\").\nedge(\"d\", \"e\").\n\
         rel reach(a: text, b: text).\n\
         reach(x, y) <- edge(x, y).\n\
         reach(x, z) <- edge(x, y), reach(y, z).\n\
         ? reach(x, y).\n",
    )
    .unwrap();
    let mut child = sb.spawn(&sb.root.join("p.dl"));
    assert!(sb.wait_ready(), "not ready");

    let base = sb.base_url();
    let sock = sb.sock();
    let root = sb.root.clone();

    // Background thread: keep the engine lock churning for a wall-clock window.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_bg = stop.clone();
    let bg = std::thread::spawn(move || {
        let mut n = 0u64;
        while !stop_bg.load(std::sync::atomic::Ordering::Relaxed) {
            n += 1;
            let _ = rpc_root(&sock, 1000 + n, "query", &root, serde_json::json!({}));
        }
        n
    });

    // Race /health against the query loop: every call must answer 200 with a
    // well-formed payload, mid-contention.
    let mut ok = 0;
    for _ in 0..60 {
        let (status, body) = http_get(&base, "/health").expect("health under contention");
        assert_eq!(
            status, 200,
            "health must stay 200 under engine-lock contention: {body}"
        );
        let v: serde_json::Value = serde_json::from_str(&body).expect("health json");
        assert!(
            v["build_id"].as_str().is_some_and(|s| !s.is_empty()),
            "health build_id: {v}"
        );
        assert!(v["pid"].as_u64().is_some(), "health pid: {v}");
        assert!(v["roots"].as_u64().is_some(), "health roots count: {v}");
        ok += 1;
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(
        ok, 60,
        "all /health calls answered while the engine lock was contended"
    );

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = bg.join();
    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// The D-arc property survives the tokio shell swap: 50 CONCURRENT `GET /health`
/// issued while a background thread keeps the engine mutex churning must ALL
/// answer 200. Under the tokio shell, engine work runs on the `spawn_blocking`
/// pool, so the shell's worker threads stay free to serve `/health` (roots-map
/// lock only) — none of the 50 block behind the held engine mutex.
#[test]
fn fifty_concurrent_health_all_answer_under_engine_lock() {
    let sb = Sandbox::new("health50");
    // A recursive closure so each `query` does real work under the engine lock.
    fs::write(
        sb.root.join("p.dl"),
        "rel edge(a: text, b: text).\n\
         edge(\"a\", \"b\").\nedge(\"b\", \"c\").\nedge(\"c\", \"d\").\nedge(\"d\", \"e\").\n\
         rel reach(a: text, b: text).\n\
         reach(x, y) <- edge(x, y).\n\
         reach(x, z) <- edge(x, y), reach(y, z).\n\
         ? reach(x, y).\n",
    )
    .unwrap();
    let mut child = sb.spawn(&sb.root.join("p.dl"));
    assert!(sb.wait_ready(), "not ready");

    let base = sb.base_url();
    let sock = sb.sock();
    let root = sb.root.clone();

    // Background thread: keep the engine lock churning for the whole window.
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_bg = stop.clone();
    let bg = std::thread::spawn(move || {
        let mut n = 0u64;
        while !stop_bg.load(std::sync::atomic::Ordering::Relaxed) {
            n += 1;
            let _ = rpc_root(&sock, 2000 + n, "query", &root, serde_json::json!({}));
        }
    });
    // Give the churn thread a moment to actually be holding the lock in a loop.
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Fire 50 /health calls CONCURRENTLY (one thread each) mid-contention.
    let mut handles = Vec::new();
    for _ in 0..50 {
        let b = base.clone();
        handles.push(std::thread::spawn(move || match http_get(&b, "/health") {
            Some((200, body)) => serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v["build_id"].as_str().map(|s| !s.is_empty()))
                .unwrap_or(false),
            _ => false,
        }));
    }
    let ok = handles
        .into_iter()
        .map(|h| h.join().unwrap_or(false))
        .filter(|&answered| answered)
        .count();

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = bg.join();
    assert_eq!(
        ok, 50,
        "all 50 concurrent /health calls must answer 200 while the engine mutex is held"
    );

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

/// `GET /watch` over the UDS socket streams a `diag_changed` SSE event when a
/// tick broadcasts. The trigger is a watched `load` (its reload path always
/// broadcasts after the re-tick), so the test is deterministic — no watcher
/// timing. This is the e2e witness for the subscribe-method replacement.
#[test]
fn watch_sse_streams_diag_changed_over_uds() {
    let sb = Sandbox::new("watchsse");
    fs::write(sb.root.join("p.dl"), EDGE_PROGRAM).unwrap();
    let mut child = sb.spawn(&sb.root.join("p.dl"));
    assert!(sb.wait_ready(), "not ready");

    // Open the SSE stream on the UDS door (kept open; chunked body).
    let mut stream = std::os::unix::net::UnixStream::connect(sb.sock()).expect("connect uds");
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(500)))
        .unwrap();
    stream
        .write_all(b"GET /watch HTTP/1.1\r\nHost: dl-daemon\r\nAccept: text/event-stream\r\n\r\n")
        .expect("send watch request");

    // Trigger a broadcast: a watched `load` re-ticks and always pushes
    // `diag_changed` after the reload.
    fs::write(
        sb.root.join("extra.dl"),
        "rel extra_mark(t: text).\nextra_mark(\"x\").\n",
    )
    .unwrap();
    let load = serde_json::json!({
        "jsonrpc": "2.0", "id": 7, "method": "load",
        "params": {
            "root": sb.root.to_string_lossy(),
            "path": sb.root.join("extra.dl").to_string_lossy(),
            "mode": "watched",
        },
    })
    .to_string();
    let load_resp = rpc(&sb.sock(), &load).expect("load rpc");
    assert!(
        load_resp.contains("result"),
        "watched load must succeed: {load_resp}"
    );

    // Accumulate the stream until the pushed notification shows up (bounded).
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut seen = String::new();
    let mut chunk = [0u8; 4096];
    while std::time::Instant::now() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => seen.push_str(&String::from_utf8_lossy(&chunk[..n])),
            Err(_) => {} // read timeout slice; keep polling until the deadline
        }
        if seen.contains("diag_changed") {
            break;
        }
    }
    assert!(seen.contains("200"), "watch stream answers 200: {seen}");
    assert!(
        seen.contains("data:"),
        "watch stream carries SSE data events: {seen}"
    );
    assert!(
        seen.contains("diag_changed"),
        "a watched load's re-tick must push diag_changed to /watch subscribers: {seen}"
    );

    sb.shutdown();
    let _ = child.wait_timeout(std::time::Duration::from_secs(5));
}

// ---------- child wait helper (mirrors daemon.rs) ----------

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
