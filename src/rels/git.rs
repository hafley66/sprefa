//! Git-derived worktree relations: `changed`, `changed_line`, `created`,
//! `git_ref`, `rev_behind`.

use anyhow::Result;
use std::collections::HashSet;
use std::process::Command;

use crate::ast::{RelDecl, Type, Value};
use crate::engine::Engine;
use crate::lower::txt_tbl;

use super::{col, git_anchors, rekey, RelKind};

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
        vec![RelDecl { name: "changed".into(), cols: vec![col("path", Type::Path)], group: "changed",
            doc: "git status --porcelain -uall vs HEAD (modified/added/renamed/untracked); empty outside git; the rails join", ..Default::default() }]
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
                "SELECT \"path\" FROM {} ORDER BY \"path\"", txt_tbl("changed")))?;
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
            cols: vec![col("path", Type::Path), col("line", Type::Int)], group: "changed",
            doc: "new-side lines of git diff -U0 HEAD hunks plus every line of untracked files; pure-deletion hunks emit nothing; line-scoped rails precision", ..Default::default() }]
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
                txt_tbl("changed_line")))?;
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

// --- git_ref -----------------------------------------------------------------

/// Ref inventory across every nameable repo (self + config):
/// `git_ref(repo, refname, kind, sha)` — one row per branch/tag/remote ref plus
/// a `("HEAD", "head")` row per repo. `sha` is the peeled commit for annotated
/// tags. Two subprocesses per repo (`for-each-ref` + `rev-parse HEAD`), batched
/// per repo, never per ref. Feeds pin-skew queries: a lockfile-derived ref
/// joins here to learn what it points at and what else exists.
pub struct GitRefKind;

impl RelKind for GitRefKind {
    fn rels(&self) -> &'static [&'static str] {
        &["git_ref"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "git_ref".into(),
            cols: vec![col("repo", Type::Text), col("refname", Type::Text),
                       col("kind", Type::Text), col("sha", Type::Text)], group: "git-ref",
            doc: "every branch/tag/remote ref plus HEAD across self + config repos (repo, refname, kind, sha); annotated tags peeled to the commit", ..Default::default() }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in ref-inventory relation"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let mut rows: Vec<(String, String, String, String)> = Vec::new();
        let mut roots: Vec<(String, std::path::PathBuf)> =
            eng.repo_roots().into_iter().collect();
        roots.sort();
        for (slug, root) in roots {
            let head = Command::new("git").arg("-C").arg(&root)
                .args(["rev-parse", "HEAD"]).output();
            if let Ok(h) = head {
                if h.status.success() {
                    let sha = String::from_utf8_lossy(&h.stdout).trim().to_string();
                    rows.push((slug.clone(), "HEAD".into(), "head".into(), sha));
                } else {
                    continue; // not a git repo (or empty): no rows for this slug
                }
            } else {
                continue;
            }
            let out = Command::new("git").arg("-C").arg(&root)
                .args(["for-each-ref",
                       "--format=%(refname)\u{1f}%(refname:short)\u{1f}%(objectname)\u{1f}%(*objectname)"])
                .output()?;
            if !out.status.success() { continue; }
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                let f: Vec<&str> = line.split('\u{1f}').collect();
                if f.len() < 4 { continue; }
                let kind = if f[0].starts_with("refs/heads/") { "branch" }
                           else if f[0].starts_with("refs/tags/") { "tag" }
                           else if f[0].starts_with("refs/remotes/") { "remote" }
                           else { "other" };
                // annotated tag: %(*objectname) is the peeled commit
                let sha = if f[3].is_empty() { f[2] } else { f[3] };
                rows.push((slug.clone(), f[1].to_string(), kind.to_string(), sha.to_string()));
            }
        }
        rows.sort();
        rows.dedup();
        let existing: Vec<(String, String, String, String)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"repo\",\"refname\",\"kind\",\"sha\" FROM {} ORDER BY 1,2,3,4",
                txt_tbl("git_ref")))?;
            let rs = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
            rs.filter_map(|x| x.ok()).collect()
        };
        if existing == rows { return Ok(false); }
        let out: Vec<Vec<Value>> = rows.into_iter()
            .map(|(r, n, k, s)| vec![Value::Text(r), Value::Text(n), Value::Text(k), Value::Text(s)])
            .collect();
        eng.refresh_rel("git_ref", &["repo", "refname", "kind", "sha"], &out)?;
        Ok(true)
    }
}

// --- rev_behind ---------------------------------------------------------------

/// Demand-driven ancestry counts. The demand set is a user-DERIVED relation
/// named `rev_cmp_want(repo, refname, upstream)` (the `org`-allowlist pattern:
/// the engine reads it by convention, users head it with rules — typically off
/// lockfile pins). For each wanted row, one
/// `git rev-list --left-right --count upstream...refname` yields
/// `rev_behind(repo, refname, upstream, behind, ahead)`: `behind` = commits in
/// `upstream` missing from `refname` (how far the pin trails), `ahead` =
/// commits in `refname` missing from `upstream` (nonzero ⇒ the pin diverged —
/// it is not an ancestor). Rows read this tick are LAST tick's derivations
/// (same one-tick latency as a data-driven scan), so a one-shot run needs a
/// second tick to see counts. A ref that fails to resolve is skipped loudly,
/// and so is every pair in a SHALLOW clone (grafted history makes the counts
/// wrong, not just incomplete).
pub struct RevBehindKind;

impl RelKind for RevBehindKind {
    fn rels(&self) -> &'static [&'static str] {
        &["rev_behind"]
    }
    fn decls(&self) -> Vec<RelDecl> {
        vec![RelDecl {
            name: "rev_behind".into(),
            cols: vec![col("repo", Type::Text), col("refname", Type::Text),
                       col("upstream", Type::Text), col("behind", Type::Int),
                       col("ahead", Type::Int)], group: "git-ref",
            doc: "demand-driven ancestry counts: derive rev_cmp_want(repo, refname, upstream) and each wanted pair yields behind/ahead commit counts (ahead>0 = the ref diverged from upstream); one-tick latency like a data-driven scan; unresolvable refs and shallow clones skip loudly", ..Default::default() }]
    }
    fn reserved_msg(&self) -> &'static str {
        "the built-in demand-driven ancestry relation (derive rev_cmp_want to fill it)"
    }
    fn refresh(&self, eng: &Engine) -> Result<bool> {
        let want: Vec<(String, String, String)> = match eng.rels.get("rev_cmp_want") {
            None => Vec::new(),
            Some(meta) => {
                if meta.cols.len() < 3 {
                    anyhow::bail!("rev_cmp_want needs 3 columns (repo, refname, upstream); \
                                   found {}", meta.cols.len());
                }
                let conn = eng.db.conn();
                let mut s = conn.prepare(&format!(
                    "SELECT DISTINCT \"{}\",\"{}\",\"{}\" FROM {} ORDER BY 1,2,3",
                    meta.col_name(0), meta.col_name(1), meta.col_name(2),
                    txt_tbl("rev_cmp_want")))?;
                let rs = s.query_map([], |r| Ok((
                    r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?)))?;
                rs.filter_map(|x| x.ok()).collect()
            }
        };
        let roots = eng.repo_roots();
        let mut rows: Vec<(String, String, String, i64, i64)> = Vec::new();
        // A shallow clone grafts HEAD, so nearly every ref counts as "ahead" —
        // garbage ancestry, silently. Skip the whole repo loudly instead
        // (`git fetch --unshallow` fixes it); checked once per repo, not per pair.
        let mut shallow: std::collections::HashMap<String, bool> = std::collections::HashMap::new();
        for (repo, refname, upstream) in want {
            // The self-form coordinates a scan accepts ("."/""/"self") resolve
            // to the self slug here too; the emitted row keeps the user's
            // spelling so it joins back to rev_cmp_want.
            let key = match repo.as_str() {
                "." | "" | "self" => eng.self_slug(),
                _ => repo.clone(),
            };
            let Some(root) = roots.get(&key) else {
                eprintln!("[rev_behind] skip {repo}/{refname}: unknown repo slug");
                continue;
            };
            let is_shallow = match shallow.get(&key) {
                Some(s) => *s,
                None => {
                    let s = Command::new("git").arg("-C").arg(root)
                        .args(["rev-parse", "--is-shallow-repository"]).output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
                        .unwrap_or(false);
                    if s {
                        eprintln!("[rev_behind] skip {repo}: shallow clone — ancestry \
                                   counts would be wrong; `git -C {} fetch --unshallow`",
                            root.display());
                    }
                    shallow.insert(key.clone(), s);
                    s
                }
            };
            if is_shallow { continue; }
            let out = Command::new("git").arg("-C").arg(root)
                .args(["rev-list", "--left-right", "--count",
                       &format!("{upstream}...{refname}")]).output()?;
            if !out.status.success() {
                eprintln!("[rev_behind] skip {repo}: {upstream}...{refname} did not resolve");
                continue;
            }
            let text = String::from_utf8_lossy(&out.stdout);
            let mut it = text.split_whitespace();
            let (Some(behind), Some(ahead)) =
                (it.next().and_then(|s| s.parse::<i64>().ok()),
                 it.next().and_then(|s| s.parse::<i64>().ok())) else { continue };
            rows.push((repo, refname, upstream, behind, ahead));
        }
        rows.sort();
        rows.dedup();
        let existing: Vec<(String, String, String, i64, i64)> = {
            let conn = eng.db.conn();
            let mut s = conn.prepare(&format!(
                "SELECT \"repo\",\"refname\",\"upstream\",\"behind\",\"ahead\" \
                 FROM {} ORDER BY 1,2,3,4,5", txt_tbl("rev_behind")))?;
            let rs = s.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?, r.get::<_, i64>(4)?)))?;
            rs.filter_map(|x| x.ok()).collect()
        };
        if existing == rows { return Ok(false); }
        let out: Vec<Vec<Value>> = rows.into_iter()
            .map(|(r, n, u, b, a)| vec![Value::Text(r), Value::Text(n), Value::Text(u),
                                        Value::Int(b), Value::Int(a)])
            .collect();
        eng.refresh_rel("rev_behind",
            &["repo", "refname", "upstream", "behind", "ahead"], &out)?;
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
                       col("email", Type::Text), col("ts", Type::Int)], group: "created",
            doc: "files added since their first appearance, with author name/email/timestamp", ..Default::default() }]
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
                txt_tbl("created")))?;
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
