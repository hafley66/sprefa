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
fn daemon_col(s: &mut UnixStream, table: &str) -> Vec<String> {
    let req = crate::rpc::Request::new(1, "query_sql",
        serde_json::json!({ "sql": format!("SELECT * FROM {table}") }));
    let Ok(resp) = crate::daemon::rpc_call(s, &req) else { return Vec::new() };
    if resp.error.is_some() { return Vec::new(); }
    resp.result.and_then(|v| v.get("rows").cloned())
        .and_then(|v| v.as_array().cloned()).unwrap_or_default()
        .iter()
        .filter_map(|row| row.as_array().and_then(|r| r.first()).map(val_str))
        .collect()
}

/// Read the emit rels from the running daemon — the primary reactive engine,
/// which already ticked when the agent's edit touched a watched source file. No
/// re-tick here: attach and read. One connection, three reads.
fn rels_via_daemon(program: Option<&str>, root: &Path) -> Result<EmitRels> {
    crate::daemon::ensure_daemon(root, program)?;
    let mut s = crate::daemon::connect(Some(root))?;
    Ok(EmitRels {
        inject: daemon_col(&mut s, "rel_inject"),
        skills: daemon_col(&mut s, "rel_inject_skill"),
        blocks: daemon_col(&mut s, "rel_block"),
        broken: false,
    })
}

/// Cold in-process tick (the no-daemon fallback): open a fresh db, tick, read the
/// emit rels. `broken` if the program has a type error.
fn rels_inproc(program: Option<&str>, db_path: Option<&str>, root: &Path) -> Result<EmitRels> {
    let files = crate::resolve_programs(program, root)?;
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
pub fn run_hook(program: Option<&str>, db_path: Option<&str>, root: PathBuf) -> Result<i32> {
    // Drain stdin (the harness pipes the event); the condition reads the agent
    // built-ins, so the live payload isn't threaded into a relation yet. Feeding
    // it in as a `hook_event` rel is the next increment.
    let mut stdin_s = String::new();
    std::io::stdin().read_to_string(&mut stdin_s).ok();

    // Prefer the daemon (primary mode): it already re-ticked on the agent's edit,
    // so the emit rels are fresh — read them, don't recompute. Fall back to a cold
    // in-process tick when no daemon serves this root, or if attach fails.
    let rels = if crate::daemon::enabled_for(&root) && db_path.is_none() {
        match rels_via_daemon(program, &root) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[daemon] hook attach failed, in-process: {e}");
                rels_inproc(program, db_path, &root)?
            }
        }
    } else {
        rels_inproc(program, db_path, &root)?
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
    let out = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "additionalContext": ctx.join("\n\n---\n\n"),
        }
    });
    println!("{out}");
    Ok(0)
}
