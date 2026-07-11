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

/// Directory portion of a `/`-separated relative path (empty string for a
/// bare filename, never a trailing slash). Used only by `narrow_ambiguous`'s
/// same-directory criterion.
fn path_dir(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some((dir, _)) => dir,
        None => "",
    }
}

/// Win D, import-scoped ambiguity narrowing. `resolve`/`resolve_callee` in
/// `refresh_type_rels`/`refresh_call_rels` call this only when a name's
/// def bucket already has more than one candidate symbol (the plain
/// unique-in-repo path stays untouched). A candidate survives the filter when
/// its declaring file is:
///   (a) the referencing file itself,
///   (b) directly imported by the referencing file (a `module_edge_rev` row
///       at this rev), or
///   (c) in the same directory as the referencing file.
/// Exactly one survivor resolves the ambiguity only when it survived via (a)
/// or (b) ("strong" reasons); a survivor that only matches via (c) stays
/// bare. Directory co-location is a weak signal (sibling files that never
/// import each other are common), so a same-directory-ONLY tie is honest
/// ambiguity, not a resolution: a wrong join is worse than a missing one.
/// More than one survivor (still genuinely ambiguous after narrowing) or zero
/// survivors also stay bare.
fn narrow_ambiguous<'a>(
    candidates: &[&'a str],
    repo: &str,
    rev: &str,
    referencing_file: &str,
    sym_file: &HashMap<(&str, &str, &str), &str>,
    imports: &HashMap<(String, String), HashSet<String>>,
) -> Option<&'a str> {
    let referencing_dir = path_dir(referencing_file);
    let imported = imports.get(&(rev.to_string(), referencing_file.to_string()));
    let mut survivor: Option<(&'a str, bool)> = None;
    let mut survivor_count = 0u32;
    for sym in candidates {
        let Some(def_file) = sym_file.get(&(repo, rev, *sym)) else { continue };
        let is_self = *def_file == referencing_file;
        let is_imported = imported.map(|set| set.contains(*def_file)).unwrap_or(false);
        let is_same_dir = path_dir(def_file) == referencing_dir;
        if is_self || is_imported || is_same_dir {
            survivor_count += 1;
            survivor = Some((sym, is_self || is_imported));
        }
    }
    match (survivor_count, survivor) {
        (1, Some((sym, true))) => Some(sym),
        _ => None,
    }
}

/// Occurrence-level SCIP resolution index (position-before-name). The name-level
/// override (`scip_name_defs`) keys a def only by (repo, file, bare descriptor
/// name), so a name carried by two DIFFERENT def symbols in one file is dropped
/// (the conflict refusal, commit 9fd029b) and every shared name (`build`/`new`/
/// `shutdown` — most of real trait-heavy code) resolves bare even though the
/// index holds the exact symbol at every span. `scip_occurrence` carries a
/// 0-based per-occurrence line for each symbol; joined to the def location, a
/// call site's (file, line) picks the ONE symbol occurring there, disambiguating
/// what the bare name cannot — the conflict refusal becomes moot wherever a
/// position exists.
///
/// Built once per call-family refresh (collect-then-index, no per-site SQL —
/// same posture as `scip_name_defs`). Empty when no index is loaded, so the
/// name path carries unchanged. Repo-scoped throughout (cross-repo SCIP
/// resolution was deliberately removed, the D3 fix); occurrences are consulted
/// only at rev == "WORK" by the caller, since a SCIP index is a working-tree
/// artifact.
#[derive(Default)]
struct ScipOccIndex {
    /// (repo, file, 0-based line) -> the symbols occurring on that line.
    occ_at: HashMap<(String, String, i64), Vec<String>>,
    /// (repo, symbol) -> its definition file (from `scip_def`, the authoritative
    /// def location the resolver joins into `sym_at`, exactly like the name
    /// map's `def_file`).
    def_file_of: HashMap<(String, String), String>,
    /// symbol -> trailing descriptor name (the as-written call text a plain or
    /// method call carries). Cached so a lookup never recomputes the moniker
    /// parse.
    desc_name: HashMap<String, String>,
    /// (repo, file, symbol) -> the LOCAL binding names an aliased import gives
    /// the symbol in that file (`import { a as b }` -> {"b"}), from
    /// `scip_binding`. A call written with the alias matches here even though the
    /// descriptor name is the canonical `a`.
    binding_names: HashMap<(String, String, String), HashSet<String>>,
}

/// The outcome of an occurrence-level lookup at one call site.
enum OccPick {
    /// Exactly one symbol occurs at this (file, line) under the call's
    /// as-written name: resolved to its def file.
    Resolved(String),
    /// More than one DISTINCT symbol shares the call's name on this line: the
    /// position can't tell them apart, so refuse (honest bare). Never falls
    /// through to the name map — that would let a coincidental single-def name
    /// resolve a site the position just refuted.
    Refuse,
    /// No occurrence on this line carries the call's name (an unindexed file, or
    /// a site the compiler never recorded): defer to the name-level map.
    Fallthrough,
}

impl ScipOccIndex {
    /// True when `callee` (the as-written call text) addresses `symbol` in
    /// `file`: it equals the symbol's descriptor name, or a local alias the file
    /// binds it to.
    fn names_match(&self, repo: &str, file: &str, symbol: &str, callee: &str) -> bool {
        if self.desc_name.get(symbol).map(String::as_str) == Some(callee) {
            return true;
        }
        self.binding_names
            .get(&(repo.to_string(), file.to_string(), symbol.to_string()))
            .is_some_and(|set| set.contains(callee))
    }

    /// Resolve a call site by position. `line1` is the call site's 1-based line
    /// (`call_site` lines are 1-based across all fronts); the SINGLE conversion
    /// to SCIP's 0-based occurrence line happens right here.
    fn resolve(&self, repo: &str, file: &str, callee: &str, line1: u32) -> OccPick {
        let line0 = line1 as i64 - 1; // 1-based call site -> 0-based scip occurrence.
        let Some(syms) = self.occ_at.get(&(repo.to_string(), file.to_string(), line0)) else {
            return OccPick::Fallthrough;
        };
        let mut matched: HashSet<&str> = HashSet::new();
        for sym in syms {
            if self.names_match(repo, file, sym, callee) {
                matched.insert(sym.as_str());
            }
        }
        match matched.len() {
            0 => OccPick::Fallthrough,
            1 => {
                let sym = *matched.iter().next().unwrap();
                match self.def_file_of.get(&(repo.to_string(), sym.to_string())) {
                    Some(def) => OccPick::Resolved(def.clone()),
                    // The one matching symbol has no in-index def (its definition
                    // is outside the indexed set): the name map can't do better,
                    // so defer rather than refuse.
                    None => OccPick::Fallthrough,
                }
            }
            _ => OccPick::Refuse,
        }
    }
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
        // Loud-skip house pattern: `sys.path` mutation is runtime state a
        // syntactic resolver can't simulate; count it and say so once per
        // refresh rather than silently leaving affected imports unresolved.
        let sys_path_mutators = modgraph::count_sys_path_mutators(&fileset, &reader);
        if sys_path_mutators > 0 {
            eprintln!(
                "[modgraph:py] {sys_path_mutators} file(s) mutate sys.path at runtime ({rev}); imports they enable may show unresolved"
            );
        }
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
                    // Alias bindings only mean anything against a resolved, non-self
                    // target file (same self-edge exclusion as `edges_rev` below);
                    // borrow before the ownership match moves `mref.target`.
                    if let Resolution::File(dst) = &mref.target {
                        if dst != path {
                            for (local, source) in &mref.bindings {
                                rows.bindings.push(vec![t(path), t(local), t(source), t(dst), t(rev)]);
                            }
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
        self.db.insert_rows(&tbl("module_binding_rev"), &["file", "local", "source", "dst", "rev"], &rows.bindings)?;
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
        let binding = tbl("module_binding");
        let binding_rev = tbl("module_binding_rev");
        self.db.exec(&format!("DELETE FROM {binding}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {binding} (\"file\", \"local\", \"source\", \"dst\") \
             SELECT \"file\", \"local\", \"source\", \"dst\" FROM {binding_rev}"
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
    pub(crate) fn refresh_module_rels(&self) -> Result<bool> {
        let by_rev = self.module_files_by_rev()?;
        // Perf gap A twin: skip the wholesale rebuild when no rev's input digest
        // moved. Module output is a pure function of file content AND manifests
        // (Cargo.toml/package.json), which reconcile does NOT track — so this
        // self-digest (files + manifests) is the only sound skip signal. Returns
        // false (no changed mark) when nothing moved; the tick then leaves the
        // module rels and their dependents untouched, instead of the old
        // unconditional `Ok(true)` that forced every dependent to re-derive on
        // every tick the family was merely `used`.
        let mut moved: Vec<(String, [u8; 32])> = Vec::new();
        for (rev, files) in &by_rev {
            let d = self.module_input_digest(rev, files);
            if self.load_rel_digest(&format!("extract:module:{rev}"))? == Some(d) { continue; }
            moved.push((rev.clone(), d));
        }
        if moved.is_empty() { return Ok(false); }
        let mut rows = ModuleRows::default();
        for (rev, files) in &by_rev {
            rows.extend(self.module_rows_for_rev(rev, files, None, true));
        }
        self.refresh_rel("module_import", &["file", "rev", "specifier", "kind", "line"], &rows.imports)?;
        self.refresh_rel("module_edge_rev", &["src", "dst", "rev"], &rows.edges_rev)?;
        self.refresh_rel("module_unresolved_rev", &["file", "rev", "specifier", "reason", "line"], &rows.unresolved_rev)?;
        self.refresh_rel("crate_edge", &["src", "dst", "kind", "rev"], &rows.crate_edges)?;
        self.refresh_rel("module_binding_rev", &["file", "local", "source", "dst", "rev"], &rows.bindings)?;
        self.insert_module_spans(&rows)?;
        self.rebuild_legacy_module_rels()?;
        for (rev, d) in &moved {
            self.save_rel_digest(&format!("extract:module:{rev}"), d)?;
        }
        Ok(true)
    }

    /// Per-rev module input digest: XOR-folds the rev's (path, hash) file set
    /// plus every manifest `collect_manifests` resolves for it. Mirrors
    /// `extract_input_digest` but folds manifests (module's Cargo.toml/
    /// package.json input that reconcile does not see).
    fn module_input_digest(&self, rev: &str, files: &[(String, String)]) -> [u8; 32] {
        let mut acc = [0u8; 32];
        let fold = |acc: &mut [u8; 32], h: &blake3::Hash| {
            for (a, b) in acc.iter_mut().zip(h.as_bytes()) { *a ^= b; }
        };
        fold(&mut acc, &blake3::hash(format!("module\0{rev}\0{:032x}", exe_stamp()).as_bytes()));
        for (path, hash) in files {
            fold(&mut acc, &blake3::hash(format!("{path}\0{hash}").as_bytes()));
        }
        let fileset: HashSet<String> = files.iter().map(|(p, _)| p.clone()).collect();
        for (mrel, content) in self.collect_manifests(rev, &fileset) {
            fold(&mut acc, &blake3::hash(format!("manifest\0{mrel}\0{content}").as_bytes()));
        }
        acc
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
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _module_refresh_rev)", tbl("module_binding_rev")))?;

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
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = '{rev}' AND \"file\" IN (SELECT path FROM _module_refresh_path)",
            tbl("module_binding_rev"),
        ))?;

        let by_rev = self.module_files_by_rev()?;
        let rows = by_rev.get(rev)
            .map(|files| self.module_rows_for_rev(rev, files, Some(paths), false))
            .unwrap_or_default();
        self.insert_module_rows(&rows, false)?;
        self.rebuild_legacy_module_rels()?;
        Ok(())
    }

    // LANG-JUNCTION(extract-file-set): the extension LIKE list gating which files reach TypeLang extraction; a new TypeLang's extensions must be added to this SQL too or its extractor never sees a file
    /// The extraction corpus: every `_file` row in a TypeLang extension, with
    /// its content address. One query serves the type/call/dataflow refreshers;
    /// the hash column is what makes both the whole-pass digest skip and the
    /// per-file fact cache content-keyed (perf gap A). The four plain-JS
    /// extensions ride `TsTypes` alongside `.ts`/`.tsx` (Win H); `.go` rides
    /// `GoTypes`.
    fn extract_file_set(&self) -> Result<Vec<ExtractFile>> {
        let mut files: Vec<ExtractFile> = Vec::new();
        let mut sel = self.db.conn().prepare(
            "SELECT repo, path, rev, hash FROM _file WHERE path LIKE '%.rs' OR path LIKE '%.kt' OR path LIKE '%.kts' \
             OR path LIKE '%.ts' OR path LIKE '%.tsx' \
             OR path LIKE '%.js' OR path LIKE '%.jsx' OR path LIKE '%.mjs' OR path LIKE '%.cjs' \
             OR path LIKE '%.go' OR path LIKE '%.py'")?;
        let rows = sel.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?.unwrap_or_default())))?;
        for row in rows.flatten() { files.push(row); }
        Ok(files)
    }

    /// XOR-folded input digest for one extractor family AT ONE REV: one blake3
    /// per (repo, path, rev, content hash) corpus row for that rev's file subset,
    /// plus the `scip_ref` override table (WORK only — SCIP indexes are
    /// working-tree artifacts, so a committed rev's resolution can't move when
    /// the index changes), plus (when `with_scip`) the `module_edge_rev` rows at
    /// this rev, plus the running binary's identity (see `exe_stamp`).
    /// `with_scip` really means "this family's resolver reads outside inputs
    /// beyond the corpus itself": both the SCIP override and the Win D
    /// import-scoped ambiguity narrowing feed the SAME `resolve`/`resolve_callee`
    /// closures in `refresh_type_rels`/`refresh_call_rels`, so one flag gates
    /// both folds; `dataflow`/`comment` pass `false` because neither family
    /// resolves names. Unlike the SCIP fold, the module-edge fold is NOT
    /// restricted to `rev == "WORK"`: `module_edge_rev` is rev-aware (a
    /// committed rev has its own import graph), and the narrowing reads
    /// exactly that rev's edges, so a committed rev's digest must move when
    /// ITS import rows change too.
    /// Persisted under `extract:<family>:<rev>` in `_reldigest`; an unchanged
    /// digest means that rev's output rows are already in this db, so the parse +
    /// resolve + write pass skips for that rev. A row with an empty content hash
    /// has no identity, so the digest is salted with the current time — never
    /// equal, never a false skip. `files` is already filtered to `rev`.
    fn extract_input_digest(&self, family: &str, rev: &str, files: &[ExtractFile], with_scip: bool) -> [u8; 32] {
        let mut acc = [0u8; 32];
        let fold = |acc: &mut [u8; 32], h: &blake3::Hash| {
            for (a, b) in acc.iter_mut().zip(h.as_bytes()) { *a ^= *b; }
        };
        fold(&mut acc, &blake3::hash(format!("{family}\0{rev}\0{:032x}", exe_stamp()).as_bytes()));
        for (repo, path, frev, hash) in files {
            if hash.is_empty() {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos()).unwrap_or(0);
                fold(&mut acc, &blake3::hash(format!("nonce\0{now}").as_bytes()));
                continue;
            }
            fold(&mut acc, &blake3::hash(format!("{repo}\0{path}\0{frev}\0{hash}").as_bytes()));
        }
        // SCIP only attributes the WORK rev's resolution (index = working tree),
        // so a committed rev's digest ignores it: folding scip into every rev's
        // digest would re-tick untouched committed revs whenever the index moves.
        if with_scip && rev == "WORK" {
            // Include the origin `repo`: two roots of the same crate emit
            // byte-identical (file, symbol, def_file) triples, so folding without
            // repo would XOR-cancel the second root's rows and leave the digest
            // unchanged when a wanted repo's index is added — a false skip that
            // strands the second repo's entities on syntactic-only resolution.
            if let Ok(mut s) = self.db.conn().prepare(
                &format!("SELECT file, symbol, def_file, repo FROM {}", tbl("scip_ref"))) {
                if let Ok(rows) = s.query_map([], |r| Ok((
                    r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))) {
                    for (f, sym, d, repo) in rows.flatten() {
                        fold(&mut acc, &blake3::hash(format!("scip\0{f}\0{sym}\0{d}\0{repo}").as_bytes()));
                    }
                }
            }
        }
        // Win D: the import-scoped ambiguity narrowing reads `module_edge_rev`
        // at this rev, so a moved import (an added/removed/retargeted `use`)
        // must flip this digest or a stale resolution would survive a warm
        // tick. Folded for every rev, not just WORK; see the doc comment above.
        if with_scip {
            if let Ok(mut s) = self.db.conn().prepare(
                &format!("SELECT src, dst FROM {} WHERE \"rev\" = ?1", tbl("module_edge_rev"))) {
                if let Ok(rows) = s.query_map([rev], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) {
                    for (src, dst) in rows.flatten() {
                        fold(&mut acc, &blake3::hash(format!("module\0{src}\0{dst}").as_bytes()));
                    }
                }
            }
            // The alias hop (see `module_binding_map`) reads `module_binding_rev`
            // at this rev, same reasoning: an edited alias must flip this digest
            // or the warm-tick skip would keep serving the stale resolution.
            if let Ok(mut s) = self.db.conn().prepare(
                &format!("SELECT file, local, source, dst FROM {} WHERE \"rev\" = ?1", tbl("module_binding_rev"))) {
                if let Ok(rows) = s.query_map([rev], |r| Ok((
                    r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))) {
                    for (file, local, source, dst) in rows.flatten() {
                        fold(&mut acc, &blake3::hash(format!("binding\0{file}\0{local}\0{source}\0{dst}").as_bytes()));
                    }
                }
            }
        }
        acc
    }

    /// Group the extraction corpus by rev and return the revs whose per-rev
    /// input digest moved since the last tick, each paired with its fresh
    /// digest. An empty result means every rev is unchanged, so the family can
    /// skip its whole pass (the per-rev twin of perf gap A's warm-tick skip).
    /// `with_scip` requests the scip fold, which `extract_input_digest` applies
    /// only to the WORK rev.
    fn moved_extract_revs(&self, family: &str, files: &[ExtractFile], with_scip: bool)
        -> Result<Vec<(String, [u8; 32])>>
    {
        let mut by_rev: HashMap<&str, Vec<ExtractFile>> = HashMap::new();
        for f in files { by_rev.entry(f.2.as_str()).or_default().push(f.clone()); }
        let mut moved: Vec<(String, [u8; 32])> = Vec::new();
        for (rev, frev) in &by_rev {
            let d = self.extract_input_digest(family, rev, frev, with_scip);
            if self.load_rel_digest(&format!("extract:{family}:{rev}"))? == Some(d) { continue; }
            moved.push(((*rev).to_string(), d));
        }
        Ok(moved)
    }

    /// Fold a rev into a df node id so two revs' `file:line:col` ids stay
    /// disjoint in one `_rev` table. Readable composition (not a hash) so the raw
    /// id is recoverable by eye and queries stay debuggable; the U+0001 separator
    /// never occurs in a rev or a df id. Deterministic, so any two df columns that
    /// join on node identity within one rev salt identically and still line up.
    fn salt_rev(id: &str, rev: &str) -> String {
        format!("{rev}\u{1}{id}")
    }

    /// Distinct revs present in the extraction corpus (the delete scope for a
    /// whole-corpus twin refresh — see `refresh_rel_for_revs` call sites).
    fn corpus_revs(files: &[ExtractFile]) -> Vec<String> {
        let mut revs: Vec<String> = files.iter().map(|f| f.2.clone()).collect();
        revs.sort();
        revs.dedup();
        revs
    }

    /// Rev-scoped twin write: wipe only the named revs' rows, then insert the
    /// fresh set in one batch. Generalizes `refresh_module_rels_for_revs`'s
    /// DELETE-by-rev pattern to any `_rev` twin. Collect-then-flush, one
    /// `insert_rows` (the tick counter screams on a per-row write).
    fn refresh_rel_for_revs(&self, rel: &str, cols: &[&str], rows: &[Vec<Value>], revs: &[&str]) -> Result<()> {
        if revs.is_empty() { return Ok(()); }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _rel_refresh_rev(rev TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _rel_refresh_rev")?;
        let rev_rows: Vec<Vec<Value>> = revs.iter().map(|rev| vec![Value::Text((*rev).to_string())]).collect();
        self.db.insert_rows("_rel_refresh_rev", &["rev"], &rev_rows)?;
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT rev FROM _rel_refresh_rev)", tbl(rel)))?;
        self.db.insert_rows(&tbl(rel), cols, rows)?;
        Ok(())
    }

    /// Every `_rev` twin an extraction family writes: type (node/link/edge),
    /// call (def/edge), df (node/node_repo/arg/field), and the module family's
    /// own pair. One shared list so `sweep_gone_revs` and any future twin
    /// addition touch a single place.
    const REV_TWINS: &[&str] = &[
        "type_entity_rev", "type_link_rev", "type_edge_rev",
        "call_def_rev", "call_edge_rev",
        "df_node_rev", "df_node_repo_rev", "df_arg_rev", "df_field_rev",
        "module_edge_rev", "module_unresolved_rev", "module_binding_rev",
    ];

    /// D5.5 — the rev-retraction sweep (plan Layer 4, "Retraction"). A rev
    /// that stops being scanned (a `scan` rule dropped it, or its file
    /// vanished from `_file` entirely) leaves its `_rev` twin rows and
    /// `extract:<family>:<rev>` digest key stranded forever: `moved_extract_
    /// revs` only ever iterates revs still present in `_file`, so a gone rev
    /// can never be "moved" and never gets deleted by the family's own write
    /// path. One DELETE per twin table (rev NOT IN the live set) and one
    /// DELETE per digest-key family prefix — the set diff is SQLite's job,
    /// never a per-rev Rust loop.
    ///
    /// The legacy rebuilds run here too, unconditionally, rather than being
    /// left to each family's own `rebuild_legacy_*` call: those live at the
    /// END of `refresh_type_rels`/`refresh_call_rels`, gated behind the same
    /// per-rev digest early return (`moved.is_empty()`) that skips the whole
    /// family when every currently-live rev's digest is unchanged. A rev that
    /// just disappeared moves nothing in that check, so a family with no
    /// other reason to run this tick would never reach its internal legacy
    /// rebuild — the gone rev's rows would vanish from the twin but linger in
    /// legacy for another tick or more. Rebuilding legacy directly here is
    /// what makes a gone rev disappear from twin AND legacy within the SAME
    /// tick, independent of whether any family's digest moved. `module` now has
    /// its own digest gate (`refresh_module_rels`), so its `extract:module:<rev>`
    /// digest is swept here like the rest — without that, a gone rev's surviving
    /// digest would make the next tick skip repopulating the wiped module rels.
    ///
    /// Called once per tick, right after `refresh_builtin_rels` settles this
    /// tick's `_file` set and before the extraction families run — see the
    /// call sites in `tick.rs`.
    pub(crate) fn sweep_gone_revs(&self) -> Result<()> {
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _live_rev_scope(rev TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _live_rev_scope")?;
        self.db.exec("INSERT OR IGNORE INTO _live_rev_scope SELECT DISTINCT rev FROM _file")?;

        for twin in Self::REV_TWINS {
            self.db.exec(&format!(
                "DELETE FROM {} WHERE \"rev\" NOT IN (SELECT rev FROM _live_rev_scope)",
                tbl(twin),
            ))?;
        }

        // `extract:<family>:<rev>` digest rows, one family prefix at a time
        // (the families whose skip-check is keyed per rev: module/type/call/
        // dataflow/doc/comment — see `moved_extract_revs` and
        // `refresh_module_rels`'s digest gate). A rev that disappeared MUST take
        // its digest with it: the digest certifies "I built and STORED this
        // rev's outputs," and the twin DELETE above just wiped those outputs, so
        // a surviving digest would make the next tick's gate skip repopulating
        // them (the lint-imports/diag_mute regression).
        for family in ["module", "type", "call", "dataflow", "doc", "comment"] {
            let prefix = format!("extract:{family}:");
            self.db.exec(&format!(
                "DELETE FROM _reldigest WHERE rel LIKE '{prefix}%' \
                 AND substr(rel, {}) NOT IN (SELECT rev FROM _live_rev_scope)",
                prefix.chars().count() + 1,
            ))?;
        }

        self.rebuild_legacy_type_rels()?;
        self.rebuild_legacy_call_rels()?;
        self.rebuild_legacy_module_rels()?;
        Ok(())
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
        // Perf gap A, per rev: skip any rev whose file subset (and, for WORK, the
        // scip override) didn't move — its rows already serve. An empty `moved`
        // means the whole family skips. When ANY rev moved, the emit below stays
        // whole-corpus (per-rev emission scoping is D5.2+; the per-file fact
        // cache keeps re-emit cheap).
        let moved = self.moved_extract_revs("type", &files, true)?;
        if moved.is_empty() { return Ok(false); }
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
        // in the SAME repo AT THE SAME REV declares it (syntactic). Keying by
        // (repo, rev) keeps two folders in view — and two revs of one folder —
        // that share a name from making each other ambiguous, and the resolved
        // sym is repo-qualified (`{repo}::{sym}`) so the edge relations
        // (type_link/type_sig — no repo column) stay distinct across
        // identical-path repos. A SCIP index, when present, overrides per
        // (repo, file, name) with the indexed def file (collision-proof) — but
        // only at rev == WORK, since a SCIP index is a working-tree artifact
        // (D5.6); committed revs resolve syntactically.
        let mut by_name: HashMap<(&str, &str, &str), Vec<&str>> = HashMap::new();
        let mut sym_at: HashMap<(&str, &str, &str, &str), &str> = HashMap::new();
        // (repo, rev, sym) -> declaring file, the reverse of `sym_at`'s (repo,
        // file, rev, name) key. Feeds `narrow_ambiguous`'s same-file/same-
        // directory checks when a bucket has more than one candidate (Win D).
        let mut sym_file: HashMap<(&str, &str, &str), &str> = HashMap::new();
        for (repo, _, rev, f) in &facts {
            for e in &f.entities {
                // Dedup the ambiguity bucket by def sym: the same physical file
                // can be scanned under two slugs that collapse to one rid (a
                // config repo pointing at the self root, or two worktrees sharing
                // a `.git` basename), so an entity declared ONCE would otherwise
                // be pushed twice and read as ambiguous. Distinct syms (two real
                // defs of one name) still stack -> len 2 -> unresolved.
                let bucket = by_name.entry((repo.as_str(), rev.as_str(), e.name.as_str())).or_default();
                if !bucket.iter().any(|s| *s == e.sym.as_str()) {
                    bucket.push(e.sym.as_str());
                }
                sym_at.insert((repo.as_str(), e.file.as_str(), rev.as_str(), e.name.as_str()), e.sym.as_str());
                sym_file.insert((repo.as_str(), rev.as_str(), e.sym.as_str()), e.file.as_str());
            }
        }
        let scip = self.scip_name_defs().unwrap_or_default();
        // NOTE: occurrence-level (position-before-name) resolution is NOT wired
        // here, only in `refresh_call_rels`. A type reference — a `TypeEdge`, a
        // `type_sig` slot, an `impl` owner name — carries no source position
        // (`TypeEdge` has only from/to/kind), so there is no (file, line) to look
        // an occurrence up by. The name-level `scip` map is the only override the
        // type graph can consult until the extractor threads per-reference spans.
        // Win D: the referencing file's own imports, read once for the whole
        // family (never per lookup), see `module_import_map`.
        let imports = self.module_import_map().unwrap_or_default();
        // Alias hop input: this file's aliased-import local bindings, read
        // once for the whole family, see `module_binding_map`.
        let aliases = self.module_binding_map().unwrap_or_default();
        let resolve = |repo: &str, rev: &str, file: &str, name: &str| -> Option<String> {
            if rev == "WORK" {
                if let Some(def_file) = scip.get(&(repo.to_string(), file.to_string(), name.to_string())) {
                    if let Some(sym) = sym_at.get(&(repo, def_file.as_str(), rev, name)) {
                        return Some(format!("{repo}::{sym}"));
                    }
                }
            }
            // Index-free alias hop: an aliased import (`use x::y as z`, TS
            // `import { a as b }`, Kotlin `import a.b.C as D`) has no by_name
            // bucket for the local name `z`/`b`/`D` — the def is keyed by its
            // real name. A local def of the SAME name shadows the import (a
            // local declaration always wins), so the hop only fires when this
            // file declares no such name itself. A hit resolves straight to
            // the aliased target's def, pinned by dst; a miss (barrel
            // re-export, unresolved default) returns None WITHOUT falling
            // through to by_name — a coincidental global match on the alias
            // name elsewhere would be a wrong join, honest bare wins.
            if sym_at.get(&(repo, file, rev, name)).is_none() {
                if let Some((source, dst)) = aliases.get(&(rev.to_string(), file.to_string())).and_then(|m| m.get(name)) {
                    return sym_at.get(&(repo, dst.as_str(), rev, source.as_str()))
                        .map(|sym| format!("{repo}::{sym}"));
                }
            }
            match by_name.get(&(repo, rev, name)) {
                Some(v) if v.len() == 1 => Some(format!("{repo}::{}", v[0])),
                // More than one candidate: narrow to the referencing file's own
                // import neighborhood (Win D) before giving up bare.
                Some(v) if v.len() > 1 =>
                    narrow_ambiguous(v, repo, rev, file, &sym_file, &imports)
                        .map(|sym| format!("{repo}::{sym}")),
                _ => None,
            }
        };

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut edge_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut entity_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut sig_rows: Vec<Vec<Value>> = Vec::new();
        let mut link_rev_rows: Vec<Vec<Value>> = Vec::new();
        // Dedup keys carry the repo AND the rev, so two folders in view that
        // share a relative path + symbol name (e.g. both have `src/index.ts`) do
        // NOT drop each other's rows, and one file present at two revs emits its
        // entity/link once PER rev — rev is a column, not folded into the sym.
        let mut seen_entity: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_link: HashSet<(String, String, &str, &str)> = HashSet::new();
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
                let src = sym_at.get(&(repo.as_str(), path.as_str(), rev.as_str(), edge.from.as_str()))
                    .map(|s| format!("{repo}::{s}")).unwrap_or_else(|| edge.from.clone());
                let dst = resolve(repo, rev, path, &edge.to).unwrap_or_else(|| edge.to.clone());
                if seen_link.insert((src.clone(), dst.clone(), edge.kind, rev.as_str())) {
                    link_rev_rows.push(vec![t(&src), t(&dst), t(edge.kind), t(rev)]);
                }
            }
            for ent in &f.entities {
                // repo-qualified sym: globally unique even when two repos share a
                // relative path, so sym-keyed rels (type_sig/type_link) and the
                // cross-rel joins to call_def stay per-repo distinct.
                let qsym = format!("{repo}::{}", ent.sym);
                if seen_entity.insert((repo.as_str(), ent.sym.as_str(), rev.as_str())) {
                    // Method-owner key. The extractor mints `parent` file-scoped
                    // (`<method-file>::<kind>::<Owner>`), which joins the owner's
                    // entity sym only when the owner is declared in the SAME file
                    // (yesterday's per-file owner-kind fix). A Rust `impl Owner`
                    // in a different file than `struct Owner` dangles: the minted
                    // key names the impl file, the owner entity names the decl
                    // file. When the file-scoped key has no matching same-file
                    // entity, resolve the owner NAME through the same in-repo
                    // bucket machinery as type_link/call_edge dst syms — a unique
                    // in-repo def rewrites the parent to the declaring-file sym
                    // (repo-qualified, carrying the owner's real kind by
                    // construction); ambiguous/external names stay file-scoped
                    // (dangling is honest, a wrong join is not). Same-file parents
                    // are never rewritten (resolve would return the same sym).
                    let qparent = ent.parent.as_deref().map(|p| {
                        let owner_name = p.rsplit("::").next().unwrap_or(p);
                        let same_file =
                            sym_at.get(&(repo.as_str(), ent.file.as_str(), rev.as_str(), owner_name)) == Some(&p);
                        if same_file {
                            format!("{repo}::{p}")
                        } else {
                            resolve(repo, rev, &ent.file, owner_name)
                                .unwrap_or_else(|| format!("{repo}::{p}"))
                        }
                    }).unwrap_or_default();
                    entity_rev_rows.push(vec![
                        t(repo), t(&qsym), t(&ent.name), t(ent.kind.tag()),
                        t(&qparent), t(&ent.file), i(ent.line), t(rev),
                    ]);
                }
                // the arrow [...A] => B, one row per referenced type per slot
                if let Some(ty) = &ent.ty {
                    for (pos, slot) in ty.params.iter().enumerate() {
                        for r in slot {
                            let rf = resolve(repo, rev, path, r.name()).unwrap_or_else(|| r.name().to_string());
                            sig_rows.push(vec![t(&qsym), t("param"), i(pos as u32), t(&rf)]);
                        }
                    }
                    for r in &ty.ret {
                        let rf = resolve(repo, rev, path, r.name()).unwrap_or_else(|| r.name().to_string());
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
        // type_edge_rev is the rev-carrying twin: write it through the rev-scoped
        // helper (the real in-tree consumer). Delete scope = every corpus rev;
        // the emit above is whole-corpus in D5.1, so wiping all corpus revs and
        // reinserting all rows is equivalent to a full `refresh_rel` wipe (a rev
        // absent from the corpus is D5.5's retraction sweep, not this path).
        let all_revs = Self::corpus_revs(&files);
        let all_rev_refs: Vec<&str> = all_revs.iter().map(|s| s.as_str()).collect();
        self.refresh_rel_for_revs("type_edge_rev", &["from", "to", "kind", "rev", "repo"], &edge_rev_rows, &all_rev_refs)?;
        self.refresh_rel_for_revs("type_entity_rev", &["repo", "sym", "name", "kind", "parent", "file", "line", "rev"], &entity_rev_rows, &all_rev_refs)?;
        self.refresh_rel("type_sig", &["sym", "slot", "pos", "ref"], &sig_rows)?;
        self.refresh_rel_for_revs("type_link_rev", &["src", "dst", "kind", "rev"], &link_rev_rows, &all_rev_refs)?;
        self.refresh_rel("doc_comment", &["repo", "sym", "line", "text"], &doc_rows)?;
        self.refresh_rel("doc_tag", &["repo", "sym", "tag", "arg", "text"], &tag_rows)?;
        self.rebuild_legacy_type_rels()?;
        // Persisted only after the writes land, so a failed refresh retries.
        for (rev, d) in &moved { self.save_rel_digest(&format!("extract:type:{rev}"), d)?; }
        Ok(true)
    }

    /// Best-effort SCIP override for resolution: read `scip_ref(file, symbol,
    /// def_file, repo)` and key it by (repo, file, trailing-descriptor-name) ->
    /// def_file. Keying on the origin repo scopes the override to the ref site's
    /// own index — a head-repo ref never resolves to a base-repo def when two
    /// roots of the same crate share (file, name). Empty when no index.scip is
    /// present, so the syntactic path carries.
    ///
    /// A name referenced from one file under TWO different def symbols (a
    /// caller using both `Resource::build` and `LoggerProvider::build`) is
    /// dropped from the map: the override keys on the bare name, so it cannot
    /// tell the call sites apart and must not guess — a plain insert was
    /// last-write-wins and mis-resolved 412 builder-pattern sites on the
    /// otel-rust corpus (precision 0.995 -> 0.819). Dropped names fall back to
    /// the syntactic path, which refuses ambiguity on its own terms.
    fn scip_name_defs(&self) -> Result<HashMap<(String, String, String), String>> {
        let mut seen: HashMap<(String, String, String), Option<String>> = HashMap::new();
        let conn = self.db.conn();
        let Ok(mut s) = conn.prepare(&format!("SELECT file, symbol, def_file, repo FROM {}", tbl("scip_ref"))) else {
            return Ok(HashMap::new());
        };
        let rows = s.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
        })?;
        for row in rows.flatten() {
            let (file, symbol, def_file, repo) = row;
            if let Some(name) = scip_descriptor_name(&symbol) {
                match seen.entry((repo, file, name)) {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        if e.get().as_deref() != Some(def_file.as_str()) {
                            *e.get_mut() = None;
                        }
                    }
                    std::collections::hash_map::Entry::Vacant(v) => {
                        v.insert(Some(def_file));
                    }
                }
            }
        }
        Ok(seen.into_iter().filter_map(|(key, def)| def.map(|d| (key, d))).collect())
    }

    /// Occurrence-level SCIP resolution input, built once per call-family
    /// refresh (see `ScipOccIndex`). `scip_def` gives each symbol's def file,
    /// `scip_occurrence` the per-line symbol positions, `scip_binding` the local
    /// aliases. One pass per table (collect-then-index, no per-site SQL). All
    /// reads via `tbl(...)` so the magic-rel audit stays green; a missing table
    /// yields an empty index, which makes every lookup a `Fallthrough` and the
    /// name path carry unchanged.
    fn scip_occ_index(&self) -> Result<ScipOccIndex> {
        let mut idx = ScipOccIndex::default();
        let conn = self.db.conn();
        // Definition file per symbol (the def sites the resolver joins into
        // sym_at). Absent scip tables => empty index, name path carries.
        {
            let Ok(mut s) = conn.prepare(&format!("SELECT symbol, file, repo FROM {}", tbl("scip_def"))) else {
                return Ok(idx);
            };
            let rows = s.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
            })?;
            for row in rows.flatten() {
                let (symbol, file, repo) = row;
                idx.def_file_of.entry((repo, symbol)).or_insert(file);
            }
        }
        // Occurrences: (repo, file, 0-based line) -> symbols; cache descriptor
        // names as we go (the moniker parse is repo-independent).
        if let Ok(mut s) = conn.prepare(&format!("SELECT file, symbol, line, repo FROM {}", tbl("scip_occurrence"))) {
            let rows = s.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?))
            })?;
            for row in rows.flatten() {
                let (file, symbol, line, repo) = row;
                if !idx.desc_name.contains_key(&symbol) {
                    if let Some(name) = scip_descriptor_name(&symbol) {
                        idx.desc_name.insert(symbol.clone(), name);
                    }
                }
                idx.occ_at.entry((repo, file, line)).or_default().push(symbol);
            }
        }
        // Aliased-import local bindings: (repo, file, symbol) -> {local name}.
        if let Ok(mut s) = conn.prepare(&format!("SELECT file, symbol, local_name, repo FROM {}", tbl("scip_binding"))) {
            let rows = s.query_map([], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?))
            })?;
            for row in rows.flatten() {
                let (file, symbol, local, repo) = row;
                idx.binding_names.entry((repo, file, symbol)).or_default().insert(local);
            }
        }
        Ok(idx)
    }

    /// Win D input: `module_edge_rev(src, dst, rev)` read once per family
    /// refresh (never per lookup, same shape as `scip_name_defs`) into
    /// (rev, importing file) -> {imported files}, for the resolver's
    /// import-scoped ambiguity narrowing (see `narrow_ambiguous`). Empty when
    /// the module graph hasn't populated yet, so narrowing is simply a no-op
    /// (every candidate fails the filter, stays bare, same as today).
    /// NOTE: `module_edge_rev` carries no `repo` column (a pre-existing gap:
    /// the module graph itself is not yet repo-scoped), so this map is a
    /// flat path->paths join; two repos sharing a relative path could in
    /// theory cross-pollinate here. Same residual as the rest of the
    /// module-graph/repo-scoping gap noted in CLAUDE.md.
    fn module_import_map(&self) -> Result<HashMap<(String, String), HashSet<String>>> {
        let mut out: HashMap<(String, String), HashSet<String>> = HashMap::new();
        let conn = self.db.conn();
        let Ok(mut s) = conn.prepare(&format!("SELECT src, dst, \"rev\" FROM {}", tbl("module_edge_rev"))) else {
            return Ok(out);
        };
        let rows = s.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        for row in rows.flatten() {
            let (src, dst, rev) = row;
            out.entry((rev, src)).or_default().insert(dst);
        }
        Ok(out)
    }

    /// Alias-hop input: `module_binding_rev(file, local, source, dst, rev)`
    /// read once per family refresh (never per lookup, same shape as
    /// `module_import_map`) into (rev, importing file) -> {local binding name
    /// -> (source name at dst, dst file)}. Empty when the module graph hasn't
    /// populated yet (the hop is then simply a no-op, same as an absent
    /// import map). Index-free equivalent of `scip_binding`: resolves an
    /// aliased import (`use x::y as z`, TS `import { a as b }`/default,
    /// Kotlin `import a.b.C as D`) that the name-keyed `by_name` bucket has
    /// no bucket for (the def's real name is `y`/`a`/`C`, not the local `z`/
    /// `b`/`D`).
    fn module_binding_map(&self) -> Result<HashMap<(String, String), HashMap<String, (String, String)>>> {
        let mut out: HashMap<(String, String), HashMap<String, (String, String)>> = HashMap::new();
        let conn = self.db.conn();
        let Ok(mut s) = conn.prepare(
            &format!("SELECT file, local, source, dst, \"rev\" FROM {}", tbl("module_binding_rev"))) else {
            return Ok(out);
        };
        let rows = s.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, String>(3)?, r.get::<_, String>(4)?))
        })?;
        for row in rows.flatten() {
            let (file, local, source, dst, rev) = row;
            out.entry((rev, file)).or_default().insert(local, (source, dst));
        }
        Ok(out)
    }

    /// Rebuild the convenient rev-less `type_edge` / `type_entity` / `type_link`
    /// from their rev-aware twins, deduped across revs (drop the `rev` column,
    /// `INSERT OR IGNORE`). Same shape as `rebuild_legacy_module_rels`: the
    /// `_rev` table is the source of truth, the legacy rel is the closure/point-
    /// query target for the single-rev (WORK) daemon. A multi-rev db's legacy
    /// rel is the rev-deduped superimposition (plan open-question 3).
    fn rebuild_legacy_type_rels(&self) -> Result<()> {
        let edge = tbl("type_edge");
        let edge_rev = tbl("type_edge_rev");
        self.db.exec(&format!("DELETE FROM {edge}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {edge} (\"from\", \"to\", \"kind\", \"repo\") \
             SELECT \"from\", \"to\", \"kind\", \"repo\" FROM {edge_rev}"
        ))?;
        let entity = tbl("type_entity");
        let entity_rev = tbl("type_entity_rev");
        self.db.exec(&format!("DELETE FROM {entity}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {entity} (\"repo\", \"sym\", \"name\", \"kind\", \"parent\", \"file\", \"line\") \
             SELECT \"repo\", \"sym\", \"name\", \"kind\", \"parent\", \"file\", \"line\" FROM {entity_rev}"
        ))?;
        let link = tbl("type_link");
        let link_rev = tbl("type_link_rev");
        self.db.exec(&format!("DELETE FROM {link}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {link} (\"src\", \"dst\", \"kind\") \
             SELECT \"src\", \"dst\", \"kind\" FROM {link_rev}"
        ))?;
        Ok(())
    }

    /// The comment corpus: every `_file` row whose path has a grammar the
    /// comment walk can parse — the oxc TS/TSX front-end (`.ts`/`.tsx`) or one of
    /// the tree-sitter grammars `cst::lang_label_for_path` recognizes (Rust,
    /// Kotlin, Python, Go, C, bash, ...). Scoping the set here (rather than
    /// reading all of `_file`) keeps the family's input digest from moving when
    /// an unparseable file (`.md`, `.json`) is edited.
    fn comment_file_set(&self) -> Result<Vec<ExtractFile>> {
        let mut files: Vec<ExtractFile> = Vec::new();
        let mut sel = self.db.conn().prepare("SELECT repo, path, rev, hash FROM _file")?;
        let rows = sel.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?.unwrap_or_default())))?;
        for row in rows.flatten() {
            let p = row.1.as_str();
            let ts = p.ends_with(".ts") || p.ends_with(".tsx");
            if ts || crate::cst::lang_label_for_path(p).is_some() { files.push(row); }
        }
        Ok(files)
    }

    /// Rebuild `comment_node` — every comment in every parseable file as a
    /// grammar-backed fact. Its OWN family (not riding the TypeLang parse like
    /// `doc_comment`): it covers EVERY comment (line/block/doc) across a broader
    /// language set (oxc for TS/TSX, tree-sitter for the `AST_LANG_TABLE`
    /// grammars), so a program reading only `comment_node` shouldn't pay for a
    /// type-entity pass, and a Python/Go comment has no `type_entity` to hang
    /// off. Same perf shape as `refresh_type_rels`: per-rev input-digest skip,
    /// per-file fact cache, parallel parse, one batched `refresh_rel`.
    ///
    /// `comment_node` has no rev column (like `doc_comment`): a file present at
    /// two revs unions its comments, deduped by span. String-literal safety is
    /// the walk's, not this method's — a `//` inside a string is never a comment
    /// node / oxc comment, so it never becomes a row.
    pub(crate) fn refresh_comment_rels(&self) -> Result<bool> {
        let files = self.comment_file_set()?;
        let moved = self.moved_extract_revs("comment", &files, false)?;
        if moved.is_empty() { return Ok(false); }
        let root = self.root.clone();
        let roots = self.repo_roots();
        // Per-file comment walk in parallel; TS/TSX via oxc, everything else via
        // the shared tree-sitter walk. Unchanged files come from the cache.
        let facts: Vec<(String, String, String, Arc<Vec<crate::cst::RawComment>>)> =
            cached_facts(&self.comment_facts_cache, &files, &self.extract_files_parsed, |repo, path, rev| {
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                let comments = if path.ends_with(".ts") || path.ends_with(".tsx") {
                    typegraph::ts_comments(&content, path.ends_with(".tsx"))
                } else {
                    let label = crate::cst::lang_label_for_path(path)?;
                    let lang = ts_lang(label).ok()?;
                    crate::cst::walk_comments(&content, &lang).unwrap_or_default()
                };
                Some((rid, comments))
            });

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        // Dedup by span across revs (no rev column): a file scanned at two revs
        // emits its comments once. Keyed on (path, span) — a comment can't occur
        // twice at one span in one file.
        let mut seen: HashSet<(String, u32, u32, u32, u32)> = HashSet::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for (_repo, path, _rev, comments) in &facts {
            for c in comments.iter() {
                if !seen.insert((path.clone(), c.start_row, c.start_col, c.end_row, c.end_col)) {
                    continue;
                }
                let (kind, text) = crate::cst::classify_comment(&c.raw);
                rows.push(vec![
                    t(path), i(c.start_row), i(c.start_col), i(c.end_row), i(c.end_col),
                    t(&text), t(kind),
                ]);
            }
        }
        self.refresh_rel("comment_node",
            &["path", "line", "col", "end_line", "end_col", "text", "kind"], &rows)?;
        for (rev, d) in &moved { self.save_rel_digest(&format!("extract:comment:{rev}"), d)?; }
        Ok(true)
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
        // Perf gap A, per rev: same per-rev digest skip + per-file cache as
        // refresh_type_rels. Empty `moved` = whole family skips.
        let moved = self.moved_extract_revs("call", &files, true)?;
        if moved.is_empty() { return Ok(false); }

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
        // Repo- AND rev-scoped, same as refresh_type_rels: a callee resolves
        // within the referencing file's repo at its own rev, and resolved syms
        // are repo-qualified so the sym-keyed call rels (call_edge/call_name)
        // stay per-repo distinct. The SCIP override is consulted only at WORK
        // (D5.6); committed revs resolve syntactically.
        let mut by_name: HashMap<(&str, &str, &str), Vec<&str>> = HashMap::new();
        let mut sym_at: HashMap<(&str, &str, &str, &str), &str> = HashMap::new();
        let mut def_by_file: HashMap<(&str, &str, &str), Vec<(u32, u32, &str)>> = HashMap::new();
        // (repo, rev, sym) -> declaring file (Win D narrowing input, see
        // refresh_type_rels' twin map).
        let mut sym_file: HashMap<(&str, &str, &str), &str> = HashMap::new();
        for (repo, _, rev, f) in &facts {
            for d in &f.defs {
                // Dedup by callable sym (see refresh_type_rels): a def scanned
                // twice under two slugs that map to one rid stays unique, while
                // two distinct callables of one name stay ambiguous.
                let bucket = by_name.entry((repo.as_str(), rev.as_str(), d.name.as_str())).or_default();
                if !bucket.iter().any(|s| *s == d.sym.as_str()) {
                    bucket.push(d.sym.as_str());
                }
                sym_at.insert((repo.as_str(), d.file.as_str(), rev.as_str(), d.name.as_str()), d.sym.as_str());
                sym_file.insert((repo.as_str(), rev.as_str(), d.sym.as_str()), d.file.as_str());
                def_by_file.entry((repo.as_str(), d.file.as_str(), rev.as_str())).or_default().push((d.line, d.end, d.sym.as_str()));
            }
        }
        let scip = self.scip_name_defs().unwrap_or_default();
        // Occurrence-level override input: the exact symbol at each call's span,
        // built once for the whole family (see `ScipOccIndex`). Empty when no
        // index is loaded, so `resolve` returns `Fallthrough` everywhere and the
        // name-level `scip` map carries unchanged.
        let occ = self.scip_occ_index().unwrap_or_default();
        // Win D: see refresh_type_rels, the same import map feeds both
        // resolvers' ambiguity narrowing.
        let imports = self.module_import_map().unwrap_or_default();
        // Alias hop input: see refresh_type_rels, the same binding map feeds
        // both resolvers' index-free alias hop.
        let aliases = self.module_binding_map().unwrap_or_default();
        let resolve_callee = |repo: &str, rev: &str, file: &str, callee: &str, line: u32| -> Option<String> {
            if rev == "WORK" {
                // Occurrence-level override (position before name): the exact
                // symbol occurring at this call's (file, line) disambiguates a
                // shared name the name-level `scip` map must drop. Preferred over
                // the name map because it tells two same-name defs apart; only a
                // site the index never recorded (`Fallthrough`) defers to it.
                match occ.resolve(repo, file, callee, line) {
                    OccPick::Resolved(def_file) => {
                        if let Some(sym) = sym_at.get(&(repo, def_file.as_str(), rev, callee)) {
                            return Some(format!("{repo}::{sym}"));
                        }
                        // def outside the scan corpus: fall through to the alias
                        // hop / by_name, same as the name map on a sym_at miss.
                    }
                    // Same-line same-name conflict: honest bare. Returning here
                    // (not falling through) is the point — by_name could resolve
                    // a coincidental single-def name the position just refuted.
                    OccPick::Refuse => return None,
                    OccPick::Fallthrough => {
                        // No occurrence names this site: the name-level map still
                        // applies (identical to the pre-occurrence behavior).
                        if let Some(def_file) = scip.get(&(repo.to_string(), file.to_string(), callee.to_string())) {
                            if let Some(sym) = sym_at.get(&(repo, def_file.as_str(), rev, callee)) {
                                return Some(format!("{repo}::{sym}"));
                            }
                        }
                    }
                }
            }
            // Index-free alias hop, see refresh_type_rels' `resolve` for the
            // full rationale: only fires when this file declares no callable
            // named `callee` itself (local def shadows an aliased import), and
            // never falls through to by_name on a miss.
            if sym_at.get(&(repo, file, rev, callee)).is_none() {
                if let Some((source, dst)) = aliases.get(&(rev.to_string(), file.to_string())).and_then(|m| m.get(callee)) {
                    return sym_at.get(&(repo, dst.as_str(), rev, source.as_str()))
                        .map(|sym| format!("{repo}::{sym}"));
                }
            }
            match by_name.get(&(repo, rev, callee)) {
                Some(v) if v.len() == 1 => Some(format!("{repo}::{}", v[0])),
                Some(v) if v.len() > 1 =>
                    narrow_ambiguous(v, repo, rev, file, &sym_file, &imports)
                        .map(|sym| format!("{repo}::{sym}")),
                _ => None,
            }
        };
        let resolve_caller = |repo: &str, rev: &str, file: &str, line: u32| -> Option<String> {
            let mut best: Option<(u32, &str)> = None; // (span, sym); smallest containing span wins
            for &(s, e, sym) in def_by_file.get(&(repo, file, rev)).into_iter().flatten() {
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
        let mut def_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut site_rows: Vec<Vec<Value>> = Vec::new();
        let mut edge_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut name_rows: Vec<Vec<Value>> = Vec::new();
        // call_kind is keyed by (caller, kind); a fn with both a read and a
        // write emits two rows. Accumulate in a set so multiple write sites in
        // the same fn collapse to one (fn, "write") row.
        let mut kind_set: HashSet<(String, &'static str)> = HashSet::new();
        // Dedup carries the rev, so one def present at two revs emits its
        // call_def_rev row once PER rev — rev is a column, not folded into the
        // sym (same crux as type_entity_rev).
        let mut seen_def: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_edge: HashSet<(String, String, &str)> = HashSet::new();
        for (repo, _path, rev, f) in &facts {
            for d in &f.defs {
                if seen_def.insert((repo.as_str(), d.sym.as_str(), rev.as_str())) {
                    let qsym = format!("{repo}::{}", d.sym);
                    def_rev_rows.push(vec![t(repo), t(&qsym), t(d.kind.tag()), t(&d.file), i(d.line), i(d.end), t(rev)]);
                    name_rows.push(vec![t(&qsym), t(&d.name)]);
                }
            }
            for s in &f.sites {
                // call_site is the raw graph: every site, caller resolved when a
                // def encloses it (repo-qualified), callee as written.
                let caller = resolve_caller(repo, rev, &s.file, s.line).unwrap_or_default();
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
                if let Some(callee_sym) = resolve_callee(repo, rev, &s.file, &s.callee, s.line) {
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

        // call_def_rev / call_edge_rev are the rev-carrying twins: write them
        // through the rev-scoped helper. Delete scope = every corpus rev
        // (whole-corpus emit in D5.1 = full `refresh_rel` wipe; see
        // refresh_type_rels' matching comment).
        let all_revs = Self::corpus_revs(&files);
        let all_rev_refs: Vec<&str> = all_revs.iter().map(|s| s.as_str()).collect();
        self.refresh_rel_for_revs("call_def_rev", &["repo", "sym", "kind", "file", "line", "end", "rev"], &def_rev_rows, &all_rev_refs)?;
        self.refresh_rel("call_site", &["repo", "caller", "callee", "file", "line"], &site_rows)?;
        self.refresh_rel_for_revs("call_edge_rev", &["caller", "callee", "kind", "rev"], &edge_rev_rows, &all_rev_refs)?;
        self.refresh_rel("call_name", &["sym", "name"], &name_rows)?;
        self.refresh_rel("call_kind", &["fn", "kind"], &kind_rows)?;
        self.rebuild_legacy_call_rels()?;
        // Persisted only after the writes land, so a failed refresh retries.
        for (rev, d) in &moved { self.save_rel_digest(&format!("extract:call:{rev}"), d)?; }
        Ok(true)
    }

    /// Rebuild the convenient rev-less `call_edge` / `call_def` from their
    /// rev-aware twins, deduped across revs. Same shape as
    /// `rebuild_legacy_type_rels`: the `_rev` table is the source of truth, the
    /// legacy rel is the closure/point-query target for the single-rev daemon.
    fn rebuild_legacy_call_rels(&self) -> Result<()> {
        let edge = tbl("call_edge");
        let edge_rev = tbl("call_edge_rev");
        self.db.exec(&format!("DELETE FROM {edge}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {edge} (\"caller\", \"callee\", \"kind\") \
             SELECT \"caller\", \"callee\", \"kind\" FROM {edge_rev}"
        ))?;
        let def = tbl("call_def");
        let def_rev = tbl("call_def_rev");
        self.db.exec(&format!("DELETE FROM {def}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {def} (\"repo\", \"sym\", \"kind\", \"file\", \"line\", \"end\") \
             SELECT \"repo\", \"sym\", \"kind\", \"file\", \"line\", \"end\" FROM {def_rev}"
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
        // Perf gap A, per rev: no resolution pass here, so each rev's digest
        // folds its corpus rows only (no scip term). Empty `moved` = skip. The
        // df rels gain `_rev` twins in D5.4; today they stay whole-table
        // `refresh_rel` writes (single-rev daemon sees today's behavior).
        let moved = self.moved_extract_revs("dataflow", &files, false)?;
        if moved.is_empty() { return Ok(false); }

        let root = self.root.clone();
        // Read each file from its OWN repo root (same as type/call), so a config
        // repo's WORK content lifts too; reading everything from `self.root`
        // stranded config-repo files at self-root/path (missing -> empty -> zero
        // df rows) or a git blob, never their working tree. The derived repo id
        // (nearest `.git` basename, like type/call) rides each fact so df_node_repo
        // can attribute every node to the folder it lives in.
        let roots = self.repo_roots();
        let facts: Vec<(String, String, String, Arc<typegraph::DataflowFacts>)> =
            cached_facts(&self.df_facts_cache, &files, &self.extract_files_parsed, |repo, path, rev| {
                let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                Some((rid, lang.extract_dataflow(path, &content)))
            });

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut node_rows: Vec<Vec<Value>> = Vec::new();
        let mut node_repo_rows: Vec<Vec<Value>> = Vec::new();
        let mut edge_rows: Vec<Vec<Value>> = Vec::new();
        let mut loop_rows: Vec<Vec<Value>> = Vec::new();
        let mut alloc_rows: Vec<Vec<Value>> = Vec::new();
        let mut nest_rows: Vec<Vec<Value>> = Vec::new();
        let mut param_rows: Vec<Vec<Value>> = Vec::new();
        let mut arg_rows: Vec<Vec<Value>> = Vec::new();
        let mut field_rows: Vec<Vec<Value>> = Vec::new();
        // Rev-carrying twins (D5.4). Every id-valued column is salted by rev so a
        // file byte-identical at two revs emits DISJOINT ids per rev (the raw ids
        // collide and would cross-wire base into head). Legacy rows above keep raw
        // ids. Twin dedup keys carry rev, so one file at two revs emits its twin
        // rows once PER rev.
        let mut node_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut node_repo_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut arg_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut field_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut seen_param: HashSet<&str> = HashSet::new();
        let mut seen_arg: HashSet<(&str, i64, &str)> = HashSet::new();
        let mut seen_field: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_node: HashSet<&str> = HashSet::new();
        let mut seen_node_repo: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_edge: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_loop: HashSet<(&str, u32)> = HashSet::new();
        let mut seen_nest: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_node_rev: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_node_repo_rev: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_arg_rev: HashSet<(&str, i64, &str, &str)> = HashSet::new();
        let mut seen_field_rev: HashSet<(&str, &str, &str, &str)> = HashSet::new();
        for (repo, _, rev, f) in &facts {
            for n in &f.nodes {
                if seen_node.insert(n.id.as_str()) {
                    node_rows.push(vec![t(&n.id), t(&n.kind), t(&n.var), t(&n.fn_sym), t(&n.file), i(n.line)]);
                }
                if seen_node_rev.insert((n.id.as_str(), rev.as_str())) {
                    node_rev_rows.push(vec![
                        Value::Text(Self::salt_rev(&n.id, rev)), t(&n.kind), t(&n.var),
                        t(&n.fn_sym), t(&n.file), i(n.line), t(rev),
                    ]);
                }
                // df_node id is `file:line:col` (path only, no repo). Attribute
                // each node to EVERY repo it appears in so a downstream join
                // (member_node field/fill in flow-panel.dl) can scope a fill to
                // its own repo instead of fanning across every repo that shares
                // the constructed type's NAME. Emitted per (id, repo) OUTSIDE the
                // node dedup: two repos with a byte-identical file share one
                // df_node row but get TWO df_node_repo rows, so the join mints the
                // field for BOTH (they both really have it). A fill unique to one
                // repo (a working-tree-only edit) yields a single (id, repo) row,
                // so it stays a per-repo fact — the property the worktree-pair diff
                // needs.
                if seen_node_repo.insert((n.id.as_str(), repo.as_str())) {
                    node_repo_rows.push(vec![t(&n.id), t(repo)]);
                }
                if seen_node_repo_rev.insert((n.id.as_str(), repo.as_str(), rev.as_str())) {
                    node_repo_rev_rows.push(vec![Value::Text(Self::salt_rev(&n.id, rev)), t(repo), t(rev)]);
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
                // both id columns salted so the arg->node join stays intra-rev
                if seen_arg_rev.insert((call.as_str(), *pos, arg.as_str(), rev.as_str())) {
                    arg_rev_rows.push(vec![
                        Value::Text(Self::salt_rev(call, rev)), Value::Int(*pos),
                        Value::Text(Self::salt_rev(arg, rev)), t(rev),
                    ]);
                }
            }
            for (id, field, value) in &f.fields {
                if seen_field.insert((id.as_str(), field.as_str(), value.as_str())) {
                    field_rows.push(vec![t(id), t(field), t(value)]);
                }
                // value is always a value df_node id (never a literal), so it
                // salts like id; the field name is a plain string, unsalted
                if seen_field_rev.insert((id.as_str(), field.as_str(), value.as_str(), rev.as_str())) {
                    field_rev_rows.push(vec![
                        Value::Text(Self::salt_rev(id, rev)), t(field),
                        Value::Text(Self::salt_rev(value, rev)), t(rev),
                    ]);
                }
            }
        }

        self.refresh_rel("df_node", &["id", "kind", "var", "fn", "file", "line"], &node_rows)?;
        self.refresh_rel("df_node_repo", &["id", "repo"], &node_repo_rows)?;
        self.refresh_rel("df_edge", &["from", "to"], &edge_rows)?;
        self.refresh_rel("loop_over", &["file", "start", "end", "var", "collection", "fn"], &loop_rows)?;
        self.refresh_rel("allocates", &["fn"], &alloc_rows)?;
        self.refresh_rel("nest", &["call_id", "loop_id", "depth", "collection"], &nest_rows)?;
        self.refresh_rel("df_param", &["id", "pos"], &param_rows)?;
        self.refresh_rel("df_arg", &["call", "pos", "arg"], &arg_rows)?;
        self.refresh_rel("df_field", &["id", "field", "value"], &field_rows)?;
        // Rev-carrying twins: same delete scope as type/call — wipe every corpus
        // rev and reinsert the whole-corpus salted rows (the emit above is
        // whole-corpus; a rev absent from the corpus is D5.5's retraction sweep,
        // not this path). Legacy df rels above stay raw-id, no rebuild needed
        // (the salt is not cleanly reversible in SQL and the raw rows are in hand).
        let all_revs = Self::corpus_revs(&files);
        let all_rev_refs: Vec<&str> = all_revs.iter().map(|s| s.as_str()).collect();
        self.refresh_rel_for_revs("df_node_rev", &["id", "kind", "var", "fn", "file", "line", "rev"], &node_rev_rows, &all_rev_refs)?;
        self.refresh_rel_for_revs("df_node_repo_rev", &["id", "repo", "rev"], &node_repo_rev_rows, &all_rev_refs)?;
        self.refresh_rel_for_revs("df_arg_rev", &["call", "pos", "arg", "rev"], &arg_rev_rows, &all_rev_refs)?;
        self.refresh_rel_for_revs("df_field_rev", &["id", "field", "value", "rev"], &field_rev_rows, &all_rev_refs)?;
        // Persisted only after the writes land, so a failed refresh retries.
        for (rev, d) in &moved { self.save_rel_digest(&format!("extract:dataflow:{rev}"), d)?; }
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
        // Per-rev skip, riding the type family per rev: the doc_ref bridge reads
        // type_entity, so each rev's doc digest folds the SAME rev's stored
        // `extract:type:<rev>` (type refresh runs before doc in both tick paths).
        // Single-rev (WORK-only) programs see the old whole-family behavior.
        let mut moved: Vec<(String, [u8; 32])> = Vec::new();
        {
            let mut by_rev: HashMap<&str, Vec<ExtractFile>> = HashMap::new();
            for f in &files { by_rev.entry(f.2.as_str()).or_default().push(f.clone()); }
            for (rev, frev) in &by_rev {
                let mut digest = self.extract_input_digest("doc", rev, frev, false);
                let ty = self.load_rel_digest(&format!("extract:type:{rev}"))?
                    .map(|d| d.iter().map(|b| format!("{b:02x}")).collect::<String>())
                    .unwrap_or_default();
                for (a, b) in digest.iter_mut()
                    .zip(blake3::hash(format!("type\0{ty}").as_bytes()).as_bytes()) { *a ^= *b; }
                if self.load_rel_digest(&format!("extract:doc:{rev}"))? == Some(digest) { continue; }
                moved.push(((*rev).to_string(), digest));
            }
        }
        if moved.is_empty() { return Ok(false); }
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
        for (rev, d) in &moved { self.save_rel_digest(&format!("extract:doc:{rev}"), d)?; }
        Ok(true)
    }
}
