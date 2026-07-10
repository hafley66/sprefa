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
        .args(["--no-daemon",
               "--db", d.join("db").to_str().unwrap()])
        .current_dir(&d)
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
        .args(["--no-daemon",
               "--db", d.join("db").to_str().unwrap()])
        .current_dir(&d)
        .output().expect("run dl");
    assert!(!out.status.success(), "bad rpc envelope must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("method"), "message names the missing column, got: {err}");
}

// ---------- adapter-served built-in tools (dl.what / dl.verb / dl.rows) ------

/// A minimal rpc-port program that ALSO scans the fixture TS, so the served
/// engine's code-graph tables populate (dl.what reads them directly).
const SERVE_TS: &str = r#"
rel req(id: text, method: text, params: text) @in(rpc).
rel resp(id: text, result: text) @out(rpc).
rel seen(path: file).
seen(path) <- scan("WORK", "**/*.ts", path, rev).
resp(id, "pong") <- req(id, "ping", _).
"#;

/// Drop a two-file TS project (model declares `lookup`, repo's `find` calls it)
/// into the sandbox so type + call families are non-empty.
fn write_ts_fixture(dir: &Path) {
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(
        dir.join("src/model.ts"),
        "export interface Entity { id: Id }\n\
         export function lookup(): Entity { return build() }\n",
    ).unwrap();
    fs::write(
        dir.join("src/repo.ts"),
        "import { Entity, lookup } from './model'\n\
         export class Repo {\n\
         \x20   find(): Entity {\n\
         \x20       return lookup()\n\
         \x20   }\n\
         }\n",
    ).unwrap();
}

/// The parsed JSON payload of a `tools/call` result (the built-in tools return a
/// single text content block carrying the answer envelope).
fn tool_payload<'a>(msgs: &'a [serde_json::Value], id: i64) -> serde_json::Value {
    let m = result_of(msgs, id).unwrap_or_else(|| panic!("no response id {id}: {msgs:?}"));
    let text = m["result"]["content"][0]["text"].as_str()
        .unwrap_or_else(|| panic!("no text content in {m}"));
    serde_json::from_str(text).expect("tool payload is JSON")
}

/// tools/list on a program with no tools/list rule still advertises the three
/// adapter-served built-ins.
#[test]
fn tools_list_advertises_builtins() {
    let d = sandbox("builtins_list");
    write_ts_fixture(&d);
    let (code, msgs, err) = serve(&d, SERVE_TS,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#]);
    assert_eq!(code, 0, "{err}");
    let names: Vec<&str> = result_of(&msgs, 1).expect("tools/list response")
        ["result"]["tools"].as_array().expect("tools array")
        .iter().filter_map(|t| t["name"].as_str()).collect();
    for builtin in ["dl.what", "dl.verb", "dl.rows"] {
        assert!(names.contains(&builtin), "{builtin} missing: {names:?}");
    }
}

/// dl.what round-trips: the anchor resolver output arrives as a text payload
/// carrying columns/rows/total, and `lookup` resolves in the type family.
#[test]
fn tool_dl_what_round_trips() {
    let d = sandbox("builtins_what");
    write_ts_fixture(&d);
    let (code, msgs, err) = serve(&d, SERVE_TS, &[
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"dl.what","arguments":{"anchor":"lookup"}}}"#,
    ]);
    assert_eq!(code, 0, "{err}");
    let payload = tool_payload(&msgs, 1);
    assert!(payload["total"].as_u64().is_some(), "count-first total missing: {payload}");
    let flat = serde_json::to_string(&payload["rows"]).unwrap();
    assert!(flat.contains("type_entity"), "type family hit missing: {payload}");
}

/// dl.verb who-calls returns the caller through the tool surface.
#[test]
fn tool_dl_verb_who_calls() {
    let d = sandbox("builtins_verb");
    write_ts_fixture(&d);
    let (code, msgs, err) = serve(&d, SERVE_TS, &[
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"dl.verb","arguments":{"verb":"who-calls","target":"lookup"}}}"#,
    ]);
    assert_eq!(code, 0, "{err}");
    let payload = tool_payload(&msgs, 1);
    let flat = serde_json::to_string(&payload["rows"]).unwrap();
    assert!(flat.contains("find"), "who-calls tool missing caller `find`: {payload}");
}

/// dl.rows carries the relation's FULL total (count-first) and caps the page.
#[test]
fn tool_dl_rows_carries_total() {
    let d = sandbox("builtins_rows");
    write_ts_fixture(&d);
    let (code, msgs, err) = serve(&d, SERVE_TS, &[
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"dl.rows","arguments":{"rel":"verb_catalog"}}}"#,
    ]);
    assert_eq!(code, 0, "{err}");
    let payload = tool_payload(&msgs, 1);
    // verb_catalog has one row per verb (>= 2).
    assert!(payload["total"].as_u64().unwrap_or(0) >= 2, "total wrong: {payload}");
    let flat = serde_json::to_string(&payload["rows"]).unwrap();
    assert!(flat.contains("who-calls"), "rows missing who-calls: {payload}");
}

/// An unknown tool name is NOT a built-in, so it falls through to the program
/// (which has no rule for it) and gets the -32601 method surface.
#[test]
fn tool_unknown_name_falls_through() {
    let d = sandbox("builtins_unknown");
    write_ts_fixture(&d);
    let (code, msgs, err) = serve(&d, SERVE_TS, &[
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
    ]);
    assert_eq!(code, 0, "{err}");
    let m = result_of(&msgs, 1).expect("response id 1");
    assert_eq!(m["error"]["code"], serde_json::json!(-32601), "{m}");
}

// ---------- generic `initialize` gap-fill ------------------------------------

/// A program with rpc ports but zero rules for `initialize` (the "just serve
/// the built-in dl.* tools" shape) still gets a valid handshake answer: the
/// adapter fills the gap generically instead of -32601.
#[test]
fn initialize_gap_filled_generically() {
    let d = sandbox("init_gap");
    let prog = concat!(
        "rel req(id: text, method: text, params: text) @in(rpc).\n",
        "rel resp(id: text, result: text) @out(rpc).\n");
    let (code, msgs, err) = serve(&d, prog, &[
        r#"{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
    ]);
    assert_eq!(code, 0, "{err}");
    let m = msgs.iter().find(|m| m.get("id") == Some(&serde_json::json!("init-1")))
        .unwrap_or_else(|| panic!("no response for init-1: {msgs:?}"));
    assert!(m.get("error").is_none(), "generic initialize must not error: {m}");
    assert_eq!(m["result"]["protocolVersion"], serde_json::json!("2024-11-05"), "{m}");
    assert_eq!(m["result"]["serverInfo"]["name"], serde_json::json!("dl"), "{m}");
    assert!(m["result"]["capabilities"]["tools"].is_object(), "{m}");

    // Then tools/list still shows the built-ins, so the full "no logic, just
    // serve the built-ins" deployment shape works end to end.
    let (code2, msgs2, err2) = serve(&d, prog, &[
        r#"{"jsonrpc":"2.0","id":"init-2","method":"initialize"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    ]);
    assert_eq!(code2, 0, "{err2}");
    let names: Vec<&str> = msgs2.iter().find(|m| m.get("id") == Some(&serde_json::json!(2)))
        .expect("tools/list response")["result"]["tools"].as_array().expect("tools array")
        .iter().filter_map(|t| t["name"].as_str()).collect();
    for builtin in ["dl.what", "dl.verb", "dl.rows"] {
        assert!(names.contains(&builtin), "{builtin} missing: {names:?}");
    }
}

/// A program that DOES head `initialize` itself keeps winning — the built-in
/// only fills the gap, mirroring tools/list's merge-not-replace behavior.
#[test]
fn initialize_program_rule_still_wins() {
    let d = sandbox("init_override");
    let prog = concat!(
        "rel req(id: text, method: text, params: text) @in(rpc).\n",
        "rel resp(id: text, result: text) @out(rpc).\n",
        "resp(id, \"{\\\"protocolVersion\\\":\\\"2024-11-05\\\",",
        "\\\"capabilities\\\":{\\\"tools\\\":{}},",
        "\\\"serverInfo\\\":{\\\"name\\\":\\\"dl-agent-eval\\\",\\\"version\\\":\\\"0.1.0\\\"}}\") ",
        "<- req(id, \"initialize\", _).\n");
    let (code, msgs, err) = serve(&d, prog, &[
        r#"{"jsonrpc":"2.0","id":"init-1","method":"initialize","params":{}}"#,
    ]);
    assert_eq!(code, 0, "{err}");
    let m = msgs.iter().find(|m| m.get("id") == Some(&serde_json::json!("init-1")))
        .unwrap_or_else(|| panic!("no response for init-1: {msgs:?}"));
    assert_eq!(m["result"]["serverInfo"]["name"], serde_json::json!("dl-agent-eval"),
        "program's own initialize rule must win over the generic fallback: {m}");
}

/// --mcp with no rpc ports declared bails with guidance instead of serving a
/// program that can never answer.
#[test]
fn mcp_without_ports_bails() {
    let d = sandbox("no_ports");
    fs::write(d.join("p.dl"), "rel f(x: int).\nf(1).\n").unwrap();
    let out = Command::new(DL)
        .arg(d.join("p.dl"))
        .args(["--mcp", "--no-daemon",
               "--db", d.join("db").to_str().unwrap()])
        .current_dir(&d)
        .stdin(Stdio::null())
        .output().expect("run dl");
    assert!(!out.status.success(), "--mcp without ports must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("@in(rpc)"), "guidance names the port decls, got: {err}");
}
