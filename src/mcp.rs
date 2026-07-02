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
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

/// The port qualifier of a named rel decl, if any.
pub fn port_decl<'a>(prog: &'a ast::Program, name: &str) -> Option<&'a ast::Port> {
    prog.items.iter().find_map(|i| match i {
        Item::Rel(d) if d.name == name => d.port.as_ref(),
        _ => None,
    })
}

/// Where requests are pumped. `Daemon` is the primary mode: the daemon owns
/// the one engine and its lock, stays warm and file-watch reactive, and this
/// process is a thin stdio adapter (`mcp_request`/`mcp_retire` RPCs beside
/// `query_sql`). `Local` is the fallback: `--no-daemon`, an explicit `--db`,
/// or a failed attach — a cold in-process engine, hermetic, right for CI.
pub enum Pump {
    Local(Box<crate::engine::Engine>),
    Daemon { stream: UnixStream, next_id: u64 },
}

impl Pump {
    fn rpc(&mut self, method: &str, params: serde_json::Value) -> Result<crate::rpc::Response> {
        let Pump::Daemon { stream, next_id } = self else {
            anyhow::bail!("rpc on a local pump");
        };
        let req = crate::rpc::Request::new(*next_id, method, params);
        *next_id += 1;
        let resp = crate::daemon::rpc_call(stream, &req)?;
        if let Some(e) = &resp.error {
            anyhow::bail!("daemon {method}: {}", e.message);
        }
        Ok(resp)
    }

    /// Inject one request, tick, drain: the per-frame pump.
    fn handle(&mut self, prog: &ast::Program, ports: &(String, String),
              id: &str, method: &str, params: &str) -> Result<Vec<(String, String)>> {
        let (in_rel, out_rel) = ports;
        match self {
            Pump::Local(eng) => {
                eng.inject_rpc(in_rel, id, method, params)?;
                eng.tick(prog, true)?;
                eng.drain_rpc(out_rel, in_rel)
            }
            Pump::Daemon { .. } => {
                let resp = self.rpc("mcp_request", serde_json::json!({
                    "in_rel": in_rel, "out_rel": out_rel,
                    "id": id, "method": method, "params": params,
                }))?;
                let rows = resp.result.as_ref().and_then(|r| r.get("rows"))
                    .and_then(|r| r.as_array()).cloned().unwrap_or_default();
                Ok(rows.iter().filter_map(|row| {
                    let r = row.as_array()?;
                    Some((r.first()?.as_str()?.to_string(), r.get(1)?.as_str()?.to_string()))
                }).collect())
            }
        }
    }

    /// Retire unanswered ids from the in-port (the -32601 path).
    fn retire(&mut self, in_rel: &str, ids: &[String]) -> Result<()> {
        match self {
            Pump::Local(eng) => eng.retire_rpc(in_rel, ids),
            Pump::Daemon { .. } => {
                self.rpc("mcp_retire", serde_json::json!({ "in_rel": in_rel, "ids": ids }))?;
                Ok(())
            }
        }
    }
}

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

/// Handle one inbound JSON-RPC message: inject, tick, drain (through the
/// pump). Returns the outbound messages (empty for a notification). A request
/// no rule answered gets a method-not-found error so the client never hangs.
pub fn handle_msg(
    pump: &mut Pump,
    prog: &ast::Program,
    ports: &(String, String),
    msg: &serde_json::Value,
) -> Result<Vec<serde_json::Value>> {
    let (in_rel, _) = ports;
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or_default().to_string();
    let Some(idv) = msg.get("id") else {
        // No id = a notification (MCP's notifications/initialized etc.):
        // nothing to answer, and rung 1 doesn't feed them into the engine.
        return Ok(Vec::new());
    };
    // The envelope id is the raw JSON serialization (`1`, `"abc"`), so both
    // integer and string JSON-RPC ids round-trip exactly. dl rules treat it as
    // an opaque text key.
    let id = idv.to_string();
    // Parse an envelope id back to the JSON value the client sent; a table row
    // that isn't valid JSON (a program constructed its own id) goes as a string.
    let id_val = |rid: &str| -> serde_json::Value {
        serde_json::from_str(rid).unwrap_or_else(|_| serde_json::Value::String(rid.to_string()))
    };
    let params = msg.get("params").map(|p| p.to_string()).unwrap_or_else(|| "null".into());
    let rows = pump.handle(prog, ports, &id, &method, &params)?;
    let mut out = Vec::new();
    let mut answered = false;
    for (rid, result) in rows {
        // `result` is the response payload as JSON text; a row that isn't
        // valid JSON is sent as a JSON string, so `resp(id, "pong")` works.
        let val: serde_json::Value =
            serde_json::from_str(&result).unwrap_or(serde_json::Value::String(result));
        answered |= rid == id;
        out.push(serde_json::json!({ "jsonrpc": "2.0", "id": id_val(&rid), "result": val }));
    }
    if !answered {
        pump.retire(in_rel, std::slice::from_ref(&id))?;
        out.push(serde_json::json!({
            "jsonrpc": "2.0", "id": id_val(&id),
            "error": { "code": -32601, "message": format!("no rule answered method `{method}`") },
        }));
    }
    Ok(out)
}

/// The serve loop: frames in, responses out, until the peer closes.
pub fn serve(
    pump: &mut Pump,
    prog: &ast::Program,
    ports: &(String, String),
    chan: &mut dyn Channel,
) -> Result<()> {
    while let Some(Frame::Rpc(msg)) = chan.recv()? {
        for resp in handle_msg(pump, prog, ports, &msg)? {
            chan.send(&Frame::Rpc(resp))?;
        }
    }
    Ok(())
}

/// Cold in-process pump: open the db, one priming tick so sources are scanned,
/// the port rels are declared, and per-request latency is the fixpoint.
fn local_pump(prog: &ast::Program, db_path: Option<&str>, root: PathBuf) -> Result<Pump> {
    let conn = crate::db::open(db_path)?;
    let mut eng = crate::engine::Engine::new(conn, root);
    eng.tick(prog, true)?;
    Ok(Pump::Local(Box::new(eng)))
}

/// `dl --mcp` entry: load the program (positional or `.dl/` discovery), strip
/// `?` queries and `gen` sinks (stdout is the transport; codegen has no place
/// in a serve loop), resolve the rpc port pair, then serve stdio. Daemon-first,
/// like `--hook`: when the daemon manages this root, this process is a thin
/// adapter over its warm engine; otherwise (opt-out, explicit `--db`, or a
/// failed attach) a cold in-process engine serves hermetically.
pub fn run_mcp(programs: &[String], db_path: Option<&str>, root: PathBuf) -> Result<()> {
    let files = crate::resolve_programs(programs, &root)?;
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
    let mut pump = if crate::daemon::enabled_for(&root) && db_path.is_none() && programs.len() <= 1 {
        let attach = || -> Result<Pump> {
            crate::daemon::ensure_daemon(&root, programs.first().map(|s| s.as_str()))?;
            let stream = crate::daemon::connect(Some(&root))?;
            Ok(Pump::Daemon { stream, next_id: 1 })
        };
        match attach() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[daemon] mcp attach failed, in-process: {e}");
                local_pump(&prog, db_path, root)?
            }
        }
    } else {
        local_pump(&prog, db_path, root)?
    };
    let stdin = std::io::stdin();
    let mut chan = StdioChannel::new(stdin.lock(), std::io::stdout());
    serve(&mut pump, &prog, &ports, &mut chan)
}
