//! `dl --mcp` e2e: a JSON-RPC stdio server authored as datalog.
//!
//! The program declares its boundary as ports — `@in(rpc)` receives the
//! request envelope (id, method, params), `@out(rpc)` yields (id, result) —
//! and `--mcp` binds the rpc ports to stdio x jsonrpc. Requests are written as
//! newline-delimited JSON on stdin; stdin closes; every request must have a
//! response on stdout (an unanswered id gets -32601, so a client never hangs).
//! Drain law 1: an answered @in row is retired, so a later request can't be
//! re-answered by a stale one.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const DL: &str = env!("CARGO_BIN_EXE_dl");

const ECHO: &str = r#"
rel req(id: text, method: text, params: text) @in(rpc).
rel resp(id: text, result: text) @out(rpc).

rel known(method: text).
known("ping").
known("echo").

rel route(id: text, result: text, prio: int) key(id) merge(MaxBy(prio)).
route(id, "pong", 100) <- req(id, "ping", _).
route(id, params, 100) <- req(id, "echo", params).
route(id, "unknown method", 1) <- req(id, method, _), !known(method).

resp(id, result) <- route(id, result, _).
"#;

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mcp_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Spawn `dl <prog> --mcp --no-daemon`, feed the ndjson `requests`, close
/// stdin, return (exit code, stdout lines as JSON, stderr).
fn serve(dir: &Path, prog: &str, requests: &[&str]) -> (i32, Vec<serde_json::Value>, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let mut child = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--mcp", "--no-daemon",
               "--root", dir.to_str().unwrap(),
               "--db", dir.join("db").to_str().unwrap()])
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

fn result_of<'a>(msgs: &'a [serde_json::Value], id: i64) -> Option<&'a serde_json::Value> {
    msgs.iter().find(|m| m.get("id").and_then(|i| i.as_i64()) == Some(id))
}

/// ping -> the MaxBy dispatch picks the prio-100 "pong" row over the fallback.
#[test]
fn ping_gets_pong() {
    let d = sandbox("ping");
    let (code, msgs, err) = serve(&d, ECHO,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#]);
    assert_eq!(code, 0, "{err}");
    let m = result_of(&msgs, 1).expect("response for id 1");
    assert_eq!(m["result"], serde_json::json!("pong"), "{msgs:?}");
}

/// echo -> the params JSON text round-trips as the result payload.
#[test]
fn echo_returns_params() {
    let d = sandbox("echo");
    let (code, msgs, err) = serve(&d, ECHO,
        &[r#"{"jsonrpc":"2.0","id":2,"method":"echo","params":{"x":1}}"#]);
    assert_eq!(code, 0, "{err}");
    let m = result_of(&msgs, 2).expect("response for id 2");
    assert_eq!(m["result"], serde_json::json!({"x": 1}), "{msgs:?}");
}

/// An unknown method falls through to the prio-1 !known rule, so the client
/// still gets a result (the program chose to answer) rather than a hang.
#[test]
fn unknown_method_hits_fallback() {
    let d = sandbox("fallback");
    let (code, msgs, err) = serve(&d, ECHO,
        &[r#"{"jsonrpc":"2.0","id":3,"method":"nope"}"#]);
    assert_eq!(code, 0, "{err}");
    let m = result_of(&msgs, 3).expect("response for id 3");
    assert_eq!(m["result"], serde_json::json!("unknown method"), "{msgs:?}");
}

/// A program with no rule for a request (and no fallback) still responds:
/// -32601 method-not-found, and the loop retires the request row.
#[test]
fn unanswered_request_gets_error() {
    let d = sandbox("unanswered");
    let prog = concat!(
        "rel req(id: text, method: text, params: text) @in(rpc).\n",
        "rel resp(id: text, result: text) @out(rpc).\n",
        "resp(id, \"pong\") <- req(id, \"ping\", _).\n");
    let (code, msgs, err) = serve(&d, prog,
        &[r#"{"jsonrpc":"2.0","id":7,"method":"nope"}"#]);
    assert_eq!(code, 0, "{err}");
    let m = result_of(&msgs, 7).expect("response for id 7");
    assert_eq!(m["error"]["code"], serde_json::json!(-32601), "{msgs:?}");
}

/// Drain law 1 (retire the answered row): two sequential requests each get
/// exactly one response — the first request's row must not re-answer under the
/// second tick's rebuild.
#[test]
fn answered_rows_retire_between_requests() {
    let d = sandbox("retire");
    let (code, msgs, err) = serve(&d, ECHO, &[
        r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"echo","params":"two"}"#,
    ]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(msgs.len(), 2, "one response per request, no re-answers: {msgs:?}");
    assert_eq!(result_of(&msgs, 1).unwrap()["result"], serde_json::json!("pong"));
    assert_eq!(result_of(&msgs, 2).unwrap()["result"], serde_json::json!("two"));
}

/// A rule heading an @in port bails loudly (the serving loop owns those rows).
#[test]
fn rule_heading_in_port_bails() {
    let d = sandbox("inport_head");
    let prog = concat!(
        "rel req(id: text, method: text, params: text) @in(rpc).\n",
        "rel resp(id: text, result: text) @out(rpc).\n",
        "req(\"1\", \"ping\", \"null\") <- true().\n",
        "resp(id, \"pong\") <- req(id, \"ping\", _).\n");
    fs::write(d.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--no-daemon", "--root", d.to_str().unwrap(),
               "--db", d.join("db").to_str().unwrap()])
        .output().expect("run dl");
    assert!(!out.status.success(), "heading an @in port must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("@in port"), "loud @in-head bail, got: {err}");
}

/// The class envelope is checked at declare time: an @in(rpc) rel missing the
/// envelope columns is rejected with the expected shape in the message.
#[test]
fn rpc_envelope_checked_at_declare() {
    let d = sandbox("envelope");
    let prog = "rel req(id: text, verb: text) @in(rpc).\n";
    fs::write(d.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--no-daemon", "--root", d.to_str().unwrap(),
               "--db", d.join("db").to_str().unwrap()])
        .output().expect("run dl");
    assert!(!out.status.success(), "bad rpc envelope must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("method"), "message names the missing column, got: {err}");
}

/// --mcp with no rpc ports declared bails with guidance instead of serving a
/// program that can never answer.
#[test]
fn mcp_without_ports_bails() {
    let d = sandbox("no_ports");
    fs::write(d.join("p.dl"), "rel f(x: int).\nf(1).\n").unwrap();
    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--mcp", "--no-daemon", "--root", d.to_str().unwrap(),
               "--db", d.join("db").to_str().unwrap()])
        .stdin(Stdio::null())
        .output().expect("run dl");
    assert!(!out.status.success(), "--mcp without ports must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("@in(rpc)"), "guidance names the port decls, got: {err}");
}
