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

type Bind = HashMap<String, Value>;
/// (path, rev) -> (content hash, mtime secs, size bytes)
type FileMeta = HashMap<(String, String), (String, i64, i64)>;

struct Reconcile { changed: bool, extracted: usize, retracted: usize, parsed: usize, total: usize }

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
        for item in &prog.items {
            if let Item::Rel(d) = item { self.declare(d)?; }
        }
        self.ensure_meta()?;

        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i {
            Item::Rule(r) => Some(r), _ => None,
        }).collect();
        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        let derived_rules: Vec<&Rule> = rules.iter().copied().filter(|r| !r.is_source()).collect();

        // source rels are heads of source rules; they get incremental retraction.
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules {
            if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); }
        }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules {
            if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); }
        }

        let t_src = std::time::Instant::now();
        let recon = self.reconcile_sources(&source_rules, &source_rels)?;
        let changed = recon.changed;
        let src_ms = t_src.elapsed().as_secs_f64() * 1000.0;

        let t_der = std::time::Instant::now();
        if changed || self.any_derived_empty(&derived_rels)? {
            for rel in &derived_rels {
                self.conn.execute(&format!("DELETE FROM {}", tbl(rel)), [])?;
            }
            let mut iters = 0;
            loop {
                let mut delta = 0usize;
                for r in &derived_rules {
                    let sql = lower_rule(r, &self.rels)?;
                    delta += self.conn.execute(&sql, [])?;
                }
                iters += 1;
                if delta == 0 { break; }
                if iters > 100_000 { bail!("fixpoint did not converge"); }
            }
        }
        let der_ms = t_der.elapsed().as_secs_f64() * 1000.0;

        if !quiet {
            eprintln!("[tick] files {}/{} parsed, +{} -{} source facts, derived {} | source {:.1}ms, derived {:.1}ms",
                recon.parsed, recon.total, recon.extracted, recon.retracted,
                if changed { "rebuilt" } else { "unchanged" }, src_ms, der_ms);
        }
        for item in &prog.items {
            if let Item::Query(q) = item { self.run_query(q)?; }
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
        for item in &prog.items { if let Item::Rel(d) = item { self.declare(d)?; } }
        self.ensure_meta()?;

        let rules: Vec<&Rule> = prog.items.iter().filter_map(|i| match i { Item::Rule(r) => Some(r), _ => None }).collect();
        let source_rules: Vec<&Rule> = rules.iter().copied().filter(|r| r.is_source()).collect();
        let derived_rules: Vec<&Rule> = rules.iter().copied().filter(|r| !r.is_source()).collect();
        let mut source_rels: Vec<String> = Vec::new();
        for r in &source_rules { if !source_rels.contains(&r.head.rel) { source_rels.push(r.head.rel.clone()); } }
        let mut derived_rels: Vec<String> = Vec::new();
        for r in &derived_rules { if !derived_rels.contains(&r.head.rel) { derived_rels.push(r.head.rel.clone()); } }

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

        if changed_facts || self.any_derived_empty(&derived_rels)? {
            for rel in &derived_rels { self.conn.execute(&format!("DELETE FROM {}", tbl(rel)), [])?; }
            let mut iters = 0;
            loop {
                let mut delta = 0usize;
                for r in &derived_rules { delta += self.conn.execute(&lower_rule(r, &self.rels)?, [])?; }
                iters += 1;
                if delta == 0 { break; }
                if iters > 100_000 { bail!("fixpoint did not converge"); }
            }
        }

        if !quiet {
            eprintln!("[tick] {npaths} path(s) changed, +{extracted} -{retracted} source facts, derived {}",
                if changed_facts { "rebuilt" } else { "unchanged" });
        }
        for item in &prog.items { if let Item::Query(q) = item { self.run_query(q)?; } }
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

        let mut retracted = 0usize;
        for path in &to_retract { retracted += self.retract_path(path, source_rels)?; }

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
        self.conn.execute("DELETE FROM _prov WHERE path = ?1", [path])?;
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

    fn run_query(&self, q: &Query) -> Result<()> {
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
            BodyItem::Ast { lang, query, line, .. } => {
                let alv = var_of(line)?;
                let hits = run_ts(&content, lang, query)?;
                let mut next: Vec<Bind> = Vec::new();
                for b in &binds {
                    for (ln, caps) in &hits {
                        let mut ext = b.clone();
                        ext.insert(alv.clone(), Value::Int(*ln));
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
/// Returns (line, captures) per match; captures are (capture_name, node_text).
fn run_ts(content: &str, lang: &str, query_str: &str) -> Result<Vec<(i64, Vec<(String, String)>)>> {
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
        for c in m.captures {
            let name = names[c.index as usize].to_string();
            let text = c.node.utf8_text(src).unwrap_or("").to_string();
            line = line.min(c.node.start_position().row as i64 + 1);
            caps.push((name, text));
        }
        if line == i64::MAX { line = 1; }
        out.push((line, caps));
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
