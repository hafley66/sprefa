//! Git-derived built-in relation families behind one trait.
//!
//! Each family (`changed`, `changed_line`, `created`) used to be four loose
//! pieces in engine.rs — a `*_RELS` const, a `*_rel_decls()` fn, a
//! `*_rels_used()` gate, and a `refresh_*_rel()` method — wired by a
//! hand-written fan-out repeated in `tick`, `tick_paths`, `declare_builtins`,
//! `all_builtin_decls`, and the reserved-name guard. This module collapses that
//! shape: one `RelKind` impl per family, one `rel_kinds()` registry the call
//! sites loop over. The refresh BODIES live here too (not thin wrappers), so the
//! code actually leaves engine.rs.
//!
//! Adding a git-derived family is now: write a unit struct, impl `RelKind`, add
//! it to `rel_kinds()`. The five call sites pick it up for free.

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ast::{Col, Program, RelDecl, Type, Value};
use crate::engine::{rels_used, Engine};
use crate::lower::tbl;

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
}

/// Every git-derived built-in family, in declaration order. `tick`,
/// `tick_paths`, `declare_builtins`, `all_builtin_decls`, and the reserved-name
/// guard iterate THIS instead of repeating the family list.
pub fn rel_kinds() -> &'static [&'static dyn RelKind] {
    &[&ChangedKind, &ChangedLineKind, &CreatedKind]
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
