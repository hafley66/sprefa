//! `dl --mcp` through the singleton daemon: the adapter attaches to the singleton
//! (XDG-homed), registers its root, and pumps each request over `mcp_request`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util::DaemonGuard;

const DL: &str = env!("CARGO_BIN_EXE_dl");

const PROG: &str = r#"
rel req(id: text, method: text, params: text) @in(rpc).
rel resp(id: text, result: text) @out(rpc).
resp(id, "pong") <- req(id, "ping", _).
"#;

struct Sandbox {
    home: PathBuf,
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("mcp_daemon_{tag}_{}", std::process::id()));
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
    fn spawn(&self, program: &Path) -> DaemonGuard {
        DaemonGuard(
            Command::new(DL)
                .args(["daemon", "start", "--foreground"])
                .arg(program)
                .current_dir(&self.root)
                .env("DL_DAEMON_ROOT", &self.root)
                .env("XDG_STATE_HOME", &self.home)
                .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn daemon"),
        )
    }
    fn wait_ready(&self) -> bool {
        for _ in 0..200 {
            if self.sock().exists() && rpc_root(&self.sock(), 1, "ping", &self.root).is_some() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        false
    }
}

fn rpc(sock: &Path, body: &str) -> String {
    crate::util::uds_rpc(sock, body).expect("daemon rpc over the uds socket")
}

fn rpc_root(sock: &Path, id: u64, method: &str, root: &Path) -> Option<serde_json::Value> {
    let body = serde_json::json!({"jsonrpc":"2.0","id":id,"method":method,
        "params":{"root": root.to_string_lossy()}})
    .to_string();
    std::panic::catch_unwind(|| rpc(sock, &body))
        .ok()
        .and_then(|r| serde_json::from_str(&r).ok())
}

fn tick_count(sock: &Path, root: &Path) -> u64 {
    rpc_root(sock, 5, "ping", root)
        .map(|v| v["result"]["tick_count"].as_u64().unwrap_or(0))
        .unwrap_or(0)
}

/// The adapter attaches to the running singleton and pumps the request through
/// its warm engine (its per-root tick counter advances).
#[test]
fn mcp_pumps_through_daemon() {
    let sb = Sandbox::new("pump");
    fs::write(sb.root.join("p.dl"), PROG).unwrap();
    let mut daemon = sb.spawn(&sb.root.join("p.dl"));
    assert!(sb.wait_ready(), "daemon not ready");
    let ticks_before = tick_count(&sb.sock(), &sb.root);

    let mut mcp = Command::new(DL)
        .arg(sb.root.join("p.dl"))
        .arg("--mcp")
        .current_dir(&sb.root)
        .env("DL_DAEMON_ROOT", &sb.root)
        .env("XDG_STATE_HOME", &sb.home)
        .env("SPREFA_CONFIG", "/nonexistent/sprefa-hermetic.toml")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn dl --mcp");
    {
        use std::io::Write;
        let stdin = mcp.stdin.as_mut().unwrap();
        writeln!(stdin, r#"{{"jsonrpc":"2.0","id":41,"method":"ping"}}"#).unwrap();
    }
    let out = mcp.wait_with_output().expect("dl --mcp exit");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(0), "{stderr}");
    assert!(
        !stderr.contains("in-process"),
        "must attach to the daemon, got: {stderr}"
    );
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one JSON response");
    assert_eq!(resp["id"], serde_json::json!(41), "{stdout}");
    assert_eq!(resp["result"], serde_json::json!("pong"), "{stdout}");
    assert!(
        tick_count(&sb.sock(), &sb.root) > ticks_before,
        "daemon tick_count must advance when it pumps the request"
    );

    let _ = rpc(
        &sb.sock(),
        r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#,
    );
    let _ = daemon.wait();
}

/// mcp_request against a rel that is NOT a port in the daemon's program is refused.
#[test]
fn daemon_refuses_non_port_rel() {
    let sb = Sandbox::new("refuse");
    fs::write(sb.root.join("p.dl"), "rel plain(x: text).\nplain(\"a\").\n").unwrap();
    let mut daemon = sb.spawn(&sb.root.join("p.dl"));
    assert!(sb.wait_ready(), "daemon not ready");
    let body = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"mcp_request","params":{
        "root": sb.root.to_string_lossy(),
        "in_rel":"plain","out_rel":"plain","id":"1","method":"ping","params":"null"}})
    .to_string();
    let resp = rpc(&sb.sock(), &body);
    assert!(
        resp.contains("error") && resp.contains("not an @in(rpc) port"),
        "injection into a non-port rel must be refused: {resp}"
    );
    let _ = rpc(
        &sb.sock(),
        r#"{"jsonrpc":"2.0","id":9,"method":"shutdown","params":{}}"#,
    );
    let _ = daemon.wait();
}
