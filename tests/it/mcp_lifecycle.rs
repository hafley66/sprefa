//! The repeatable MCP lifecycle harness: drives the REAL examples/mcp-server.dl
//! through the protocol handshake a real client performs — initialize ->
//! notifications/initialized -> tools/list -> tools/call — over stdio, exactly
//! as `claude mcp add dl-demo -- dl examples/mcp-server.dl --mcp` would (the
//! harness adds --no-daemon for hermetic runs; tests/it/mcp_daemon.rs covers
//! the daemon-attached mode). Registering in Claude Code and re-testing by
//! hand is replaced by `cargo test --test it mcp_lifecycle`.
//!
//! Also pins the id contract: the rpc envelope id is the request id's raw JSON
//! text, so integer AND string JSON-RPC ids round-trip exactly (real MCP
//! clients use both).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn example(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples").join(name)
}

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mcp_lc_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Serve `prog` with `--mcp --no-daemon`, feed the ndjson `requests`, close
/// stdin, collect (exit code, stdout messages as JSON, stderr). The harness
/// entry point — any MCP conversation is a request list.
fn serve(dir: &Path, prog: &Path, requests: &[&str]) -> (i32, Vec<serde_json::Value>, String) {
    let mut child = Command::new(DL)
        .arg(prog)
        .args(["--mcp", "--no-daemon",
               "--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().expect("spawn dl --mcp");
    {
        let stdin = child.stdin.as_mut().unwrap();
        for r in requests {
            writeln!(stdin, "{r}").unwrap();
        }
    } // drop closes stdin -> EOF ends the serve loop
    let out = child.wait_with_output().expect("dl --mcp exit");
    let msgs = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("stdout line is JSON"))
        .collect();
    (out.status.code().unwrap_or(-1), msgs,
     String::from_utf8_lossy(&out.stderr).into_owned())
}

fn by_id<'a>(msgs: &'a [serde_json::Value], id: serde_json::Value) -> &'a serde_json::Value {
    msgs.iter().find(|m| m.get("id") == Some(&id))
        .unwrap_or_else(|| panic!("no response with id {id}: {msgs:?}"))
}

/// The full client handshake against examples/mcp-server.dl. One process, the
/// four-message sequence a real MCP client opens with, every response checked.
#[test]
fn full_lifecycle_handshake() {
    let d = sandbox("handshake");
    let (code, msgs, err) = serve(&d, &example("mcp-server.dl"), &[
        r#"{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"harness","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ping","arguments":{}}}"#,
    ]);
    assert_eq!(code, 0, "{err}");
    // 4 messages in, 3 requests -> exactly 3 responses (the notification is
    // silent), every one a result, none an error.
    assert_eq!(msgs.len(), 3, "{msgs:?}");
    assert!(msgs.iter().all(|m| m.get("error").is_none()), "no errors expected: {msgs:?}");

    let init = by_id(&msgs, serde_json::json!("init-1"));
    assert_eq!(init["result"]["protocolVersion"], serde_json::json!("2024-11-05"));
    assert_eq!(init["result"]["serverInfo"]["name"], serde_json::json!("dl"));

    let tools = by_id(&msgs, serde_json::json!(2));
    let names: Vec<&str> = tools["result"]["tools"].as_array().expect("tools array")
        .iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(names, vec!["ping"], "{tools}");

    let call = by_id(&msgs, serde_json::json!(3));
    assert_eq!(call["result"]["content"][0]["text"], serde_json::json!("pong"), "{call}");
}

/// Integer and string ids round-trip exactly — the response id is the value
/// the client sent, with its JSON type intact.
#[test]
fn int_and_string_ids_round_trip() {
    let d = sandbox("ids");
    let (code, msgs, err) = serve(&d, &example("mcp-server.dl"), &[
        r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":"abc","method":"ping"}"#,
    ]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(msgs.len(), 2, "{msgs:?}");
    assert!(by_id(&msgs, serde_json::json!(7))["id"].is_number());
    assert!(by_id(&msgs, serde_json::json!("abc"))["id"].is_string());
}

/// A notification (no id) alone produces no output at all and a clean exit.
#[test]
fn notification_is_silent() {
    let d = sandbox("notif");
    let (code, msgs, err) = serve(&d, &example("mcp-server.dl"),
        &[r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#]);
    assert_eq!(code, 0, "{err}");
    assert!(msgs.is_empty(), "a notification must get no response: {msgs:?}");
}

/// A method outside the program's handler table still gets a response
/// (-32601), with the client's string id intact — no hang, no dropped id.
#[test]
fn unhandled_method_errors_with_id_intact() {
    let d = sandbox("unhandled");
    let (code, msgs, err) = serve(&d, &example("mcp-server.dl"),
        &[r#"{"jsonrpc":"2.0","id":"x-9","method":"resources/list"}"#]);
    assert_eq!(code, 0, "{err}");
    let m = by_id(&msgs, serde_json::json!("x-9"));
    assert_eq!(m["error"]["code"], serde_json::json!(-32601), "{m}");
}
