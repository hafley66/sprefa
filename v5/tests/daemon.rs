//! End-to-end daemon lifecycle tests. Drives the compiled `dl` binary through
//! spawn-if-missing, ping, query, diag, shutdown. Each test uses a fresh tempdir
//! so the daemon socket never collides across files.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("dl_daemon_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

fn run_daemon_explicit(dir: &PathBuf) -> std::process::Child {
    // Spawn the daemon in foreground; test reads stderr / connects via socket.
    Command::new(DL)
        .args(["--daemon"]).arg("--root").arg(dir)
        .arg(dir.join("p.dl"))
        .spawn().expect("spawn dl --daemon")
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
    let out = Command::new(DL).arg("--stop").arg("--root").arg(&dir)
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
        .arg("--root").arg(&dir)
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
