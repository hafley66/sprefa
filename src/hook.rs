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
//! Harness dialects: the stdin/stdout JSON shape is a pure I/O arm per harness
//! ([`HookDialect`]). The rel contract (`inject`/`inject_skill`/`block`) and
//! the daemon-first feed are dialect-independent.
//!
//! - **claude**: Claude Code's hook JSON (`hook_event_name` in,
//!   `hookSpecificOutput` / `{"decision":"block"}` out).
//! - **codex**: byte-compatible with claude — Codex CLI implements the Claude
//!   Code hook wire format (schema evidence at the codex note on
//!   [`HookEvent::parse`]). Selected only via `--dialect codex`, never
//!   auto-detected.
//! - **opencode**: opencode has no native hook config; our shipped plugin
//!   (`assets/dl-opencode-plugin.js`) translates its plugin events into the
//!   neutral input `{kind, session, json}` and applies the neutral output
//!   (`{"inject": text}` / `{"block": reason}`). Our plugin, our schema.

use crate::ast;
use crate::engine::DiagRow;
use crate::stage;
use anyhow::Result;
use std::collections::HashSet;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

/// R7: how many sample diags an `agent-session` summary lists under the counts.
const SESSION_TOP_N: usize = 10;

/// The harness whose JSON shapes `dl --hook` reads on stdin and writes on
/// stdout. Parse/render arms only — no engine or rel-contract difference.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HookDialect {
    Claude,
    Codex,
    OpenCode,
}

impl HookDialect {
    /// The `--dialect` flag values, named plainly after the harnesses.
    pub fn from_flag(s: &str) -> Result<Self> {
        match s {
            "claude" => Ok(HookDialect::Claude),
            "codex" => Ok(HookDialect::Codex),
            "opencode" => Ok(HookDialect::OpenCode),
            other => anyhow::bail!(
                "unknown --dialect `{other}` (expected claude, codex, or opencode)"
            ),
        }
    }

    /// Conservative auto-detect off the payload shape: `hook_event_name`
    /// present = claude; the neutral `{kind, ...}` shape = opencode (only our
    /// own plugin sends it). Codex is NEVER auto-detected — its payload is
    /// byte-identical to claude's, so an explicit `--dialect codex` is the
    /// only way to select it (parse/render are shared anyway). Anything else
    /// falls back to claude (the historical default of a bare `dl --hook`).
    fn detect(v: &serde_json::Value) -> Self {
        if v.get("hook_event_name").is_some() {
            HookDialect::Claude
        } else if v.get("kind").is_some() {
            HookDialect::OpenCode
        } else {
            HookDialect::Claude
        }
    }
}

/// Skill search order: `<root>/.agents/skills` (the cross-harness authoring
/// home) -> `<root>/.claude/skills` -> `~/.agents/skills` -> `~/.claude/skills`.
/// Returns the SKILL.md body (the text injected as additionalContext).
fn resolve_skill(name: &str, root: &Path) -> Option<String> {
    let mut cands = vec![
        root.join(".agents/skills").join(name).join("SKILL.md"),
        root.join(".claude/skills").join(name).join("SKILL.md"),
    ];
    if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
        cands.push(h.join(".agents/skills").join(name).join("SKILL.md"));
        cands.push(h.join(".claude/skills").join(name).join("SKILL.md"));
    }
    cands.into_iter().find_map(|c| std::fs::read_to_string(&c).ok())
}

/// The three emit relations read off one tick, plus the R7 staged-diagnostics
/// inputs. `broken` is the in-process 1/2-split signal (a malformed program
/// surfaces to the user, never the agent).
#[derive(Default)]
struct EmitRels {
    inject: Vec<String>,
    skills: Vec<String>,
    blocks: Vec<String>,
    broken: bool,
    /// R7: every `diag` row, unfiltered; the routing filter runs at render time.
    diags: Vec<DiagRow>,
    /// R7: the `diag_stage` [code, stage] routing rows.
    stage_rows: Vec<Vec<String>>,
    /// R7: the latest agent turn's touched paths (`agent_touch`).
    touch_paths: Vec<String>,
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
    /// Parse the harness event per dialect, tolerant of absent fields (empty
    /// string). The raw text is kept verbatim as `json`. `dialect = None` =
    /// auto-detect ([`HookDialect::detect`]); returns the dialect used so the
    /// render arm matches the parse arm.
    ///
    /// The **codex** arm is byte-identical to claude's: Codex CLI 0.144.1
    /// implements the Claude Code hook wire format. Verified against the
    /// installed binary's embedded schemas (codex 0.144.1); live fire blocked
    /// by hook trust (codex only runs hooks whose hash is trusted in
    /// `~/.codex/config.toml [hooks.state]`). Extracted evidence, e.g.
    /// `post-tool-use.command.input` (draft-07, additionalProperties:false):
    ///   required: cwd, hook_event_name (const "PostToolUse"), model,
    ///             permission_mode, session_id, tool_input, tool_name,
    ///             tool_response, tool_use_id, transcript_path, turn_id
    /// and `user-prompt-submit.command.input`:
    ///   required: cwd, hook_event_name (const "UserPromptSubmit"), model,
    ///             permission_mode, prompt, session_id, transcript_path, turn_id
    /// Output wires: `BlockDecisionWire` = {"decision":"block","reason":...};
    /// `hookSpecificOutput` = {hookEventName, additionalContext}. Because the
    /// output schemas set additionalProperties:false, the render arm must emit
    /// nothing beyond those fields (the claude arm already complies).
    ///
    /// The **opencode** arm reads the neutral shape our shipped plugin sends:
    /// `{kind, session, json?}` — kind/session are strings; the raw stdin text
    /// is kept whole as the row's `json` either way.
    fn parse(raw: &str, dialect: Option<HookDialect>) -> (Self, HookDialect) {
        let v: serde_json::Value = serde_json::from_str(raw).unwrap_or(serde_json::Value::Null);
        let dialect = dialect.unwrap_or_else(|| HookDialect::detect(&v));
        let field = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let (kind, session) = match dialect {
            HookDialect::Claude | HookDialect::Codex => {
                (field("hook_event_name"), field("session_id"))
            }
            HookDialect::OpenCode => (field("kind"), field("session")),
        };
        let seq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        (HookEvent { kind, session, seq, json: raw.to_string() }, dialect)
    }
}

/// Render a block decision per dialect. claude/codex share the Claude Code
/// wire (`BlockDecisionWire`); opencode gets the neutral `{"block": reason}`
/// our plugin applies.
fn render_block(dialect: HookDialect, reason: &str) -> String {
    match dialect {
        HookDialect::Claude | HookDialect::Codex => {
            serde_json::json!({ "decision": "block", "reason": reason }).to_string()
        }
        HookDialect::OpenCode => serde_json::json!({ "block": reason }).to_string(),
    }
}

/// Render the context injection per dialect. `event_kind` is the event name to
/// echo (claude/codex key the output arm off `hookEventName`); opencode gets
/// the neutral `{"inject": text}`.
fn render_inject(dialect: HookDialect, event_kind: &str, ctx: &str) -> String {
    match dialect {
        HookDialect::Claude | HookDialect::Codex => serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": event_kind,
                "additionalContext": ctx,
            }
        })
        .to_string(),
        HookDialect::OpenCode => serde_json::json!({ "inject": ctx }).to_string(),
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

/// One `diag` row parsed off the daemon `diag` RPC's JSON (the `diag_to_json`
/// shape: `message` for the text, `line`/`col`/`endLine`/`endCol` ints).
fn diag_from_json(v: &serde_json::Value) -> DiagRow {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let i = |k: &str| v.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
    let hint = v.get("hint").and_then(|x| x.as_str()).filter(|h| !h.is_empty()).map(str::to_string);
    DiagRow {
        path: s("path"),
        line: i("line"),
        col: i("col"),
        end_line: i("endLine"),
        end_col: i("endCol"),
        severity: s("severity"),
        code: s("code"),
        msg: s("message"),
        hint,
    }
}

/// R7: read the diags + `diag_stage` routes + `agent_touch` paths off the
/// daemon in one `diag` RPC (the handler bundles all three). A missing field is
/// tolerated as empty.
fn diags_via_daemon(s: &mut UnixStream, root: &Path) -> (Vec<DiagRow>, Vec<Vec<String>>, Vec<String>) {
    let req = crate::rpc::Request::new(1, "diag",
        serde_json::json!({ "root": root.to_string_lossy() }));
    let Ok(resp) = crate::daemon::rpc_call(s, &req) else { return (vec![], vec![], vec![]) };
    if resp.error.is_some() { return (vec![], vec![], vec![]); }
    let Some(result) = resp.result else { return (vec![], vec![], vec![]) };
    let diags = result.get("rows").and_then(|v| v.as_array())
        .map(|a| a.iter().map(diag_from_json).collect()).unwrap_or_default();
    let stage_rows = result.get("stages").and_then(|v| v.as_array()).map(|a| a.iter()
        .filter_map(|row| row.as_array())
        .map(|cells| cells.iter().map(|c| c.as_str().unwrap_or("").to_string()).collect())
        .collect()).unwrap_or_default();
    let touch = result.get("touch").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    (diags, stage_rows, touch)
}

/// Read the emit rels from the running daemon — the primary reactive engine.
/// Feed the event (append the `hook_event` row + tick), then read the emit rels
/// off the re-derived tables. One connection: feed, then the reads.
fn rels_via_daemon(ev: &HookEvent, program: Option<&str>, root: &Path) -> Result<EmitRels> {
    crate::daemon::ensure_daemon(root, program)?;
    let mut s = crate::daemon::connect()?;
    daemon_feed_event(&mut s, root, ev)?;
    let (diags, stage_rows, touch_paths) = diags_via_daemon(&mut s, root);
    Ok(EmitRels {
        inject: daemon_col(&mut s, root, "rel_inject"),
        skills: daemon_col(&mut s, root, "rel_inject_skill"),
        blocks: daemon_col(&mut s, root, "rel_block"),
        broken: false,
        diags,
        stage_rows,
        touch_paths,
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
    let touch_paths = eng.rel_rows("agent_touch", 3)
        .into_iter().filter_map(|r| r.into_iter().nth(2)).collect();
    Ok(EmitRels {
        inject: emit_col(&eng, "inject"),
        skills: emit_col(&eng, "inject_skill"),
        blocks: emit_col(&eng, "block"),
        broken: false,
        diags: eng.diags(None).unwrap_or_default(),
        stage_rows: eng.rel_rows("diag_stage", 2),
        touch_paths,
    })
}

/// R7: render the routed diagnostics as agent context for this hook event. A
/// session-boundary event (`SessionStart`/`SessionEnd`) gets the `agent-session`
/// summary — per-code counts plus a top-N sample; every other event gets the
/// `agent-turn` list, gated to the files the latest turn touched (an agent
/// hears about what it just touched, never the whole audit list). Returns None
/// when nothing routes to this surface.
fn staged_diag_context(
    kind: &str,
    diags: Vec<DiagRow>,
    stage_rows: &[Vec<String>],
    touch_paths: &[String],
) -> Option<String> {
    let routes = stage::routes_from_rows(stage_rows);
    if stage::is_session_event(kind) {
        let staged = stage::stage_filter(diags, "agent-session", &routes);
        if staged.is_empty() {
            return None;
        }
        let (counts, sample) = stage::session_summary(&staged, SESSION_TOP_N);
        let mut out = String::from("# dl diagnostics (session summary)\n\n");
        for (code, n) in &counts {
            let label = if code.is_empty() { "(no code)" } else { code.as_str() };
            out.push_str(&format!("- {label}: {n}\n"));
        }
        out.push_str("\ntop:\n");
        for d in sample {
            out.push_str(&format!("- {}:{}: {}: {}\n", d.path, d.line, d.severity, d.msg));
        }
        tracing::debug!(kind, codes = counts.len(), total = staged.len(),
            "hook agent-session diag summary");
        Some(out)
    } else {
        let staged = stage::stage_filter(diags, "agent-turn", &routes);
        let touched: HashSet<String> = touch_paths.iter().cloned().collect();
        let staged = stage::touched_only(staged, &touched);
        if staged.is_empty() {
            return None;
        }
        let mut out = String::from("# dl diagnostics (files edited this turn)\n\n");
        for d in &staged {
            let code = if d.code.is_empty() { String::new() } else { format!("[{}]", d.code) };
            out.push_str(&format!("- {}:{}: {}{}: {}\n", d.path, d.line, d.severity, code, d.msg));
        }
        tracing::debug!(kind, kept = staged.len(), "hook agent-turn diag routing");
        Some(out)
    }
}

/// `dl --hook`: read the event, get the emit rels (daemon-first, in-process
/// fallback), emit the harness hook JSON. Exit 0 for a well-formed program (block
/// rides the JSON, not the code), 1 if the program is broken (user-facing only,
/// never fed to the agent — same 1/2 split as `--check`).
pub fn run_hook(
    programs: &[String],
    db_path: Option<&str>,
    root: PathBuf,
    dialect: Option<HookDialect>,
) -> Result<i32> {
    // Drain stdin (the harness pipes the event) and parse it into a `hook_event`
    // row: kind = the event name, session = its session id, json = the raw text.
    // The condition reads the row via term-form json/jsonp, so any event field is
    // reachable from the .dl program without a per-field engine column.
    let mut stdin_s = String::new();
    std::io::stdin().read_to_string(&mut stdin_s).ok();
    let (ev, dialect) = HookEvent::parse(&stdin_s, dialect);

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
        eprintln!("[hook] no daemon serving this root — one-shot engine on .dl/.state/cache.db (start one: dl daemon start)");
        rels_inproc(&ev, programs, db_path, &root)?
    };
    if rels.broken {
        return Ok(1);
    }

    // A block short-circuits: emit the decision, inject nothing.
    if let Some(reason) = rels.blocks.into_iter().next() {
        println!("{}", render_block(dialect, &reason));
        return Ok(0);
    }

    // The rule already filtered `inject_skill` against `!skill_loaded`, so every
    // row here is a skill to inject. Resolve each to its SKILL.md body.
    let mut ctx = rels.inject;
    for name in rels.skills {
        match resolve_skill(&name, &root) {
            Some(body) => ctx.push(format!("# Skill `{name}` (auto-loaded by dl --hook)\n\n{body}")),
            None => eprintln!("dl --hook: skill `{name}` not found under .agents/skills or .claude/skills"),
        }
    }

    // R7: append the routed diagnostics for this surface — the agent-turn list
    // (gated to touched files) or the agent-session summary. The db keeps every
    // diag; this is presentation-time routing only.
    if let Some(staged) = staged_diag_context(&ev.kind, rels.diags, &rels.stage_rows, &rels.touch_paths) {
        ctx.push(staged);
    }

    if ctx.is_empty() {
        return Ok(0); // silent: condition didn't fire (or all skills already loaded)
    }
    // claude/codex: `additionalContext` rides `hookSpecificOutput.hookEventName`,
    // which must echo the event we received (UserPromptSubmit / PostToolUse /
    // ...); the harness keys the output arm off it. Fall back to PostToolUse
    // when the event carried no name (an old harness, or a bare invocation).
    let event_name = if ev.kind.is_empty() { "PostToolUse" } else { ev.kind.as_str() };
    println!("{}", render_inject(dialect, event_name, &ctx.join("\n\n---\n\n")));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_flag_parses_the_three_harness_names() {
        assert_eq!(HookDialect::from_flag("claude").unwrap(), HookDialect::Claude);
        assert_eq!(HookDialect::from_flag("codex").unwrap(), HookDialect::Codex);
        assert_eq!(HookDialect::from_flag("opencode").unwrap(), HookDialect::OpenCode);
        assert!(HookDialect::from_flag("cursor").is_err());
    }

    #[test]
    fn auto_detect_claude_on_hook_event_name() {
        let raw = r#"{"hook_event_name":"PostToolUse","session_id":"sess9"}"#;
        let (ev, dialect) = HookEvent::parse(raw, None);
        assert_eq!(dialect, HookDialect::Claude);
        assert_eq!(ev.kind, "PostToolUse");
        assert_eq!(ev.session, "sess9");
        assert_eq!(ev.json, raw);
    }

    #[test]
    fn auto_detect_opencode_on_neutral_kind_shape() {
        let raw = r#"{"kind":"PostToolUse","session":"oc1","json":"{}"}"#;
        let (ev, dialect) = HookEvent::parse(raw, None);
        assert_eq!(dialect, HookDialect::OpenCode);
        assert_eq!(ev.kind, "PostToolUse");
        assert_eq!(ev.session, "oc1");
    }

    #[test]
    fn auto_detect_never_picks_codex_and_defaults_to_claude() {
        // A claude/codex-shaped payload auto-detects claude (codex is
        // byte-compatible; only --dialect codex selects it), and a shapeless
        // payload falls back to claude.
        for raw in [
            r#"{"hook_event_name":"UserPromptSubmit","session_id":"a","turn_id":"t"}"#,
            r#"{"something":"else"}"#,
            "not json",
        ] {
            let (_, dialect) = HookEvent::parse(raw, None);
            assert_eq!(dialect, HookDialect::Claude, "payload {raw} must not pick codex");
        }
    }

    #[test]
    fn explicit_codex_dialect_parses_the_claude_fields() {
        let raw = r#"{"hook_event_name":"PostToolUse","session_id":"cx1","tool_name":"shell"}"#;
        let (ev, dialect) = HookEvent::parse(raw, Some(HookDialect::Codex));
        assert_eq!(dialect, HookDialect::Codex);
        assert_eq!(ev.kind, "PostToolUse");
        assert_eq!(ev.session, "cx1");
    }

    #[test]
    fn parse_tolerates_missing_fields_per_dialect() {
        for dialect in [HookDialect::Claude, HookDialect::Codex, HookDialect::OpenCode] {
            let (ev, _) = HookEvent::parse("{}", Some(dialect));
            assert_eq!(ev.kind, "");
            assert_eq!(ev.session, "");
        }
    }

    #[test]
    fn render_matrix_block_and_inject_per_dialect() {
        // claude and codex must be byte-identical (codex output schemas set
        // additionalProperties:false, so nothing extra may ride along).
        let claude_block = render_block(HookDialect::Claude, "no");
        assert_eq!(claude_block, render_block(HookDialect::Codex, "no"));
        assert_eq!(claude_block, r#"{"decision":"block","reason":"no"}"#);

        let claude_inject = render_inject(HookDialect::Claude, "PostToolUse", "ctx");
        assert_eq!(claude_inject, render_inject(HookDialect::Codex, "PostToolUse", "ctx"));
        let v: serde_json::Value = serde_json::from_str(&claude_inject).unwrap();
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PostToolUse");
        assert_eq!(v["hookSpecificOutput"]["additionalContext"], "ctx");
        assert_eq!(v.as_object().unwrap().len(), 1, "nothing beyond hookSpecificOutput");
        assert_eq!(v["hookSpecificOutput"].as_object().unwrap().len(), 2);

        // opencode: the neutral shapes the shipped plugin applies.
        assert_eq!(render_block(HookDialect::OpenCode, "no"), r#"{"block":"no"}"#);
        assert_eq!(render_inject(HookDialect::OpenCode, "PostToolUse", "ctx"), r#"{"inject":"ctx"}"#);
    }
}
