//! Access-log tests for both daemon request surfaces (deliverable 2b): the
//! axum HTTP transport (`src/daemon_shell/http.rs`) and the UDS socket
//! (`src/daemon_shell/uds.rs`). Both mint a request id (`src/reqid.rs`) and
//! log one `[access]` line via `daemon::log_access` into the SAME rolling
//! `dl.log` deliverable 2 installs — this asserts that line actually lands,
//! carrying the request id, for one request on each surface.
//!
//! HERMETICITY: mirrors `tests/it/daemon_http.rs`'s `Sandbox` — an isolated
//! `XDG_STATE_HOME` per test and `SPREFA_CONFIG` pointed at a nonexistent
//! file, so nothing here can touch a developer's real daemon or state.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

struct Sandbox {
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("dl_access_log_{tag}_{}", std::process::id()));
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

    fn dl_log(&self) -> PathBuf {
        self.home.join("sprefa").join("log").join("dl.log")
    }

    fn spawn(&self, program: &Path) -> DaemonGuard {
        let mut cmd = Command::new(DL);
        cmd.args(["daemon", "start", "--foreground"])
            .arg(program)
            .current_dir(&self.root)
            .env("DL_DAEMON_ROOT", &self.root)
            .env("XDG_STATE_HOME", &self.home)
            .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
            .env("DL_LOG", "info")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        DaemonGuard(cmd.spawn().expect("spawn singleton daemon"))
    }

    fn wait_ready(&self) -> bool {
        for _ in 0..200 {
            if self.sock().exists()
                && rpc_root(&self.sock(), 1, "ping", &self.root, serde_json::json!({})).is_some()
                && self.http_json().exists()
            {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn base_url(&self) -> String {
        let text = fs::read_to_string(self.http_json()).expect("http.json present");
        let info: serde_json::Value = serde_json::from_str(&text).unwrap();
        let port = info["port"].as_u64().expect("port in http.json");
        format!("http://127.0.0.1:{port}")
    }

    fn shutdown(&self) {
        let _ = rpc(
            &self.sock(),
            r#"{"jsonrpc":"2.0","id":9000,"method":"shutdown","params":{}}"#,
        );
    }

    /// Poll `dl.log` until `needle` appears, or give up after `timeout`.
    fn wait_for_log_line(&self, needle: &str, timeout: Duration) -> Option<String> {
        let start = std::time::Instant::now();
        loop {
            if let Ok(text) = fs::read_to_string(self.dl_log()) {
                if text.contains(needle) {
                    return Some(text);
                }
            }
            if start.elapsed() > timeout {
                return None;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

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

fn http_post(base: &str, path: &str, body: &str) -> Option<(u16, String)> {
    let addr = base.strip_prefix("http://")?;
    let mut stream = TcpStream::connect(addr).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
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

const EDGE_PROGRAM: &str =
    "rel edge(a: text, b: text).\nedge(\"a\", \"b\").\nedge(\"b\", \"c\").\n? edge(a, b).\n";

/// One `POST /rpc` on the HTTP surface produces an `[access]` line in
/// `dl.log` carrying `surface=http` and a non-empty `req_id`.
#[test]
fn http_request_logs_an_access_line_with_a_request_id() {
    let sb = Sandbox::new("http");
    fs::write(sb.root.join("p.dl"), EDGE_PROGRAM).unwrap();
    let mut child = sb.spawn(&sb.root.join("p.dl"));
    assert!(sb.wait_ready(), "daemon not ready");

    let body = serde_json::json!({
        "jsonrpc": "2.0", "id": 5, "method": "query",
        "params": { "root": sb.root.to_string_lossy() },
    })
    .to_string();
    let (status, resp) = http_post(&sb.base_url(), "/rpc", &body).expect("http query");
    assert_eq!(status, 200, "query should succeed: {resp}");

    let log = sb
        .wait_for_log_line("surface=\"http\"", Duration::from_secs(5))
        .expect("dl.log should carry an http access line");
    assert!(
        log.contains("[access]"),
        "access line marker present:\n{log}"
    );
    assert!(log.contains("method=\"query\""), "method recorded:\n{log}");
    assert!(log.contains("req_id=\""), "req_id field present:\n{log}");
    // The req_id value itself must be non-empty: find the field and check it
    // is not immediately followed by whitespace/end-of-line.
    let after = log.split("req_id=\"").nth(1).unwrap_or("");
    let id_value = after.split('"').next().unwrap_or("");
    assert!(
        !id_value.is_empty(),
        "req_id must carry an actual id:\n{log}"
    );

    sb.shutdown();
    let _ = child.wait_timeout(Duration::from_secs(5));
}

/// One socket RPC (`ping`) produces an `[access]` line in `dl.log` carrying
/// `surface=sock` and a non-empty `req_id`.
#[test]
fn socket_request_logs_an_access_line_with_a_request_id() {
    let sb = Sandbox::new("sock");
    fs::write(sb.root.join("p.dl"), EDGE_PROGRAM).unwrap();
    let mut child = sb.spawn(&sb.root.join("p.dl"));
    assert!(sb.wait_ready(), "daemon not ready");

    let resp = rpc_root(&sb.sock(), 42, "status", &sb.root, serde_json::json!({}));
    assert!(resp.is_some(), "status RPC should answer");

    let log = sb
        .wait_for_log_line("surface=\"sock\"", Duration::from_secs(5))
        .expect("dl.log should carry a socket access line");
    assert!(
        log.contains("[access]"),
        "access line marker present:\n{log}"
    );
    assert!(
        log.contains("method=\"status\"") || log.contains("method=\"ping\""),
        "method recorded (status, or the readiness ping):\n{log}"
    );
    assert!(log.contains("req_id=\""), "req_id field present:\n{log}");

    sb.shutdown();
    let _ = child.wait_timeout(Duration::from_secs(5));
}

trait WaitTimeoutExt {
    fn wait_timeout(&mut self, dur: Duration) -> Option<std::process::ExitStatus>;
}
impl WaitTimeoutExt for std::process::Child {
    fn wait_timeout(&mut self, dur: Duration) -> Option<std::process::ExitStatus> {
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
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => return None,
            }
        }
    }
}
