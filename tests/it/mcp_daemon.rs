//! `dl --mcp` through the daemon: the adapter mode. A daemon owns the root's
//! engine; the --mcp process (spawned WITHOUT --no-daemon/--db) attaches and
//! pumps each request over the `mcp_request` RPC instead of opening its own
//! engine — one engine, one lock, warm between requests.

use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

const PROG: &str = r#"
rel req(id: text, method: text, params: text) @in(rpc).
rel resp(id: text, result: text) @out(rpc).
resp(id, "pong") <- req(id, "ping", _).
"#;

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("mcp_daemon_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(p.join(".dl")).unwrap();
    p
}

fn rpc(sock: &PathBuf, body: &str) -> String {
    let mut s = UnixStream::connect(sock).expect("connect daemon");
    write!(s, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
    // Read one Content-Length frame back.
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if s.read(&mut byte).unwrap() == 0 { panic!("daemon closed"); }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") { break; }
    }
    let header = String::from_utf8_lossy(&buf);
    let len: usize = header.lines()
        .find_map(|l| l.strip_prefix("Content-Length:"))
        .expect("Content-Length").trim().parse().unwrap();
    let mut body = vec![0u8; len];
    s.read_exact(&mut body).unwrap();
    String::from_utf8(body).unwrap()
}

/// Spawn a foreground daemon on the sandbox, run `dl p.dl --mcp` (no
/// --no-daemon, no --db) against it, and check the request was answered AND
/// pumped through the daemon (its tick counter moves).
#[test]
fn mcp_pumps_through_daemon() {
    let dir = sandbox("pump");
    fs::write(dir.join("p.dl"), PROG).unwrap();
    let mut daemon = DaemonGuard(Command::new(DL)
        .args(["--daemon"]).arg("--root").arg(&dir).arg(dir.join("p.dl"))
        .stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().expect("spawn daemon"));

    let sock = dir.join(".dl").join("daemon.sock");
    let mut up = false;
    for _ in 0..100 {
        if sock.exists()
            && rpc_try(&sock, r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#).is_some() {
            up = true; break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(up, "daemon socket not ready");
    let ticks_before = tick_count(&sock);

    // The adapter: attaches to the running daemon (enabled_for = .dl exists,
    // no --db), so the answer must come from the daemon's engine.
    let mut mcp = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--mcp", "--root", dir.to_str().unwrap()])
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().expect("spawn dl --mcp");
    {
        let stdin = mcp.stdin.as_mut().unwrap();
        writeln!(stdin, r#"{{"jsonrpc":"2.0","id":41,"method":"ping"}}"#).unwrap();
    }
    let out = mcp.wait_with_output().expect("dl --mcp exit");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(!stderr.contains("in-process"), "must attach to the daemon, got: {stderr}");
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one JSON response");
    assert_eq!(resp["id"], serde_json::json!(41), "{stdout}");
    assert_eq!(resp["result"], serde_json::json!("pong"), "{stdout}");

    // The daemon did the work: its tick counter advanced.
    assert!(tick_count(&sock) > ticks_before,
            "daemon tick_count must advance when it pumps the request");

    let _ = rpc(&sock, r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#);
    let _ = daemon.wait();
}

/// mcp_request against a rel that is NOT a port in the daemon's program is
/// refused (the drift guard), surfacing as an error on the adapter's request.
#[test]
fn daemon_refuses_non_port_rel() {
    let dir = sandbox("refuse");
    fs::write(dir.join("p.dl"), "rel plain(x: text).\nplain(\"a\").\n").unwrap();
    let mut daemon = DaemonGuard(Command::new(DL)
        .args(["--daemon"]).arg("--root").arg(&dir).arg(dir.join("p.dl"))
        .stdout(Stdio::null()).stderr(Stdio::null())
        .spawn().expect("spawn daemon"));
    let sock = dir.join(".dl").join("daemon.sock");
    for _ in 0..100 {
        if sock.exists()
            && rpc_try(&sock, r#"{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}"#).is_some() { break; }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let resp = rpc(&sock, r#"{"jsonrpc":"2.0","id":2,"method":"mcp_request","params":{"in_rel":"plain","out_rel":"plain","id":"1","method":"ping","params":"null"}}"#);
    assert!(resp.contains("error") && resp.contains("not an @in(rpc) port"),
            "injection into a non-port rel must be refused: {resp}");
    let _ = rpc(&sock, r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#);
    let _ = daemon.wait();
}

fn rpc_try(sock: &PathBuf, body: &str) -> Option<String> {
    std::panic::catch_unwind(|| rpc(sock, body)).ok()
}

fn tick_count(sock: &PathBuf) -> u64 {
    let resp = rpc(sock, r#"{"jsonrpc":"2.0","id":5,"method":"ping","params":{}}"#);
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    v["result"]["tick_count"].as_u64().unwrap_or(0)
}
