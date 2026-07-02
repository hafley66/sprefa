//! `dl --mcp`: serve a dl program as a JSON-RPC (MCP stdio) server.
//!
//! `--mcp` is a BINDING PROFILE, not a language feature: it binds the
//! program's `rpc`-class ports to stdio x jsonrpc. The program declares WHICH
//! rels are its boundary — `rel req(id: int, method: text, params: text)
//! @in(rpc).` receives, `rel resp(id: int, result: text) @out(rpc).` replies —
//! and never names a transport, so the same program serves any wire that
//! speaks the rpc class (`--bind rpc=http:jsonrpc` is a later rung).
//!
//! The loop: read one request frame -> inject it into the `@in(rpc)` rel ->
//! tick -> drain the `@out(rpc)` rel back to the transport -> retire the
//! answered in-port rows (drain law 1). The dl program IS the handler table: a
//! dispatch rel with `key(id) merge(MaxBy(prio))` picks the winning rule per
//! request (the lattice = select), and a plain `resp(id, result) <- route(id,
//! result, _)` bridge yields the response. See examples/mcp-echo.dl.
//!
//! Rung 1 of the channel ladder (chat_log/20260630.5): the out-port is an
//! ordinary IDB rel the loop reads + clears; `@yield` later automates the same
//! select->push->delete. Completion is channel-level (JSON-RPC is 1->1: every
//! response closes its request), so nothing streams yet — that's the
//! stream-class rung.

use crate::ast::{self, Item, PortDir};
use crate::channel::{Channel, Frame, StdioChannel};
use anyhow::Result;
use std::path::PathBuf;

/// The program's rpc port pair: (in rel, out rel). Exactly one of each —
/// with no named-instance syntax yet (`@in(rpc: api)` is deferred), two ports
/// of the same class/direction are ambiguous and the profile bails loudly.
pub fn rpc_ports(prog: &ast::Program) -> Result<(String, String)> {
    let mut ins = Vec::new();
    let mut outs = Vec::new();
    for item in &prog.items {
        let Item::Rel(d) = item else { continue };
        let Some(p) = &d.port else { continue };
        if p.class == "rpc" {
            match p.dir {
                PortDir::In => ins.push(d.name.clone()),
                PortDir::Out => outs.push(d.name.clone()),
            }
        }
    }
    match (ins.len(), outs.len()) {
        (1, 1) => Ok((ins.pop().unwrap(), outs.pop().unwrap())),
        (0, _) | (_, 0) => anyhow::bail!(
            "--mcp needs one @in(rpc) and one @out(rpc) port rel; found {} in, {} out. \
             Declare e.g. `rel req(id: int, method: text, params: text) @in(rpc).` and \
             `rel resp(id: int, result: text) @out(rpc).`", ins.len(), outs.len()),
        _ => anyhow::bail!(
            "--mcp found multiple rpc ports (in: {}; out: {}); named port instances \
             aren't supported yet — declare exactly one @in(rpc) and one @out(rpc)",
            ins.join(", "), outs.join(", ")),
    }
}

/// Handle one inbound JSON-RPC message: inject, tick, drain. Returns the
/// outbound messages (empty for a notification). A request no rule answered
/// gets a method-not-found error so the client never hangs.
pub fn handle_msg(
    eng: &mut crate::engine::Engine,
    prog: &ast::Program,
    ports: &(String, String),
    msg: &serde_json::Value,
) -> Result<Vec<serde_json::Value>> {
    let (in_rel, out_rel) = ports;
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or_default().to_string();
    let Some(id) = msg.get("id").and_then(|i| i.as_i64()) else {
        // No id = a notification (MCP's notifications/initialized etc.):
        // nothing to answer, and rung 1 doesn't feed them into the engine.
        // A non-integer id is skipped loudly — the rpc envelope's id is int.
        if let Some(v) = msg.get("id") {
            eprintln!("[mcp] non-integer request id skipped: {v}");
        }
        return Ok(Vec::new());
    };
    let params = msg.get("params").map(|p| p.to_string()).unwrap_or_else(|| "null".into());
    eng.inject_rpc(in_rel, id, &method, &params)?;
    eng.tick(prog, true)?;
    let rows = eng.drain_rpc(out_rel, in_rel)?;
    let mut out = Vec::new();
    let mut answered = false;
    for (rid, result) in rows {
        // `result` is the response payload as JSON text; a row that isn't
        // valid JSON is sent as a JSON string, so `resp(id, "pong")` works.
        let val: serde_json::Value =
            serde_json::from_str(&result).unwrap_or(serde_json::Value::String(result));
        answered |= rid == id;
        out.push(serde_json::json!({ "jsonrpc": "2.0", "id": rid, "result": val }));
    }
    if !answered {
        eng.retire_rpc(in_rel, &[id])?;
        out.push(serde_json::json!({
            "jsonrpc": "2.0", "id": id,
            "error": { "code": -32601, "message": format!("no rule answered method `{method}`") },
        }));
    }
    Ok(out)
}

/// The serve loop: frames in, responses out, until the peer closes.
pub fn serve(
    eng: &mut crate::engine::Engine,
    prog: &ast::Program,
    ports: &(String, String),
    chan: &mut dyn Channel,
) -> Result<()> {
    while let Some(Frame::Rpc(msg)) = chan.recv()? {
        for resp in handle_msg(eng, prog, ports, &msg)? {
            chan.send(&Frame::Rpc(resp))?;
        }
    }
    Ok(())
}

/// `dl --mcp` entry: load the program (positional or `.dl/` discovery), strip
/// `?` queries and `gen` sinks (stdout is the transport; codegen has no place
/// in a serve loop), resolve the rpc port pair, then serve stdio. One priming
/// tick runs before the first request so sources are scanned, the port rels
/// are declared, and per-request latency is the fixpoint, not a cold scan.
pub fn run_mcp(program: Option<&str>, db_path: Option<&str>, root: PathBuf) -> Result<()> {
    let files = crate::resolve_programs(program, &root)?;
    let (mut prog, type_diags, _) = crate::prepare_paths(&files)?;
    prog.items.retain(|i| !matches!(i, ast::Item::Query(_) | ast::Item::Gen(_)));
    let n_err = type_diags.iter().filter(|d| d.severity == ast::Severity::Error).count();
    if n_err > 0 {
        for d in type_diags.iter().filter(|d| d.severity == ast::Severity::Error) {
            eprintln!("dl --mcp: program error: {}", d.msg);
        }
        anyhow::bail!("{n_err} program error(s)");
    }
    let ports = rpc_ports(&prog)?;
    let conn = crate::db::open(db_path)?;
    let mut eng = crate::engine::Engine::new(conn, root);
    eng.tick(&prog, true)?;
    let stdin = std::io::stdin();
    let mut chan = StdioChannel::new(stdin.lock(), std::io::stdout());
    serve(&mut eng, &prog, &ports, &mut chan)
}
