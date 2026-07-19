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
use std::path::{Path, PathBuf};

/// R7: how many sample diags an `agent-session` summary lists under the counts.
const SESSION_TOP_N: usize = 10;

/// Whole-invocation wall deadline for `dl --hook`, in milliseconds
/// (`DL_HOOK_DEADLINE_MS`; `0` disables the deadline AND the cold-db skip —
/// the internal test-harness escape the `--hook` it-suites run under).
/// Default 900ms: an agent harness gives a hook command roughly one second
/// before the turn visibly stalls, and the give-up path still needs headroom
/// for process spawn, stdin drain, and the final write — so the engine work
/// gets 900 of those ~1000ms. On expiry dl emits the dialect-correct no-op
/// (empty stdout, exit 0 — never 2, which would block the agent) and abandons
/// the worker. Prior art: the one-shot `DL_MAX_WALL_SECS` watchdog
/// (`src/watchdog.rs`) — same thread+channel shape, hook-sized budget.
const DEFAULT_HOOK_DEADLINE_MS: u64 = 900;

/// The hook deadline in milliseconds (`DL_HOOK_DEADLINE_MS`, `0` = disabled).
/// A malformed value falls back to the default rather than disabling the guard
/// (same posture as `watchdog::max_wall_secs`).
fn hook_deadline_ms() -> u64 {
    match std::env::var("DL_HOOK_DEADLINE_MS") {
        Ok(s) => s.trim().parse().unwrap_or(DEFAULT_HOOK_DEADLINE_MS),
        Err(_) => DEFAULT_HOOK_DEADLINE_MS,
    }
}

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
fn daemon_col(s: &mut crate::daemon_client::DaemonClient, root: &Path, table: &str) -> Vec<String> {
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
fn daemon_feed_event(s: &mut crate::daemon_client::DaemonClient, root: &Path, ev: &HookEvent) -> Result<()> {
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
fn diags_via_daemon(s: &mut crate::daemon_client::DaemonClient, root: &Path) -> (Vec<DiagRow>, Vec<Vec<String>>, Vec<String>) {
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
///
/// Attach-only: the caller checked `is_running`; a lost race lands in the
/// attach-failed fallback. A hook NEVER spawns (or build-id-replaces) the
/// daemon — a boot + cold build can never answer within the hook deadline, and
/// implicit autostart is exactly what seeded the kill-respawn storms
/// (failure-modes classes 16+17).
///
/// Cancellation residual: on deadline expiry the client process exits, which
/// closes this socket — but the daemon's `hook_event` handler runs its tick to
/// completion regardless. Cheap daemon-side cancellation would need the tick
/// loop to poll a per-request cancel flag keyed by `req_id` (`src/reqid.rs`;
/// `JobRow.req_id` is plumbed but always `None` today) — deep plumbing, so the
/// let-go stops at dropping the connection.
fn rels_via_daemon(ev: &HookEvent, root: &Path) -> Result<EmitRels> {
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

/// In-process tick (the no-daemon fallback): open the db (the shared per-root
/// `roots/<key>/db.sqlite` in discovery mode — storage-endgame L2 — warm when
/// a daemon ever served this root), prime a tick so the built-in `hook_event`
/// table exists, append the event, tick again to re-derive, read the emit
/// rels. `broken` if the program has a type error. WAL + busy_timeout covers
/// the daemon-up-but-attach-failed write pairing.
fn rels_inproc(ev: &HookEvent, programs: &[String], db_path: Option<&str>, root: &Path) -> Result<EmitRels> {
    let files = crate::resolve_programs(programs, root)?;
    let (mut prog, type_diags, _) = crate::prepare_paths(&files)?;
    // `?` queries and `gen` never run from a hook tick: emit/observe, not codegen.
    prog.items.retain(|i| !matches!(i, ast::Item::Query(_) | ast::Item::Gen(_)));
    if type_diags.iter().any(|d| d.severity == ast::Severity::Error) {
        for d in type_diags.iter().filter(|d| d.severity == ast::Severity::Error) {
            let msg = &d.msg;
            tracing::warn!(msg = %msg, "dl --hook: program error: {msg}");
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

/// What the hook run decided to say: `payload` is the one stdout JSON line
/// (`None` = the silent no-op) and `code` the process exit code. Rendering is
/// separated from printing so the deadline wrapper owns stdout: an abandoned
/// worker must never race a line onto the pipe after the give-up.
struct HookOutcome {
    payload: Option<String>,
    code: i32,
}

impl HookOutcome {
    /// The dialect-correct no-op: empty stdout + exit 0 reads as "proceed
    /// silently" in every supported harness (claude/codex/opencode).
    fn noop() -> Self {
        HookOutcome { payload: None, code: 0 }
    }
}

/// True when the in-process fallback would be a COLD engine: no db path (the
/// in-memory engine is blank by construction) or a blank/absent db file.
fn inproc_db_is_cold(db_path: Option<&str>) -> bool {
    match db_path {
        None => true,
        Some(p) => std::fs::metadata(p).map(|m| m.len() == 0).unwrap_or(true),
    }
}

/// The whole hook body — stdin drain, event parse, daemon-first emit-rel read,
/// render — runs on the deadline worker thread. `cold_skip` (deadline active)
/// refuses to start a cold in-process build: it can never fit under the hook
/// budget, and a hook is the wrong place to start one.
fn hook_work(
    programs: Vec<String>,
    db_path: Option<String>,
    root: PathBuf,
    dialect: Option<HookDialect>,
    cold_skip: bool,
) -> Result<HookOutcome> {
    // Drain stdin (the harness pipes the event) and parse it into a `hook_event`
    // row: kind = the event name, session = its session id, json = the raw text.
    // The condition reads the row via term-form json/jsonp, so any event field is
    // reachable from the .dl program without a per-field engine column.
    let mut stdin_s = String::new();
    std::io::stdin().read_to_string(&mut stdin_s).ok();
    let (ev, dialect) = HookEvent::parse(&stdin_s, dialect);
    // Injected slow point (DL_TEST_HANG_SECS): inert in real runs; lets the
    // deadline it-test face work that exceeds the budget deterministically.
    crate::watchdog::test_hang_hook();

    let db_path = db_path.as_deref();
    // The in-process arm, gated: a blank/absent db under an active deadline is
    // an immediate no-op (`None`) instead of a cold build.
    let inproc_or_skip = |note: &str| -> Result<Option<EmitRels>> {
        if cold_skip && inproc_db_is_cold(db_path) {
            tracing::warn!(
                db = db_path.unwrap_or("<in-memory>"),
                note,
                "[hook] cold in-process engine skipped — no-op (a cold build cannot fit the hook deadline)"
            );
            tracing::warn!(
                note,
                "[hook] db blank or absent — cold build skipped, no-op ({note}); warm it: dl daemon start, or a one-shot dl run"
            );
            return Ok(None);
        }
        rels_inproc(&ev, &programs, db_path, &root).map(Some)
    };

    // Prefer the daemon (primary mode): feed the event into its warm engine, then
    // read the emit rels off the re-derived tables. Attach-only — see
    // [`rels_via_daemon`] for why a hook never autostarts. Fall back to the
    // (cold-gated) in-process tick when no daemon serves this root or attach fails.
    let rels = if crate::daemon::enabled_for(&root) && db_path.is_none() && programs.len() <= 1 {
        if crate::daemon::is_running() {
            match rels_via_daemon(&ev, &root) {
                Ok(r) => Some(r),
                Err(e) => {
                    tracing::warn!(error = %e, "[daemon] hook attach failed, in-process: {e}");
                    inproc_or_skip("daemon attach failed")?
                }
            }
        } else {
            tracing::warn!("[hook] no daemon running — a hook never starts one (dl daemon start)");
            inproc_or_skip("no daemon running")?
        }
    } else {
        tracing::warn!("[hook] no daemon serving this root — one-shot engine on the shared root db (roots/<key>/db.sqlite; start a daemon: dl daemon start)");
        inproc_or_skip("root not daemon-served")?
    };
    let Some(rels) = rels else {
        return Ok(HookOutcome::noop());
    };
    if rels.broken {
        return Ok(HookOutcome { payload: None, code: 1 });
    }

    // A block short-circuits: emit the decision, inject nothing.
    if let Some(reason) = rels.blocks.into_iter().next() {
        return Ok(HookOutcome { payload: Some(render_block(dialect, &reason)), code: 0 });
    }

    // The rule already filtered `inject_skill` against `!skill_loaded`, so every
    // row here is a skill to inject. Resolve each to its SKILL.md body.
    let mut ctx = rels.inject;
    for name in rels.skills {
        match resolve_skill(&name, &root) {
            Some(body) => ctx.push(format!("# Skill `{name}` (auto-loaded by dl --hook)\n\n{body}")),
            None => tracing::warn!(name = %name, "dl --hook: skill `{name}` not found under .agents/skills or .claude/skills"),
        }
    }

    // R7: append the routed diagnostics for this surface — the agent-turn list
    // (gated to touched files) or the agent-session summary. The db keeps every
    // diag; this is presentation-time routing only.
    if let Some(staged) = staged_diag_context(&ev.kind, rels.diags, &rels.stage_rows, &rels.touch_paths) {
        ctx.push(staged);
    }

    if ctx.is_empty() {
        return Ok(HookOutcome::noop()); // silent: condition didn't fire (or all skills already loaded)
    }
    // claude/codex: `additionalContext` rides `hookSpecificOutput.hookEventName`,
    // which must echo the event we received (UserPromptSubmit / PostToolUse /
    // ...); the harness keys the output arm off it. Fall back to PostToolUse
    // when the event carried no name (an old harness, or a bare invocation).
    let event_name = if ev.kind.is_empty() { "PostToolUse" } else { ev.kind.as_str() };
    Ok(HookOutcome {
        payload: Some(render_inject(dialect, event_name, &ctx.join("\n\n---\n\n"))),
        code: 0,
    })
}

/// `dl --hook`: read the event, get the emit rels (daemon-first, in-process
/// fallback), emit the harness hook JSON. Exit 0 for a well-formed program (block
/// rides the JSON, not the code), 1 if the program is broken (user-facing only,
/// never fed to the agent — same 1/2 split as `--check`).
///
/// The whole invocation self-times-out (`DL_HOOK_DEADLINE_MS`, default
/// [`DEFAULT_HOOK_DEADLINE_MS`]): the body runs on a worker thread under
/// [`crate::watchdog::run_with_deadline`]; on expiry dl lets go — no-op reply
/// (empty stdout), exit 0, a warn-level trace naming elapsed + engine phase —
/// and the abandoned worker dies with the process (the CLI exit right after
/// this return closes any daemon socket it held).
pub fn run_hook(
    programs: &[String],
    db_path: Option<&str>,
    root: PathBuf,
    dialect: Option<HookDialect>,
) -> Result<i32> {
    let deadline_ms = hook_deadline_ms();
    let started = std::time::Instant::now();
    let outcome = if deadline_ms == 0 {
        // Test-harness escape: unbounded, inline, cold builds allowed.
        Some(hook_work(programs.to_vec(), db_path.map(str::to_string), root, dialect, false)?)
    } else {
        let programs = programs.to_vec();
        let db_path = db_path.map(str::to_string);
        crate::watchdog::run_with_deadline(
            std::time::Duration::from_millis(deadline_ms),
            "dl-hook-work",
            move || hook_work(programs, db_path, root, dialect, true),
        )?
    };
    match outcome {
        Some(out) => {
            if let Some(line) = out.payload {
                println!("{line}");
            }
            Ok(out.code)
        }
        None => {
            // Deadline hit: the give-up is the answer to "why was the hook
            // slow" — record elapsed + the engine phase the worker was in
            // (warn-level, lands in dl.log), plus one stderr line (exit-0
            // stderr only surfaces in harness debug views). The invocation row
            // closes with exit 0 in invlog at the CLI exit seam.
            let a = crate::activity::snapshot();
            let phase = a.phase.as_str();
            tracing::warn!(
                elapsed_ms = started.elapsed().as_millis() as u64,
                deadline_ms,
                phase,
                detail = %a.detail,
                "[hook] deadline hit — abandoning work, no-op reply"
            );
            tracing::warn!(
                deadline_ms,
                phase,
                "[hook] deadline {deadline_ms}ms hit (phase={phase}) — no-op; raise with DL_HOOK_DEADLINE_MS=<ms> or 0 to disable"
            );
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The deadline env parse (the 0-disables + malformed-falls-back contract);
    // the give-up behavior itself is exercised end-to-end by the subprocess
    // it-test (tests/it/hook_deadline.rs), the only place an abandoned worker
    // thread and process exit are safe to observe.
    #[test]
    fn deadline_env_parse() {
        std::env::remove_var("DL_HOOK_DEADLINE_MS");
        assert_eq!(hook_deadline_ms(), DEFAULT_HOOK_DEADLINE_MS);
        std::env::set_var("DL_HOOK_DEADLINE_MS", "0");
        assert_eq!(hook_deadline_ms(), 0, "0 disables");
        std::env::set_var("DL_HOOK_DEADLINE_MS", "250");
        assert_eq!(hook_deadline_ms(), 250);
        std::env::set_var("DL_HOOK_DEADLINE_MS", "garbage");
        assert_eq!(hook_deadline_ms(), DEFAULT_HOOK_DEADLINE_MS, "malformed falls back to default");
        std::env::remove_var("DL_HOOK_DEADLINE_MS");
    }

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
