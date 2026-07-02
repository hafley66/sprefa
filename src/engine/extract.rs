//! Built-in graph/CST/spine/daemon extractor methods on `Engine`, lifted out of
//! `engine/mod.rs` to shrink the file an AI re-reads each session. These are the
//! "bucket E" families of the breakdown proposal — `module_*`, `type_*`,
//! `call_*`, `dataflow`/`df_*`, `doc_*`, `node`/`child`, the `string`/`ref`
//! spine projection, and the `daemon`/`effect` read-back projections. Pure
//! relocation: the bodies are unchanged and `tick`/`tick_paths` still call them
//! as `self.refresh_*`. As a child module of `engine`, this file reaches
//! `Engine`'s private fields/helpers/types directly; the lifted methods are
//! `pub(crate)` only so the parent's tick orchestrator can call them.
//!
//! Bucket-E full RelKind migration (uniform registry dispatch, `refresh_delta`)
//! stays deferred — the module family's full-vs-delta classification is welded
//! into the per-file change loop in `tick_paths`. See
//! `plans/2026-06-30-engine-breakdown-proposal.md`.

use anyhow::Result;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::lower::tbl;
use crate::modgraph::{self, ProjectCx, Resolution};
use crate::spine;
use crate::typegraph;

/// One extraction-corpus row: (repo, path, rev, content hash) from `_file`.
type ExtractFile = (String, String, String, String);

/// Per-file fact cache for one extractor family (perf gap A): (repo, path,
/// content hash) -> (derived repo id, extracted facts). See the field docs on
/// `Engine`.
pub(super) type FactCache<T> =
    std::cell::RefCell<HashMap<(String, String, String), (String, Arc<T>)>>;

/// Identity of the running binary, folded into every `extract:*` input digest
/// so a rebuilt `dl` re-extracts even over an unchanged corpus (the extractor
/// logic may have changed). (len, mtime) of the current executable; a stat
/// failure yields a fixed stamp (the digest then keys on inputs alone).
fn exe_stamp() -> u128 {
    static STAMP: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    *STAMP.get_or_init(|| {
        std::env::current_exe().ok()
            .and_then(|p| std::fs::metadata(&p).ok())
            .map(|m| {
                let mt = m.modified().ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos()).unwrap_or(0);
                (m.len() as u128) << 64 | (mt & u128::from(u64::MAX))
            })
            .unwrap_or(0)
    })
}

/// Split `files` into cache hits and misses (keyed by (repo, path, content
/// hash)), run `parse` over the misses in parallel, then replace the cache
/// with exactly the current file set's entries. Returns one
/// (repo id, path, rev, facts) tuple per input row; order is not preserved.
/// A row with an empty content hash is never cached (no identity to key on).
/// `parsed_counter` accumulates the miss count (the `extract_files_parsed`
/// instrumentation).
fn cached_facts<T: Send + Sync>(
    cache: &FactCache<T>,
    files: &[ExtractFile],
    parsed_counter: &std::cell::Cell<usize>,
    parse: impl Fn(&str, &str, &str) -> Option<(String, T)> + Sync,
) -> Vec<(String, String, String, Arc<T>)> {
    let mut out: Vec<(String, String, String, Arc<T>)> = Vec::with_capacity(files.len());
    let mut next: HashMap<(String, String, String), (String, Arc<T>)> =
        HashMap::with_capacity(files.len());
    let mut misses: Vec<&ExtractFile> = Vec::new();
    {
        let cur = cache.borrow();
        for f in files {
            let hit = if f.3.is_empty() { None } else {
                cur.get(&(f.0.clone(), f.1.clone(), f.3.clone()))
            };
            match hit {
                Some((rid, facts)) => {
                    out.push((rid.clone(), f.1.clone(), f.2.clone(), facts.clone()));
                    next.insert((f.0.clone(), f.1.clone(), f.3.clone()),
                                (rid.clone(), facts.clone()));
                }
                None => misses.push(f),
            }
        }
    }
    let parsed: Vec<(&ExtractFile, String, Arc<T>)> = misses.par_iter().filter_map(|f| {
        let (rid, facts) = parse(&f.0, &f.1, &f.2)?;
        Some((*f, rid, Arc::new(facts)))
    }).collect();
    parsed_counter.set(parsed_counter.get() + parsed.len());
    for (f, rid, facts) in parsed {
        out.push((rid.clone(), f.1.clone(), f.2.clone(), facts.clone()));
        if !f.3.is_empty() {
            next.insert((f.0.clone(), f.1.clone(), f.3.clone()), (rid, facts));
        }
    }
    *cache.borrow_mut() = next;
    out
}

impl Engine {
    /// Project the durable `_strings` / `_where_bytes` meta tables into the
    /// query-facing `string` / `ref` relations. Wholesale wipe + repopulate,
    /// skipping the zero sentinels so queries see only real interned rows.
    ///
    /// `delta = None` executes the wholesale body verbatim (current behavior).
    /// `delta = Some(_)` is the incremental path: only the rows named in the
    /// delta need to be merged into `string` / `ref`. That path is not yet
    /// expanded; callers pass `None` at every existing call site. When the
    /// incremental path is wired, replace the `None` calls in the per-tick
    /// changed-file loop with `Some(&delta)` after `insert_spine_where_bytes`
    /// flushes the staged vecs -- collect-then-flush, one `insert_rows` call
    /// per table, never per-row.
    fn refresh_spine_rels_delta(&self, delta: Option<&SpineDelta>) -> Result<()> {
        // The incremental path is not yet expanded; fall through to wholesale.
        // When Some() is implemented, remove this comment and the wholesale read
        // below for the Some branch.
        let _ = delta; // future Some() will drive a targeted merge instead
        let conn = self.db.conn();
        let mut s = conn.prepare("SELECT id, content, norm FROM _strings WHERE id != '0'")?;
        let strings: Vec<Vec<Value>> = s
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        let mut w = conn.prepare(
            "SELECT id, string_id, file_id, lo, hi FROM _where_bytes WHERE id != '0'")?;
        let refs: Vec<Vec<Value>> = w
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
                Value::Int(r.get::<_, i64>(3)?),
                Value::Int(r.get::<_, i64>(4)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        drop(s);
        drop(w);
        self.refresh_rel("string", &["id", "text", "norm"], &strings)?;
        self.refresh_rel("ref", &["id", "string", "file", "lo", "hi"], &refs)?;
        Ok(())
    }

    /// Thin shim so existing call sites require no signature change.
    pub(crate) fn refresh_spine_rels(&self) -> Result<()> {
        self.refresh_spine_rels_delta(None)
    }

    /// Project the persisted daemon-state meta tables (`_program` / `_ref` /
    /// `_rev_log`) into the `program` / `head` / `rev_advanced` query relations.
    /// Wholesale wipe + repopulate, same shape as `refresh_spine_rels`; the
    /// underlying tables are written out of band by the daemon, so this is a
    /// pure read-back projection (cheap, bounded by the loaded program + watched
    /// ref count).
    pub fn refresh_daemon_rels(&self) -> Result<()> {
        let conn = self.db.conn();
        let mut p = conn.prepare("SELECT path, hash, mtime FROM _program")?;
        let programs: Vec<Vec<Value>> = p
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Int(r.get::<_, i64>(2)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        let mut h = conn.prepare("SELECT repo, name, oid FROM _ref")?;
        let heads: Vec<Vec<Value>> = h
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        let mut a = conn.prepare("SELECT repo, name, old, new FROM _rev_log ORDER BY id")?;
        let advances: Vec<Vec<Value>> = a
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
                Value::Text(r.get::<_, String>(3)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        drop(p);
        drop(h);
        drop(a);
        self.refresh_rel("program", &["path", "hash", "mtime"], &programs)?;
        self.refresh_rel("head", &["repo", "name", "oid"], &heads)?;
        self.refresh_rel("rev_advanced", &["repo", "name", "old", "new"], &advances)?;
        Ok(())
    }

    /// Project `pending_effect` into the `effect_log` query rel — a thin view (like
    /// `refresh_daemon_rels`), one row per distinct request, exposing the drain
    /// queue to a `?` query / dashboard. The job table IS the call log; this names
    /// it relationally. Reflects the queue as of tick start (rebuild_async appends
    /// new rows at tick end, the daemon drains BETWEEN ticks), so a tick sees the
    /// state its inputs were in when it began. Wipe+repopulate, plural seam.
    pub fn refresh_effect_rels(&self) -> Result<()> {
        let conn = self.db.conn();
        let mut p = conn.prepare(
            "SELECT id, kind, head_rel, state, args_json, req_tx \
             FROM pending_effect ORDER BY req_tx, id")?;
        let rows: Vec<Vec<Value>> = p
            .query_map([], |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
                Value::Text(r.get::<_, String>(3)?),
                Value::Text(r.get::<_, String>(4)?),
                Value::Int(r.get::<_, i64>(5)?),
            ]))?
            .filter_map(|x| x.ok()).collect();
        drop(p);
        self.refresh_rel("effect_log",
            &["id", "kind", "head", "state", "args", "req_tx"], &rows)?;
        Ok(())
    }

    fn module_files_by_rev(&self) -> Result<HashMap<String, Vec<(String, String)>>> {
        let mut by_rev: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let conn = self.db.conn();
        let mut sel = conn.prepare("SELECT path, rev, hash FROM _file")?;
        let rows = sel.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        for row in rows.flatten() { by_rev.entry(row.1).or_default().push((row.0, row.2)); }
        Ok(by_rev)
    }

    fn module_rows_for_rev(
        &self,
        rev: &str,
        files: &[(String, String)],
        only_paths: Option<&HashSet<String>>,
        include_crate_edges: bool,
    ) -> ModuleRows {
        let t = |s: &str| Value::Text(s.to_string());
        let root = self.root.clone();
        let resolvers = modgraph::resolvers(&root);
        let fileset: HashSet<String> = files.iter().map(|(p, _)| p.clone()).collect();
        let manifests = self.collect_manifests(rev, &fileset);
        let reader = |p: &str| read_content(&root, rev, p).ok();
        let cx = ProjectCx::new(&root, &fileset, &manifests).with_reader(&reader);
        let selected: Vec<&(String, String)> = files.iter()
            .filter(|(path, _)| match only_paths {
                Some(paths) => paths.contains(path.as_str()),
                None => true,
            })
            .collect();

        let batches: Vec<ModuleRows> = selected.par_iter().map(|(path, hash)| {
            let mut rows = ModuleRows::default();
            let ext = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
            if let Some(res) = resolvers.iter().find(|r| r.exts().contains(&ext)) {
                let content = read_content(&root, rev, path).unwrap_or_default();
                // Same content-addressed file id `_files`/parse_file use, so import
                // spans join `_files` for both WORK and committed revs.
                let where_file = spine::FileId::from_content_address(hash, content.len() as i64)
                    .filter(|f| *f != spine::FileId::SYNTHETIC);
                for mref in res.edges(path, &content, &cx) {
                    rows.imports.push(vec![t(path), t(rev), t(&mref.specifier), Value::Text(mref.kind.to_string()), Value::Int(mref.line as i64)]);
                    if let (Some(file), Some((lo, hi))) = (where_file, mref.span) {
                        let text = content.get(lo as usize..hi as usize).unwrap_or("");
                        if !text.is_empty() {
                            rows.spans.push((path.to_string(), text.to_string(), spine::WhereBytes {
                                string: spine::StringId::of(text), file, lo, hi, ..Default::default()
                            }));
                        }
                    }
                    match mref.target {
                        // A self-edge (e.g. `use crate::X` where X is defined in this
                        // crate root) is not a dependency; drop it so the graph and
                        // its closure have no spurious self-loops.
                        Resolution::File(dst) if &dst != path => {
                            rows.edges_rev.push(vec![t(path), t(&dst), t(rev)]);
                        }
                        Resolution::File(_) => {}
                        Resolution::Unresolved(reason) => {
                            rows.unresolved_rev.push(vec![t(path), t(rev), t(&mref.specifier), t(&reason), Value::Int(mref.line as i64)]);
                        }
                        Resolution::External(_) => {}
                    }
                }
            }
            rows
        }).collect();

        let mut out = ModuleRows::default();
        for batch in batches { out.extend(batch); }
        if include_crate_edges {
            for edge in modgraph::crate_edges(&manifests) {
                out.crate_edges.push(vec![t(&edge.src), t(&edge.dst), t(edge.kind), t(rev)]);
            }
        }
        out
    }

    fn insert_module_rows(&self, rows: &ModuleRows, include_crate_edges: bool) -> Result<()> {
        self.db.insert_rows(&tbl("module_import"), &["file", "rev", "specifier", "kind", "line"], &rows.imports)?;
        self.db.insert_rows(&tbl("module_edge_rev"), &["src", "dst", "rev"], &rows.edges_rev)?;
        self.db.insert_rows(&tbl("module_unresolved_rev"), &["file", "rev", "specifier", "reason", "line"], &rows.unresolved_rev)?;
        if include_crate_edges {
            self.db.insert_rows(&tbl("crate_edge"), &["src", "dst", "kind", "rev"], &rows.crate_edges)?;
        }
        self.insert_module_spans(rows)?;
        Ok(())
    }

    /// Intern each import ref's leaf text into `_strings` and its span into
    /// `_where_bytes`, both through their batched chokepoints, so `string ⋈ ref`
    /// covers the import graph. Called by every module-refresh path.
    fn insert_module_spans(&self, rows: &ModuleRows) -> Result<()> {
        let slug = self.self_slug();
        let string_rows: Vec<(String, String, Vec<Value>)> = rows.spans.iter()
            .map(|(path, text, _)| (slug.clone(), path.clone(), vec![Value::Text(text.clone())])).collect();
        self.insert_spine_strings(&string_rows)?;
        let where_rows: Vec<(String, String, spine::WhereBytes, Option<String>)> = rows.spans.iter()
            .map(|(path, _, wb)| (slug.clone(), path.clone(), *wb, None)).collect();
        self.insert_spine_where_bytes(&where_rows)?;
        Ok(())
    }

    fn rebuild_legacy_module_rels(&self) -> Result<()> {
        let edge = tbl("module_edge");
        let edge_rev = tbl("module_edge_rev");
        let unresolved = tbl("module_unresolved");
        let unresolved_rev = tbl("module_unresolved_rev");
        self.db.exec(&format!("DELETE FROM {edge}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {edge} (\"src\", \"dst\") SELECT \"src\", \"dst\" FROM {edge_rev}"
        ))?;
        self.db.exec(&format!("DELETE FROM {unresolved}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {unresolved} (\"file\", \"specifier\", \"reason\", \"line\") \
             SELECT \"file\", \"specifier\", \"reason\", \"line\" FROM {unresolved_rev}"
        ))?;
        Ok(())
    }

    /// Rebuild the module-graph relations from the `_file` set, per rev. Reads each
    /// file's content, picks the language resolver by extension, and writes one
    /// `module_import` row per reference plus `module_edge(src,dst)` /
    /// `module_edge_rev(src,dst,rev)` for resolved project files and unresolved
    /// relations for ones that should have resolved.
    /// Wholesale wipe + repopulate; gated by `module_rels_used` at the call site.
    /// Edges are resolved within a single rev (cross-rev merge is a Stage-1 corner).
    pub(crate) fn refresh_module_rels(&self) -> Result<()> {
        let by_rev = self.module_files_by_rev()?;
        let mut rows = ModuleRows::default();
        for (rev, files) in &by_rev {
            rows.extend(self.module_rows_for_rev(rev, files, None, true));
        }
        self.refresh_rel("module_import", &["file", "rev", "specifier", "kind", "line"], &rows.imports)?;
        self.refresh_rel("module_edge_rev", &["src", "dst", "rev"], &rows.edges_rev)?;
        self.refresh_rel("module_unresolved_rev", &["file", "rev", "specifier", "reason", "line"], &rows.unresolved_rev)?;
        self.refresh_rel("crate_edge", &["src", "dst", "kind", "rev"], &rows.crate_edges)?;
        self.insert_module_spans(&rows)?;
        self.rebuild_legacy_module_rels()?;
        Ok(())
    }

    pub(crate) fn refresh_module_rels_for_revs(&self, revs: &[&str]) -> Result<()> {
        if revs.is_empty() { return Ok(()); }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _module_refresh_rev(rev TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _module_refresh_rev")?;
        let rev_rows: Vec<Vec<Value>> = revs.iter().map(|rev| vec![Value::Text((*rev).to_string())]).collect();
        self.db.insert_rows("_module_refresh_rev", &["rev"], &rev_rows)?;
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _module_refresh_rev)", tbl("module_import")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _module_refresh_rev)", tbl("module_edge_rev")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _module_refresh_rev)", tbl("module_unresolved_rev")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _module_refresh_rev)", tbl("crate_edge")))?;

        let by_rev = self.module_files_by_rev()?;
        let mut rows = ModuleRows::default();
        for rev in revs {
            if let Some(files) = by_rev.get(*rev) {
                rows.extend(self.module_rows_for_rev(rev, files, None, true));
            }
        }
        self.insert_module_rows(&rows, true)?;
        self.rebuild_legacy_module_rels()?;
        Ok(())
    }

    pub(crate) fn refresh_module_rels_for_paths(&self, rev: &str, paths: &HashSet<String>) -> Result<()> {
        if paths.is_empty() { return Ok(()); }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _module_refresh_path(path TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _module_refresh_path")?;
        let path_rows: Vec<Vec<Value>> = paths.iter().map(|p| vec![Value::Text(p.clone())]).collect();
        self.db.insert_rows("_module_refresh_path", &["path"], &path_rows)?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = '{rev}' AND \"file\" IN (SELECT path FROM _module_refresh_path)",
            tbl("module_import"),
        ))?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = '{rev}' AND \"src\" IN (SELECT path FROM _module_refresh_path)",
            tbl("module_edge_rev"),
        ))?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = '{rev}' AND \"file\" IN (SELECT path FROM _module_refresh_path)",
            tbl("module_unresolved_rev"),
        ))?;

        let by_rev = self.module_files_by_rev()?;
        let rows = by_rev.get(rev)
            .map(|files| self.module_rows_for_rev(rev, files, Some(paths), false))
            .unwrap_or_default();
        self.insert_module_rows(&rows, false)?;
        self.rebuild_legacy_module_rels()?;
        Ok(())
    }

    /// The extraction corpus: every `_file` row in a TypeLang extension, with
    /// its content address. One query serves the type/call/dataflow refreshers;
    /// the hash column is what makes both the whole-pass digest skip and the
    /// per-file fact cache content-keyed (perf gap A).
    fn extract_file_set(&self) -> Result<Vec<ExtractFile>> {
        let mut files: Vec<ExtractFile> = Vec::new();
        let mut sel = self.db.conn().prepare(
            "SELECT repo, path, rev, hash FROM _file WHERE path LIKE '%.rs' OR path LIKE '%.kt' OR path LIKE '%.kts' \
             OR path LIKE '%.ts' OR path LIKE '%.tsx'")?;
        let rows = sel.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?.unwrap_or_default())))?;
        for row in rows.flatten() { files.push(row); }
        Ok(files)
    }

    /// XOR-folded input digest for one extractor family: one blake3 per
    /// (repo, path, rev, content hash) corpus row, plus the `scip_ref` override
    /// table when the family resolves through it, plus the running binary's
    /// identity (see `exe_stamp`). Persisted under `extract:<family>` in
    /// `_reldigest`; an unchanged digest means the family's output rows are
    /// already in this db, so the whole parse + resolve + write pass skips.
    /// A row with an empty content hash has no identity, so the digest is
    /// salted with the current time — never equal, never a false skip.
    fn extract_input_digest(&self, family: &str, files: &[ExtractFile], with_scip: bool) -> [u8; 32] {
        let mut acc = [0u8; 32];
        let fold = |acc: &mut [u8; 32], h: &blake3::Hash| {
            for (a, b) in acc.iter_mut().zip(h.as_bytes()) { *a ^= *b; }
        };
        fold(&mut acc, &blake3::hash(format!("{family}\0{:032x}", exe_stamp()).as_bytes()));
        for (repo, path, rev, hash) in files {
            if hash.is_empty() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos()).unwrap_or(0);
                fold(&mut acc, &blake3::hash(format!("nonce\0{now}").as_bytes()));
                continue;
            }
            fold(&mut acc, &blake3::hash(format!("{repo}\0{path}\0{rev}\0{hash}").as_bytes()));
        }
        if with_scip {
            if let Ok(mut s) = self.db.conn().prepare(
                &format!("SELECT file, symbol, def_file FROM {}", tbl("scip_ref"))) {
                if let Ok(rows) = s.query_map([], |r| Ok((
                    r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))) {
                    for (f, sym, d) in rows.flatten() {
                        fold(&mut acc, &blake3::hash(format!("scip\0{f}\0{sym}\0{d}").as_bytes()));
                    }
                }
            }
        }
        acc
    }

    /// Rebuild the type graph from the `_file` set. This is the same L3
    /// shape as module graph: read tracked Rust/Kotlin/TS files, run a
    /// deterministic syntax extractor, flush one built-in relation through
    /// `refresh_rel`.
    /// Returns whether the family's inputs moved (false = digest skip, the
    /// stored rows already serve): the tick marks the family's rels changed
    /// only on true, so dependents of an untouched family are not re-derived
    /// (perf gap C).
    pub(crate) fn refresh_type_rels(&self) -> Result<bool> {
        let files = self.extract_file_set()?;
        // Perf gap A: a warm tick whose corpus (and scip override) didn't move
        // already has this family's rows in the db — skip the whole pass.
        let digest = self.extract_input_digest("type", &files, true);
        if self.load_rel_digest("extract:type")? == Some(digest) { return Ok(false); }
        // Parse + extract per file in parallel (same shape as module_rows_for_rev),
        // then flatten and write once. Keeps the cold-build parse working set bounded
        // by the rayon pool, not the corpus (peak-RSS invariant). Rows carry their
        // rev so the type graph is history-aware like module_edge_rev.
        let root = self.root.clone();
        let roots = self.repo_roots();
        // Per-file extraction via the language registry (no extension if-chain;
        // registry order makes .kts match Kotlin before .ts would). Each file
        // yields its declared entities + edge graph; collected before resolution
        // because name->def resolution is corpus-global (a barrier). Content is
        // read from the file's OWN repo root so config-repo files index too.
        // facts carry the derived repo id (nearest `.git` of the file) so each
        // entity/edge row is attributed to the folder it lives in. Unchanged
        // files come out of the per-file cache without a parse.
        let facts: Vec<(String, String, String, Arc<typegraph::TypeFacts>)> =
            cached_facts(&self.type_facts_cache, &files, &self.extract_files_parsed, |repo, path, rev| {
                let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                Some((rid, lang.extract(path, &content)))
            });

        // Resolver: a name maps to its definition symbol when exactly one entity
        // in the SAME repo declares it (syntactic). Keying by repo keeps two
        // folders in view that share a name from making each other ambiguous,
        // and the resolved sym is repo-qualified (`{repo}::{sym}`) so the edge
        // relations (type_link/type_sig — no repo column) stay distinct across
        // identical-path repos. A SCIP index, when present, overrides per
        // (file, name) with the indexed def file (collision-proof).
        let mut by_name: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
        let mut sym_at: HashMap<(&str, &str, &str), &str> = HashMap::new();
        for (repo, _, _, f) in &facts {
            for e in &f.entities {
                by_name.entry((repo.as_str(), e.name.as_str())).or_default().push(e.sym.as_str());
                sym_at.insert((repo.as_str(), e.file.as_str(), e.name.as_str()), e.sym.as_str());
            }
        }
        let scip = self.scip_name_defs().unwrap_or_default();
        let resolve = |repo: &str, file: &str, name: &str| -> Option<String> {
            if let Some(def_file) = scip.get(&(file.to_string(), name.to_string())) {
                if let Some(sym) = sym_at.get(&(repo, def_file.as_str(), name)) {
                    return Some(format!("{repo}::{sym}"));
                }
            }
            match by_name.get(&(repo, name)) {
                Some(v) if v.len() == 1 => Some(format!("{repo}::{}", v[0])),
                _ => None,
            }
        };

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut edge_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut entity_rows: Vec<Vec<Value>> = Vec::new();
        let mut sig_rows: Vec<Vec<Value>> = Vec::new();
        let mut link_rows: Vec<Vec<Value>> = Vec::new();
        // Dedup keys carry the repo, so two folders in view that share a relative
        // path + symbol name (e.g. both have `src/index.ts`) do NOT drop each
        // other's rows — each repo's entity survives.
        let mut seen_entity: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_link: HashSet<(String, String, &str)> = HashSet::new();
        let mut doc_rows: Vec<Vec<Value>> = Vec::new();
        let mut tag_rows: Vec<Vec<Value>> = Vec::new();
        let mut seen_doc: HashSet<(&str, &str)> = HashSet::new();
        for (repo, path, rev, f) in &facts {
            // historic name-keyed edges, now repo-tagged so two trees sharing a
            // type name don't collapse into one node when scanned together
            // (closure/scc still walk cols[0]/cols[1] = from/to, untouched).
            for edge in &f.edges {
                edge_rev_rows.push(vec![t(&edge.from), t(&edge.to), t(edge.kind), t(rev), t(repo)]);
                // SCIP-resolved graph: owner sym -> resolved target sym (or the
                // bare name when external/ambiguous, so leaf types still appear)
                let src = sym_at.get(&(repo.as_str(), path.as_str(), edge.from.as_str()))
                    .map(|s| format!("{repo}::{s}")).unwrap_or_else(|| edge.from.clone());
                let dst = resolve(repo, path, &edge.to).unwrap_or_else(|| edge.to.clone());
                if seen_link.insert((src.clone(), dst.clone(), edge.kind)) {
                    link_rows.push(vec![t(&src), t(&dst), t(edge.kind)]);
                }
            }
            for ent in &f.entities {
                // repo-qualified sym: globally unique even when two repos share a
                // relative path, so sym-keyed rels (type_sig/type_link) and the
                // cross-rel joins to call_def stay per-repo distinct.
                let qsym = format!("{repo}::{}", ent.sym);
                if seen_entity.insert((repo.as_str(), ent.sym.as_str())) {
                    let qparent = ent.parent.as_deref().map(|p| format!("{repo}::{p}")).unwrap_or_default();
                    entity_rows.push(vec![
                        t(repo), t(&qsym), t(&ent.name), t(ent.kind.tag()),
                        t(&qparent), t(&ent.file), i(ent.line),
                    ]);
                }
                // the arrow [...A] => B, one row per referenced type per slot
                if let Some(ty) = &ent.ty {
                    for (pos, slot) in ty.params.iter().enumerate() {
                        for r in slot {
                            let rf = resolve(repo, path, r.name()).unwrap_or_else(|| r.name().to_string());
                            sig_rows.push(vec![t(&qsym), t("param"), i(pos as u32), t(&rf)]);
                        }
                    }
                    for r in &ty.ret {
                        let rf = resolve(repo, path, r.name()).unwrap_or_else(|| r.name().to_string());
                        sig_rows.push(vec![t(&qsym), t("ret"), i(0), t(&rf)]);
                    }
                }
            }
            // Doc comments per entity (Tier 1) + their structured tags (Tier 2).
            // Same repo-qualified sym + first-seen dedup as the entity rows, so a
            // file present at two revs doesn't duplicate (doc_comment has no rev).
            for doc in &f.docs {
                if !seen_doc.insert((repo.as_str(), doc.sym.as_str())) { continue; }
                let qsym = format!("{repo}::{}", doc.sym);
                doc_rows.push(vec![t(repo), t(&qsym), i(doc.line), t(&doc.text)]);
                for tag in &doc.tags {
                    tag_rows.push(vec![t(repo), t(&qsym), t(&tag.tag), t(&tag.arg), t(&tag.text)]);
                }
            }
        }
        self.refresh_rel("type_edge_rev", &["from", "to", "kind", "rev", "repo"], &edge_rev_rows)?;
        self.refresh_rel("type_entity", &["repo", "sym", "name", "kind", "parent", "file", "line"], &entity_rows)?;
        self.refresh_rel("type_sig", &["sym", "slot", "pos", "ref"], &sig_rows)?;
        self.refresh_rel("type_link", &["src", "dst", "kind"], &link_rows)?;
        self.refresh_rel("doc_comment", &["repo", "sym", "line", "text"], &doc_rows)?;
        self.refresh_rel("doc_tag", &["repo", "sym", "tag", "arg", "text"], &tag_rows)?;
        self.rebuild_legacy_type_rels()?;
        // Persisted only after the writes land, so a failed refresh retries.
        self.save_rel_digest("extract:type", &digest)?;
        Ok(true)
    }

    /// Best-effort SCIP override for resolution: read `scip_ref(file, symbol,
    /// def_file)` and key it by (file, trailing-descriptor-name) -> def_file.
    /// Empty when no index.scip is present, so the syntactic path carries.
    fn scip_name_defs(&self) -> Result<HashMap<(String, String), String>> {
        let mut out = HashMap::new();
        let conn = self.db.conn();
        let Ok(mut s) = conn.prepare(&format!("SELECT file, symbol, def_file FROM {}", tbl("scip_ref"))) else {
            return Ok(out);
        };
        let rows = s.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        for row in rows.flatten() {
            let (file, symbol, def_file) = row;
            if let Some(name) = scip_descriptor_name(&symbol) {
                out.insert((file, name), def_file);
            }
        }
        Ok(out)
    }

    /// Rebuild the convenient rev-less `type_edge(from, to, kind)` from the
    /// rev-aware table, deduped across revs. Same shape as
    /// `rebuild_legacy_module_rels`: the `_rev` table is the source of truth,
    /// the legacy view is the simple closure target.
    fn rebuild_legacy_type_rels(&self) -> Result<()> {
        let edge = tbl("type_edge");
        let edge_rev = tbl("type_edge_rev");
        self.db.exec(&format!("DELETE FROM {edge}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {edge} (\"from\", \"to\", \"kind\", \"repo\") \
             SELECT \"from\", \"to\", \"kind\", \"repo\" FROM {edge_rev}"
        ))?;
        Ok(())
    }

    /// CST-as-relation (christmas #3): walk every NAMED tree-sitter node of every
    /// scanned file (across all 11 `ts_lang` grammars) into `node`/`child`. Same
    /// shape as `refresh_type_rels`: parallel per-file parse (CPU-bound, no DB),
    /// then collect-then-flush (three batched writes: `_strings`/`_where_bytes`
    /// for the spine, `node`, `child`). NO per-row write — the N+1 counter stays
    /// quiet. Gated on `node_rels_used`, so a program that never asks pays
    /// nothing (the full-tree walk is ~100x type_edge; christmas #19 chunked
    /// flush is the later mitigation, NOT built here).
    ///
    /// Each node's id is a kind-salted `_where_bytes` id: `salted(of_located(
    /// raw-slice WhereBytes, repo, path), kind)`. The salt keeps a wrapper node
    /// and its sole identical-span child distinct (else innermost-containment
    /// merges them), while the underlying `_where_bytes` row carries the RAW
    /// slice's StringId, so `ref(id, sid, ..)` -> `string(sid, text, ..)` recovers
    /// the node's source bytes (riding step 1's intern fix).
    pub(crate) fn refresh_node_rels(&self) -> Result<bool> {
        // Whole-corpus walk: read every scanned file from `_file`.
        let files = self.node_file_set(None)?;
        let parsed = self.node_walk(&files);
        self.last_node_files_walked.set(parsed.len());
        let (node_rows, child_rows, path_by_id, str_by_id, wb_by_id) = self.node_rows_from_walk(&parsed);

        // Node ids are content-addressed and kind-salted, so each node row is
        // already unique within a tick (a node can't appear twice in one walk;
        // path folds into the id across files). Early-out by comparing the stored
        // id set to the computed one — if identical, no file's tree moved.
        let computed: std::collections::HashSet<String> = node_rows.iter()
            .filter_map(|row| if let Value::Text(s) = &row[0] { Some(s.clone()) } else { None }).collect();
        let stored: std::collections::HashSet<String> = {
            let conn = self.db.conn();
            let mut s = conn.prepare(&format!("SELECT id FROM {}", tbl("node")))?;
            let set: std::collections::HashSet<String> =
                s.query_map([], |r| r.get::<_, String>(0))?.filter_map(|x| x.ok()).collect();
            set
        };
        if stored == computed { return Ok(false); }

        // Full replace: spine first (so node ids resolve), then a whole-table
        // wipe + reinsert of node/child via refresh_rel. `_node_path` (id->path
        // attribution, not a public rel column) is rebuilt wholesale too so the
        // delta refresh can later prune one file's rows.
        self.flush_node_spine(str_by_id, wb_by_id)?;
        self.refresh_rel("node", &["id", "kind", "file", "lo", "hi", "parent"], &node_rows)?;
        self.refresh_rel("child", &["parent", "child"], &child_rows)?;
        self.db.exec("DELETE FROM _node_path")?;
        self.db.insert_rows("_node_path", &["id", "path"], &path_by_id)?;
        Ok(true)
    }

    /// Path-scoped CST refresh for the incremental tick: re-walk ONLY the changed
    /// files, prune their OLD node/child + spine rows, insert the new walk. The
    /// OTHER files' node/child rows are untouched. Mirrors
    /// `refresh_module_rels_for_paths`. The `_where_bytes`/`_strings` node spans
    /// for the changed files were already pruned by `retract_path` (keyed by
    /// (repo, path)); this re-inserts the fresh spans. Returns true if any node
    /// row changed.
    pub(crate) fn refresh_node_rels_delta(&self, paths: &HashSet<String>) -> Result<bool> {
        if paths.is_empty() { return Ok(false); }
        let files = self.node_file_set(Some(paths))?;
        let parsed = self.node_walk(&files);
        self.last_node_files_walked.set(parsed.len());
        let (node_rows, child_rows, path_by_id, str_by_id, wb_by_id) = self.node_rows_from_walk(&parsed);

        // Prune this tick's changed files' OLD rows: `node` rows whose id is
        // attributed to a changed path (via `_node_path`), plus the `_node_path`
        // rows themselves. `node.file` is a content FileId shared by
        // byte-identical files, so it can't key the prune; `_node_path` keys by
        // the real source path. Other files' node rows stay untouched.
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _node_refresh_path(path TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _node_refresh_path")?;
        let path_rows: Vec<Vec<Value>> = paths.iter().map(|p| vec![Value::Text(p.clone())]).collect();
        self.db.insert_rows("_node_refresh_path", &["path"], &path_rows)?;
        let node_tbl = tbl("node");
        let child_tbl = tbl("child");
        let changed_ids_sql =
            "SELECT id FROM _node_path WHERE path IN (SELECT path FROM _node_refresh_path)";
        // child edges of the changed files: their `child` endpoint id is in the
        // changed-path id set (CST is per-file, so an edge never crosses files).
        // Delete BEFORE pruning `_node_path` so the id subquery still resolves.
        self.db.exec(&format!(
            "DELETE FROM {child_tbl} WHERE \"child\" IN ({changed_ids_sql})"))?;
        self.db.exec(&format!("DELETE FROM {node_tbl} WHERE \"id\" IN ({changed_ids_sql})"))?;
        self.db.exec("DELETE FROM _node_path WHERE path IN (SELECT path FROM _node_refresh_path)")?;

        // Spine first (so the new node ids resolve through `ref`/`string`), then
        // the node rows + their `_node_path` attribution, then re-derive `child`
        // so it never references a node id that no longer exists.
        self.flush_node_spine(str_by_id, wb_by_id)?;
        let nodes_changed = !node_rows.is_empty();
        if nodes_changed {
            self.db.insert_rows(&node_tbl, &["id", "kind", "file", "lo", "hi", "parent"], &node_rows)?;
            self.db.insert_rows("_node_path", &["id", "path"], &path_by_id)?;
        }
        // Re-insert the fresh walk's child edges (the stale ones were deleted
        // above by the changed-path id set). One plural write; other files'
        // edges untouched (no whole-corpus child rebuild).
        if !child_rows.is_empty() {
            self.db.insert_rows(&child_tbl, &["parent", "child"], &child_rows)?;
        }
        Ok(nodes_changed)
    }

    /// The scanned file set for a node walk, keyed by (repo, path, rev, hash).
    /// `only` restricts to a changed-path subset (the delta refresh); `None`
    /// reads every `_file` row (the cold/full refresh).
    pub(crate) fn node_file_set(&self, only: Option<&HashSet<String>>) -> Result<Vec<(String, String, String, String)>> {
        let mut files: Vec<(String, String, String, String)> = Vec::new();
        let conn = self.db.conn();
        let mut sel = conn.prepare("SELECT repo, path, rev, hash FROM _file")?;
        let rows = sel.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?,
            r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
        for row in rows.flatten() {
            if let Some(set) = only { if !set.contains(&row.1) { continue; } }
            files.push(row);
        }
        Ok(files)
    }

    /// Per-file parse + tree-sitter walk in parallel (no DB touch). Each yields
    /// the file's node records plus the repo id + path + FileId its spans key off.
    fn node_walk(&self, files: &[(String, String, String, String)]) -> Vec<FileNodes> {
        let root = self.root.clone();
        let roots = self.repo_roots();
        files.par_iter().filter_map(|(repo, path, rev, hash)| {
            let label = crate::cst::lang_label_for_path(path)?;
            let lang = ts_lang(label).ok()?;
            let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, rev, path).unwrap_or_default();
            let file = spine::FileId::from_content_address(hash, content.len() as i64)
                .filter(|f| *f != spine::FileId::SYNTHETIC)?;
            let nodes = crate::cst::walk_cst(&content, &lang).ok()?;
            if nodes.is_empty() { return None; }
            let rid = repo_id_of(froot, path, repo);
            Some(FileNodes { repo: rid, path: path.clone(), file, content, nodes })
        }).collect()
    }

    /// Build the node/child rel rows + the spine (`_strings`/`_where_bytes`)
    /// interns from a parsed walk. Collect-then-flush; no DB touch.
    fn node_rows_from_walk(&self, parsed: &[FileNodes])
        -> (Vec<Vec<Value>>, Vec<Vec<Value>>, Vec<Vec<Value>>, BTreeMap<String, (String, String)>, BTreeMap<String, Vec<Value>>)
    {
        let mut node_rows: Vec<Vec<Value>> = Vec::new();
        let mut child_rows: Vec<Vec<Value>> = Vec::new();
        // (node id, path) attribution rows for the `_node_path` side table.
        let mut path_by_id: Vec<Vec<Value>> = Vec::new();
        let mut str_by_id: BTreeMap<String, (String, String)> = BTreeMap::new();
        let mut wb_by_id: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for fln in parsed {
            let FileNodes { repo, path, file, content, nodes } = fln;
            // Pre-compute each node's salted id (an index-aligned Vec) so the
            // child edges reference the parent's id without recomputing.
            let mut ids: Vec<String> = Vec::with_capacity(nodes.len());
            for n in nodes {
                let slice = content.get(n.lo..n.hi).unwrap_or("");
                let raw_sid = spine::StringId::of(slice);
                let raw_wb = spine::WhereBytes { string: raw_sid, file: *file, lo: n.lo as u32, hi: n.hi as u32, ..Default::default() };
                let base = spine::WhereBytesId::of_located(raw_wb, repo, path);
                let node_id = base.salted(&n.kind).to_string();
                ids.push(node_id.clone());
                // Spine rows: the `_where_bytes` row uses the SALTED id but the
                // RAW StringId, so ref(node_id) -> string(raw_sid) = raw slice.
                if !slice.is_empty() {
                    str_by_id.entry(raw_sid.to_string())
                        .or_insert_with(|| (slice.to_string(), spine::normalize(slice)));
                    wb_by_id.entry(node_id.clone()).or_insert_with(|| vec![
                        Value::Text(node_id.clone()),
                        Value::Text(raw_sid.to_string()),
                        Value::Text(file.to_string()),
                        Value::Int(n.lo as i64),
                        Value::Int(n.hi as i64),
                        Value::Text(repo.clone()),
                        Value::Text(spine::RevId::default().to_string()),
                        Value::Text(path.clone()),
                    ]);
                }
            }
            for (ix, n) in nodes.iter().enumerate() {
                let parent_id = n.parent_ix.map(|p| ids[p].clone()).unwrap_or_default();
                node_rows.push(vec![
                    Value::Text(ids[ix].clone()),
                    Value::Text(n.kind.clone()),
                    Value::Text(file.to_string()),
                    Value::Int(n.lo as i64),
                    Value::Int(n.hi as i64),
                    Value::Text(parent_id),
                ]);
                path_by_id.push(vec![Value::Text(ids[ix].clone()), Value::Text(path.clone())]);
                if let Some(p) = n.parent_ix {
                    child_rows.push(vec![Value::Text(ids[p].clone()), Value::Text(ids[ix].clone())]);
                }
            }
        }
        (node_rows, child_rows, path_by_id, str_by_id, wb_by_id)
    }

    /// Flush the node walk's spine interns: `_strings` then `_where_bytes`
    /// (both INSERT OR IGNORE, content-addressed), so a node id resolves through
    /// `ref`/`string`. One plural write each.
    fn flush_node_spine(
        &self,
        str_by_id: BTreeMap<String, (String, String)>,
        wb_by_id: BTreeMap<String, Vec<Value>>,
    ) -> Result<()> {
        if !str_by_id.is_empty() {
            let string_rows: Vec<Vec<Value>> = str_by_id.into_iter()
                .map(|(id, (content, norm))| vec![Value::Text(id), Value::Text(content), Value::Text(norm)])
                .collect();
            self.db.insert_rows("_strings", &["id", "content", "norm"], &string_rows)?;
        }
        if !wb_by_id.is_empty() {
            let wb_rows: Vec<Vec<Value>> = wb_by_id.into_values().collect();
            self.db.insert_rows("_where_bytes", &["id", "string_id", "file_id", "lo", "hi", "repo", "rev", "path"], &wb_rows)?;
        }
        Ok(())
    }

    /// Wholesale repopulation of the Phase D call-graph relations. Same shape
    /// as `refresh_type_rels`: parallel per-file extraction via the language
    /// registry, one write per relation. Extractors return empty `CallFacts`
    /// today (the trait default), so this wires the lazy-indexer plumbing end
    /// to end with zero rows; per-language extractor bodies fill it in next.
    /// The caller-resolution second pass (span containment + bare-name resolve,
    /// the type_link path) lands with the first real extractor body; the row
    /// vecs already flow through it so the write path is exercised now.
    /// Change-reporting contract mirrors `refresh_type_rels`.
    pub(crate) fn refresh_call_rels(&self) -> Result<bool> {
        let files = self.extract_file_set()?;
        // Perf gap A: same digest skip + per-file cache as refresh_type_rels.
        let digest = self.extract_input_digest("call", &files, true);
        if self.load_rel_digest("extract:call")? == Some(digest) { return Ok(false); }

        let root = self.root.clone();
        let roots = self.repo_roots();
        let facts: Vec<(String, String, String, Arc<typegraph::CallFacts>)> =
            cached_facts(&self.call_facts_cache, &files, &self.extract_files_parsed, |repo, path, rev| {
                let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                Some((rid, lang.extract_calls(path, &content)))
            });

        // Corpus-global def index: a barrier before any edge is emitted, same
        // shape as refresh_type_rels. by_name resolves a bare callee to a def
        // sym when exactly one callable declares it; sym_at backs the SCIP
        // override; def_by_file drives span-containment caller resolution
        // (innermost enclosing def wins, so calls inside a nested block attach
        // to the nearest fn, not the outermost).
        // Repo-scoped, same as refresh_type_rels: a callee resolves within the
        // referencing file's repo, and resolved syms are repo-qualified so the
        // sym-keyed call rels (call_edge/call_name) stay per-repo distinct.
        let mut by_name: HashMap<(&str, &str), Vec<&str>> = HashMap::new();
        let mut sym_at: HashMap<(&str, &str, &str), &str> = HashMap::new();
        let mut def_by_file: HashMap<(&str, &str), Vec<(u32, u32, &str)>> = HashMap::new();
        for (repo, _, _, f) in &facts {
            for d in &f.defs {
                by_name.entry((repo.as_str(), d.name.as_str())).or_default().push(d.sym.as_str());
                sym_at.insert((repo.as_str(), d.file.as_str(), d.name.as_str()), d.sym.as_str());
                def_by_file.entry((repo.as_str(), d.file.as_str())).or_default().push((d.line, d.end, d.sym.as_str()));
            }
        }
        let scip = self.scip_name_defs().unwrap_or_default();
        let resolve_callee = |repo: &str, file: &str, callee: &str| -> Option<String> {
            if let Some(def_file) = scip.get(&(file.to_string(), callee.to_string())) {
                if let Some(sym) = sym_at.get(&(repo, def_file.as_str(), callee)) {
                    return Some(format!("{repo}::{sym}"));
                }
            }
            match by_name.get(&(repo, callee)) {
                Some(v) if v.len() == 1 => Some(format!("{repo}::{}", v[0])),
                _ => None,
            }
        };
        let resolve_caller = |repo: &str, file: &str, line: u32| -> Option<String> {
            let mut best: Option<(u32, &str)> = None; // (span, sym); smallest containing span wins
            for &(s, e, sym) in def_by_file.get(&(repo, file)).into_iter().flatten() {
                if line >= s && line <= e {
                    let span = e - s;
                    match best {
                        Some((bs, _)) if span >= bs => {}
                        _ => best = Some((span, sym)),
                    }
                }
            }
            best.map(|(_, s)| format!("{repo}::{s}"))
        };

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut def_rows: Vec<Vec<Value>> = Vec::new();
        let mut site_rows: Vec<Vec<Value>> = Vec::new();
        let mut edge_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut name_rows: Vec<Vec<Value>> = Vec::new();
        // call_kind is keyed by (caller, kind); a fn with both a read and a
        // write emits two rows. Accumulate in a set so multiple write sites in
        // the same fn collapse to one (fn, "write") row.
        let mut kind_set: HashSet<(String, &'static str)> = HashSet::new();
        let mut seen_def: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_edge: HashSet<(String, String, &str)> = HashSet::new();
        for (repo, _path, rev, f) in &facts {
            for d in &f.defs {
                if seen_def.insert((repo.as_str(), d.sym.as_str())) {
                    let qsym = format!("{repo}::{}", d.sym);
                    def_rows.push(vec![t(repo), t(&qsym), t(d.kind.tag()), t(&d.file), i(d.line), i(d.end)]);
                    name_rows.push(vec![t(&qsym), t(&d.name)]);
                }
            }
            for s in &f.sites {
                // call_site is the raw graph: every site, caller resolved when a
                // def encloses it (repo-qualified), callee as written.
                let caller = resolve_caller(repo, &s.file, s.line).unwrap_or_default();
                site_rows.push(vec![t(repo), t(&caller), t(&s.callee), t(&s.file), i(s.line)]);
                // call_kind: classify the callee's bare name as read/write. The
                // fn-aggregate is the precision axis the conn-loop-reachable
                // rail needs (a fn that only reads through its .conn() does not
                // fire). Heuristic by name: $R.execute(...) is a write on any
                // receiver; the rail's conn_fn join narrows to db-shaped sites.
                if !caller.is_empty() {
                    if let Some(k) = classify_call_kind(&s.callee) {
                        kind_set.insert((caller.clone(), k));
                    }
                }
                // call_edge is the resolved graph: emit only when both endpoints
                // resolve to def syms, so closure(call_edge) walks one identity
                // space (same contract as type_link). Unresolved calls stay in
                // call_site with their bare callee.
                if let Some(callee_sym) = resolve_callee(repo, &s.file, &s.callee) {
                    if !caller.is_empty() && seen_edge.insert((caller.clone(), callee_sym.clone(), rev)) {
                        edge_rev_rows.push(vec![t(&caller), t(&callee_sym), t("call"), t(rev)]);
                    }
                }
            }
        }

        let mut kind_pairs: Vec<(String, &'static str)> = kind_set.into_iter().collect();
        kind_pairs.sort();
        let kind_rows: Vec<Vec<Value>> = kind_pairs
            .into_iter()
            .map(|(f, k)| vec![t(&f), t(k)])
            .collect();

        self.refresh_rel("call_def", &["repo", "sym", "kind", "file", "line", "end"], &def_rows)?;
        self.refresh_rel("call_site", &["repo", "caller", "callee", "file", "line"], &site_rows)?;
        self.refresh_rel("call_edge_rev", &["caller", "callee", "kind", "rev"], &edge_rev_rows)?;
        self.refresh_rel("call_name", &["sym", "name"], &name_rows)?;
        self.refresh_rel("call_kind", &["fn", "kind"], &kind_rows)?;
        self.rebuild_legacy_call_rels()?;
        // Persisted only after the writes land, so a failed refresh retries.
        self.save_rel_digest("extract:call", &digest)?;
        Ok(true)
    }

    /// Rebuild the convenient rev-less `call_edge(caller, callee, kind)` from
    /// the rev-aware table, deduped across revs. Same shape as
    /// `rebuild_legacy_type_rels`: `call_edge_rev` is the source of truth, the
    /// legacy view is the simple closure target.
    fn rebuild_legacy_call_rels(&self) -> Result<()> {
        let edge = tbl("call_edge");
        let edge_rev = tbl("call_edge_rev");
        self.db.exec(&format!("DELETE FROM {edge}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {edge} (\"caller\", \"callee\", \"kind\") \
             SELECT \"caller\", \"callee\", \"kind\" FROM {edge_rev}"
        ))?;
        Ok(())
    }

    /// Intra-procedural dataflow lift over the corpus. Each `.rs/.kt/.kts/.ts/.tsx`
    /// file in `_file` is parsed once by the matching front-end's
    /// `extract_dataflow`; nodes and edges are corpus-deduped by id (the
    /// `file:line:col` start span is already unique across files). No resolution
    /// pass is needed — node ids and the enclosing `fn` sym are self-contained,
    /// so this is a straight extract + bulk write.
    /// Change-reporting contract mirrors `refresh_type_rels`.
    pub(crate) fn refresh_dataflow_rels(&self) -> Result<bool> {
        let files = self.extract_file_set()?;
        // Perf gap A: no resolution pass here, so the digest folds the corpus
        // rows only (no scip term).
        let digest = self.extract_input_digest("dataflow", &files, false);
        if self.load_rel_digest("extract:dataflow")? == Some(digest) { return Ok(false); }

        let root = self.root.clone();
        let facts: Vec<(String, String, String, Arc<typegraph::DataflowFacts>)> =
            cached_facts(&self.df_facts_cache, &files, &self.extract_files_parsed, |_repo, path, rev| {
                let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
                let content = read_content(&root, rev, path).unwrap_or_default();
                Some((String::new(), lang.extract_dataflow(path, &content)))
            });

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut node_rows: Vec<Vec<Value>> = Vec::new();
        let mut edge_rows: Vec<Vec<Value>> = Vec::new();
        let mut loop_rows: Vec<Vec<Value>> = Vec::new();
        let mut alloc_rows: Vec<Vec<Value>> = Vec::new();
        let mut nest_rows: Vec<Vec<Value>> = Vec::new();
        let mut param_rows: Vec<Vec<Value>> = Vec::new();
        let mut arg_rows: Vec<Vec<Value>> = Vec::new();
        let mut field_rows: Vec<Vec<Value>> = Vec::new();
        let mut seen_param: HashSet<&str> = HashSet::new();
        let mut seen_arg: HashSet<(&str, i64, &str)> = HashSet::new();
        let mut seen_field: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_node: HashSet<&str> = HashSet::new();
        let mut seen_edge: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_loop: HashSet<(&str, u32)> = HashSet::new();
        let mut seen_nest: HashSet<(&str, &str)> = HashSet::new();
        for (_, _, _, f) in &facts {
            for n in &f.nodes {
                if seen_node.insert(n.id.as_str()) {
                    node_rows.push(vec![t(&n.id), t(&n.kind), t(&n.var), t(&n.fn_sym), t(&n.file), i(n.line)]);
                }
            }
            for e in &f.edges {
                if seen_edge.insert((e.from.as_str(), e.to.as_str())) {
                    edge_rows.push(vec![t(&e.from), t(&e.to)]);
                }
            }
            for l in &f.loops {
                if seen_loop.insert((l.file.as_str(), l.start)) {
                    loop_rows.push(vec![t(&l.file), i(l.start), i(l.end), t(&l.var), t(&l.collection), t(&l.fn_sym)]);
                }
            }
            for fn_sym in &f.allocators {
                alloc_rows.push(vec![t(fn_sym)]);
            }
            for ns in &f.nests {
                if seen_nest.insert((ns.call_id.as_str(), ns.loop_id.as_str())) {
                    nest_rows.push(vec![t(&ns.call_id), t(&ns.loop_id), i(ns.depth), t(&ns.collection)]);
                }
            }
            for (id, pos) in &f.param_pos {
                if seen_param.insert(id.as_str()) {
                    param_rows.push(vec![t(id), i(*pos)]);
                }
            }
            for (call, pos, arg) in &f.args {
                if seen_arg.insert((call.as_str(), *pos, arg.as_str())) {
                    arg_rows.push(vec![t(call), Value::Int(*pos), t(arg)]);
                }
            }
            for (id, field, value) in &f.fields {
                if seen_field.insert((id.as_str(), field.as_str(), value.as_str())) {
                    field_rows.push(vec![t(id), t(field), t(value)]);
                }
            }
        }

        self.refresh_rel("df_node", &["id", "kind", "var", "fn", "file", "line"], &node_rows)?;
        self.refresh_rel("df_edge", &["from", "to"], &edge_rows)?;
        self.refresh_rel("loop_over", &["file", "start", "end", "var", "collection", "fn"], &loop_rows)?;
        self.refresh_rel("allocates", &["fn"], &alloc_rows)?;
        self.refresh_rel("nest", &["call_id", "loop_id", "depth", "collection"], &nest_rows)?;
        self.refresh_rel("df_param", &["id", "pos"], &param_rows)?;
        self.refresh_rel("df_arg", &["call", "pos", "arg"], &arg_rows)?;
        self.refresh_rel("df_field", &["id", "field", "value"], &field_rows)?;
        // Persisted only after the writes land, so a failed refresh retries.
        self.save_rel_digest("extract:dataflow", &digest)?;
        Ok(true)
    }

    /// Refresh `doc_node` from the document-grammar registry. Same shape as the
    /// source-lang refreshers but simpler: no corpus-global resolution pass, no
    /// legacy mirror -- each DocNode is self-contained. Files come from `_file`
    /// (a source rule scanning `**/*.md` feeds it, exactly as `**/*.rs` feeds
    /// the type graph); the SQL prefilter narrows to document extensions, then
    /// the registry's `matches` decides the real dispatch.
    /// Change-reporting contract mirrors `refresh_type_rels`. The `doc_ref`
    /// bridge reads `type_entity`, so the input digest folds the md corpus
    /// PLUS the stored `extract:type` digest (which identifies the type
    /// family's inputs; type refresh runs before doc in both tick paths).
    pub(crate) fn refresh_doc_rels(&self) -> Result<bool> {
        let mut files: Vec<ExtractFile> = Vec::new();
        {
            let mut sel = self.db.conn().prepare(
                "SELECT repo, path, rev, hash FROM _file WHERE path LIKE '%.md' OR path LIKE '%.markdown'")?;
            let rows = sel.query_map([], |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?.unwrap_or_default())))?;
            for row in rows.flatten() { files.push(row); }
        }
        let mut digest = self.extract_input_digest("doc", &files, false);
        let ty = self.load_rel_digest("extract:type")?
            .map(|d| d.iter().map(|b| format!("{b:02x}")).collect::<String>())
            .unwrap_or_default();
        for (a, b) in digest.iter_mut()
            .zip(blake3::hash(format!("type\0{ty}").as_bytes()).as_bytes()) { *a ^= *b; }
        if self.load_rel_digest("extract:doc")? == Some(digest) { return Ok(false); }
        let root = self.root.clone();
        let roots = self.repo_roots();
        let facts: Vec<(String, String, ingest::DocFacts)> = files.par_iter().filter_map(|(repo, path, rev, _)| {
            let lang = ingest::ingest_langs().iter().find(|l| l.matches(path))?;
            let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
            let content = read_content(froot, rev, path).unwrap_or_default();
            let rid = repo_id_of(froot, path, repo);
            Some((rid, path.clone(), lang.extract_docs(path, &content)))
        }).collect();
        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut seen: HashSet<(String, String, u32, &str, String)> = HashSet::new();
        for (repo, path, f) in &facts {
            for n in &f.nodes {
                if seen.insert((repo.clone(), path.clone(), n.line, n.kind, n.name.clone())) {
                    rows.push(vec![t(repo), t(path), i(n.line), t(n.kind), t(&n.name), t(&n.parent)]);
                }
            }
        }
        self.refresh_rel("doc_node", &["repo", "file", "line", "kind", "name", "parent"], &rows)?;

        // doc_ref bridge: doc_node -> type_entity, three sources.
        //   (1) heading exact:    doc_node.name == type_entity.name
        //   (2) heading norm:     normalize(doc_node.name) == type_entity.name
        //       (strips leading articles + trailing kind words; lowercase).
        //   (3) code_block text:  identifiers in doc_node.text matched against
        //       type_entity.name (lowercase). The whole block counts as one
        //       doc position (the fence line); dedup collapses repeats.
        //
        // Normalization lives in `normalize_doc_name` (below). type_entity names
        // are already clean identifiers, so the symbol side only lowercases.
        // Empty if the program doesn't use type relations (type_entity table
        // exists but is unpopulated -> empty map -> no rows).
        let type_rows: Vec<(String, String)> = {
            let mut sel = self.db.prepare(
                &format!("SELECT sym, name FROM {}", tbl("type_entity")))?;
            let rows: Vec<(String, String)> = sel
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok()).collect();
            rows
        };
        // Map lowercase type name -> Vec of (sym, original_name) so multiple
        // symbols of the same name all bridge (e.g. two `Engine` in different
        // files).
        let mut by_name: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for (sym, name) in &type_rows {
            by_name.entry(name.to_ascii_lowercase())
                .or_default()
                .push((sym.clone(), name.clone()));
        }
        let mut ref_rows: Vec<Vec<Value>> = Vec::new();
        let mut ref_seen: HashSet<(String, u32, String, &'static str, String)> = HashSet::new();
        let push_ref = |repo: &str, file: &str, line: u32, sym: &str, kind: &'static str,
                        matched: &str, rows: &mut Vec<Vec<Value>>,
                        seen: &mut HashSet<(String, u32, String, &'static str, String)>| {
            if seen.insert((file.to_string(), line, sym.to_string(), kind, matched.to_string())) {
                rows.push(vec![t(repo), t(file), i(line), t(sym), t(kind), t(matched)]);
            }
        };
        for (repo, path, f) in &facts {
            for n in &f.nodes {
                match n.kind {
                    "heading" => {
                        // Try exact name first, then normalized. The original
                        // heading text is recorded as matched_name so a rule
                        // can cross-reference doc_node directly.
                        let exact = by_name.get(&n.name.to_ascii_lowercase());
                        if let Some(hits) = exact {
                            for (sym, _orig) in hits {
                                push_ref(repo, path, n.line, sym, "heading", &n.name,
                                         &mut ref_rows, &mut ref_seen);
                            }
                            continue;
                        }
                        let norm = normalize_doc_name(&n.name);
                        if !norm.is_empty() {
                            if let Some(hits) = by_name.get(&norm) {
                                for (sym, _orig) in hits {
                                    push_ref(repo, path, n.line, sym, "heading", &n.name,
                                             &mut ref_rows, &mut ref_seen);
                                }
                            }
                        }
                    }
                    "code_block" => {
                        // Scan the block body for identifiers that match a type
                        // name. Each unique (sym, token) pair emits one row.
                        for tok in identifiers_in(&n.text) {
                            if let Some(hits) = by_name.get(&tok.to_ascii_lowercase()) {
                                for (sym, _orig) in hits {
                                    push_ref(repo, path, n.line, sym, "code_block", tok,
                                             &mut ref_rows, &mut ref_seen);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        self.refresh_rel("doc_ref",
            &["repo", "file", "line", "sym", "kind", "matched_name"], &ref_rows)?;
        // Persisted only after the writes land, so a failed refresh retries.
        self.save_rel_digest("extract:doc", &digest)?;
        Ok(true)
    }
}
