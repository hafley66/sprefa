use anyhow::{bail, Result};
use rayon::prelude::*;
use regex::Regex;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::ast::*;
use crate::lower::{lower_query, lower_rule, tbl};
use crate::scc;

fn scc_node_tbl(edge: &str) -> String { format!("scc_node_{edge}") }
fn scc_edge_tbl(edge: &str) -> String { format!("scc_edge_{edge}") }

/// head relation -> edge relation, for every `head(..) <- closure(edge).` rule.
fn closure_map(rules: &[&Rule]) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for r in rules {
        if let Some(edge) = r.closure_edge() { m.insert(r.head.rel.clone(), edge.to_string()); }
    }
    m
}

/// Unique edge relations across all closure heads (one condensation per graph).
fn dedup_edges(closures: &HashMap<String, String>) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    for e in closures.values() { if !out.contains(&e.as_str()) { out.push(e.as_str()); } }
    out
}

/// The literal a query pins head position `pos` to, via a literal head term or a
/// `where col = "lit"` constraint. None if that position is a free variable.
fn pinned_value(q: &Query, pos: usize) -> Option<String> {
    match &q.head.terms[pos] {
        Term::Str(s) => Some(s.clone()),
        Term::Var(v) => q.wheres.iter().find_map(|c| {
            if c.op != CmpOp::Eq { return None; }
            match (&c.lhs, &c.rhs) {
                (Term::Var(lv), Term::Str(s)) | (Term::Str(s), Term::Var(lv)) if lv == v => Some(s.clone()),
                _ => None,
            }
        }),
        _ => None,
    }
}

/// Closure heads are rebuilt *after* the derived fixpoint, so a derived rule body
/// that reads one would see stale/empty data in the same tick. Reject it (queries
/// run last and are fine; only rule bodies are stratified wrong).
fn check_stratification(derived_rules: &[&Rule], closures: &HashMap<String, String>) -> Result<()> {
    for r in derived_rules {
        for item in &r.body {
            if let BodyItem::Pos(a) | BodyItem::Neg(a) = item {
                if closures.contains_key(&a.rel) {
                    bail!("rule '{}' reads closure relation '{}' in its body; closures are \
                           rebuilt after the derived fixpoint and cannot be consumed by a rule \
                           in the same tick (queries can). Materialize '{}' into a base relation \
                           first, or query it directly.", r.head.rel, a.rel, a.rel);
                }
            }
        }
    }
    Ok(())
}

fn intern_rel(s: &str, id: &mut HashMap<String, u32>, name: &mut Vec<String>) -> u32 {
    if let Some(&i) = id.get(s) { return i; }
    let i = name.len() as u32; id.insert(s.to_string(), i); name.push(s.to_string()); i
}

/// stratum(C) = max over edges C->D of (stratum(D) + 1 if that edge is negative).
/// The condensed graph is a DAG, so this memoized recursion terminates.
fn comp_stratum(c: usize, succ: &[Vec<(u32, u32)>], memo: &mut [u32]) -> u32 {
    if memo[c] != u32::MAX { return memo[c]; }
    let mut s = 0u32;
    for &(d, w) in &succ[c] { s = s.max(comp_stratum(d as usize, succ, memo) + w); }
    memo[c] = s;
    s
}

/// Stratify derived rules: a rule that negates relation R lands in a stratum
/// strictly above every rule defining R, so `!R` reads a finished relation.
/// Returns rule indices grouped by stratum, ascending. Errors if a negation
/// sits inside a recursive cycle (unstratifiable; positive recursion is fine).
fn stratify(rules: &[&Rule]) -> Result<Vec<Vec<usize>>> {
    let mut id: HashMap<String, u32> = HashMap::new();
    let mut name: Vec<String> = Vec::new();
    let mut edges: Vec<(u32, u32, bool)> = Vec::new(); // (head, body, negative)
    for r in rules {
        let h = intern_rel(&r.head.rel, &mut id, &mut name);
        for item in &r.body {
            let (b, neg) = match item {
                BodyItem::Pos(a) => (intern_rel(&a.rel, &mut id, &mut name), false),
                BodyItem::Neg(a) => (intern_rel(&a.rel, &mut id, &mut name), true),
                _ => continue,
            };
            edges.push((h, b, neg));
        }
    }
    let n = name.len();
    let mut adj = vec![Vec::new(); n];
    for &(h, b, _) in &edges { adj[h as usize].push(b); }
    let (comp, ncomp) = scc::tarjan(&adj);

    // negation inside a recursive cycle has no stratified meaning
    for &(h, b, neg) in &edges {
        if neg && comp[h as usize] == comp[b as usize] {
            bail!("unstratifiable: relation '{}' is negated inside a recursive cycle", name[b as usize]);
        }
    }
    // condensed edge weight: 1 if any negative edge crosses these components
    let mut cw: HashMap<(u32, u32), u32> = HashMap::new();
    for &(h, b, neg) in &edges {
        let (cu, cv) = (comp[h as usize], comp[b as usize]);
        if cu != cv {
            let e = cw.entry((cu, cv)).or_insert(0);
            *e = (*e).max(if neg { 1 } else { 0 });
        }
    }
    let mut succ = vec![Vec::new(); ncomp];
    for (&(cu, cv), &w) in &cw { succ[cu as usize].push((cv, w)); }

    let mut memo = vec![u32::MAX; ncomp];
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (ri, r) in rules.iter().enumerate() {
        let c = comp[id[&r.head.rel] as usize] as usize;
        let s = comp_stratum(c, &succ, &mut memo) as usize;
        if s >= groups.len() { groups.resize(s + 1, Vec::new()); }
        groups[s].push(ri);
    }
    Ok(groups)
}

type Bind = HashMap<String, Value>;
/// (path, rev) -> (content hash, mtime secs, size bytes)
type FileMeta = HashMap<(String, String), (String, i64, i64)>;

struct Reconcile { changed: bool, extracted: usize, retracted: usize, parsed: usize, total: usize }

/// In-memory condensation for a closure edge relation, held for the tick's query
/// phase. A src-pinned `reaches` query becomes a seeded BFS (microseconds)
/// instead of materializing the recursive-CTE view's whole component closure.
struct ClosureCache {
    cond: scc::Cond,
    names: Vec<String>,       // node id -> name
    id: HashMap<String, u32>, // name -> node id
}

fn mtime_secs(md: &std::fs::Metadata) -> i64 {
    md.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct Engine {
    conn: Connection,
    rels: Rels,
    root: PathBuf,
    pub dropped: usize,
    rev_cache: HashMap<String, String>,
    rev_index: std::collections::HashSet<(String, String)>,
}

impl Engine {
    pub fn new(conn: Connection, root: PathBuf) -> Self {
        Engine {
            conn, rels: HashMap::new(), root, dropped: 0,
            rev_cache: HashMap::new(),
            rev_index: std::collections::HashSet::new(),
        }
    }

    /// Resolve a declared rev to a stable commit SHA (WORK stays WORK).
    /// Cached per tick so a moving ref is re-resolved each tick.
    fn resolve_rev(&mut self, rev: &str) -> Result<String> {
        if rev == "WORK" { return Ok("WORK".to_string()); }
        if let Some(s) = self.rev_cache.get(rev) { return Ok(s.clone()); }
        let out = Command::new("git").arg("-C").arg(&self.root)
            .args(["rev-parse", rev]).output()?;
        if !out.status.success() { bail!("git rev-parse {rev} failed"); }
        let sha = String::from_utf8_lossy(&out.stdout).trim().to_string();
        self.rev_cache.insert(rev.to_string(), sha.clone());
        Ok(sha)
    }

    pub fn run(&mut self, prog: &Program) -> Result<()> {
        self.tick(prog, false)
    }

    /// One reactive tick: declare, reconcile sources incrementally, rebuild
    /// derived only if a source fact changed, then run queries.
    pub fn tick(&mut self, prog: &Program, quiet: bool) -> Result<()> {
        self.rev_cache.clear();
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i {
            Item::Rule(r) => Some(r), _ => None,
        }).collect();
        let closures = closure_map(&rules);
        self.declare_all(prog, &closures)?;
        self.ensure_meta()?;

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        // derived = neither source nor a closure rule (closures bypass lower_rule).
        let derived_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && r.closure_edge().is_none()).collect();

        // source rels are heads of source rules; they get incremental retraction.
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules {
            if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); }
        }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules {
            if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); }
        }
        let edges: Vec<&str> = dedup_edges(&closures);
        check_stratification(&derived_rules, &closures)?;

        let t_src = std::time::Instant::now();
        let recon = self.reconcile_sources(&source_rules, &source_rels)?;
        let changed = recon.changed;
        let src_ms = t_src.elapsed().as_secs_f64() * 1000.0;

        let t_der = std::time::Instant::now();
        if changed || self.any_derived_empty(&derived_rels)? || self.any_closure_empty(&edges)? {
            self.rebuild_derived(&derived_rules, &derived_rels)?;
            self.rebuild_closures(&edges)?;
        }
        let der_ms = t_der.elapsed().as_secs_f64() * 1000.0;

        if !quiet {
            eprintln!("[tick] files {}/{} parsed, +{} -{} source facts, derived {} | source {:.1}ms, derived {:.1}ms",
                recon.parsed, recon.total, recon.extracted, recon.retracted,
                if changed { "rebuilt" } else { "unchanged" }, src_ms, der_ms);
        }
        let cond_cache = self.build_cond_cache(&edges)?;
        for item in &prog.items {
            if let Item::Query(q) = item { self.run_query(q, &closures, &cond_cache)?; }
        }
        if self.dropped > 0 {
            eprintln!("[checked-type] dropped {} rows failing file/dir/path checks", self.dropped);
            self.dropped = 0;
        }
        Ok(())
    }

    /// Reactive tick driven by a known set of changed paths (from the file
    /// watcher): reconciles only those paths, never walking or statting the
    /// tree. Only WORK source rules participate; route git-rev changes to `tick`.
    pub fn tick_paths(&mut self, prog: &Program, changed: &[PathBuf], quiet: bool) -> Result<()> {
        self.rev_cache.clear();
        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i { Item::Rule(r) => Some(r), _ => None }).collect();
        let closures = closure_map(&rules);
        self.declare_all(prog, &closures)?;
        self.ensure_meta()?;

        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        let derived_rules: Vec<&Rule> = rules.iter().copied()
            .filter(|r| !r.is_source() && r.closure_edge().is_none()).collect();
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules { if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); } }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules { if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); } }
        let edges: Vec<&str> = dedup_edges(&closures);
        check_stratification(&derived_rules, &closures)?;

        // WORK source rules with compiled glob matchers
        let mut work_rules: Vec<(&Rule, globset::GlobMatcher)> = Vec::new();
        for r in &source_rules {
            let (declared, glob, _, _) = scan_spec(r)?;
            if declared == "WORK" { work_rules.push((*r, globset::Glob::new(&glob)?.compile_matcher())); }
        }

        let prev = self.load_file_meta()?;
        let mut changed_facts = false;
        let (mut extracted, mut retracted, mut npaths) = (0usize, 0usize, 0usize);
        let mut seen: HashSet<String> = HashSet::new();

        for p in changed {
            let rel = match p.strip_prefix(&self.root) { Ok(r) => r.to_string_lossy().replace('\\', "/"), Err(_) => continue };
            if !seen.insert(rel.clone()) { continue; }
            let matching: Vec<&Rule> = work_rules.iter().filter(|(_, m)| m.is_match(&rel)).map(|(r, _)| *r).collect();
            if matching.is_empty() { continue; }
            npaths += 1;
            let abs = self.root.join(&rel);
            if abs.is_file() {
                let bytes = std::fs::read(&abs).unwrap_or_default();
                let h = blake3::hash(&bytes).to_hex().to_string();
                if prev.get(&(rel.clone(), "WORK".to_string())).map(|t| &t.0) == Some(&h) { continue; }
                retracted += self.retract_path(&rel, &source_rels)?;
                for rule in &matching {
                    let (rows, dropped) = parse_file(rule, &rel, "WORK", &self.root, &self.rels, &self.rev_index)?;
                    self.dropped += dropped;
                    let meta = self.rels.get(&rule.head.rel)
                        .ok_or_else(|| anyhow::anyhow!("unknown relation {}", rule.head.rel))?.clone();
                    extracted += self.insert_source_rows(&rule.head.rel, &meta, &rel, &rows)?;
                }
                let (mt, sz) = std::fs::metadata(&abs).ok().map(|m| (mtime_secs(&m), m.len() as i64)).unwrap_or((0, 0));
                self.conn.execute(
                    "INSERT INTO _file(path, rev, hash, mtime, size) VALUES (?1, 'WORK', ?2, ?3, ?4)
                     ON CONFLICT(path, rev) DO UPDATE SET hash=excluded.hash, mtime=excluded.mtime, size=excluded.size",
                    rusqlite::params![rel, h, mt, sz])?;
                changed_facts = true;
            } else {
                retracted += self.retract_path(&rel, &source_rels)?;
                self.conn.execute("DELETE FROM _file WHERE path = ?1 AND rev = 'WORK'", [&rel])?;
                changed_facts = true;
            }
        }

        if changed_facts || self.any_derived_empty(&derived_rels)? || self.any_closure_empty(&edges)? {
            self.rebuild_derived(&derived_rules, &derived_rels)?;
            self.rebuild_closures(&edges)?;
        }

        if !quiet {
            eprintln!("[tick] {npaths} path(s) changed, +{extracted} -{retracted} source facts, derived {}",
                if changed_facts { "rebuilt" } else { "unchanged" });
        }
        let cond_cache = self.build_cond_cache(&edges)?;
        for item in &prog.items { if let Item::Query(q) = item { self.run_query(q, &closures, &cond_cache)?; } }
        if self.dropped > 0 { eprintln!("[checked-type] dropped {} rows", self.dropped); self.dropped = 0; }
        Ok(())
    }

    fn ensure_meta(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _file (path TEXT, rev TEXT, hash TEXT,
                 mtime INTEGER DEFAULT 0, size INTEGER DEFAULT 0, PRIMARY KEY (path, rev));
             CREATE TABLE IF NOT EXISTS _prov (rel TEXT, path TEXT, src TEXT, PRIMARY KEY (rel, path, src));"
        )?;
        // tolerate dbs created before mtime/size existed
        let _ = self.conn.execute("ALTER TABLE _file ADD COLUMN mtime INTEGER DEFAULT 0", []);
        let _ = self.conn.execute("ALTER TABLE _file ADD COLUMN size INTEGER DEFAULT 0", []);
        Ok(())
    }

    fn any_derived_empty(&self, derived_rels: &[String]) -> Result<bool> {
        for rel in derived_rels {
            let n: i64 = self.conn.query_row(&format!("SELECT COUNT(*) FROM {}", tbl(rel)), [], |r| r.get(0))?;
            if n == 0 { return Ok(true); }
        }
        Ok(false)
    }

    fn reconcile_sources(&mut self, source_rules: &[&Rule], source_rels: &[String]) -> Result<Reconcile> {
        // Load prior file metadata first so enumerate can use the mtime fast-path.
        let prev = self.load_file_meta()?;

        let mut current: FileMeta = HashMap::new();
        let mut rule_files: Vec<(usize, String, String, String)> = Vec::new();
        for (idx, rule) in source_rules.iter().enumerate() {
            let (declared, glob, _, _) = scan_spec(rule)?;
            let rev = self.resolve_rev(&declared)?;
            for (path, h, mt, sz) in self.enumerate_with_hash(&rev, &glob, &prev)? {
                current.insert((path.clone(), rev.clone()), (h.clone(), mt, sz));
                rule_files.push((idx, path, rev.clone(), h));
            }
        }
        self.rev_index = current.keys().map(|(p, r)| (r.clone(), p.clone())).collect();

        let hash_of = |m: &FileMeta, p: &str, r: &str| m.get(&(p.to_string(), r.to_string())).map(|t| t.0.clone());

        let mut to_retract: HashSet<String> = HashSet::new();
        for ((path, rev), (h, _, _)) in &current {
            if hash_of(&prev, path, rev).as_ref() != Some(h) { to_retract.insert(path.clone()); }
        }
        for key in prev.keys() {
            if !current.contains_key(key) { to_retract.insert(key.0.clone()); }
        }

        let retract_list: Vec<&str> = to_retract.iter().map(|s| s.as_str()).collect();
        let retracted = self.retract_paths(&retract_list, source_rels)?;

        let to_extract: Vec<(usize, String, String)> = rule_files.iter()
            .filter(|(_, p, r, h)| hash_of(&prev, p, r).as_ref() != Some(h))
            .map(|(idx, p, r, _)| (*idx, p.clone(), r.clone()))
            .collect();
        let parsed = to_extract.len();

        // Parse + extract in parallel across files (CPU-bound, no DB touch),
        // then insert serially (SQLite is single-writer).
        let results: Vec<Result<(usize, String, Vec<Vec<Value>>, usize)>> = {
            let Engine { rels, rev_index, root, .. } = &*self;
            to_extract.par_iter().map(|(idx, path, rev)| {
                let (rows, dropped) = parse_file(source_rules[*idx], path, rev, root, rels, rev_index)?;
                Ok((*idx, path.clone(), rows, dropped))
            }).collect()
        };

        let mut extracted = 0usize;
        for res in results {
            let (idx, path, rows, dropped) = res?;
            self.dropped += dropped;
            let rel = source_rules[idx].head.rel.clone();
            let meta = self.rels.get(&rel)
                .ok_or_else(|| anyhow::anyhow!("unknown head relation {}", rel))?.clone();
            extracted += self.insert_source_rows(&rel, &meta, &path, &rows)?;
        }

        self.save_file_meta(&current, &prev)?;
        Ok(Reconcile {
            changed: retracted > 0 || extracted > 0,
            extracted,
            retracted,
            parsed,
            total: rule_files.len(),
        })
    }

    fn retract_path(&self, path: &str, source_rels: &[String]) -> Result<usize> {
        self.retract_paths(&[path], source_rels)
    }

    /// Retract every row sourced only from these paths. Prune `_prov` for all
    /// paths first, then run the orphan sweep once per relation (not once per
    /// path): a row survives iff some remaining path still provides its `__src`.
    /// Turns the old O(paths x rels x table) into O(rels x table).
    fn retract_paths(&self, paths: &[&str], source_rels: &[String]) -> Result<usize> {
        if paths.is_empty() { return Ok(0); }
        for path in paths {
            self.conn.execute("DELETE FROM _prov WHERE path = ?1", [path])?;
        }
        let mut removed = 0usize;
        for rel in source_rels {
            let sql = format!(
                "DELETE FROM {} WHERE __src NOT IN (SELECT src FROM _prov WHERE rel = ?1)",
                tbl(rel)
            );
            removed += self.conn.execute(&sql, [rel])?;
        }
        Ok(removed)
    }

    fn load_file_meta(&self) -> Result<FileMeta> {
        let mut stmt = self.conn.prepare("SELECT path, rev, hash, mtime, size FROM _file")?;
        let rows = stmt.query_map([], |r| Ok((
            (r.get::<_, String>(0)?, r.get::<_, String>(1)?),
            (r.get::<_, String>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?),
        )))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    fn save_file_meta(&self, current: &FileMeta, prev: &FileMeta) -> Result<()> {
        for ((path, rev), (h, mt, sz)) in current {
            self.conn.execute(
                "INSERT INTO _file(path, rev, hash, mtime, size) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(path, rev) DO UPDATE SET hash = excluded.hash, mtime = excluded.mtime, size = excluded.size",
                rusqlite::params![path, rev, h, mt, sz],
            )?;
        }
        for (path, rev) in prev.keys() {
            if !current.contains_key(&(path.clone(), rev.clone())) {
                self.conn.execute("DELETE FROM _file WHERE path = ?1 AND rev = ?2", rusqlite::params![path, rev])?;
            }
        }
        Ok(())
    }

    fn declare(&mut self, d: &RelDecl) -> Result<()> {
        let cols: Vec<String> = d.cols.iter()
            .map(|c| format!("\"{}\" {}", c.name, c.ty.sql())).collect();
        let pk: Vec<String> = d.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {} ({}, __src TEXT DEFAULT '', PRIMARY KEY ({}))",
            tbl(&d.name), cols.join(", "), pk.join(", ")
        );
        self.conn.execute(&sql, [])?;
        self.rels.insert(d.name.clone(), RelMeta { cols: d.cols.clone() });
        Ok(())
    }

    /// Declare every relation: closure heads become a VIEW over the condensation,
    /// everything else a base table.
    fn declare_all(&mut self, prog: &Program, closures: &HashMap<String, String>) -> Result<()> {
        for item in &prog.items {
            if let Item::Rel(d) = item {
                match closures.get(&d.name) {
                    Some(edge) => self.declare_closure(d, edge)?,
                    None => self.declare(d)?,
                }
            }
        }
        Ok(())
    }

    /// A closure head `rel_<head>` is a recursive-CTE view over the condensation
    /// tables of its edge relation. The view yields cross-component reach plus
    /// same-cyclic-component pairs (so a node on a cycle reaches itself).
    fn declare_closure(&mut self, d: &RelDecl, edge: &str) -> Result<()> {
        if d.cols.len() != 2 { bail!("closure head {} must have 2 columns", d.name); }
        self.rels.insert(d.name.clone(), RelMeta { cols: d.cols.clone() });
        let (nt, et, v) = (scc_node_tbl(edge), scc_edge_tbl(edge), tbl(&d.name));
        self.conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {nt} (name TEXT PRIMARY KEY, comp INTEGER, cyclic INTEGER);
             CREATE TABLE IF NOT EXISTS {et} (comp_src INTEGER, comp_dst INTEGER, PRIMARY KEY(comp_src, comp_dst));"
        ))?;
        // a prior run may have left rel_<head> as a view or a real table; clear both.
        self.conn.execute(&format!("DROP VIEW IF EXISTS {v}"), [])?;
        self.conn.execute(&format!("DROP TABLE IF EXISTS {v}"), [])?;
        let (c0, c1) = (&d.cols[0].name, &d.cols[1].name);
        self.conn.execute_batch(&format!(
            "CREATE VIEW {v} AS
             WITH RECURSIVE cr(a, b) AS (
               SELECT comp_src, comp_dst FROM {et}
               UNION
               SELECT cr.a, e.comp_dst FROM cr JOIN {et} e ON e.comp_src = cr.b
             )
             SELECT na.name AS \"{c0}\", nb.name AS \"{c1}\"
               FROM cr JOIN {nt} na ON na.comp = cr.a JOIN {nt} nb ON nb.comp = cr.b
             UNION
             SELECT na.name AS \"{c0}\", nb.name AS \"{c1}\"
               FROM {nt} na JOIN {nt} nb ON na.comp = nb.comp AND na.cyclic = 1;"
        ))?;
        Ok(())
    }

    /// Wipe derived tables and run the semi-naive fixpoint to convergence.
    fn rebuild_derived(&self, derived_rules: &[&Rule], derived_rels: &[String]) -> Result<()> {
        for rel in derived_rels { self.conn.execute(&format!("DELETE FROM {}", tbl(rel)), [])?; }
        // Evaluate stratum by stratum: each runs a positive (monotone) semi-naive
        // fixpoint to convergence, so a higher stratum's negation reads relations
        // that lower strata have already finished.
        for group in stratify(derived_rules)? {
            let mut iters = 0;
            loop {
                let mut delta = 0usize;
                for &ri in &group { delta += self.conn.execute(&lower_rule(derived_rules[ri], &self.rels)?, [])?; }
                iters += 1;
                if delta == 0 { break; }
                if iters > 100_000 { bail!("fixpoint did not converge"); }
            }
        }
        Ok(())
    }

    fn any_closure_empty(&self, edges: &[&str]) -> Result<bool> {
        for edge in edges {
            let n: i64 = self.conn.query_row(
                &format!("SELECT COUNT(*) FROM {}", scc_node_tbl(edge)), [], |r| r.get(0))?;
            if n == 0 { return Ok(true); }
        }
        Ok(false)
    }

    /// Load a 2-col edge relation, intern node names to dense u32 (transient),
    /// return adjacency + id->name. No persistent interning (see plan).
    fn load_edges(&self, edge: &str, c0: &str, c1: &str) -> Result<(Vec<Vec<u32>>, Vec<String>)> {
        let sql = format!("SELECT \"{c0}\", \"{c1}\" FROM {}", tbl(edge));
        let mut stmt = self.conn.prepare(&sql)?;
        let mut intern: HashMap<String, u32> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows.flatten() {
            let mut id = |s: String| -> u32 {
                if let Some(&i) = intern.get(&s) { return i; }
                let i = names.len() as u32; intern.insert(s.clone(), i); names.push(s); i
            };
            let a = id(row.0); let b = id(row.1);
            pairs.push((a, b));
        }
        let mut adj = vec![Vec::new(); names.len()];
        for (a, b) in pairs { adj[a as usize].push(b); }
        Ok((adj, names))
    }

    /// For each edge relation: condense, then replace its scc_node/scc_edge tables.
    /// The closure VIEW reads these; the Theta(V^2) pair table is never built.
    fn rebuild_closures(&self, edges: &[&str]) -> Result<()> {
        for edge in edges {
            let meta = self.rels.get(*edge)
                .ok_or_else(|| anyhow::anyhow!("closure edge relation {edge} not declared"))?;
            if meta.cols.len() != 2 { bail!("closure edge {edge} must have 2 columns"); }
            let (c0, c1) = (meta.cols[0].name.clone(), meta.cols[1].name.clone());
            let (adj, names) = self.load_edges(edge, &c0, &c1)?;
            let cond = scc::build_condensed(&adj);
            let (nt, et) = (scc_node_tbl(edge), scc_edge_tbl(edge));
            let tx = self.conn.unchecked_transaction()?;
            tx.execute(&format!("DELETE FROM {nt}"), [])?;
            tx.execute(&format!("DELETE FROM {et}"), [])?;
            {
                let mut ins = tx.prepare(&format!("INSERT INTO {nt}(name, comp, cyclic) VALUES (?1, ?2, ?3)"))?;
                for (id, name) in names.iter().enumerate() {
                    let comp = cond.comp[id] as i64;
                    let cyc = cond.cyclic[cond.comp[id] as usize] as i64;
                    ins.execute(rusqlite::params![name, comp, cyc])?;
                }
                let mut ins_e = tx.prepare(&format!("INSERT OR IGNORE INTO {et}(comp_src, comp_dst) VALUES (?1, ?2)"))?;
                for (cu, succ) in cond.cadj.iter().enumerate() {
                    for &cw in succ { ins_e.execute(rusqlite::params![cu as i64, cw as i64])?; }
                }
            }
            tx.commit()?;
        }
        Ok(())
    }

    fn insert_source_rows(&self, rel: &str, meta: &RelMeta, path: &str, rows: &[Vec<Value>]) -> Result<usize> {
        if rows.is_empty() { return Ok(0); }
        let n = meta.cols.len();
        let cols: Vec<String> = meta.cols.iter().map(|c| format!("\"{}\"", c.name)).collect();
        let ph: Vec<String> = (1..=n).map(|i| format!("?{i}")).collect();
        let sql = format!("INSERT OR IGNORE INTO {} ({}, __src) VALUES ({}, ?{})",
            tbl(rel), cols.join(", "), ph.join(", "), n + 1);
        let mut stmt = self.conn.prepare(&sql)?;
        let mut prov = self.conn.prepare("INSERT OR IGNORE INTO _prov(rel, path, src) VALUES (?1, ?2, ?3)")?;
        let mut inserted = 0usize;
        for row in rows {
            let src = row_hash(row);
            let mut params: Vec<rusqlite::types::Value> = row.iter().map(|v| match v {
                Value::Text(s) => rusqlite::types::Value::Text(s.clone()),
                Value::Int(k) => rusqlite::types::Value::Integer(*k),
            }).collect();
            params.push(rusqlite::types::Value::Text(src.clone()));
            inserted += stmt.execute(rusqlite::params_from_iter(params))?;
            prov.execute(rusqlite::params![rel, path, src])?;
        }
        Ok(inserted)
    }

    /// Enumerate (path, hash, mtime, size) for a rev. For WORK, stat each file
    /// and reuse the stored hash when mtime+size are unchanged (the fast-path),
    /// reading+hashing only changed files. A git rev uses the blob OID from
    /// `ls-tree`, so unchanged blobs are detected without fetching content.
    fn enumerate_with_hash(&self, rev: &str, glob: &str, prev: &FileMeta) -> Result<Vec<(String, String, i64, i64)>> {
        let matcher = globset::Glob::new(glob)?.compile_matcher();
        if rev == "WORK" {
            let mut files: Vec<(PathBuf, String, i64, i64)> = Vec::new();
            for entry in ignore::WalkBuilder::new(&self.root).hidden(false).build().flatten() {
                if !entry.path().is_file() { continue; }
                let rel = match entry.path().strip_prefix(&self.root) { Ok(r) => r, Err(_) => continue };
                let rel = rel.to_string_lossy().replace('\\', "/");
                if !matcher.is_match(&rel) { continue; }
                let (mt, sz) = entry.metadata().ok().map(|m| (mtime_secs(&m), m.len() as i64)).unwrap_or((0, 0));
                files.push((entry.path().to_path_buf(), rel, mt, sz));
            }
            // reuse stored hash when mtime+size match; otherwise read+hash (parallel)
            let mut out: Vec<(String, String, i64, i64)> = files.par_iter().map(|(abs, rel, mt, sz)| {
                if let Some((h, pmt, psz)) = prev.get(&(rel.clone(), "WORK".to_string())) {
                    if pmt == mt && psz == sz {
                        return (rel.clone(), h.clone(), *mt, *sz);
                    }
                }
                let bytes = std::fs::read(abs).unwrap_or_default();
                (rel.clone(), blake3::hash(&bytes).to_hex().to_string(), *mt, *sz)
            }).collect();
            out.sort();
            Ok(out)
        } else {
            // `git ls-tree -r <rev>` lines: "<mode> <type> <oid>\t<path>"
            let output = Command::new("git")
                .arg("-C").arg(&self.root)
                .args(["ls-tree", "-r", rev])
                .output()?;
            if !output.status.success() { return Ok(Vec::new()); }
            let text = String::from_utf8_lossy(&output.stdout);
            let mut out = Vec::new();
            for line in text.lines() {
                let Some((meta, path)) = line.split_once('\t') else { continue };
                let parts: Vec<&str> = meta.split_whitespace().collect();
                if parts.get(1) != Some(&"blob") { continue; }
                let oid = parts.get(2).copied().unwrap_or_default();
                if matcher.is_match(path) { out.push((path.to_string(), oid.to_string(), 0, 0)); }
            }
            Ok(out)
        }
    }

    /// Build the in-memory condensation for each closure edge relation, for the
    /// query phase. One build per edge per tick (a few ms even on a large repo).
    fn build_cond_cache(&self, edges: &[&str]) -> Result<HashMap<String, ClosureCache>> {
        let mut m = HashMap::new();
        for edge in edges {
            let meta = self.rels.get(*edge)
                .ok_or_else(|| anyhow::anyhow!("closure edge relation {edge} not declared"))?;
            if meta.cols.len() != 2 { continue; }
            let (c0, c1) = (meta.cols[0].name.clone(), meta.cols[1].name.clone());
            let (adj, names) = self.load_edges(edge, &c0, &c1)?;
            let cond = scc::build_condensed(&adj);
            let id = names.iter().enumerate().map(|(i, n)| (n.clone(), i as u32)).collect();
            m.insert(edge.to_string(), ClosureCache { cond, names, id });
        }
        Ok(m)
    }

    /// Answer `reaches(src=SEED, dst=?)` as a seeded BFS over the condensation.
    /// Same row set as the view's src-pinned slice, computed in microseconds.
    fn run_reaches_point(&self, q: &Query, cc: &ClosureCache, seed: &str) -> Result<()> {
        let meta = self.rels.get(&q.head.rel).unwrap();
        let header = |pos: usize| match &q.head.terms[pos] {
            Term::Var(v) => v.clone(),
            _ => meta.col_name(pos).to_string(),
        };
        println!("? {} => {}\t{}", q.head.rel, header(0), header(1));
        let mut n = 0;
        if let Some(&sid) = cc.id.get(seed) {
            let mut hits: Vec<&str> = scc::reaches_from(&cc.cond, sid)
                .iter().map(|&i| cc.names[i as usize].as_str()).collect();
            hits.sort_unstable();
            for h in hits { println!("  {seed}\t{h}"); n += 1; }
        }
        println!("  ({n} rows)\n");
        Ok(())
    }

    fn run_query(&self, q: &Query, closures: &HashMap<String, String>,
                 cache: &HashMap<String, ClosureCache>) -> Result<()> {
        // Seeded Rust path: a closure head with src pinned and dst free is a
        // forward reachability walk. Anything else (dst-pinned reverse, both
        // pinned, both free) falls through to the SQL view.
        if let Some(edge) = closures.get(&q.head.rel) {
            if q.head.terms.len() == 2 && matches!(q.head.terms[1], Term::Var(_)) {
                if let (Some(seed), None) = (pinned_value(q, 0), pinned_value(q, 1)) {
                    if let Some(cc) = cache.get(edge) {
                        return self.run_reaches_point(q, cc, &seed);
                    }
                }
            }
        }
        let (sql, headers) = lower_query(q, &self.rels)?;
        let mut stmt = self.conn.prepare(&sql)?;
        let ncols = stmt.column_count();
        let mut rows = stmt.query([])?;
        println!("? {} => {}", q.head.rel, if headers.is_empty() { "(count)".into() } else { headers.join("\t") });
        let mut n = 0;
        while let Some(row) = rows.next()? {
            let cells: Vec<String> = (0..ncols).map(|i| {
                match row.get::<_, rusqlite::types::Value>(i).unwrap_or(rusqlite::types::Value::Null) {
                    rusqlite::types::Value::Text(s) => s,
                    rusqlite::types::Value::Integer(n) => n.to_string(),
                    rusqlite::types::Value::Real(f) => f.to_string(),
                    _ => String::new(),
                }
            }).collect();
            println!("  {}", cells.join("\t"));
            n += 1;
        }
        println!("  ({n} rows)\n");
        Ok(())
    }
}

fn scan_spec(rule: &Rule) -> Result<(String, String, String, String)> {
    for item in &rule.body {
        if let BodyItem::Scan { rev, glob, path, rev_out } = item {
            return Ok((str_of(rev)?, str_of(glob)?, var_of(path)?, var_of(rev_out)?));
        }
    }
    bail!("source rule {} missing scan", rule.head.rel)
}

fn read_content(root: &Path, rev: &str, path: &str) -> Result<String> {
    if rev == "WORK" {
        Ok(std::fs::read_to_string(root.join(path))?)
    } else {
        let output = Command::new("git")
            .arg("-C").arg(root)
            .args(["show", &format!("{rev}:{path}")])
            .output()?;
        if !output.status.success() { bail!("git show failed for {rev}:{path}"); }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

fn check_type(ty: Type, v: &Value, rev: &str, root: &Path, rev_index: &HashSet<(String, String)>) -> bool {
    let p = match v { Value::Text(s) => s, Value::Int(_) => return ty == Type::Int || ty == Type::Text };
    if rev != "WORK" {
        return match ty {
            Type::File | Type::Path => rev_index.contains(&(rev.to_string(), p.clone())),
            Type::Dir => rev_index.iter().any(|(r, pp)| r == rev && pp.starts_with(&format!("{p}/"))),
            Type::Text | Type::Int => true,
        };
    }
    let full = root.join(p);
    match ty {
        Type::File => full.is_file(),
        Type::Dir => full.is_dir(),
        Type::Path => full.exists(),
        Type::Text | Type::Int => true,
    }
}

/// Literal identifier tokens a pattern requires (metavars stripped). Used as a
/// cheap prefilter: skip parsing a file that cannot contain a match.
fn pattern_literals(pat: &str) -> Vec<String> {
    static META: OnceLock<Regex> = OnceLock::new();
    static IDENT: OnceLock<Regex> = OnceLock::new();
    let meta = META.get_or_init(|| Regex::new(r"\$+[A-Za-z0-9_]*").unwrap());
    let ident = IDENT.get_or_init(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").unwrap());
    let stripped = meta.replace_all(pat, " ");
    let mut out = Vec::new();
    for m in ident.find_iter(&stripped) {
        let s = m.as_str().to_string();
        if !out.contains(&s) { out.push(s); }
    }
    out
}

/// Parse one file for one source rule (no DB access); returns (rows, dropped).
/// Safe to call in parallel: reads file content, runs extractors, builds rows.
fn parse_file(
    rule: &Rule, path: &str, rev: &str,
    root: &Path, rels: &Rels, rev_index: &HashSet<(String, String)>,
) -> Result<(Vec<Vec<Value>>, usize)> {
    let (_, _, pathvar, revvar) = scan_spec(rule)?;
    let cmps: Vec<&Constraint> = rule.body.iter()
        .filter_map(|i| if let BodyItem::Cmp(c) = i { Some(c) } else { None }).collect();
    let content = read_content(root, rev, path).unwrap_or_default();
    let head_meta = rels.get(&rule.head.rel)
        .ok_or_else(|| anyhow::anyhow!("unknown head relation {}", rule.head.rel))?;
    let mut re_cache: HashMap<String, Regex> = HashMap::new();

    let mut binds: Vec<Bind> = vec![{
        let mut b = Bind::new();
        b.insert(pathvar.clone(), Value::Text(path.to_string()));
        b.insert(revvar.clone(), Value::Text(rev.to_string()));
        b
    }];

    for item in &rule.body {
        match item {
            BodyItem::Match { regex, line, .. } => {
                let mlv = var_of(line)?;
                if !re_cache.contains_key(regex) { re_cache.insert(regex.clone(), Regex::new(regex)?); }
                let re = &re_cache[regex];
                let names: Vec<&str> = re.capture_names().flatten().collect();
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (lineno, ln) in content.lines().enumerate() {
                        for caps in re.captures_iter(ln) {
                            let mut ext = b.clone();
                            ext.insert(mlv.clone(), Value::Int((lineno + 1) as i64));
                            for n in &names {
                                if let Some(m) = caps.name(n) {
                                    ext.insert((*n).to_string(), Value::Text(m.as_str().to_string()));
                                }
                            }
                            next.push(ext);
                        }
                    }
                }
                binds = next;
            }
            BodyItem::Ast { lang, query, line, end, .. } => {
                let alv = var_of(line)?;
                let elv = end.as_ref().map(var_of).transpose()?;
                let hits = run_ts(&content, lang, query)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (start, endln, caps) in &hits {
                        let mut ext = b.clone();
                        ext.insert(alv.clone(), Value::Int(*start));
                        if let Some(ev) = &elv { ext.insert(ev.clone(), Value::Int(*endln)); }
                        for (n, t) in caps { ext.insert(n.clone(), Value::Text(t.clone())); }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Sg { lang, pattern, line, .. } => {
                let slv = var_of(line)?;
                // prefilter: a file lacking any literal token cannot match
                let lits = pattern_literals(pattern);
                if !lits.iter().all(|t| content.contains(t.as_str())) {
                    binds = Vec::new();
                    continue;
                }
                let hits = crate::sg::run_sg(&content, lang, pattern)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (ln, caps) in &hits {
                        let mut ext = b.clone();
                        ext.insert(slv.clone(), Value::Int(*ln));
                        for (n, t) in caps { ext.insert(n.clone(), Value::Text(t.clone())); }
                        next.push(ext);
                    }
                }
                binds = next;
            }
            BodyItem::Json { jpath, out, .. } => {
                let ov = var_of(out)?;
                let vals = json_extract(&content, jpath);
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for v in &vals {
                        let mut ext = b.clone();
                        ext.insert(ov.clone(), Value::Text(v.clone()));
                        next.push(ext);
                    }
                }
                binds = next;
            }
            _ => {}
        }
    }

    let mut rows: Vec<Vec<Value>> = Vec::new();
    let mut dropped = 0usize;
    'bind: for b in binds {
        for c in &cmps {
            if !eval_cmp(c, &b)? { continue 'bind; }
        }
        let mut row = Vec::with_capacity(head_meta.cols.len());
        for (i, term) in rule.head.terms.iter().enumerate() {
            let v = match term {
                Term::Var(v) => b.get(v).cloned()
                    .ok_or_else(|| anyhow::anyhow!("head var {v} unbound in source rule"))?,
                Term::Str(s) => Value::Text(s.clone()),
                Term::Int(n) => Value::Int(*n),
                Term::Wild => bail!("'_' in head not allowed"),
            };
            if !check_type(head_meta.cols[i].ty, &v, rev, root, rev_index) { dropped += 1; continue 'bind; }
            row.push(v);
        }
        rows.push(row);
    }
    Ok((rows, dropped))
}

fn row_hash(row: &[Value]) -> String {
    let mut s = String::new();
    for (i, v) in row.iter().enumerate() {
        if i > 0 { s.push('\u{1}'); }
        s.push_str(&v.as_str());
    }
    blake3::hash(s.as_bytes()).to_hex().to_string()
}

fn str_of(t: &Term) -> Result<String> {
    match t { Term::Str(s) => Ok(s.clone()), _ => bail!("expected string literal, got {t:?}") }
}
fn var_of(t: &Term) -> Result<String> {
    match t { Term::Var(v) => Ok(v.clone()), _ => bail!("expected variable, got {t:?}") }
}

fn val_of(t: &Term, b: &Bind) -> Result<Value> {
    match t {
        Term::Var(v) => b.get(v).cloned().ok_or_else(|| anyhow::anyhow!("unbound var {v} in constraint")),
        Term::Str(s) => Ok(Value::Text(s.clone())),
        Term::Int(n) => Ok(Value::Int(*n)),
        Term::Wild => bail!("'_' in constraint"),
    }
}

fn eval_cmp(c: &Constraint, b: &Bind) -> Result<bool> {
    let l = val_of(&c.lhs, b)?;
    let r = val_of(&c.rhs, b)?;
    let ord = match (&l, &r) {
        (Value::Int(a), Value::Int(b)) => a.cmp(b),
        _ => l.as_str().cmp(&r.as_str()),
    };
    Ok(match c.op {
        CmpOp::Eq => ord.is_eq(), CmpOp::Ne => ord.is_ne(),
        CmpOp::Lt => ord.is_lt(), CmpOp::Le => ord.is_le(),
        CmpOp::Gt => ord.is_gt(), CmpOp::Ge => ord.is_ge(),
    })
}

fn ts_lang(lang: &str) -> Result<tree_sitter::Language> {
    match lang {
        "rust" | "rs" => Ok(tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)),
        other => bail!("no ast grammar for :{other} (compiled in: rust)"),
    }
}

/// Run a tree-sitter S-expression query over file content.
/// Returns (start_line, end_line, captures) per match; start = min capture start
/// row, end = max capture end row (the matched region's span). Captures are
/// (capture_name, node_text).
fn run_ts(content: &str, lang: &str, query_str: &str) -> Result<Vec<(i64, i64, Vec<(String, String)>)>> {
    use streaming_iterator::StreamingIterator;
    let language = ts_lang(lang)?;
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language)?;
    let tree = parser.parse(content, None).ok_or_else(|| anyhow::anyhow!("ast parse failed"))?;
    let query = tree_sitter::Query::new(&language, query_str)?;
    let names = query.capture_names();
    let src = content.as_bytes();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut out = Vec::new();
    let mut it = cursor.matches(&query, tree.root_node(), src);
    while let Some(m) = it.next() {
        let mut caps = Vec::new();
        let mut line = i64::MAX;
        let mut end = 1i64;
        for c in m.captures {
            let name = names[c.index as usize].to_string();
            let text = c.node.utf8_text(src).unwrap_or("").to_string();
            line = line.min(c.node.start_position().row as i64 + 1);
            end = end.max(c.node.end_position().row as i64 + 1);
            caps.push((name, text));
        }
        if line == i64::MAX { line = 1; }
        out.push((line, end, caps));
    }
    Ok(out)
}

/// Extract leaf values along a dotted path; `*` matches any object key or array index.
fn json_extract(content: &str, jpath: &str) -> Vec<String> {
    let root: serde_json::Value = match serde_json::from_str(content) { Ok(v) => v, Err(_) => return vec![] };
    let mut cur: Vec<&serde_json::Value> = vec![&root];
    for seg in jpath.split('.') {
        let mut next: Vec<&serde_json::Value> = Vec::new();
        for node in cur {
            if seg == "*" {
                match node {
                    serde_json::Value::Object(m) => next.extend(m.values()),
                    serde_json::Value::Array(a) => next.extend(a.iter()),
                    _ => {}
                }
            } else if let serde_json::Value::Object(m) = node {
                if let Some(v) = m.get(seg) { next.push(v); }
            }
        }
        cur = next;
    }
    cur.iter().map(|v| match v {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }).collect()
}
