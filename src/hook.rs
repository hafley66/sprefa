//! `dl --hook`: dl as a coding-agent harness hook command.
//!
//! Registered as a Claude Code `PostToolUse` hook `command`, dl reads the
//! harness's event JSON from stdin, ticks the rules, and emits the harness-native
//! hook output on stdout. The CONDITION is a dl rule: the program heads
//! `inject(text)` / `inject_skill(name)` / `block(reason)` over the existing agent
//! built-ins (`agent_touch`, `agent_changed`, `changed`, ...). No editor, no bash
//! — dl is the binary the harness execs.
//!
//! Emit relations the program may declare (all single-column):
//!   inject(text: text)        -> additionalContext (raw text)
//!   inject_skill(name: text)  -> additionalContext = the named skill's SKILL.md body
//!   block(reason: text)       -> {"decision":"block","reason":...}
//!
//! "Load once" is the RULE's job, declaratively: negate the built-in
//! `skill_loaded(harness, session, name)` relation (derived from the transcript:
//! explicit `Skill` calls + dl's own prior injections), so
//! `inject_skill("testing") <- ..., !skill_loaded(_, s, "testing")` never
//! re-injects. No state files here; the dedup is a fact in the engine.
//!
//! Output is Claude Code's hook JSON shape today. A second harness = a second
//! `render`-style arm here, not a change to any dl program.

use crate::ast;
use anyhow::Result;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

/// `<root>/.claude/skills/<name>/SKILL.md`, then `~/.claude/skills/<name>/SKILL.md`.
/// Returns the file body (the text injected as additionalContext).
fn resolve_skill(name: &str, root: &Path) -> Option<String> {
    let mut cands = vec![root.join(".claude/skills").join(name).join("SKILL.md")];
    if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
        cands.push(h.join(".claude/skills").join(name).join("SKILL.md"));
    }
    cands.into_iter().find_map(|c| std::fs::read_to_string(&c).ok())
}

/// The three emit relations read off one tick. `broken` is the in-process
/// 1/2-split signal (a malformed program surfaces to the user, never the agent).
#[derive(Default)]
struct EmitRels {
    inject: Vec<String>,
    skills: Vec<String>,
    blocks: Vec<String>,
    broken: bool,
}

/// Read a single-column emit relation off an in-process engine (empty if the
/// program never declares it).
fn emit_col(eng: &crate::engine::Engine, rel: &str) -> Vec<String> {
    eng.rel_rows(rel, 1).into_iter().filter_map(|r| r.into_iter().next()).collect()
}

fn val_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

/// One `query_sql` read of a rel's column 0 off the daemon. A rel the program
/// never declared has no `rel_<name>` table; the daemon returns an error
/// response, read here as empty.
fn daemon_col(s: &mut UnixStream, root: &Path, table: &str) -> Vec<String> {
    let req = crate::rpc::Request::new(1, "query_sql",
        serde_json::json!({ "root": root.to_string_lossy(), "sql": format!("SELECT * FROM {table}") }));
    let Ok(resp) = crate::daemon::rpc_call(s, &req) else { return Vec::new() };
    if resp.error.is_some() { return Vec::new(); }
    resp.result.and_then(|v| v.get("rows").cloned())
        .and_then(|v| v.as_array().cloned()).unwrap_or_default()
        .iter()
        .filter_map(|row| row.as_array().and_then(|r| r.first()).map(val_str))
        .collect()
}

/// One harness-hook event, parsed off the stdin payload. `json` is the raw event
/// text (the program extracts fields via term-form json/jsonp); `kind`/`session`
/// are the two coordinates the engine keys the row on. `seq` is an ingest-time
/// monotone millis stamp, so events order within a session.
struct HookEvent {
    kind: String,
    session: String,
    seq: i64,
    json: String,
}

impl HookEvent {
    /// Parse the harness event: `hook_event_name` and `session_id`, both tolerant
    /// of absence (empty string). The raw text is kept verbatim as `json`.
    fn parse(raw: &str) -> Self {
        let v: serde_json::Value = serde_json::from_str(raw).unwrap_or(serde_json::Value::Null);
        let field = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let seq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        HookEvent { kind: field("hook_event_name"), session: field("session_id"), seq, json: raw.to_string() }
    }
}

/// Feed one event into the daemon's warm engine (`hook_event` RPC: append + tick).
fn daemon_feed_event(s: &mut UnixStream, root: &Path, ev: &HookEvent) -> Result<()> {
    let req = crate::rpc::Request::new(1, "hook_event", serde_json::json!({
        "root": root.to_string_lossy(),
        "kind": ev.kind, "session": ev.session, "seq": ev.seq, "json": ev.json,
    }));
    let resp = crate::daemon::rpc_call(s, &req)?;
    if let Some(e) = &resp.error {
        anyhow::bail!("daemon hook_event: {}", e.message);
    }
    Ok(())
}

/// Read the emit rels from the running daemon — the primary reactive engine.
/// Feed the event (append the `hook_event` row + tick), then read the emit rels
/// off the re-derived tables. One connection: feed, then three reads.
fn rels_via_daemon(ev: &HookEvent, program: Option<&str>, root: &Path) -> Result<EmitRels> {
    crate::daemon::ensure_daemon(root, program)?;
    let mut s = crate::daemon::connect()?;
    daemon_feed_event(&mut s, root, ev)?;
    Ok(EmitRels {
        inject: daemon_col(&mut s, root, "rel_inject"),
        skills: daemon_col(&mut s, root, "rel_inject_skill"),
        blocks: daemon_col(&mut s, root, "rel_block"),
        broken: false,
    })
}

/// Cold in-process tick (the no-daemon fallback): open a fresh db, prime a tick
/// so the built-in `hook_event` table exists, append the event, tick again to
/// re-derive, read the emit rels. `broken` if the program has a type error.
fn rels_inproc(ev: &HookEvent, programs: &[String], db_path: Option<&str>, root: &Path) -> Result<EmitRels> {
    let files = crate::resolve_programs(programs, root)?;
    let (mut prog, type_diags, _) = crate::prepare_paths(&files)?;
    // `?` queries and `gen` never run from a hook tick: emit/observe, not codegen.
    prog.items.retain(|i| !matches!(i, ast::Item::Query(_) | ast::Item::Gen(_)));
    if type_diags.iter().any(|d| d.severity == ast::Severity::Error) {
        for d in type_diags.iter().filter(|d| d.severity == ast::Severity::Error) {
            eprintln!("dl --hook: program error: {}", d.msg);
        }
        return Ok(EmitRels { broken: true, ..Default::default() });
    }
    let conn = crate::db::open(db_path)?;
    let mut eng = crate::engine::Engine::new(conn, root.to_path_buf());
    // Prime: declares the built-in tables (hook_event included) so the append
    // never races the schema. Then feed + re-tick.
    eng.tick(&prog, true)?;
    eng.insert_hook_event(&ev.kind, &ev.session, ev.seq, &ev.json)?;
    eng.tick(&prog, true)?;
    Ok(EmitRels {
        inject: emit_col(&eng, "inject"),
        skills: emit_col(&eng, "inject_skill"),
        blocks: emit_col(&eng, "block"),
        broken: false,
    })
}

/// `dl --hook`: read the event, get the emit rels (daemon-first, in-process
/// fallback), emit the harness hook JSON. Exit 0 for a well-formed program (block
/// rides the JSON, not the code), 1 if the program is broken (user-facing only,
/// never fed to the agent — same 1/2 split as `--check`).
pub fn run_hook(programs: &[String], db_path: Option<&str>, root: PathBuf) -> Result<i32> {
    // Drain stdin (the harness pipes the event) and parse it into a `hook_event`
    // row: kind = the event name, session = its session id, json = the raw text.
    // The condition reads the row via term-form json/jsonp, so any event field is
    // reachable from the .dl program without a per-field engine column.
    let mut stdin_s = String::new();
    std::io::stdin().read_to_string(&mut stdin_s).ok();
    let ev = HookEvent::parse(&stdin_s);

    // Prefer the daemon (primary mode): feed the event into its warm engine, then
    // read the emit rels off the re-derived tables. Fall back to a cold in-process
    // tick when no daemon serves this root, or if attach fails.
    let rels = if crate::daemon::enabled_for(&root) && db_path.is_none() && programs.len() <= 1 {
        match rels_via_daemon(&ev, programs.first().map(|s| s.as_str()), &root) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[daemon] hook attach failed, in-process: {e}");
                rels_inproc(&ev, programs, db_path, &root)?
            }
        }
    } else {
        rels_inproc(&ev, programs, db_path, &root)?
    };
    if rels.broken {
        return Ok(1);
    }

    // A block short-circuits: emit the decision, inject nothing.
    if let Some(reason) = rels.blocks.into_iter().next() {
        println!("{}", serde_json::json!({ "decision": "block", "reason": reason }));
        return Ok(0);
    }

    // The rule already filtered `inject_skill` against `!skill_loaded`, so every
    // row here is a skill to inject. Resolve each to its SKILL.md body.
    let mut ctx = rels.inject;
    for name in rels.skills {
        match resolve_skill(&name, &root) {
            Some(body) => ctx.push(format!("# Skill `{name}` (auto-loaded by dl --hook)\n\n{body}")),
            None => eprintln!("dl --hook: skill `{name}` not found under .claude/skills"),
        }
    }

    if ctx.is_empty() {
        return Ok(0); // silent: condition didn't fire (or all skills already loaded)
    }
    // `additionalContext` rides `hookSpecificOutput.hookEventName`, which must
    // echo the event we received (UserPromptSubmit / PostToolUse / ...); the
    // harness keys the output arm off it. Fall back to PostToolUse when the event
    // carried no name (an old harness, or a bare invocation).
    let event_name = if ev.kind.is_empty() { "PostToolUse" } else { ev.kind.as_str() };
    let out = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": event_name,
            "additionalContext": ctx.join("\n\n---\n\n"),
        }
    });
    println!("{out}");
    Ok(0)
}
