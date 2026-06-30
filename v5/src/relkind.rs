//! Built-in source-relation families behind one trait.
//!
//! Each family (`changed`, `changed_line`, `created`, the analysis-derived
//! `agent` / `type_shape` / `type_lgg` / catalog families, the SCIP importer
//! `scip_*`, the clone proposers `propose_extract` / `propose_clone`, and the
//! embedding `similar`) used to be four loose pieces in engine.rs — a `*_RELS`
//! const, a `*_rel_decls()` fn, a `*_rels_used()` gate, and a `refresh_*_rel()`
//! method — wired by a hand-written fan-out repeated in `tick`, `tick_paths`,
//! `declare_builtins`, `all_builtin_decls`, and the reserved-name guard. This
//! module collapses that shape: one `RelKind` impl per family, one `rel_kinds()`
//! registry the call sites loop over. The refresh BODIES live here too (not thin
//! wrappers), so the code actually leaves engine.rs.
//!
//! Adding a family is now: write a unit struct, impl `RelKind`, add it to
//! `rel_kinds()`. The five call sites pick it up for free.
//!
//! Contract a family must match to live here: a no-arg, whole-set
//! `refresh(eng) -> Ok(changed?)` that self-diffs against what is stored
//! (returns `Ok(false)` on the steady-state no-op). A family that should NOT
//! re-run on every incremental tick overrides `dirty(changed)` to gate on the
//! changed-path set (`ScipKind` gates on `index.scip`). Bodies that need more of
//! the `Engine` surface reach it through the `pub(crate)` read helpers
//! (`repo_roots` / `node_file_set` / `read_content` / `knn_rows`); bounding that
//! surface behind a `RelCtx` borrow struct is the deferred encapsulation step in
//! `plans/2026-06-30-engine-breakdown-proposal.md`. Families that still don't fit
//! — a delta refresh (spine/node/module), extracted args (every/clock
//! intervals), or a `()` return that always runs (builtin/type/call/dataflow/
//! doc/daemon/effect) — await the further staged trait extensions there.

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ast::{Col, Program, RelDecl, Type, Value};
use crate::engine::{all_builtin_decls, builtin_rel_docs, fn_docs, knn_rows, op_docs, read_content,
                    rels_used, scip_descriptor_name, Engine};
use crate::lower::tbl;
use crate::scip_import;
use crate::typegraph;

/// A built-in, git-derived relation family: its name(s), column schema, the
/// lazy-use gate, and the whole-set refresh.
pub trait RelKind: Sync {
    /// The relation name(s) this family owns. Reserved against user `.dl`
    /// programs; the `changed_source_rels` keys on an incremental tick.
    fn rels(&self) -> &'static [&'static str];
    /// Column schema, one `RelDecl` per name in `rels()`.
    fn decls(&self) -> Vec<RelDecl>;
    /// Phrase for the reserved-name bail message ("`<name>` is {phrase}").
    fn reserved_msg(&self) -> &'static str;
    /// Whole-set recompute against the engine's git state; `Ok(true)` iff the
    /// stored set changed (drives the `changed` flag / rebuild scope).
    fn refresh(&self, eng: &Engine) -> Result<bool>;
    /// Whether the program references any owned name (default: lazy `rels_used`).
    fn used(&self, prog: &Program) -> bool {
        rels_used(prog, self.rels())
    }
    /// Should an *incremental* tick (`tick_paths`) call `refresh`? Default: yes,
    /// every tick — the self-diffing families re-read and early-out on a no-op.
    /// `ScipKind` overrides to gate on `index.scip` being in the changed set, so
    /// editing source code never forces a full SCIP-index reload. Not consulted
    /// on a full `tick` (which always refreshes every used family). `changed` is
    /// the set of repo-relative paths the incremental tick saw move.
    fn dirty(&self, _changed: &HashSet<String>) -> bool {
        true
    }
}

/// Every git-derived built-in family, in declaration order. `tick`,
/// `tick_paths`, `declare_builtins`, `all_builtin_decls`, and the reserved-name
/// guard iterate THIS instead of repeating the family list.
pub fn rel_kinds() -> &'static [&'static dyn RelKind] {
    &[&ChangedKind, &ChangedLineKind, &CreatedKind,
      &AgentKind, &DlDiagKind, &TypeShapeKind, &TypeLggKind, &CatalogKind,
      &ScipKind, &ProposeExtractKind, &ProposeCloneKind, &EmbedKind]
}

/// Flattened column decls across the registry, for `all_builtin_decls` /
/// `declare_builtins`.
pub fn rel_kind_decls() -> Vec<RelDecl> {
    rel_kinds().iter().flat_map(|k| k.decls()).collect()
}

fn col(n: &str, t: Type) -> Col {
    Col::plain(n.to_string(), t)
}

/// The two anchors every git-derived family needs to re-key git's paths to
/// repo-relative: `(toplevel, canonical root)`. git prints the PHYSICAL toplevel
/// (macOS `/private/var`) while `--root` may be the symlink, so a path is joined
/// onto `toplevel` then stripped of `root`. `None` when the root isn't a git
/// repo — every caller then yields an empty relation, not an error.
fn git_anchors(eng: &Engine) -> Option<(PathBuf, PathBuf)> {
    let out = Command::new("git").arg("-C").arg(&eng.root)
        .args(["rev-parse", "--show-toplevel"]).output().ok()?;
    if !out.status.success() { return None; }
    let toplevel = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
    let root = std::fs::canonicalize(&eng.root).unwrap_or_else(|_| eng.root.clone());
    Some((toplevel, root))
}

/// Re-key one git-printed path to repo-relative: join onto `toplevel`, strip
/// `root`, normalize separators. `None` drops a path outside the root.
fn rekey(toplevel: &Path, root: &Path, p: &str) -> Option<String> {
    toplevel.join(p).strip_prefix(root).ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
}

// --- changed -----------------------------------------------------------------

/// Worktree diff relation. `changed(path)` holds every path `git status` says
/// differs from HEAD in the self repo: modified, added, renamed (new side),
/// untracked. Lazily refreshed like the other built-in indexers; the rails use
/// case is `diag(...) <- some_hit(p, ...), changed(p).` so a check scoped to
/// what an edit session touched never fires on pre-existing repo debt.
pub struct ChangedKind;

impl RelKind for ChangedKind {
    fn rels(&self) -> &'static [&'static str] {
        &["changed"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "changed".into(), cols: vec![col("path", Type::Path)] }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in worktree-diff relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let mut paths: Vec<String> = Vec::new();
        if let Some((toplevel, root)) = git_anchors(eng) {
            let status = Command::new("git").arg("-C").arg(&eng.root)
                .args(["status", "--porcelain", "-uall"]).output()?;
            if status.status.success() {
                for line in String::from_utf8_lossy(&status.stdout).lines() {
                    if line.len() < 4 { continue; }
                    let entry = &line[3..];
                    // a rename prints "old -> new"; the worktree file is the new side
                    let p = entry.rsplit(" -> ").next().unwrap_or(entry).trim_matches('"');
                    if let Some(rel) = rekey(&toplevel, &root, p) {
                        paths.push(rel);
                    }
                }
            }
        }
        paths.sort();
        paths.dedup();
        let existing: Vec<String> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\" FROM {} ORDER BY \"path\"", tbl("changed")))?;
            let rows = s.query_map([], |r| r.get::<_, String>(0))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if existing == paths { return Ok(false); }
        let rows: Vec<Vec<Value>> = paths.into_iter().map(|p| vec![Value::Text(p)]).collect();
        eng.refresh_rel("changed", &["path"], &rows)?;
        Ok(true)
    }
}

// --- changed_line ------------------------------------------------------------

/// Line-level worktree diff: `(path, line)` for every new-side line of every
/// hunk in `git diff -U0 HEAD`, plus every line of untracked files (which the
/// diff omits). Lets a rail scope to the touched lines, not the touched path:
/// `diag(p, l, ...) <- hit(p, l), changed_line(p, l).` instead of `changed(p)`,
/// so a touch on engine.rs surfaces only the `.conn()` calls on edited lines.
pub struct ChangedLineKind;

impl RelKind for ChangedLineKind {
    fn rels(&self) -> &'static [&'static str] {
        &["changed_line"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "changed_line".into(),
            cols: vec![col("path", Type::Path), col("line", Type::Int)],
        }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in line-diff relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let mut rows: Vec<(String, i64)> = Vec::new();
        if let Some((toplevel, root)) = git_anchors(eng) {
            let canon = |p: &str| rekey(&toplevel, &root, p);
            // (1) tracked modifications via context-free hunks.
            let diff = Command::new("git").arg("-C").arg(&eng.root)
                .args(["diff", "-U0", "HEAD"]).output();
            if let Ok(d) = diff {
                if d.status.success() {
                    let stdout = String::from_utf8_lossy(&d.stdout);
                    let mut cur: Option<String> = None;
                    for line in stdout.lines() {
                        if let Some(rest) = line.strip_prefix("+++ ") {
                            cur = if rest == "/dev/null" { None }
                                  else { canon(rest.strip_prefix("b/").unwrap_or(rest)) };
                        } else if line.starts_with("@@") {
                            if let (Some(path), Some((c, n))) = (&cur, hunk_new_range(line)) {
                                for ln in c..c + n {
                                    rows.push((path.clone(), ln));
                                }
                            }
                        }
                    }
                }
            }
            // (2) untracked files: emit every line; git diff HEAD omits them.
            let status = Command::new("git").arg("-C").arg(&eng.root)
                .args(["status", "--porcelain", "-uall"]).output()?;
            if status.status.success() {
                for line in String::from_utf8_lossy(&status.stdout).lines() {
                    if line.len() < 4 || &line[..2] != "??" { continue; }
                    let p = line[3..].rsplit(" -> ").next().unwrap_or(&line[3..])
                        .trim_matches('"');
                    if let Some(rel) = canon(p) {
                        if let Ok(s) = std::fs::read_to_string(eng.root.join(&rel)) {
                            for ln in 1..=(s.lines().count() as i64) {
                                rows.push((rel.clone(), ln));
                            }
                        }
                    }
                }
            }
        }
        rows.sort();
        rows.dedup();
        let existing: Vec<(String, i64)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\", \"line\" FROM {} ORDER BY \"path\", \"line\"",
                tbl("changed_line")))?;
            let rs = s.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            rs.filter_map(|x| x.ok()).collect()
        };
        if existing == rows { return Ok(false); }
        let out: Vec<Vec<Value>> = rows.into_iter()
            .map(|(p, l)| vec![Value::Text(p), Value::Int(l)]).collect();
        eng.refresh_rel("changed_line", &["path", "line"], &out)?;
        Ok(true)
    }
}

/// `@@ -a,b +c,d @@` -> the new-side `(start, count)`. A bare `+c` (no comma)
/// means count 1. Pure; shared by `ChangedLineKind`.
fn hunk_new_range(line: &str) -> Option<(i64, i64)> {
    let f: Vec<&str> = line.split_whitespace().collect();
    // ["@@", "-a,b", "+c,d", "@@", <optional fn label>...]
    let new = f.get(2)?.strip_prefix('+')?;
    let (c, n) = match new.split_once(',') {
        Some((c, n)) => (c.parse::<i64>().ok()?, n.parse::<i64>().ok()?),
        None => (new.parse::<i64>().ok()?, 1),
    };
    Some((c, n))
}

// --- created -----------------------------------------------------------------

/// File-authorship relation. `created(path, name, email, ts)` is one row per
/// tracked file: the author of the commit that ADDED it (its creation), from
/// `git log --reverse --diff-filter=A --name-only`. `--reverse` orders
/// oldest-first, so the first time a path appears is its add. Renames are `R`,
/// not `A`, so a file moved with `git mv` keeps its original creation row only
/// if history is followed (it is not here — the new path looks un-created until
/// it is next added). Pairs with `changed` for "who wrote what".
pub struct CreatedKind;

impl RelKind for CreatedKind {
    fn rels(&self) -> &'static [&'static str] {
        &["created"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "created".into(),
            cols: vec![col("path", Type::Path), col("name", Type::Text),
                       col("email", Type::Text), col("ts", Type::Int)],
        }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in file-authorship relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let mut created: Vec<(String, String, String, i64)> = Vec::new(); // path, name, email, ts
        if let Some((toplevel, root)) = git_anchors(eng) {
            // \x01 prefixes the per-commit author line; \x1f separates fields.
            let log = Command::new("git").arg("-C").arg(&eng.root)
                .args(["log", "--reverse", "--diff-filter=A", "--name-only",
                       "--format=\x01%an\x1f%ae\x1f%at"]).output()?;
            if log.status.success() {
                let mut seen: HashSet<String> = HashSet::new();
                let (mut name, mut email, mut ts) = (String::new(), String::new(), 0i64);
                for line in String::from_utf8_lossy(&log.stdout).lines() {
                    if let Some(hdr) = line.strip_prefix('\x01') {
                        let mut it = hdr.split('\x1f');
                        name = it.next().unwrap_or("").to_string();
                        email = it.next().unwrap_or("").to_string();
                        ts = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                    } else if !line.is_empty() {
                        if let Some(rel) = rekey(&toplevel, &root, line.trim_matches('"')) {
                            if seen.insert(rel.clone()) {
                                created.push((rel, name.clone(), email.clone(), ts));
                            }
                        }
                    }
                }
            }
        }
        created.sort();
        let existing: Vec<(String, String, String, i64)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\",\"name\",\"email\",\"ts\" FROM {} ORDER BY 1,2,3,4",
                tbl("created")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if existing == created { return Ok(false); }
        let rows: Vec<Vec<Value>> = created.into_iter()
            .map(|(p, n, e, t)| vec![Value::Text(p), Value::Text(n), Value::Text(e), Value::Int(t)])
            .collect();
        eng.refresh_rel("created", &["path", "name", "email", "ts"], &rows)?;
        Ok(true)
    }
}

// --- agent (analysis-derived) ------------------------------------------------

/// Agent-harness edit relations. `agent_edit(harness, session, idx, path)` is one
/// row per edit a coding agent made under `--root`; `agent_touch(harness, session,
/// path)` is the last-edited path per session. Read from `agent::agent_harnesses`.
pub struct AgentKind;

impl RelKind for AgentKind {
    fn rels(&self) -> &'static [&'static str] {
        &["agent_edit", "agent_touch"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![
            RelDecl { name: "agent_edit".into(), cols: vec![
                col("harness", Type::Text), col("session", Type::Text),
                col("idx", Type::Int), col("path", Type::Path)] },
            RelDecl { name: "agent_touch".into(), cols: vec![
                col("harness", Type::Text), col("session", Type::Text),
                col("path", Type::Path)] },
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "a built-in agent-harness relation (agent_edit / agent_touch)"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let root = std::fs::canonicalize(&eng.root).unwrap_or_else(|_| eng.root.clone());
        let mut edits: Vec<(String, String, i64, String)> = Vec::new(); // harness, session, idx, path
        let mut touch: Vec<(String, String, String)> = Vec::new();      // harness, session, path @ max idx
        for h in crate::agent::agent_harnesses() {
            let hn = h.name().to_string();
            for sess in h.sessions_for(&root) {
                let maxidx = sess.edits.iter().map(|e| e.idx).max();
                for e in &sess.edits {
                    edits.push((hn.clone(), sess.id.clone(), e.idx, e.path.clone()));
                    if Some(e.idx) == maxidx {
                        touch.push((hn.clone(), sess.id.clone(), e.path.clone()));
                    }
                }
            }
        }
        edits.sort(); edits.dedup();
        touch.sort(); touch.dedup();
        // Early-out: compare agent_edit (the superset) against what is stored.
        let existing: Vec<(String, String, i64, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"harness\", \"session\", \"idx\", \"path\" FROM {} ORDER BY 1,2,3,4",
                tbl("agent_edit")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if existing == edits { return Ok(false); }
        let edit_rows: Vec<Vec<Value>> = edits.into_iter()
            .map(|(h, s, i, p)| vec![Value::Text(h), Value::Text(s), Value::Int(i), Value::Text(p)])
            .collect();
        eng.refresh_rel("agent_edit", &["harness", "session", "idx", "path"], &edit_rows)?;
        let touch_rows: Vec<Vec<Value>> = touch.into_iter()
            .map(|(h, s, p)| vec![Value::Text(h), Value::Text(s), Value::Text(p)])
            .collect();
        eng.refresh_rel("agent_touch", &["harness", "session", "path"], &touch_rows)?;
        Ok(true)
    }
}

// --- dl_diag (self-validation) -----------------------------------------------

/// `dl_diag(path, line, col, end_line, end_col, severity, code, msg)` — the
/// engine's own lexer/parser/typechecker run over every scanned `.dl` file (the
/// `file` rows whose path ends `.dl`), so dl lints dl the way rust-analyzer lints
/// Rust. Byte spans from `TypeDiag` map to 1-based line / 0-based byte col. A lex
/// or parse failure is one whole-file error row (code `lex`/`parse`); typecheck
/// diagnostics (brand-mismatch, unknown-anchor, type errors, stratification)
/// carry their real span. Same pass as `--check`, relocated into a relation.
///
/// Validated FILE-LOCALLY: a `use`-split program is checked per file, so a
/// relation defined in an *included* file is out of scope here (run `dl --check`
/// on the whole set for cross-file `use` resolution). Column names mirror the
/// `diag` sink so a rail forwards by name:
/// `diag(...) <- agent_changed(p), p =~ /\.dl$/, dl_diag(p, ...).`
pub struct DlDiagKind;

/// Byte offset -> (1-based line, 0-based byte col) within `content`.
fn offset_to_line_col(content: &str, off: u32) -> (i64, i64) {
    let off = (off as usize).min(content.len());
    let mut line = 1i64;
    let mut line_start = 0usize;
    for (i, b) in content.as_bytes().iter().enumerate() {
        if i >= off { break; }
        if *b == b'\n' { line += 1; line_start = i + 1; }
    }
    (line, (off - line_start) as i64)
}

type DlDiagRow = (String, i64, i64, i64, i64, String, String, String);

/// Lex+parse+typecheck one `.dl` source in isolation. A lex/parse failure is one
/// whole-file row; typecheck diags carry their byte span mapped to line/col.
fn validate_dl_source(content: &str, path: &str) -> Vec<DlDiagRow> {
    let toks = match crate::lex::lex(content) {
        Ok(t) => t,
        Err(e) => return vec![(path.into(), 1, 0, 1, 0, "error".into(), "lex".into(), e.to_string())],
    };
    let mut prog = match crate::parse::parse(toks) {
        Ok(p) => p,
        Err(e) => return vec![(path.into(), 1, 0, 1, 0, "error".into(), "parse".into(), e.to_string())],
    };
    crate::typecheck::check_and_normalize(&mut prog, path).into_iter().map(|d| {
        let (l0, c0) = offset_to_line_col(content, d.span.0);
        let (l1, c1) = offset_to_line_col(content, d.span.1);
        (path.to_string(), l0, c0, l1, c1, d.severity.as_str().to_string(), d.code, d.msg)
    }).collect()
}

impl RelKind for DlDiagKind {
    fn rels(&self) -> &'static [&'static str] { &["dl_diag"] }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "dl_diag".into(), cols: vec![
            col("path", Type::Path), col("line", Type::Int), col("col", Type::Int),
            col("end_line", Type::Int), col("end_col", Type::Int),
            col("severity", Type::Text), col("code", Type::Text), col("msg", Type::Text)] }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in dl self-diagnostics relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        // Every scanned .dl path; the WORK text is read from disk (file.content is a
        // content hash, not the source). Validates the on-disk working copy — the
        // lint-on-edit target. A path that can't be read (a non-self repo root, a
        // git-only rev) is skipped, never a false diag.
        let paths: Vec<String> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT DISTINCT \"path\" FROM {} WHERE \"path\" LIKE '%.dl' ORDER BY \"path\"",
                tbl("file")))?;
            let rows = s.query_map([], |r| r.get::<_, String>(0))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let mut rows: Vec<DlDiagRow> = Vec::new();
        for path in &paths {
            let Ok(content) = std::fs::read_to_string(eng.root.join(path)) else { continue };
            rows.extend(validate_dl_source(&content, path));
        }
        rows.sort();
        rows.dedup();
        let existing: Vec<DlDiagRow> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\",\"line\",\"col\",\"end_line\",\"end_col\",\"severity\",\"code\",\"msg\" \
                 FROM {} ORDER BY 1,2,3,4,5,6,7,8", tbl("dl_diag")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?, r.get::<_, String>(7)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if existing == rows { return Ok(false); }
        let out: Vec<Vec<Value>> = rows.into_iter().map(|(p, l, c, el, ec, sev, code, msg)| vec![
            Value::Text(p), Value::Int(l), Value::Int(c), Value::Int(el), Value::Int(ec),
            Value::Text(sev), Value::Text(code), Value::Text(msg)]).collect();
        eng.refresh_rel("dl_diag",
            &["path", "line", "col", "end_line", "end_col", "severity", "code", "msg"], &out)?;
        Ok(true)
    }
}

// --- type_shape (analysis-derived) -------------------------------------------

/// `type_shape(name, hash)` — Merkle shape hash per type, from the current
/// `type_edge` rows via `typegraph::type_shape_hashes` (fixpoint). Reads the edge
/// set the type-rels refresh already populated, so it must run AFTER it (the
/// `rel_kinds()` loop sits after `refresh_type_rels` in both tick paths).
pub struct TypeShapeKind;

impl RelKind for TypeShapeKind {
    fn rels(&self) -> &'static [&'static str] {
        &["type_shape"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "type_shape".into(),
            cols: vec![col("name", Type::Text), col("hash", Type::Text)] }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in type-shape relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let edges: Vec<(String, String, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"from\",\"to\",\"kind\" FROM {}", tbl("type_edge")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let computed: Vec<(String, String)> = typegraph::type_shape_hashes(&edges);
        let stored: Vec<(String, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"name\",\"hash\" FROM {} ORDER BY \"name\",\"hash\"", tbl("type_shape")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if stored == computed { return Ok(false); }
        let rows: Vec<Vec<Value>> = computed.into_iter()
            .map(|(n, h)| vec![Value::Text(n), Value::Text(h)]).collect();
        eng.refresh_rel("type_shape", &["name", "hash"], &rows)?;
        Ok(true)
    }
}

// --- type_lgg (analysis-derived) ---------------------------------------------

/// `type_lgg(a, b, vars)` — least-general-generalization variable count per type
/// pair, from the resolved `type_link` graph via `typegraph::type_lgg_pairs`.
/// Uses `type_link` (SCIP-resolved syms), not `type_edge` (bare names), so the
/// LGG recurses into resolved local types. Runs after the type-rels refresh.
pub struct TypeLggKind;

impl RelKind for TypeLggKind {
    fn rels(&self) -> &'static [&'static str] {
        &["type_lgg"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "type_lgg".into(),
            cols: vec![col("a", Type::Text), col("b", Type::Text), col("vars", Type::Int)] }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in type-lgg relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let edges: Vec<(String, String, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"src\",\"dst\",\"kind\" FROM {}", tbl("type_link")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let computed: Vec<(String, String, i64)> = typegraph::type_lgg_pairs(&edges);
        let stored: Vec<(String, String, i64)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"a\",\"b\",\"vars\" FROM {} ORDER BY \"a\",\"b\",\"vars\"", tbl("type_lgg")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if stored == computed { return Ok(false); }
        let rows: Vec<Vec<Value>> = computed.into_iter()
            .map(|(a, b, v)| vec![Value::Text(a), Value::Text(b), Value::Int(v)]).collect();
        eng.refresh_rel("type_lgg", &["a", "b", "vars"], &rows)?;
        Ok(true)
    }
}

// --- catalog (self-describing) -----------------------------------------------

/// `rel_catalog(name, group, cols, doc)` + `fn_catalog(name, arity, group, doc)`
/// — the engine describing its own built-in relations and scalar functions, from
/// `all_builtin_decls` / `builtin_rel_docs` / `fn_docs`. Static (no git/file
/// input), so `refresh` always re-emits and reports changed; cheap (bounded by
/// the built-in count).
pub struct CatalogKind;

impl RelKind for CatalogKind {
    fn rels(&self) -> &'static [&'static str] {
        &["rel_catalog", "fn_catalog", "op_catalog"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![
            RelDecl { name: "rel_catalog".into(), cols: vec![
                col("name", Type::Text), col("group", Type::Text),
                col("cols", Type::Text), col("doc", Type::Text)] },
            RelDecl { name: "fn_catalog".into(), cols: vec![
                col("name", Type::Text), col("arity", Type::Int),
                col("group", Type::Text), col("doc", Type::Text)] },
            RelDecl { name: "op_catalog".into(), cols: vec![
                col("op", Type::Text), col("kind", Type::Text),
                col("syntax", Type::Text), col("doc", Type::Text)] },
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in self-describing relation catalog (rel_catalog / fn_catalog / op_catalog)"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let docs: HashMap<&str, (&str, &str)> =
            builtin_rel_docs().iter().map(|(n, g, s)| (*n, (*g, *s))).collect();
        let rows: Vec<Vec<Value>> = all_builtin_decls().iter().map(|d| {
            let cols = format!("({})",
                d.cols.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(", "));
            let (group, summary) = docs.get(d.name.as_str()).copied().unwrap_or(("", ""));
            vec![Value::Text(d.name.clone()), Value::Text(group.to_string()),
                 Value::Text(cols), Value::Text(summary.to_string())]
        }).collect();
        eng.refresh_rel("rel_catalog", &["name", "group", "cols", "doc"], &rows)?;

        let fn_rows: Vec<Vec<Value>> = fn_docs().iter().map(|(n, a, g, d)| {
            vec![Value::Text(n.to_string()), Value::Int(*a as i64),
                 Value::Text(g.to_string()), Value::Text(d.to_string())]
        }).collect();
        eng.refresh_rel("fn_catalog", &["name", "arity", "group", "doc"], &fn_rows)?;

        let op_rows: Vec<Vec<Value>> = op_docs().iter().map(|(op, kind, syn, d)| {
            vec![Value::Text(op.to_string()), Value::Text(kind.to_string()),
                 Value::Text(syn.to_string()), Value::Text(d.to_string())]
        }).collect();
        eng.refresh_rel("op_catalog", &["op", "kind", "syntax", "doc"], &op_rows)?;
        Ok(true)
    }
}

// --- scip (importer, reload-gated) -------------------------------------------

/// SCIP-importer relations, loaded from an existing `index.scip`.
/// `scip_def(symbol, file)` / `scip_ref(file, symbol, def_file)` /
/// `scip_edge(src, dst)` are the file-level def/ref/import graph;
/// `scip_name(symbol, name)` is the descriptor's trailing identifier (computed
/// where the moniker grammar lives — a pure-dl split can't isolate it);
/// `scip_fn_edge(caller, callee)` is the function-level call graph;
/// `scip_callee_type(sym, type)` maps a method moniker to its receiver type;
/// `scip_local(fn, name)` the locals; `scip_impl(impl, iface)` the
/// implementation edges. Unlike the self-diffing families, the importer always
/// re-emits when run, so `dirty` gates an incremental tick on `index.scip`
/// itself moving.
pub struct ScipKind;

impl RelKind for ScipKind {
    fn rels(&self) -> &'static [&'static str] {
        &["scip_def", "scip_name", "scip_ref", "scip_edge",
          "scip_fn_edge", "scip_callee_type", "scip_local", "scip_impl"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![
            RelDecl { name: "scip_def".into(), cols: vec![col("symbol", Type::Text), col("file", Type::Path)] },
            RelDecl { name: "scip_name".into(), cols: vec![col("symbol", Type::Text), col("name", Type::Text)] },
            RelDecl { name: "scip_ref".into(), cols: vec![col("file", Type::Path), col("symbol", Type::Text), col("def_file", Type::Path)] },
            RelDecl { name: "scip_edge".into(), cols: vec![col("src", Type::Path), col("dst", Type::Path)] },
            RelDecl { name: "scip_fn_edge".into(), cols: vec![col("caller", Type::Text), col("callee", Type::Text)] },
            RelDecl { name: "scip_callee_type".into(), cols: vec![col("sym", Type::Text), col("type", Type::Text)] },
            RelDecl { name: "scip_local".into(), cols: vec![col("fn", Type::Text), col("name", Type::Text)] },
            RelDecl { name: "scip_impl".into(), cols: vec![col("impl", Type::Text), col("iface", Type::Text)] },
        ]
    }
    fn reserved_msg(&self) -> &'static str {
        "a built-in SCIP relation"
    }
    fn dirty(&self, changed: &HashSet<String>) -> bool {
        changed.contains("index.scip")
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let t = |s: &str| Value::Text(s.to_string());
        let Some(path) = scip_import::index_path(&eng.root) else {
            eng.refresh_rel("scip_def", &["symbol", "file"], &[])?;
            eng.refresh_rel("scip_name", &["symbol", "name"], &[])?;
            eng.refresh_rel("scip_ref", &["file", "symbol", "def_file"], &[])?;
            eng.refresh_rel("scip_edge", &["src", "dst"], &[])?;
            eng.refresh_rel("scip_fn_edge", &["caller", "callee"], &[])?;
            eng.refresh_rel("scip_callee_type", &["sym", "type"], &[])?;
            eng.refresh_rel("scip_local", &["fn", "name"], &[])?;
            eng.refresh_rel("scip_impl", &["impl", "iface"], &[])?;
            return Ok(true);
        };
        let rows = scip_import::load(&path)?;
        let defs: Vec<Vec<Value>> = rows.defs.iter().map(|(sym, file)| vec![t(sym), t(file)]).collect();
        // The symbol's descriptor name (last identifier run), computed where the
        // SCIP moniker grammar lives. A pure-dl `split` chain can't isolate it:
        // `…/impl#[Type]method().` needs the `[`/`]`/`#` separators that single-
        // separator split can't all honor. One row per distinct (symbol, name).
        let mut name_set: HashSet<(String, String)> = HashSet::new();
        for (sym, _) in &rows.defs {
            if let Some(name) = scip_descriptor_name(sym) {
                name_set.insert((sym.clone(), name));
            }
        }
        let names: Vec<Vec<Value>> = name_set.iter().map(|(sym, name)| vec![t(sym), t(name)]).collect();
        let refs: Vec<Vec<Value>> = rows.refs.iter()
            .map(|(file, sym, def)| vec![t(file), t(sym), t(def)]).collect();
        let edges: Vec<Vec<Value>> = rows.edges.iter().map(|(src, dst)| vec![t(src), t(dst)]).collect();
        let fn_edges: Vec<Vec<Value>> = rows.fn_edges.iter()
            .map(|(caller, callee)| vec![t(caller), t(callee)]).collect();
        let callee_types: Vec<Vec<Value>> = rows.callee_types.iter()
            .map(|(sym, ty)| vec![t(sym), t(ty)]).collect();
        let locals: Vec<Vec<Value>> = rows.locals.iter()
            .map(|(fn_, name)| vec![t(fn_), t(name)]).collect();
        let impls: Vec<Vec<Value>> = rows.impls.iter()
            .map(|(im, iface)| vec![t(im), t(iface)]).collect();
        eng.refresh_rel("scip_def", &["symbol", "file"], &defs)?;
        eng.refresh_rel("scip_name", &["symbol", "name"], &names)?;
        eng.refresh_rel("scip_ref", &["file", "symbol", "def_file"], &refs)?;
        eng.refresh_rel("scip_edge", &["src", "dst"], &edges)?;
        eng.refresh_rel("scip_fn_edge", &["caller", "callee"], &fn_edges)?;
        eng.refresh_rel("scip_callee_type", &["sym", "type"], &callee_types)?;
        eng.refresh_rel("scip_local", &["fn", "name"], &locals)?;
        eng.refresh_rel("scip_impl", &["impl", "iface"], &impls)?;
        Ok(true)
    }
}

// --- propose_extract (clone proposer) ----------------------------------------

/// Extract-function proposals: one row `(path, lo, hi, param)` per free var of
/// each verbatim-duplicated block found in a scanned Rust file. `lo`/`hi` bound
/// the block's first occurrence (1-based lines); the param set is the inferred
/// extract-fn signature (free vars = read in the block, not bound inside it).
/// Whole-corpus: recompute all, compare to stored, early-out if equal. Reuses
/// `node_file_set` for the file list and `propose::extract_proposals`.
pub struct ProposeExtractKind;

impl RelKind for ProposeExtractKind {
    fn rels(&self) -> &'static [&'static str] {
        &["propose_extract"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "propose_extract".into(),
            cols: vec![col("path", Type::Path), col("lo", Type::Int),
                       col("hi", Type::Int), col("param", Type::Text)] }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in extract-proposal relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let roots = eng.repo_roots();
        let root = eng.root.clone();
        let files = eng.node_file_set(None)?;
        let mut computed: Vec<(String, i64, i64, String)> = Vec::new();
        for (repo, path, rev, _hash) in files {
            if crate::cst::lang_label_for_path(&path) != Some("rust") { continue; }
            let froot = roots.get(&repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, &rev, &path).unwrap_or_default();
            for prop in crate::propose::extract_proposals(&content) {
                for p in prop.params {
                    computed.push((path.clone(), prop.lo as i64, prop.hi as i64, p));
                }
            }
        }
        computed.sort(); computed.dedup();
        let stored: Vec<(String, i64, i64, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"path\",\"lo\",\"hi\",\"param\" FROM {} ORDER BY \"path\",\"lo\",\"hi\",\"param\"",
                tbl("propose_extract")))?;
            let rows = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if stored == computed { return Ok(false); }
        let rows: Vec<Vec<Value>> = computed.into_iter()
            .map(|(p, lo, hi, pm)| vec![Value::Text(p), Value::Int(lo), Value::Int(hi), Value::Text(pm)])
            .collect();
        eng.refresh_rel("propose_extract", &["path", "lo", "hi", "param"], &rows)?;
        Ok(true)
    }
}

// --- propose_clone (multi-kernel clone proposer) -----------------------------

/// Multi-kernel clone-detection relation: `propose_clone(kernel, path, lo, hi,
/// param)`. Runs all 9 clone-detection kernels (verbatim, ast, tree, cfg, ddg,
/// cgraph, ngram, symbol, call) on every scanned Rust file; `kernel` selects the
/// detector. Symbol and call-seq kernels need `index.scip`; they emit no rows if
/// the index is absent.
pub struct ProposeCloneKind;

impl RelKind for ProposeCloneKind {
    fn rels(&self) -> &'static [&'static str] {
        &["propose_clone"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "propose_clone".into(),
            cols: vec![col("kernel", Type::Text), col("path", Type::Path),
                       col("lo", Type::Int), col("hi", Type::Int),
                       col("param", Type::Text)] }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in clone-detection relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let roots = eng.repo_roots();
        let root = eng.root.clone();
        let files = eng.node_file_set(None)?;
        let scip_spans: HashMap<String, Vec<(i32, i32, String)>> =
            if let Some(idx) = scip_import::index_path(&root) {
                match scip_import::load(&idx) {
                    Ok(rows) => {
                        let mut map: HashMap<String, Vec<(i32, i32, String)>> = HashMap::new();
                        for (file, l, c, sym) in rows.occ_spans {
                            map.entry(file).or_default().push((l, c, sym));
                        }
                        map
                    }
                    Err(_) => HashMap::new(),
                }
            } else {
                HashMap::new()
            };
        let mut computed: Vec<(String, String, i64, i64, String)> = Vec::new();
        for (repo, path, rev, _hash) in files {
            if crate::cst::lang_label_for_path(&path) != Some("rust") {
                continue;
            }
            let froot = roots.get(&repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, &rev, &path).unwrap_or_default();
            let spans_owned = scip_spans.get(&path).cloned().unwrap_or_default();
            let spans: Vec<(i32, i32, &str)> = spans_owned
                .iter()
                .map(|(l, c, s)| (*l, *c, s.as_str()))
                .collect();
            let kernels: Vec<(&str, Vec<crate::propose::Proposal>)> = vec![
                ("verbatim", crate::propose::extract_proposals(&content)),
                ("ast", crate::propose::ast_shape_proposals(&content)),
                ("tree", crate::propose::tree_shape_proposals(&content)),
                ("cfg", crate::propose::cfg_shape_proposals(&content)),
                ("ddg", crate::propose::ddg_shape_proposals(&content)),
                ("cgraph", crate::propose::callgraph_shape_proposals(&content)),
                ("ngram", crate::propose::ngram_stat_proposals(&content)),
                ("symbol", crate::propose::symbol_shape_proposals(&content, &spans)),
                ("call", crate::propose::call_seq_proposals(&content, &spans)),
            ];
            for (kname, props) in kernels {
                for prop in props {
                    for p in &prop.params {
                        computed.push((
                            kname.to_string(),
                            path.clone(),
                            prop.lo as i64,
                            prop.hi as i64,
                            p.clone(),
                        ));
                    }
                }
            }
        }
        computed.sort();
        computed.dedup();
        let stored: Vec<(String, String, i64, i64, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"kernel\",\"path\",\"lo\",\"hi\",\"param\" FROM {} ORDER BY \"kernel\",\"path\",\"lo\",\"hi\",\"param\"",
                tbl("propose_clone")
            ))?;
            let rows = s.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?;
            rows.filter_map(|x| x.ok()).collect()
        };
        if stored == computed {
            return Ok(false);
        }
        let rows: Vec<Vec<Value>> = computed
            .into_iter()
            .map(|(k, p, lo, hi, pm)| {
                vec![
                    Value::Text(k),
                    Value::Text(p),
                    Value::Int(lo),
                    Value::Int(hi),
                    Value::Text(pm),
                ]
            })
            .collect();
        eng.refresh_rel("propose_clone", &["kernel", "path", "lo", "hi", "param"], &rows)?;
        Ok(true)
    }
}

// --- embed (embedding similarity) --------------------------------------------

/// Embedding-similarity relation. `similar(a, b, score)` is the top-k nearest
/// neighbors of each embedded interned string by cosine; `score` is an Int =
/// round(cosine * 1_000_000), so a `.dl` rule can threshold in Int-only value
/// space. Vectors are content-addressed: one row per (StringId, backend) in
/// `_embeddings`, so identical content embeds once. Lazy like the other
/// indexers; brute-force O(n^2) cosine over the embedded set (capped by
/// SPREFA_EMBED_MAX, default 4096); SPREFA_SIMILAR_K (default 8) sets neighbors
/// per row. The sqlite-vec ANN path is the scale follow-on.
pub struct EmbedKind;

impl RelKind for EmbedKind {
    fn rels(&self) -> &'static [&'static str] {
        &["similar"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl { name: "similar".into(),
            cols: vec![col("a", Type::Text), col("b", Type::Text), col("score", Type::Int)] }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in embedding-similarity relation (similar)"
    }
    /// Encode every interned `_strings` row lacking a vector for the active
    /// backend (embed-once per (StringId, backend)), then materialize `similar`.
    /// Returns true if the `similar` row set could have changed, false on the
    /// steady-state no-op so the derived rebuild stays scoped.
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let embedder = crate::embed::make(None)?;
        let backend = embedder.name().to_string();
        let max: usize = std::env::var("SPREFA_EMBED_MAX").ok()
            .and_then(|s| s.parse().ok()).unwrap_or(4096);

        // Content with no vector for THIS backend. Capped: only the first `max`
        // un-embedded strings are encoded per tick (the rest catch up next tick).
        let to_embed: Vec<(String, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(
                "SELECT s.id, s.content FROM _strings s
                 WHERE s.id != '0'
                   AND NOT EXISTS (SELECT 1 FROM _embeddings e
                                   WHERE e.sid = s.id AND e.backend = ?1)
                 LIMIT ?2")?;
            let v: Vec<(String, String)> = s.query_map(rusqlite::params![backend, max as i64], |r|
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok()).collect();
            v
        };
        if !to_embed.is_empty() {
            let texts: Vec<&str> = to_embed.iter().map(|(_, c)| c.as_str()).collect();
            let vecs = embedder.encode(&texts)?;
            let dim = embedder.dim() as i64;
            // collect-then-flush: one insert_rows, never per-row (the spine rule).
            let mut rows: Vec<Vec<Value>> = Vec::with_capacity(vecs.len());
            for ((sid, _), mut v) in to_embed.iter().cloned().zip(vecs) {
                crate::embed::l2_normalize(&mut v);
                rows.push(vec![
                    Value::Text(sid), Value::Text(backend.clone()),
                    Value::Int(dim), Value::Text(crate::embed::encode_vec(&v))]);
            }
            eng.db.insert_rows("_embeddings", &["sid", "backend", "dim", "vec"], &rows)?;
        }

        // Steady state: no new content AND `similar` already built -> no recompute.
        let similar_rows: i64 = eng.db.conn().query_row(
            &format!("SELECT count(*) FROM {}", tbl("similar")), [], |r| r.get(0))?;
        if to_embed.is_empty() && similar_rows > 0 { return Ok(false); }

        refresh_similar_rel(eng, &backend, max)?;
        Ok(true)
    }
}

/// Materialize `similar(a, b, score)`: top-k cosine neighbors of each embedded
/// string for `backend`. Brute-force pairwise over the (capped) embedded pool;
/// vectors are L2-normalized at store time so cosine is a dot product. `score` =
/// round(cosine * 1e6) as Int. Shares the `knn_rows` chokepoint with node2vec.
fn refresh_similar_rel(eng: &Engine, backend: &str, max: usize) -> Result<()> {
    let k: usize = std::env::var("SPREFA_SIMILAR_K").ok()
        .and_then(|s| s.parse().ok()).unwrap_or(8);
    let pool: Vec<(String, Vec<f32>)> = {
        let conn = eng.db.conn();
        let mut s = conn.prepare("SELECT sid, vec FROM _embeddings WHERE backend = ?1 LIMIT ?2")?;
        let v: Vec<(String, Vec<f32>)> = s.query_map(rusqlite::params![backend, max as i64], |r|
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .filter_map(|x| x.ok())
            .map(|(sid, txt): (String, String)| (sid, crate::embed::parse_vec(&txt)))
            .collect();
        v
    };
    if pool.len() > 2000 {
        eprintln!("[similar] brute-force KNN over {} vectors (O(n^2)); \
                   cap with SPREFA_EMBED_MAX or wire sqlite-vec", pool.len());
    }
    let rows = knn_rows(&pool, k);
    eng.refresh_rel("similar", &["a", "b", "score"], &rows)?;
    Ok(())
}
