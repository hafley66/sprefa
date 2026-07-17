//! The `@async` / `@stream` / `sh` effect runtime, extracted from engine.rs
//! (engine breakdown Stage 5). Two parts: the executor surface (the `EffectExec`
//! trait + the real-IO `ShellEffectExec` + the program-derived registries) and
//! the engine-side drain methods (`rebuild_async` queues `pending_effect` rows
//! per @async body solution; `drain_effects` / `drain_streams` run them off-tick
//! through an `EffectExec`). The drain methods stay `impl Engine` (full access to
//! the engine's pub(crate) seam: `db` / `rels` / `insert_rows` / `refresh_rel`),
//! just defined here so the ~430 lines leave engine.rs. engine.rs re-exports the
//! externally-used names (`EffectExec` / `ShellEffectExec` / `async_effect_arity`
//! / `shell_templates`) so daemon/test call sites keep their `engine::` paths.

use anyhow::{bail, Result};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use crate::ast::*;
use crate::engine::Engine;

/// One head slot of an `@async` rule, classifying how its value is filled when
/// the effect response lands: a literal constant, a body-bound request arg
/// (echoed from the request), or the i-th value the executor returned.
/// The distinct variables a positive body atom binds, in first-appearance order.
/// For an `@async` rule these are the request args passed to the executor.
pub(crate) fn async_bound_vars(rule: &Rule) -> Vec<String> {
    let mut seen = Vec::new();
    for b in &rule.body {
        if let BodyItem::Pos(a) = b {
            for t in &a.terms {
                if let Term::Var(v) = t {
                    if !seen.contains(v) { seen.push(v.clone()); }
                }
            }
        }
    }
    seen
}

/// Runs one queued `@async` request. `kind` is the response rel name; `args` is
/// the bound-var object the rule body projected; the return is one string per
/// unbound head slot, in head order. Pure: it holds no engine state, lives in the
/// daemon, and is the only place an `@async` effect actually executes (the shell
/// `gh`/`git`/http call). See docs/research-reactive-effectful-datalog.md §8.
/// `Sync` so one executor instance is shared across the rayon pool when
/// `drain_effects` runs the pending requests in parallel (the subprocess spawn is
/// the slow part; the DB writes stay serial and batched).
pub trait EffectExec: Sync {
    fn run(&self, kind: &str, args: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<String>>;
    /// A streaming/batch executor: each OUTPUT LINE becomes one response row,
    /// split into the kind's output slots (a `@tsv` row, D-7). A `sh*` (`@stream`)
    /// effect drains through this so one subprocess yields N head rows; the
    /// default wraps `run` as a single row, so a plain mock exec still drives a
    /// stream in tests. See `drain_streams`.
    fn run_stream(&self, kind: &str, args: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<Vec<String>>> {
        Ok(vec![self.run(kind, args)?])
    }
    /// Whether `kind` has an executor registered to run it. `drain_effects` /
    /// `drain_streams` call this BEFORE spawning anything so a `pending_effect`
    /// row whose kind lost its template (a `.dl` edit dropped the `sh`/
    /// `effect_cmd` decl while a request for the old kind was still queued)
    /// is parked instead of aborting the whole drain via `run`'s `Err`.
    /// Default `true`: a test/mock executor with no notion of "registered
    /// kinds" keeps the old behavior of letting `run` itself decide.
    fn has_template(&self, kind: &str) -> bool {
        let _ = kind;
        true
    }
}

/// An `EffectExec` that shells out per request. `templates` keys on the effect
/// kind (the `@async` head rel name); `{var}` placeholders in the template are
/// filled from the request args. stdout lines fill the output slots in order; if
/// there are more lines than slots the LAST slot absorbs the remainder (so a
/// `status\nbody...` response maps to `[status, body]`). `n_out` is the slot
/// count per kind (from `async_effect_arity`). Same trust + `sh -c` model as the
/// `cmd` source op. This is the real-IO executor the daemon runs off-tick.
pub struct ShellEffectExec {
    pub templates: HashMap<String, String>,
    pub n_out: HashMap<String, usize>,
    /// Default working directory for the spawned command — the engine root, so a
    /// `git rev-parse HEAD` / `gh ...` effect resolves against the watched repo
    /// (mirrors the `cmd` source op's `.current_dir(root)`). `Default` is the
    /// process cwd. A per-row `cwd` arg overrides this (see `run`).
    pub cwd: PathBuf,
}

impl ShellEffectExec {
    /// Render the template, spawn `sh -c`, return stdout. Shared by `run` (one
    /// response, line-slotted) and `run_stream` (N responses, one per line).
    fn spawn_stdout(&self, kind: &str, args: &serde_json::Map<String, serde_json::Value>) -> Result<String> {
        let tmpl = self.templates.get(kind)
            .ok_or_else(|| anyhow::anyhow!("no effect command registered for kind `{kind}`"))?;
        // Each arg is available two ways: `{k}` is raw text substitution (handy
        // for clean values like a path), and `$k` is an environment variable the
        // shell expands. The env form is METACHARACTER-SAFE — a value containing
        // quotes/slashes (an HTTP etag `W/"..."`, a JSON blob) corrupts a raw
        // `{k}` if it sits inside a quoted template span, but survives intact as
        // `"$k"` because the shell never re-parses an env value. Templates that
        // carry opaque values should use `$k`.
        let mut cmdline = tmpl.clone();
        let mut envs: Vec<(String, String)> = Vec::new();
        for (k, v) in args {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            cmdline = cmdline.replace(&format!("{{{k}}}"), &s);
            envs.push((k.clone(), s));
        }
        // Per-row working directory: a `cwd` arg the rule binds wins over the
        // engine root, so one effect kind can run each request in its own repo
        // (poll repo A in dir A, repo B in dir B). A relative `cwd` resolves
        // against the engine root; absolute is used as-is. The dir is computed in
        // dl (e.g. `cwd = parent(file)`), bound in the @async body like any arg.
        let run_in = match args.get("cwd").and_then(|v| v.as_str()) {
            Some(d) if !d.is_empty() => {
                let p = std::path::Path::new(d);
                if p.is_absolute() { p.to_path_buf() } else { self.cwd.join(p) }
            }
            _ => self.cwd.clone(),
        };
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&cmdline);
        for (k, v) in &envs { cmd.env(k, v); }
        if run_in.as_os_str().len() > 0 { cmd.current_dir(&run_in); }
        // Effect trace (DL_TRACE=debug): the rendered command actually spawned,
        // truncated so a giant inlined body stays one readable line. This is the
        // "what request am I about to make" half of the ghcacher-style log.
        tracing::debug!(target: "dl::effect", kind, cmd = %preview(&cmdline), "spawn");
        let output = cmd.output()?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        tracing::debug!(target: "dl::effect", kind, exit = ?output.status.code(),
            bytes = stdout.len(), "result");
        // nonzero exit WITH stdout is the findings-exist convention (the `cmd` op
        // shares it); nonzero with empty stdout is a broken command — be loud.
        if !output.status.success() && stdout.trim().is_empty() {
            bail!("effect `{cmdline}` failed (exit {:?}): {}",
                  output.status.code(), String::from_utf8_lossy(&output.stderr).trim());
        }
        Ok(stdout)
    }
}

impl EffectExec for ShellEffectExec {
    fn has_template(&self, kind: &str) -> bool {
        self.templates.contains_key(kind)
    }

    fn run(&self, kind: &str, args: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<String>> {
        let stdout = self.spawn_stdout(kind, args)?;
        let nout = self.n_out.get(kind).copied().unwrap_or(1);
        Ok(split_outputs(&stdout, nout))
    }

    fn run_stream(&self, kind: &str, args: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<Vec<String>>> {
        let stdout = self.spawn_stdout(kind, args)?;
        let nout = self.n_out.get(kind).copied().unwrap_or(1);
        // Each non-empty line is one response row; its columns are tab-separated
        // (the `@tsv` convention). A line short of `nout` tabs pads with empties;
        // the last slot absorbs any extra tabs (so a body with tabs stays whole).
        Ok(stdout.lines().filter(|l| !l.is_empty())
            .map(|l| split_tsv(l, nout)).collect())
    }
}

/// Split one stdout LINE into `nout` tab-separated slots (the per-row form of
/// `split_outputs`, which splits a whole stdout by newline). The last slot
/// absorbs any trailing tab-separated fields.
fn split_tsv(line: &str, nout: usize) -> Vec<String> {
    if nout == 0 { return Vec::new(); }
    if nout == 1 { return vec![line.to_string()]; }
    let fields: Vec<&str> = line.split('\t').collect();
    let mut out: Vec<String> = (0..nout - 1)
        .map(|i| fields.get(i).copied().unwrap_or("").to_string())
        .collect();
    let rest = if fields.len() > nout - 1 { fields[nout - 1..].join("\t") } else { String::new() };
    out.push(rest);
    out
}

/// One-line preview of an effect command/value for the `dl::effect` trace:
/// collapse newlines and cap length so a multi-line shell body or a big JSON
/// blob stays one readable log line.
fn preview(s: &str) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 160 {
        format!("{}…", flat.chars().take(160).collect::<String>())
    } else {
        flat
    }
}

/// Split shell stdout into `nout` response values: one per line, the last slot
/// absorbing any trailing lines (so a multi-line body stays one value).
fn split_outputs(stdout: &str, nout: usize) -> Vec<String> {
    if nout == 0 { return Vec::new(); }
    if nout == 1 { return vec![stdout.trim_end_matches('\n').to_string()]; }
    let lines: Vec<&str> = stdout.lines().collect();
    let mut out: Vec<String> = (0..nout - 1)
        .map(|i| lines.get(i).copied().unwrap_or("").to_string())
        .collect();
    let rest = if lines.len() > nout - 1 { lines[nout - 1..].join("\n") } else { String::new() };
    out.push(rest);
    out
}

/// Each `@async` rule's effect kind (its head rel name) and the number of unbound
/// head slots its executor must fill. The daemon uses this to size `ShellEffectExec`
/// and to know how to split a command's stdout. See `drain_effects`.
pub fn async_effect_arity(prog: &Program) -> HashMap<String, usize> {
    let mut m = HashMap::new();
    for item in &prog.items {
        let Item::Rule(r) = item else { continue };
        if !r.is_async() && !r.is_stream() { continue; }
        // After desugar every @async rule carries a body-effect; its `outs` are
        // the response columns (the stdout slots the executor fills). A rule
        // loaded without the frontend desugar (raw parse) falls back to the
        // unbound-head-var count, which is identical.
        // Key by the EFFECT name (the executor's template key = the `sh` decl
        // name), which equals the head rel in the head-response form but differs
        // in the explicit `gh(..) -> (..)` form. The value is the response-column
        // count (the stdout slots the executor must fill).
        let (kind, nout) = match r.effect() {
            Some((k, _, outs)) => (k.to_string(), outs.len()),
            None => {
                let vars = async_bound_vars(r);
                let nout = r.head.terms.iter()
                    .filter(|t| matches!(t, Term::Var(v) if !vars.contains(v))).count();
                (r.head.rel.clone(), nout)
            }
        };
        m.insert(kind, nout);
    }
    m
}

/// The program's `sh`/`sh!`/`sh*` declarations as a `kind -> template` map. The
/// daemon builds a `ShellEffectExec` from this typed registry; the kind is the
/// decl name, which matches the `@async` head rel in the head-response form. The
/// dynamic `effect_cmd(kind, template)` relation overlays this (a data-driven
/// fallback for templates not known at parse time).
pub fn shell_templates(prog: &Program) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for item in &prog.items {
        if let Item::Shell(f) = item {
            m.insert(f.name.clone(), f.body.clone());
        }
    }
    m
}

/// Each `sh` decl's parameter names, keyed by the decl name (== effect kind).
/// In the explicit body-effect form `gh(repo, path) -> (..)`, the call args fill
/// the `{param}` holes POSITIONALLY, so the hole map is keyed by these param
/// names. The head-response form has no declared params and keys holes by the
/// body var name instead (see `rebuild_async`).
fn shell_fn_params(prog: &Program) -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    for item in &prog.items {
        if let Item::Shell(f) = item {
            m.insert(f.name.clone(), f.params.clone());
        }
    }
    m
}

/// Each declared `sh` decl's read/mutate/stream kind, keyed by name (== effect
/// kind). A head-response effect with no matching `sh` decl (the legacy
/// `effect_cmd` path) is absent here and the drain treats it as `Read` (the
/// cached, re-runnable default). Drives the Phase 3 exactly-once claim: a
/// `Mutate` (`sh!`) effect is claimed before it runs so a crash mid-flight
/// cannot double-fire it.
fn shell_kinds(prog: &Program) -> HashMap<String, ShellKind> {
    let mut m = HashMap::new();
    for item in &prog.items {
        if let Item::Shell(f) = item {
            m.insert(f.name.clone(), f.kind);
        }
    }
    m
}

/// Evaluate an effect-arg/head term against a body solution object: a var reads
/// the solution cell, a literal is itself. Builds the effect hole map (and, with
/// the response outs folded in, the head row) in the async runtime.
/// The `collect(x)` aggregate wrapper in an effect arg: returns the inner var
/// name. `collect(x)` gathers `x` across ALL body solutions so the effect fires
/// once with the whole set (the provider-native batch-by-id). Only a single bare
/// var is supported inside `collect`.
fn collect_inner_var(t: &Term) -> Option<&str> {
    match t {
        Term::Call { name, args }
            if name == "collect" && (args.len() == 1 || args.len() == 2) =>
        {
            match &args[0] {
                Term::Var(v) => Some(v.as_str()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// `collect(x, N)` chunk size: cap each batched request at N values (the
/// provider's per-call limit, e.g. a GraphQL query-complexity ceiling), firing
/// ceil(|values|/N) requests. `collect(x)` (no N) batches the whole set into one.
fn collect_chunk(t: &Term) -> Option<usize> {
    match t {
        Term::Call { name, args } if name == "collect" && args.len() == 2 => match &args[1] {
            Term::Int(n) if *n > 0 => Some(*n as usize),
            _ => None,
        },
        _ => None,
    }
}

fn eval_term_json(t: &Term, sol: &serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    match t {
        Term::Var(v) => sol.get(v).cloned().unwrap_or(serde_json::Value::Null),
        Term::Str(s) => serde_json::json!(s),
        Term::Int(n) => serde_json::json!(n),
        _ => serde_json::Value::Null,
    }
}

/// Coerce a JSON arg value (from `pending_effect.args_json`) into a db cell.
fn json_to_value(v: Option<&serde_json::Value>) -> Value {
    match v {
        Some(serde_json::Value::Number(n)) if n.is_i64() => Value::Int(n.as_i64().unwrap()),
        Some(serde_json::Value::String(s)) => Value::Text(s.clone()),
        Some(other) => Value::Text(other.to_string()),
        None => Value::Text(String::new()),
    }
}

// `pending_effect.state` values. Keep them centralized so the orphaned/failed
// distinction (new in the daemon CPU-hog fix Part 3) is explicit.
const STATE_QUEUED: &str = "queued";
const STATE_RUNNING: &str = "running";
const STATE_DONE: &str = "done";
const STATE_FAILED: &str = "failed";
const STATE_ORPHANED: &str = "orphaned";

impl Engine {
    /// Emit one `pending_effect` request row per @async-rule body solution. The
    /// body binds the request args (the distinct positive-atom vars); each
    /// solution becomes a JSON arg object keyed by var name. The row's `id` is a
    /// digest of (kind, args) so re-emitting the same request across ticks before
    /// it runs does not double-fire (INSERT OR IGNORE on the PK). The executor
    /// runs off-tick in `drain_effects`; nothing here touches the network.
    pub(crate) fn rebuild_async(&self, prog: &Program, async_rules: &[&Rule], cur: i64) -> Result<()> {
        let params_by_kind = shell_fn_params(prog);
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for r in async_rules {
            let head_rel = &r.head.rel;
            let (kind, eff_args, _outs) = r.effect()
                .ok_or_else(|| anyhow::anyhow!("@async rule (rel `{}`) was not desugared to a \
                    body-effect (frontend bug)", r.head.rel))?;
            // The FULL body solution (all positive-atom-bound vars) is the head
            // rebuild env (D-4); the effect args are a subset/expression over it.
            let vars = async_bound_vars(r);
            if vars.is_empty() {
                bail!("@async rule (rel `{kind}`) binds no request args; its body must have \
                       a positive atom with at least one variable");
            }
            let sql = crate::lower::lower_body_projection(&r.body, &self.rels, &vars)?;
            let solutions: Vec<serde_json::Map<String, serde_json::Value>> = {
                let mut stmt = self.db.conn().prepare(&sql)?;
                let it = stmt.query_map([], |row| {
                    let mut obj = serde_json::Map::new();
                    for (i, v) in vars.iter().enumerate() {
                        let val: rusqlite::types::Value = row.get(i)?;
                        let jv = match val {
                            rusqlite::types::Value::Integer(n) => serde_json::json!(n),
                            rusqlite::types::Value::Real(f) => serde_json::json!(f),
                            rusqlite::types::Value::Text(s) => serde_json::json!(s),
                            _ => serde_json::Value::Null,
                        };
                        obj.insert(v.clone(), jv);
                    }
                    Ok(obj)
                })?;
                it.filter_map(|x| x.ok()).collect()
            };
            // Hole-name resolution: in the explicit body-effect form the holes are
            // the `sh` decl's params, filled positionally; in the head-response
            // form (desugared, no matching params) they are the arg var names. Use
            // declared params iff their arity matches the call args, else var names.
            let hole_keys: Vec<String> = match params_by_kind.get(kind) {
                Some(ps) if ps.len() == eff_args.len() => ps.clone(),
                _ => eff_args.iter().map(|a| match a {
                    Term::Var(v) => v.clone(),
                    _ => String::new(),
                }).collect(),
            };
            // `collect(x)` makes this an AGGREGATE effect: gather `x` over ALL
            // solutions and fire ONE request whose response fans back out (one head
            // row per output line, marked `batch=1`). The other (non-collected)
            // args must be constant across solutions — collect batches a single
            // request, so a varying arg is a loud split-this-rule error.
            let collect_idx = eff_args.iter().position(|a| collect_inner_var(a).is_some());
            if let Some(ci) = collect_idx {
                let cvar = collect_inner_var(&eff_args[ci]).unwrap().to_string();
                let mut base: Option<serde_json::Map<String, serde_json::Value>> = None;
                let mut values: Vec<String> = Vec::new();
                for sol in &solutions {
                    // the non-collected holes (must agree across the batch).
                    let mut h = serde_json::Map::new();
                    for (i, a) in eff_args.iter().enumerate() {
                        if i == ci { continue; }
                        let key = &hole_keys[i];
                        if key.is_empty() { continue; }
                        h.insert(key.clone(), eval_term_json(a, sol));
                    }
                    match &base {
                        None => base = Some(h),
                        Some(b) if *b != h => bail!("collect effect `{kind}`: the non-collected \
                            args vary across body solutions; collect batches ONE request — give the \
                            varying arg its own rule"),
                        _ => {}
                    }
                    if let Some(v) = sol.get(&cvar) {
                        values.push(match v {
                            serde_json::Value::String(s) => s.clone(),
                            o => o.to_string(),
                        });
                    }
                }
                if values.is_empty() { continue; }
                values.sort();
                values.dedup();
                let ckey = &hole_keys[ci];
                let base = base.unwrap_or_default();
                // `collect(x, N)` caps each request at N values; `collect(x)`
                // (no N) batches the whole set into one request. ceil(|x|/N)
                // batched requests, each marked `batch=1`.
                let chunk = collect_chunk(&eff_args[ci]).unwrap_or(values.len().max(1));
                for group in values.chunks(chunk) {
                    let mut hole = base.clone();
                    if !ckey.is_empty() {
                        // The batch list joins comma-separated (the common CLI form,
                        // `--ids a,b,c`); the template controls how `{ids}` is consumed.
                        hole.insert(ckey.clone(), serde_json::json!(group.join(",")));
                    }
                    let args_json = serde_json::Value::Object(hole).to_string();
                    let id = blake3::hash(
                        format!("{head_rel}\u{0}{kind}\u{0}{args_json}").as_bytes()
                    ).to_hex().to_string();
                    // No single body solution keys the batch — the head rebuilds
                    // purely from the fanned-out response (`full_json` empty), so a
                    // batch head must read only response outs / the echoed id (not a
                    // per-row body var). `batch=1`.
                    rows.push(vec![
                        Value::Text(id), Value::Text(kind.to_string()),
                        Value::Text(head_rel.clone()),
                        Value::Text(args_json), Value::Text(String::new()), Value::Int(cur),
                        Value::Int(1),
                    ]);
                }
                continue;
            }
            for sol in solutions {
                // hole map (the template fill + the digest key): one entry per arg.
                let mut hole = serde_json::Map::new();
                for (i, a) in eff_args.iter().enumerate() {
                    let key = &hole_keys[i];
                    if key.is_empty() { continue; } // unnamed literal arg, head-only
                    hole.insert(key.clone(), eval_term_json(a, &sol));
                }
                let args_json = serde_json::Value::Object(hole).to_string();
                // The digest keys on (head rel, effect kind, hole map) — two rules
                // may call the same `sh` fn with the same args but reconstruct
                // different heads, so the head rel is part of the identity.
                let id = blake3::hash(
                    format!("{head_rel}\u{0}{kind}\u{0}{args_json}").as_bytes()
                ).to_hex().to_string();
                let full_json = serde_json::Value::Object(sol).to_string();
                rows.push(vec![
                    Value::Text(id), Value::Text(kind.to_string()),
                    Value::Text(head_rel.clone()),
                    Value::Text(args_json), Value::Text(full_json), Value::Int(cur),
                    Value::Int(0),
                ]);
            }
        }
        // `done` defaults to 0; INSERT OR IGNORE keeps an already-queued (or
        // already-run, done=1) request untouched.
        self.db.insert_rows("pending_effect",
            &["id", "kind", "head_rel", "args_json", "full_json", "req_tx", "batch"], &rows)?;
        Ok(())
    }

    /// Run every outstanding `@async` request through `exec` and land its response
    /// in the head relation, then flip `done`. Called by the daemon BETWEEN ticks
    /// (never inside `tick`): this is the only place an effect actually executes.
    /// The response row is assembled in head order — body-bound head terms echo
    /// from the request args, unbound head terms take the executor's outputs in
    /// order. The response rel persists (it is neither source nor derived), so the
    /// next tick reads it like any fact. Returns the number of requests drained.
    /// Count `pending_effect` rows still owed an off-tick drain — queued or
    /// running, EXCLUDING `@stream` (`sh*`) subscriptions (which stay 'running'
    /// forever by design and are steady state, not in-flight work). The settle
    /// report reads this; a program with an undrained `@async`/`sh` request is
    /// not settled until the daemon (or `dl --settle`) runs `drain_effects`.
    pub fn inflight_nonstream(&self, prog: &Program) -> Result<usize> {
        let kinds = shell_kinds(prog);
        let rows: Vec<String> = {
            let mut stmt = self.db.conn().prepare(
                "SELECT kind FROM pending_effect WHERE state IN (?1,?2)")?;
            let params = [STATE_QUEUED, STATE_RUNNING];
            let v = stmt.query_map(params, |r| r.get(0))?.filter_map(|x| x.ok()).collect();
            v
        };
        // A kind with no `sh` decl defaults to Read (the legacy effect_cmd path),
        // so it counts as in-flight, matching drain_effects' `kind_of` default.
        Ok(rows.into_iter()
            .filter(|k| kinds.get(k).copied() != Some(ShellKind::Stream))
            .count())
    }

    /// Cheap COUNT of `pending_effect` rows still owed a drain — queued or
    /// running, EVERY kind including `@stream` subscriptions (which sit
    /// 'running' forever and need a continuing drain, unlike
    /// `inflight_nonstream`'s settle-report count). One indexed SQL COUNT, no
    /// corpus walk: the daemon's poll loop uses this as the idle probe (a root
    /// with 0 pending rows and no source motion since its last full tick has
    /// nothing to integrate, so the poll cycle skips the tick entirely).
    pub fn pending_effect_count(&self) -> Result<i64> {
        self.db.conn().query_row(
            "SELECT COUNT(*) FROM pending_effect WHERE state IN (?1,?2)", [STATE_QUEUED, STATE_RUNNING],
            |r| r.get(0)).map_err(Into::into)
    }

    /// Count `pending_effect` rows in terminal failure states for status surfacing.
    /// Returns `(failed_count, orphaned_count)`.
    pub fn effect_status_counts(&self) -> Result<(i64, i64)> {
        let failed: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM pending_effect WHERE state = ?1", [STATE_FAILED],
            |r| r.get(0))?;
        let orphaned: i64 = self.db.conn().query_row(
            "SELECT COUNT(*) FROM pending_effect WHERE state = ?1", [STATE_ORPHANED],
            |r| r.get(0))?;
        Ok((failed, orphaned))
    }

    /// Park `pending_effect` rows whose kind has no registered executor template
    /// (Part 2 of the daemon CPU-hog fix). Sets `state = 'orphaned'` so every
    /// `state IN ('queued','running')` scan — `drain_effects`, `drain_streams`,
    /// `inflight_nonstream`, this method — never revisits it: a crash-loop-y
    /// orphaned kind stops being rescanned every poll instead of aborting the
    /// drain via `?` forever. One batched UPDATE, not a per-row write.
    fn park_orphan_effects(&self, ids: &[String]) -> Result<()> {
        if ids.is_empty() { return Ok(()); }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!("UPDATE pending_effect SET state = '{STATE_ORPHANED}' WHERE id IN ({placeholders})");
        let params: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        self.db.conn().execute(&sql, params.as_slice())?;
        Ok(())
    }

    /// Re-queue `pending_effect` rows parked as `orphaned` whose kind NOW has a
    /// registered executor template. One batched UPDATE by recoverable kind,
    /// never per-row. Heals first-tick races where the effect row was parked
    /// before `effect_cmd` landed. `failed` (real execution failure) is NOT
    /// covered: a failing command's kind has a template by definition, so
    /// re-queueing it here would retry it every drain forever — the crash loop
    /// the park path exists to prevent. Rows the OLD park path wrote as
    /// `failed` need a one-time manual `UPDATE ... SET state='queued'`.
    fn requeue_orphaned_effects(&self, exec: &dyn EffectExec) -> Result<()> {
        let kinds: Vec<String> = {
            let mut stmt = self.db.conn().prepare(
                "SELECT DISTINCT kind FROM pending_effect WHERE state = ?1")?;
            let v = stmt.query_map([STATE_ORPHANED], |r| r.get(0))?
                .filter_map(|x| x.ok()).collect();
            v
        };
        if kinds.is_empty() { return Ok(()); }
        let mut recoverable: Vec<String> = kinds.iter()
            .filter(|k| exec.has_template(k))
            .cloned()
            .collect();

        // Also recover kinds whose template comes from the dynamic `effect_cmd`
        // overlay relation, even if the executor registry passed to this drain
        // has not been rebuilt yet. The base `rel_effect_cmd` stores interned
        // ids, so the text view is the source of truth for kind names.
        if recoverable.len() < kinds.len() {
            let placeholders = kinds.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT DISTINCT kind FROM rel_effect_cmd_txt WHERE kind IN ({placeholders})");
            let params: Vec<&dyn rusqlite::ToSql> = kinds.iter()
                .map(|s| s as &dyn rusqlite::ToSql).collect();
            if let Ok(mut stmt) = self.db.conn().prepare(&sql) {
                let dynamic: Vec<String> = stmt.query_map(params.as_slice(), |r| r.get::<_, String>(0))?
                    .filter_map(|x| x.ok()).collect();
                for k in dynamic {
                    if !recoverable.contains(&k) { recoverable.push(k); }
                }
            }
        }

        if recoverable.is_empty() { return Ok(()); }
        let placeholders = recoverable.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "UPDATE pending_effect SET state = ?1 WHERE state = ?2 AND kind IN ({placeholders})");
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(2 + recoverable.len());
        params.push(&STATE_QUEUED as &dyn rusqlite::ToSql);
        params.push(&STATE_ORPHANED as &dyn rusqlite::ToSql);
        for k in &recoverable { params.push(k as &dyn rusqlite::ToSql); }
        self.db.conn().execute(&sql, params.as_slice())?;
        Ok(())
    }

    /// Log an orphaned-effect-kind warning at most once per (root, kind) for
    /// this engine's lifetime — not once per poll cycle. `self` is one
    /// `Engine` per served root, so `warned_orphan_effect_kinds` is already
    /// root-scoped; the set is never cleared (unlike the per-tick diag
    /// buffers), so a kind that keeps re-queueing new distinct request ids
    /// (the digest is per-args, not per-kind) still warns only once.
    fn warn_orphan_kind_once(&self, kind: &str) {
        let mut seen = self.warned_orphan_effect_kinds.borrow_mut();
        if seen.insert(kind.to_string()) {
            eprintln!(
                "[{}] no effect command registered for kind `{kind}`; parking its queued \
                 request(s) (state=orphaned) instead of retrying every poll",
                self.self_slug()
            );
        }
    }

    pub fn drain_effects(&mut self, prog: &Program, exec: &dyn EffectExec) -> Result<usize> {
        // Phase 0 recovery: rows parked as orphaned (or legacy failed) may now
        // have a template available; re-queue them in one UPDATE before the
        // queued/running scan so this drain can run them.
        self.requeue_orphaned_effects(exec)?;

        // Per-kind head reassembly plan, keyed by the @async head rel name. After
        // desugar the head is rebuilt over (full body solution) ∪ (response outs):
        // each head term resolves against that env (a body var, an out var, or a
        // literal). `out_vars` are the effect's response columns, in order.
        let mut plans: HashMap<String, (Vec<Term>, Vec<String>)> = HashMap::new();
        for item in &prog.items {
            let Item::Rule(r) = item else { continue };
            if !r.is_async() { continue; }
            let (_, _, outs) = r.effect()
                .ok_or_else(|| anyhow::anyhow!("@async rule (rel `{}`) was not desugared to a \
                    body-effect (frontend bug)", r.head.rel))?;
            let out_vars: Vec<String> = outs.iter().map(|t| match t {
                Term::Var(v) => Ok(v.clone()),
                other => Err(anyhow::anyhow!("@async effect output {other:?} (rel `{}`) must be a \
                    fresh variable", r.head.rel)),
            }).collect::<Result<_>>()?;
            plans.insert(r.head.rel.clone(), (r.head.terms.clone(), out_vars));
        }

        // Phase 3 reconcile axis: `kind` -> read/mutate/stream. A `Mutate` (`sh!`)
        // effect is claimed exactly once (a crash mid-flight must not double-POST);
        // a `Read` (`sh`) is cached/re-runnable; a `Stream` (`sh*`) is owned by the
        // Phase 4 reader, not this one-shot drain.
        let kinds = shell_kinds(prog);
        let kind_of = |k: &str| kinds.get(k).copied().unwrap_or(ShellKind::Read);

        // A `Read` row is drained from queued OR a crash-orphaned running; a
        // `Mutate` row is drained ONLY from queued and atomically claimed below, so
        // a row left 'running' by a crash is quarantined (never silently re-fired).
        // A `Stream` row is skipped here (Phase 4 owns it).
        // (id, kind = executor/template key, head_rel = reconstruction key, state).
        let pending: Vec<(String, String, String, String, String, String, i64)> = {
            let mut stmt = self.db.conn().prepare(
                "SELECT id, kind, head_rel, args_json, full_json, state, batch \
                 FROM pending_effect WHERE state IN (?1,?2)")?;
            let v = stmt.query_map([STATE_QUEUED, STATE_RUNNING], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)))?
                .filter_map(|x| x.ok()).collect();
            v
        };
        // The exactly-once claim for `sh!`: flip queued -> running under the row's
        // own conditional UPDATE (changes()==1 wins the claim). A row not claimed
        // (already running/done, or a concurrent drainer took it) is dropped from
        // this drain. `Read`/`Stream` rows pass through unclaimed.
        let claim = |id: &str| -> Result<bool> {
            let n = self.db.conn().execute(
                "UPDATE pending_effect SET state = ?1, idem_key = id \
                 WHERE id = ?2 AND state = ?3", [STATE_RUNNING, id, STATE_QUEUED])?;
            Ok(n == 1)
        };

        // Parse the hole map AND the full head env serially (cheap, and a bad JSON
        // blob should bail before any subprocess spawns). Drop requests whose head
        // rel is not an @async rule in the current program — a different program
        // may own it, leave it queued.
        let mut work: Vec<(String, String, String,
            serde_json::Map<String, serde_json::Value>,
            serde_json::Map<String, serde_json::Value>, bool)> = Vec::new();
        // Rows whose kind has no registered executor template (Part 2): parked
        // (state='orphaned') and warned about once per kind, never fed to the
        // rayon pool below — a missing template must not abort the whole
        // batch via `?` (the crash-loop `poll error: no effect command
        // registered for kind ...` symptom the daemon CPU-hog fix targets).
        let mut orphan_ids: Vec<String> = Vec::new();
        let mut orphan_kinds: Vec<String> = Vec::new();
        for (id, kind, head_rel, args_json, full_json, state, batch) in pending {
            // Pre-migration rows have head_rel='' (head-response 1:1 with kind).
            let head_rel = if head_rel.is_empty() { kind.clone() } else { head_rel };
            if !plans.contains_key(&head_rel) { continue; }
            if !exec.has_template(&kind) {
                orphan_ids.push(id);
                if !orphan_kinds.contains(&kind) { orphan_kinds.push(kind.clone()); }
                continue;
            }
            match kind_of(&kind) {
                // A `sh*` subscription is run by the Phase 4 reader, not the
                // one-shot drain; never execute it here.
                ShellKind::Stream => continue,
                // `sh!` is exactly-once: claim queued->running or skip. A row found
                // already 'running' (crash orphan) is left quarantined, not re-run.
                ShellKind::Mutate => { if state != "queued" || !claim(&id)? { continue; } }
                // `sh` is cached/re-runnable: a crash-orphaned 'running' row is fair
                // game; no claim needed (re-firing a read is harmless).
                ShellKind::Read => {}
            }
            let args = match serde_json::from_str::<serde_json::Value>(&args_json)? {
                serde_json::Value::Object(m) => m,
                _ => bail!("pending_effect.args_json for `{kind}` is not a JSON object"),
            };
            // `full_json` is empty only for rows queued before this column existed
            // (migration default ''); treat it as the hole map (the legacy 1:1).
            let full = if full_json.is_empty() {
                args.clone()
            } else {
                match serde_json::from_str::<serde_json::Value>(&full_json)? {
                    serde_json::Value::Object(m) => m,
                    _ => bail!("pending_effect.full_json for `{kind}` is not a JSON object"),
                }
            };
            work.push((id, kind, head_rel, args, full, batch != 0));
        }
        if !orphan_ids.is_empty() {
            self.park_orphan_effects(&orphan_ids)?;
            for kind in &orphan_kinds { self.warn_orphan_kind_once(kind); }
        }
        if !work.is_empty() {
            tracing::info!(target: "dl::effect", n = work.len(), "draining effects");
        }

        // Run the executors across the rayon pool: the shell spawn / network call
        // is the slow part, and one request never depends on another's response
        // (downstream rules join the landed rows on a LATER tick). Each closure
        // assembles its head row(s); the DB writes below stay serial and batched.
        // The executor is keyed by `kind` (the `sh` template); the head is rebuilt
        // by `head_rel` (the response rel). A `collect` batch (`batch=true`) runs
        // ONE request whose response fans into N rows (run_stream); a plain request
        // is one row (run). Both mark `id` done once.
        let results: Vec<Result<Vec<(String, String, Vec<Value>)>>> = work.par_iter()
            .map(|(id, kind, head_rel, args, full, batch)| {
                let (head_terms, out_vars) = &plans[head_rel];
                // Each response line -> the out slots. A batch fans many lines; a
                // plain request is the single `run` row wrapped as one line.
                let lines: Vec<Vec<String>> = if *batch {
                    exec.run_stream(kind, args)?
                } else {
                    vec![exec.run(kind, args)?]
                };
                // Effect trace (DL_TRACE=info): one line per response. `args` is
                // the digest key (the endpoint + carried etag + cadence bucket),
                // and `status` is the first response slot — for a gh poll that is
                // the HTTP status, so a 200-vs-304 cache hit is visible here.
                tracing::info!(target: "dl::effect",
                    kind = %kind, head = %head_rel,
                    args = %preview(&serde_json::to_string(args).unwrap_or_default()),
                    status = %lines.first().and_then(|l| l.first()).map(String::as_str).unwrap_or(""),
                    rows = lines.len(), "response");
                let mut out_rows = Vec::with_capacity(lines.len());
                for out in &lines {
                    if out.len() != out_vars.len() {
                        bail!("@async executor for `{kind}` returned {} value(s), expected {} \
                               (one per effect output)", out.len(), out_vars.len());
                    }
                    // env = full body solution ∪ {out_var -> out_value}; the head
                    // row is each head term evaluated against it (var, out, literal).
                    let mut env = full.clone();
                    for (v, o) in out_vars.iter().zip(out.iter()) {
                        env.insert(v.clone(), serde_json::json!(o));
                    }
                    let row: Vec<Value> = head_terms.iter().map(|t| match t {
                        Term::Str(s) => Value::Text(s.clone()),
                        Term::Int(n) => Value::Int(*n),
                        Term::Var(v) => json_to_value(env.get(v)),
                        other => unreachable!("@async head term {other:?} survived parse"),
                    }).collect();
                    out_rows.push((id.clone(), head_rel.clone(), row));
                }
                Ok(out_rows)
            })
            .collect();

        let mut assembled: Vec<(String, String, Vec<Value>)> = Vec::new();
        let mut failed_ids: Vec<String> = Vec::new();
        for (i, res) in results.into_iter().enumerate() {
            match res {
                Ok(rows) => assembled.extend(rows),
                Err(e) => {
                    let id = work[i].0.clone();
                    tracing::warn!(target: "dl::effect", %id, error = %e, "effect execution failed");
                    failed_ids.push(id);
                }
            }
        }
        if !failed_ids.is_empty() {
            let placeholders = failed_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "UPDATE pending_effect SET state = ?1 WHERE id IN ({placeholders})");
            let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(1 + failed_ids.len());
            params.push(&STATE_FAILED as &dyn rusqlite::ToSql);
            for id in &failed_ids { params.push(id as &dyn rusqlite::ToSql); }
            self.db.conn().execute(&sql, params.as_slice())?;
        }

        if assembled.is_empty() { return Ok(0); }

        // Batch the response rows by head rel (one `insert_rows` per response rel)
        // and mark every drained request done in a single transaction. A `collect`
        // batch produced N rows under ONE id, so `done_ids` dedups to mark the
        // request done once (and return the request count, not the row count).
        let mut by_rel: HashMap<String, Vec<Vec<Value>>> = HashMap::new();
        let mut done_ids: Vec<String> = Vec::new();
        for (id, head_rel, row) in assembled {
            by_rel.entry(head_rel).or_default().push(row);
            if !done_ids.contains(&id) { done_ids.push(id); }
        }
        for (head_rel, rows) in &by_rel {
            let cols: Vec<String> = {
                let meta = self.rels.get(head_rel)
                    .ok_or_else(|| anyhow::anyhow!("@async response relation `{head_rel}` is not declared"))?;
                meta.cols.iter().map(|c| c.name.clone()).collect()
            };
            let col_refs: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
            self.insert_rel_rows(head_rel, &col_refs, rows)?;
        }
        {
            let conn = self.db.conn();
            let tx = conn.unchecked_transaction()?;
            for id in &done_ids {
                tx.execute("UPDATE pending_effect SET done = 1, state = ?1 WHERE id = ?2", [STATE_DONE, id])?;
            }
            tx.commit()?;
        }
        self.gc_done_effects(&kinds)?;
        Ok(done_ids.len())
    }

    /// Reclaim old `done` `pending_effect` rows so a long-lived poller's audit
    /// log does not grow without bound (the one effect-table leak: a
    /// cadence-bucketed poll queues a fresh row every `clock` bucket forever).
    ///
    /// Scope is deliberately narrow:
    /// - **`Read` (`sh`) only.** A `Read` row is cache-safe to drop — re-firing a
    ///   read is defined-harmless — and a cadence bucket only moves FORWARD, so an
    ///   old (endpoint, etag, bucket) request can never recur to be re-fired.
    /// - **`Mutate` (`sh!`) rows are kept.** Their persisted `done` state IS the
    ///   exactly-once guard (`INSERT OR IGNORE` on the same id is what stops a
    ///   re-POST); GC-ing one would re-open the window. `Stream` rows are
    ///   long-lived and owned by the stream reader, so they are kept too.
    /// - Only rows older than the retention window (`DL_EFFECT_RETAIN_TICKS`,
    ///   default 256 ticks) are removed, so the live `effect_log` view still
    ///   shows recent activity.
    fn gc_done_effects(&self, kinds: &HashMap<String, ShellKind>) -> Result<()> {
        let read_kinds: Vec<&String> = kinds.iter()
            .filter(|(_, k)| **k == ShellKind::Read)
            .map(|(name, _)| name)
            .collect();
        if read_kinds.is_empty() { return Ok(()); }
        let keep: i64 = std::env::var("DL_EFFECT_RETAIN_TICKS").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(256);
        // Read the carry clock directly (the private `current_tx` lives in the
        // engine module); `_carry_meta` is the same row `set_tx` advances.
        let cur_tx: i64 = self.db.conn().query_row(
            "SELECT tx FROM _carry_meta WHERE k = 'tx'", [], |r| r.get(0))?;
        if cur_tx <= keep { return Ok(()); }
        let cutoff = cur_tx - keep;
        let placeholders = read_kinds.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "DELETE FROM pending_effect WHERE state = ?1 AND req_tx < ?2 AND kind IN ({placeholders})");
        let conn = self.db.conn();
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(read_kinds.len() + 2);
        params.push(&STATE_DONE as &dyn rusqlite::ToSql);
        params.push(&cutoff);
        for k in &read_kinds { params.push(*k); }
        let n = conn.execute(&sql, params.as_slice())?;
        if n > 0 {
            tracing::debug!(target: "dl::effect", reclaimed = n, cutoff, "gc done effects");
        }
        Ok(())
    }

    /// Drain `@stream` (`sh*`) subscriptions. Unlike `drain_effects` (one row per
    /// request, then done), a stream effect produces MANY head rows per drain (one
    /// per output line, D-7) and the job STAYS 'running' so it keeps yielding on
    /// later drains. The cursor that advances a stream between drains is an
    /// ordinary `@next` fact in the rule body (no special machinery here). Output
    /// rows are batched into one `insert_rows` per head rel (the N+1 ban holds);
    /// `OR IGNORE` dedups a line already seen, so a re-emitted cursor is harmless.
    /// Returns the number of head rows appended. The daemon calls this alongside
    /// `drain_effects` between ticks.
    pub fn drain_streams(&mut self, prog: &Program, exec: &dyn EffectExec) -> Result<usize> {
        // Head reassembly plan per @stream head rel: (head terms, response outs).
        let mut plans: HashMap<String, (Vec<Term>, Vec<String>)> = HashMap::new();
        for item in &prog.items {
            let Item::Rule(r) = item else { continue };
            if !r.is_stream() { continue; }
            let (_, _, outs) = r.effect()
                .ok_or_else(|| anyhow::anyhow!("@stream rule (rel `{}`) was not desugared to a \
                    body-effect (frontend bug)", r.head.rel))?;
            let out_vars: Vec<String> = outs.iter().map(|t| match t {
                Term::Var(v) => Ok(v.clone()),
                other => Err(anyhow::anyhow!("@stream effect output {other:?} (rel `{}`) must be a \
                    fresh variable", r.head.rel)),
            }).collect::<Result<_>>()?;
            plans.insert(r.head.rel.clone(), (r.head.terms.clone(), out_vars));
        }
        if plans.is_empty() { return Ok(0); }
        let kinds = shell_kinds(prog);

        // Pending stream rows: queued (just subscribed) or running (live). Only a
        // kind declared `sh*` is a stream; anything else is a one-shot (handled by
        // `drain_effects`) and skipped here.
        let pending: Vec<(String, String, String, String, String)> = {
            let mut stmt = self.db.conn().prepare(
                "SELECT id, kind, head_rel, args_json, full_json \
                 FROM pending_effect WHERE state IN (?1,?2)")?;
            let v = stmt.query_map([STATE_QUEUED, STATE_RUNNING], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
                .filter_map(|x| x.ok()).collect();
            v
        };

        // Subscribe a freshly-queued stream (queued -> running); a stream never
        // returns to 'done' on its own (it is long-lived). Build the run set.
        let mut work: Vec<(String, String,
            serde_json::Map<String, serde_json::Value>,
            serde_json::Map<String, serde_json::Value>)> = Vec::new();
        // Same orphaned-kind gate as `drain_effects` (Part 2): a stream row
        // whose kind lost its executor template is parked, warned about once,
        // and never subscribed — not left to abort the whole drain.
        let mut orphan_ids: Vec<String> = Vec::new();
        let mut orphan_kinds: Vec<String> = Vec::new();
        for (id, kind, head_rel, args_json, full_json) in pending {
            if kinds.get(&kind).copied() != Some(ShellKind::Stream) { continue; }
            let head_rel = if head_rel.is_empty() { kind.clone() } else { head_rel };
            if !plans.contains_key(&head_rel) { continue; }
            if !exec.has_template(&kind) {
                orphan_ids.push(id);
                if !orphan_kinds.contains(&kind) { orphan_kinds.push(kind.clone()); }
                continue;
            }
            self.db.conn().execute(
                "UPDATE pending_effect SET state = ?1 WHERE id = ?2 AND state = ?3",
                [STATE_RUNNING, &id, STATE_QUEUED])?;
            let args = match serde_json::from_str::<serde_json::Value>(&args_json)? {
                serde_json::Value::Object(m) => m,
                _ => bail!("pending_effect.args_json for `{kind}` is not a JSON object"),
            };
            let full = if full_json.is_empty() {
                args.clone()
            } else {
                match serde_json::from_str::<serde_json::Value>(&full_json)? {
                    serde_json::Value::Object(m) => m,
                    _ => bail!("pending_effect.full_json for `{kind}` is not a JSON object"),
                }
            };
            work.push((kind, head_rel, args, full));
        }
        if !orphan_ids.is_empty() {
            self.park_orphan_effects(&orphan_ids)?;
            for kind in &orphan_kinds { self.warn_orphan_kind_once(kind); }
        }

        // Each stream yields N rows (one per output line). Assemble head rows the
        // same way `drain_effects` does, but fan over the line set.
        let assembled: Vec<(String, Vec<Value>)> = work.par_iter()
            .map(|(kind, head_rel, args, full)| {
                let (head_terms, out_vars) = &plans[head_rel];
                let lines = exec.run_stream(kind, args)?;
                let mut rows = Vec::with_capacity(lines.len());
                for out in lines {
                    if out.len() != out_vars.len() {
                        bail!("@stream executor for `{kind}` produced a row of {} value(s), expected \
                               {} (one per effect output)", out.len(), out_vars.len());
                    }
                    let mut env = full.clone();
                    for (v, o) in out_vars.iter().zip(out.iter()) {
                        env.insert(v.clone(), serde_json::json!(o));
                    }
                    let row: Vec<Value> = head_terms.iter().map(|t| match t {
                        Term::Str(s) => Value::Text(s.clone()),
                        Term::Int(n) => Value::Int(*n),
                        Term::Var(v) => json_to_value(env.get(v)),
                        other => unreachable!("@stream head term {other:?} survived parse"),
                    }).collect();
                    rows.push((head_rel.clone(), row));
                }
                Ok(rows)
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter().flatten().collect();

        if assembled.is_empty() { return Ok(0); }
        let mut by_rel: HashMap<String, Vec<Vec<Value>>> = HashMap::new();
        for (head_rel, row) in &assembled {
            by_rel.entry(head_rel.clone()).or_default().push(row.clone());
        }
        for (head_rel, rows) in &by_rel {
            let cols: Vec<String> = {
                let meta = self.rels.get(head_rel)
                    .ok_or_else(|| anyhow::anyhow!("@stream response relation `{head_rel}` is not declared"))?;
                meta.cols.iter().map(|c| c.name.clone()).collect()
            };
            let col_refs: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
            self.insert_rel_rows(head_rel, &col_refs, rows)?;
        }
        Ok(assembled.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::PathBuf;

    /// Mock executor whose `has_template` answer is configurable per kind.
    struct TemplatableExec {
        kinds: HashSet<String>,
    }

    impl EffectExec for TemplatableExec {
        fn run(&self, _kind: &str, _args: &serde_json::Map<String, serde_json::Value>) -> Result<Vec<String>> {
            Ok(vec!["ok".to_string()])
        }
        fn has_template(&self, kind: &str) -> bool {
            self.kinds.contains(kind)
        }
    }

    fn engine_with(state: &str, kind: &str) -> (Engine, String) {
        let mut eng = Engine::new(crate::db::open(None).unwrap(), PathBuf::from("/tmp"));
        eng.ensure_meta().unwrap();
        let id = format!("test-{state}-{kind}");
        eng.db.insert_rows(
            "pending_effect",
            &["id", "kind", "head_rel", "args_json", "full_json", "req_tx", "batch", "state"],
            &[vec![
                Value::Text(id.clone()),
                Value::Text(kind.to_string()),
                Value::Text("resp".to_string()),
                Value::Text(r#"{"x":"1"}"#.to_string()),
                Value::Text(r#"{"x":"1"}"#.to_string()),
                Value::Int(1),
                Value::Int(0),
                Value::Text(state.to_string()),
            ]],
        ).unwrap();
        (eng, id)
    }

    fn state_of(eng: &Engine, id: &str) -> String {
        eng.db
            .conn()
            .query_row("SELECT state FROM pending_effect WHERE id = ?1", [id], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
    }

    #[test]
    fn orphaned_effect_requeues_when_template_appears() {
        let (mut eng, id) = engine_with(STATE_ORPHANED, "gh");
        let prog = Program::default();

        // No template registered: the orphaned row stays orphaned.
        let no_template = TemplatableExec { kinds: HashSet::new() };
        eng.drain_effects(&prog, &no_template).unwrap();
        assert_eq!(state_of(&eng, &id), STATE_ORPHANED);

        // Template now registered: the orphaned row is re-queued for drain.
        let has_template = TemplatableExec {
            kinds: HashSet::from_iter(vec!["gh".to_string()]),
        };
        eng.drain_effects(&prog, &has_template).unwrap();
        assert_eq!(state_of(&eng, &id), STATE_QUEUED);
    }
}
