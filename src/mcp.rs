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
//!
//! `initialize` is protocol-mandatory (a real MCP client sends it before
//! `tools/list` and drops the connection on anything but a result), so a
//! program with zero rules — the natural shape for "just serve the built-in
//! dl.* tools" — still needs an answer. `handle_msg` fills that gap the same
//! way `tools/list` merges rather than replaces: if no program rule answers
//! `initialize`, the adapter answers generically; a program rule that DOES
//! head it keeps winning.

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
    Daemon { stream: UnixStream, next_id: u64, root: PathBuf },
}

impl Pump {
    fn rpc(&mut self, method: &str, mut params: serde_json::Value) -> Result<crate::rpc::Response> {
        let Pump::Daemon { stream, next_id, root } = self else {
            anyhow::bail!("rpc on a local pump");
        };
        // Name the served root so the singleton routes to this program's engine.
        params["root"] = serde_json::json!(root.to_string_lossy());
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

    /// `dl.what` tool: the anchor resolver output, as `{columns, rows, total,
    /// notes}`. Local reads the cold engine directly; Daemon rides the warm
    /// `what` RPC.
    fn tool_what(&mut self, anchor: &str, limit: Option<usize>, offset: Option<usize>)
        -> Result<serde_json::Value>
    {
        match self {
            Pump::Local(eng) => {
                // A fresh probe-forced engine, so the graph is warm regardless of
                // what the served program references (and never disturbing it).
                let warm = crate::anchor::warm_engine(eng.root.clone(), None)?;
                Ok(query_out_json(&crate::anchor::what(&warm, anchor, limit, offset)))
            }
            Pump::Daemon { .. } => {
                let mut params = serde_json::json!({ "anchor": anchor });
                if let Some(l) = limit { params["limit"] = serde_json::json!(l); }
                if let Some(o) = offset { params["offset"] = serde_json::json!(o); }
                Ok(self.rpc("what", params)?.result.unwrap_or_default())
            }
        }
    }

    /// `dl.verb` tool: the `dl q` runner output, as `{columns, rows, total,
    /// notes}`. Local spins its own scratch engine (never disturbing this
    /// program's tables); Daemon rides the `q` RPC (a scratch engine server-side).
    fn tool_verb(&mut self, verb: &str, target: &str, limit: Option<usize>, offset: Option<usize>)
        -> Result<serde_json::Value>
    {
        match self {
            Pump::Local(eng) => {
                let root = eng.root.clone();
                Ok(query_out_json(&crate::verbs::run_inproc(root, None, verb, target, limit, offset)?))
            }
            Pump::Daemon { .. } => {
                let mut params = serde_json::json!({ "verb": verb, "target": target });
                if let Some(l) = limit { params["limit"] = serde_json::json!(l); }
                if let Some(o) = offset { params["offset"] = serde_json::json!(o); }
                Ok(self.rpc("q", params)?.result.unwrap_or_default())
            }
        }
    }

    /// `dl.rows` tool: a capped dump of one relation, as `{columns, rows,
    /// total}`. `total` is the relation's FULL row count so the model sees when
    /// it is only seeing a page (count-first contract). `cap` is already clamped
    /// to the hard max by the caller.
    fn tool_rows(&mut self, rel: &str, cap: usize) -> Result<serde_json::Value> {
        match self {
            Pump::Local(eng) => {
                // Warm probe-forced engine so any built-in / graph rel is
                // dumpable without the served program having to reference it.
                let warm = crate::anchor::warm_engine(eng.root.clone(), None)?;
                let (columns, all): (Vec<String>, Vec<Vec<String>>) = match warm.rels.get(rel) {
                    Some(meta) => {
                        let cols: Vec<String> = meta.cols.iter().map(|c| c.name.clone()).collect();
                        let n = cols.len();
                        (cols, warm.rel_rows(rel, n))
                    }
                    None => (Vec::new(), Vec::new()),
                };
                let total = all.len();
                let rows: Vec<Vec<String>> = all.into_iter().take(cap).collect();
                Ok(serde_json::json!({ "columns": columns, "rows": rows, "total": total }))
            }
            Pump::Daemon { .. } => {
                let result = self.rpc("query_rel", serde_json::json!({ "rel": rel }))?
                    .result.unwrap_or_default();
                let columns = result.get("columns").cloned().unwrap_or_else(|| serde_json::json!([]));
                let all = result.get("rows").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let total = all.len();
                let rows: Vec<serde_json::Value> = all.into_iter().take(cap).collect();
                Ok(serde_json::json!({ "columns": columns, "rows": rows, "total": total }))
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

/// Parse an envelope id string back to the JSON value the client sent (mirrors
/// the `id_val` closure for the free helper functions below).
fn id_to_json(rid: &str) -> serde_json::Value {
    serde_json::from_str(rid).unwrap_or_else(|_| serde_json::Value::String(rid.to_string()))
}

/// Shape an `anchor::QueryOut` into the wire envelope the tools return.
fn query_out_json(out: &crate::anchor::QueryOut) -> serde_json::Value {
    serde_json::json!({
        "columns": out.columns, "rows": out.rows,
        "total": out.total, "notes": out.notes,
    })
}

/// The adapter's fallback answer to `initialize` when no program rule heads
/// it. Echoes the client's requested `protocolVersion` back (falling back to
/// the baseline `examples/mcp-server.dl` advertises), declares tool
/// capabilities (the only surface the adapter itself offers — dl.what/
/// dl.verb/dl.rows via `tools/list`), and names this build in `serverInfo`.
/// Used from both `Pump::Local` and `Pump::Daemon`: the gap-fill happens in
/// `handle_msg` before either pump is touched, so it's pump-agnostic.
fn generic_initialize_response(id: &str, msg: &serde_json::Value) -> serde_json::Value {
    let protocol_version = msg
        .get("params")
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str())
        .unwrap_or("2024-11-05");
    serde_json::json!({
        "jsonrpc": "2.0", "id": id_to_json(id),
        "result": {
            "protocolVersion": protocol_version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "dl", "version": env!("CARGO_PKG_VERSION") },
        }
    })
}

/// The three adapter-served MCP tool descriptors. Strict input schemas
/// (`required` + `additionalProperties: false`) so misuse is hard, and every
/// rows-shaped tool documents the count-first contract (check `total` before
/// paging).
pub fn builtin_tool_descriptors() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!({
            "name": "dl.what",
            "description": "Resolve a code anchor and describe its neighborhood. The anchor is a symbol name, a `Name*` glob, a file path, or `path:line`. Returns {columns, rows, total, notes}: each row places one hit (family, rel, repo, file, line, detail with caller/callee/type counts). `total` is the full pre-paging count — check it before paging with limit/offset.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "anchor": { "type": "string", "description": "symbol name, `Name*` glob, file path, or path:line" },
                    "limit": { "type": "integer", "minimum": 1, "description": "max rows to return" },
                    "offset": { "type": "integer", "minimum": 0, "description": "rows to skip (paging)" }
                },
                "required": ["anchor"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "dl.verb",
            "description": "Run a concept verb over the code graph. `who-calls` lists callers of a function; `where-defined` lists definition sites across the type/call/scip families. Returns {columns, rows, total, notes}; `notes` reports how the name resolved (candidate count + families). `total` is the full count — check it before paging with limit/offset.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "verb": { "type": "string", "enum": ["who-calls", "where-defined"], "description": "the concept verb" },
                    "target": { "type": "string", "description": "the symbol name to run the verb on" },
                    "limit": { "type": "integer", "minimum": 1, "description": "max rows to return" },
                    "offset": { "type": "integer", "minimum": 0, "description": "rows to skip (paging)" }
                },
                "required": ["verb", "target"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "dl.rows",
            "description": "Dump the current rows of a built-in or derived relation, capped. Returns {columns, rows, total}; `total` is the relation's FULL row count. If total exceeds the returned rows you are seeing only a page — narrow with dl.what / dl.verb instead of paging a huge relation. limit defaults to 200, hard max 1000.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "rel": { "type": "string", "description": "relation name, e.g. call_edge, type_entity, verb_catalog" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "max rows (default 200, hard max 1000)" }
                },
                "required": ["rel"],
                "additionalProperties": false
            }
        }),
    ]
}

/// `tools/list`: the program's own tools (best-effort) with the built-in
/// descriptors appended. A program with no `tools/list` rule still advertises
/// the built-ins.
fn tools_list_merged(
    pump: &mut Pump,
    prog: &ast::Program,
    ports: &(String, String),
    msg: &serde_json::Value,
    id: &str,
) -> Result<serde_json::Value> {
    let mut tools = program_tool_list(pump, prog, ports, msg, id)?;
    tools.extend(builtin_tool_descriptors());
    Ok(serde_json::json!({
        "jsonrpc": "2.0", "id": id_to_json(id),
        "result": { "tools": tools }
    }))
}

/// The program's advertised tools for a `tools/list` request (empty when the
/// program answers nothing). Injects/ticks/drains through the pump and retires
/// an unanswered row so drain law 1 holds.
fn program_tool_list(
    pump: &mut Pump,
    prog: &ast::Program,
    ports: &(String, String),
    msg: &serde_json::Value,
    id: &str,
) -> Result<Vec<serde_json::Value>> {
    let (in_rel, _) = ports;
    let params = msg.get("params").map(|p| p.to_string()).unwrap_or_else(|| "null".into());
    let rows = pump.handle(prog, ports, id, "tools/list", &params)?;
    let mut tools = Vec::new();
    let mut answered = false;
    for (rid, result) in &rows {
        if rid == id {
            answered = true;
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(result) {
                if let Some(arr) = value.get("tools").and_then(|t| t.as_array()) {
                    tools.extend(arr.iter().cloned());
                }
            }
        }
    }
    if !answered {
        pump.retire(in_rel, std::slice::from_ref(&id.to_string()))?;
    }
    Ok(tools)
}

/// A JSON-RPC invalid-params error response for a malformed built-in tool call.
fn tool_error(id: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0", "id": id_to_json(id),
        "error": { "code": -32602, "message": message }
    })
}

/// Wrap a built-in tool's JSON answer as an MCP `tools/call` result (a single
/// text content block carrying the serialized envelope).
fn tool_result(id: &str, answer: &serde_json::Value) -> serde_json::Value {
    let text = serde_json::to_string(answer).unwrap_or_default();
    serde_json::json!({
        "jsonrpc": "2.0", "id": id_to_json(id),
        "result": { "content": [ { "type": "text", "text": text } ] }
    })
}

/// Answer a `tools/call` whose name is a `dl.*` built-in. `Ok(None)` means the
/// name is not a built-in, so the caller passes it through to the program.
fn builtin_tool_call(
    pump: &mut Pump,
    msg: &serde_json::Value,
    id: &str,
) -> Result<Option<serde_json::Value>> {
    let params = msg.get("params");
    let name = params.and_then(|p| p.get("name")).and_then(|n| n.as_str()).unwrap_or_default();
    let args = params
        .and_then(|p| p.get("arguments"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let u = |v: Option<&serde_json::Value>| v.and_then(|x| x.as_u64()).map(|n| n as usize);
    let answer = match name {
        "dl.what" => {
            let Some(anchor) = args.get("anchor").and_then(|v| v.as_str()) else {
                return Ok(Some(tool_error(id, "dl.what requires string `anchor`")));
            };
            pump.tool_what(anchor, u(args.get("limit")), u(args.get("offset")))?
        }
        "dl.verb" => {
            let Some(verb) = args.get("verb").and_then(|v| v.as_str()) else {
                return Ok(Some(tool_error(id, "dl.verb requires string `verb`")));
            };
            let Some(target) = args.get("target").and_then(|v| v.as_str()) else {
                return Ok(Some(tool_error(id, "dl.verb requires string `target`")));
            };
            if crate::verbs::find(verb).is_none() {
                return Ok(Some(tool_error(id,
                    &format!("unknown verb {verb:?}; available: {}", crate::verbs::verb_list()))));
            }
            pump.tool_verb(verb, target, u(args.get("limit")), u(args.get("offset")))?
        }
        "dl.rows" => {
            let Some(rel) = args.get("rel").and_then(|v| v.as_str()) else {
                return Ok(Some(tool_error(id, "dl.rows requires string `rel`")));
            };
            let cap = u(args.get("limit")).unwrap_or(200).min(1000);
            pump.tool_rows(rel, cap)?
        }
        _ => return Ok(None),
    };
    Ok(Some(tool_result(id, &answer)))
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
    // Adapter-served built-in tools (dl.what / dl.verb / dl.rows), merged with
    // the program's own MCP surface. `tools/list` appends the built-in
    // descriptors to whatever the program advertises; `tools/call` on a `dl.*`
    // name is answered here, and any other tool falls through to the program.
    if method == "tools/list" {
        return Ok(vec![tools_list_merged(pump, prog, ports, msg, &id)?]);
    }
    if method == "tools/call" {
        if let Some(resp) = builtin_tool_call(pump, msg, &id)? {
            return Ok(vec![resp]);
        }
    }
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
        if method == "initialize" {
            // Protocol-mandatory and program-agnostic: fill the gap instead
            // of -32601, so a program with zero rules still completes the
            // handshake (see the module doc + generic_initialize_response).
            out.push(generic_initialize_response(&id, msg));
        } else {
            out.push(serde_json::json!({
                "jsonrpc": "2.0", "id": id_val(&id),
                "error": { "code": -32601, "message": format!("no rule answered method `{method}`") },
            }));
        }
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
            let msg = &d.msg;
            tracing::warn!(msg = %msg, "dl --mcp: program error: {msg}");
        }
        anyhow::bail!("{n_err} program error(s)");
    }
    let ports = rpc_ports(&prog)?;
    let mut pump = if crate::daemon::enabled_for(&root) && db_path.is_none() && programs.len() <= 1 {
        let attach = || -> Result<Pump> {
            crate::daemon::ensure_daemon(&root, programs.first().map(|s| s.as_str()))?;
            // Register + cold-tick this root before the first mcp_request, so the
            // port drift-guard reads the served program (auto-add would race it).
            crate::daemon::add_root(&root)?;
            let stream = crate::daemon::connect()?;
            Ok(Pump::Daemon { stream, next_id: 1, root: root.clone() })
        };
        match attach() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "[daemon] mcp attach failed, in-process: {e}");
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
