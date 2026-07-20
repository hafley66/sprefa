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
use crate::db::SqlVal;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::lower::{tbl, txt_tbl};
use crate::modgraph::{self, ProjectCx, Resolution};
use crate::spine;
use crate::typegraph;

// Per-family extractor clusters, each a sibling `impl Engine` block. Pure
// relocation out of this file to hold it under the file-size rail; the bodies
// call the shared digest/skip spine (`extract_file_set`, `moved_extract_revs`,
// `refresh_rel`) which stays here as parent privates the children can reach.
mod call;
mod dataflow;
mod doc;
mod node;
mod scip_narrow;
mod text;
mod type_rels;
#[cfg(test)]
mod verdict_tests;

pub(crate) use scip_narrow::{narrow_ambiguous, OccPick, ScipOccIndex};

/// One extraction-corpus row: (repo, path, rev, content hash) from `_file`.
pub(crate) type ExtractFile = (String, String, String, String);

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
        // Test/override hook: a distinct value stands in for a distinct binary
        // so the digest-namespace behavior is checkable without a real reinstall.
        if let Ok(s) = std::env::var("DL_EXE_STAMP") {
            return u128::from_le_bytes(blake3::hash(s.as_bytes()).as_bytes()[..16].try_into().unwrap());
        }
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

/// The stored key for a family/rev extract digest: three atomic columns, never
/// a `format!` fold. The running binary's stamp is a KEY NAMESPACE segment
/// rather than part of the digest VALUE, so two `dl` binaries sharing one
/// `cache.db` (the classic case: a running daemon on the old image while a
/// freshly `cargo install`ed CLI runs the same repo) keep SEPARATE skip-state
/// instead of clobbering each other at a shared key and forcing a full
/// re-extract every tick. A new binary sees `None` (its namespace is empty),
/// does exactly one cold rebuild, then skips.
///
/// The predecessor folded all three into one `_reldigest.rel` TEXT value, which
/// forced the gone-rev sweep into a `LIKE 'prefix%'` scan plus a substring cut to get
/// the rev back — a hand-rolled column projection over a key that should have
/// had columns. Recovering a component is now a column read.
fn extract_digest_exe() -> i64 {
    exe_stamp() as u64 as i64
}

fn intern(text: &str) -> i64 {
    crate::spine::StringId::of(text).sqlite()
}

impl Engine {
    /// Skip-state for one (binary, family, rev). `None` means this binary has
    /// never stored that family's outputs for that rev.
    pub(crate) fn load_extract_digest(&self, family: &str, rev: &str) -> Result<Option<[u8; 32]>> {
        let hex: Option<String> = self.db.query_opt(
            "_extract_digest",
            "SELECT digest FROM _extract_digest WHERE exe = ?1 AND family = ?2 AND rev = ?3",
            &[extract_digest_exe().into(), intern(family).into(), intern(rev).into()],
            |row| Ok(row.get::<_, String>(0)?),
        )?;
        Ok(hex.and_then(|h| crate::engine::hex_to_32(&h).ok()))
    }

    pub(crate) fn save_extract_digest(&self, family: &str, rev: &str, digest: &[u8; 32]) -> Result<()> {
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        self.db.exec_params(
            "_extract_digest",
            "INSERT INTO _extract_digest(exe, family, rev, digest) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(exe, family, rev) DO UPDATE SET digest = excluded.digest",
            &[
                extract_digest_exe().into(),
                intern(family).into(),
                intern(rev).into(),
                hex.into(),
            ],
        )?;
        Ok(())
    }
}

/// Split `files` into cache hits and misses (keyed by (repo, path, content
/// hash)), run `parse` over the misses in parallel, then replace the cache
/// with exactly the current file set's entries. Returns one
/// (repo id, path, rev, facts) tuple per input row in the same order as
/// `files`.
/// A row with an empty content hash is never cached (no identity to key on).
/// `parsed_counter` accumulates the miss count (the `extract_files_parsed`
/// instrumentation).
fn cached_facts<T: Send + Sync>(
    cache: &FactCache<T>,
    files: &[ExtractFile],
    parsed_counter: &std::cell::Cell<usize>,
    parse: impl Fn(&str, &str, &str) -> Option<(String, T)> + Sync,
) -> Vec<(String, String, String, Arc<T>)> {
    cached_facts_profiled(cache, files, parsed_counter, "", parse)
}

/// `cached_facts` plus per-file parse timing. `family` names the extraction
/// family for the profile records (`type`/`call`/`dataflow`/...); pass `""` to
/// skip the family tag. Every MISS (a file actually re-parsed this tick) emits a
/// `profile` record with the parse ms and the byte count, but ONLY when
/// `perflog::profile_enabled()` — the timing is always taken (cheap `Instant`),
/// the emit is gated. This is the "which file / how big" breakdown behind an
/// `extract:<family>` phase total.
fn cached_facts_profiled<T: Send + Sync>(
    cache: &FactCache<T>,
    files: &[ExtractFile],
    parsed_counter: &std::cell::Cell<usize>,
    family: &str,
    parse: impl Fn(&str, &str, &str) -> Option<(String, T)> + Sync,
) -> Vec<(String, String, String, Arc<T>)> {
    let mut out: Vec<(String, String, String, Arc<T>)> = Vec::with_capacity(files.len());
    let mut next: HashMap<(String, String, String), (String, Arc<T>)> =
        HashMap::with_capacity(files.len());
    // Parsed misses keyed by (repo, path, rev) so `out` can be assembled in
    // strict `files` order independent of hit/miss scheduling.
    let mut miss_map: HashMap<(String, String, String), (String, Arc<T>)> =
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
                    next.insert((f.0.clone(), f.1.clone(), f.3.clone()),
                                (rid.clone(), facts.clone()));
                }
                None => misses.push(f),
            }
        }
    }
    // Time every miss (a file actually re-parsed this tick). The `Instant` is
    // cheap and taken unconditionally; the per-file emit below is gated on
    // `profile_enabled()`. `ms` is the parse+extract cost for that one file.
    let parsed: Vec<(&ExtractFile, String, Arc<T>, u64)> = misses.par_iter().filter_map(|f| {
        crate::budget::throttle_point();
        tracing::trace!(file = %f.1, family, "[extract] parse file");
        let start = std::time::Instant::now();
        let (rid, facts) = parse(&f.0, &f.1, &f.2)?;
        Some((*f, rid, Arc::new(facts), start.elapsed().as_millis() as u64))
    }).collect();
    parsed_counter.set(parsed_counter.get() + parsed.len());
    if crate::perflog::profile_enabled() {
        for (f, _, _, ms) in &parsed {
            crate::perflog::emit_profile("parse", &f.1, *ms, 0, family);
        }
    }
    for (f, rid, facts, _) in parsed {
        miss_map.insert((f.0.clone(), f.1.clone(), f.2.clone()), (rid.clone(), facts.clone()));
        if !f.3.is_empty() {
            next.insert((f.0.clone(), f.1.clone(), f.3.clone()), (rid, facts));
        }
    }
    for f in files {
        let miss_key = (f.0.clone(), f.1.clone(), f.2.clone());
        if let Some((rid, facts)) = miss_map.get(&miss_key) {
            out.push((rid.clone(), f.1.clone(), f.2.clone(), facts.clone()));
            continue;
        }
        let cache_key = (f.0.clone(), f.1.clone(), f.3.clone());
        if let Some((rid, facts)) = next.get(&cache_key) {
            out.push((rid.clone(), f.1.clone(), f.2.clone(), facts.clone()));
        }
    }
    *cache.borrow_mut() = next;
    out
}

/// Warm the type/call/dataflow fact caches with one language-front-end bundle
/// per changed file when at least two of those families will refresh. The
/// bundle itself is generation-local: only the already-existing, bounded
/// per-family fact caches retain its projections; source text and parsed ASTs
/// are dropped in the Rayon job.
///
/// Family digests remain owned by each refresher. This only primes cache misses,
/// so the normal refresh methods still perform their existing digest checks,
/// resolution passes, row writes, and digest commits unchanged.
pub(crate) fn prime_analysis_bundles(eng: &Engine, prog: &Program) -> Result<()> {
    if eng.force_separate_analysis_extractors.get() {
        return Ok(());
    }
    let want_types = type_rels_used(prog)
        || rels_used(prog, &["type_shape", "type_lgg"])
        || doc_text_rels_used(prog)
        || const_value_rels_used(prog);
    let want_calls = call_rels_used(prog);
    let want_dataflow = dataflow_rels_used(prog);
    if usize::from(want_types) + usize::from(want_calls) + usize::from(want_dataflow) < 2 {
        return Ok(());
    }

    let files = eng.extract_file_set()?;
    let family_moved = |family: &str, with_scip: bool| -> Result<bool> {
        let mut by_rev: HashMap<&str, Vec<ExtractFile>> = HashMap::new();
        for f in &files {
            by_rev.entry(f.2.as_str()).or_default().push(f.clone());
        }
        for (rev, frev) in by_rev {
            let digest = eng.extract_input_digest(family, rev, &frev, with_scip);
            if eng.load_extract_digest(family, rev)? != Some(digest) {
                return Ok(true);
            }
        }
        Ok(false)
    };

    let do_types = want_types && family_moved("type", true)?;
    let do_calls = want_calls && family_moved("call", true)?;
    let do_dataflow = want_dataflow && family_moved("dataflow", false)?;
    if usize::from(do_types) + usize::from(do_calls) + usize::from(do_dataflow) < 2 {
        return Ok(());
    }

    let roots = eng.repo_roots();
    let root = eng.root.clone();
    let misses: Vec<(&ExtractFile, typegraph::AnalysisMask)> = files.iter().filter_map(|f| {
        // Without a content identity the family caches intentionally refuse a
        // hit. Let the ordinary refreshers handle that rare row; priming it
        // would only cause a second parse when the projection cannot be kept.
        if f.3.is_empty() { return None }
        let key = (f.0.clone(), f.1.clone(), f.3.clone());
        let types = do_types && !eng.type_facts_cache.borrow().contains_key(&key);
        let calls = do_calls && !eng.call_facts_cache.borrow().contains_key(&key);
        let dataflow = do_dataflow && !eng.df_facts_cache.borrow().contains_key(&key);
        (usize::from(types) + usize::from(calls) + usize::from(dataflow) >= 2).then_some((
            f,
            typegraph::AnalysisMask { types, calls, dataflow },
        ))
    }).collect();

    struct Primed<'a> {
        file: &'a ExtractFile,
        rid: String,
        mask: typegraph::AnalysisMask,
        bundle: typegraph::AnalysisBundle,
    }
    let primed: Vec<Primed<'_>> = misses.par_iter().filter_map(|(f, mask)| {
        crate::budget::throttle_point();
        tracing::trace!(file = %f.1, "[extract] prime analysis bundle");
        let lang = typegraph::type_langs().iter().find(|l| l.matches(&f.1))?;
        if !lang.supports_analysis_bundle() { return None }
        let froot = roots.get(&f.0).map(|p| p.as_path()).unwrap_or(&root);
        let content = read_content(froot, &f.2, &f.1).unwrap_or_default();
        let rid = repo_id_of(froot, &f.1, &f.0);
        let bundle = lang.extract_bundle(&f.1, &content, *mask);
        Some(Primed { file: f, rid, mask: *mask, bundle })
    }).collect();
    eng.extract_files_parsed.set(eng.extract_files_parsed.get() + primed.len());

    for mut item in primed {
        let key = (item.file.0.clone(), item.file.1.clone(), item.file.3.clone());
        if item.mask.types {
            let facts = item.bundle.types.take().unwrap_or_default();
            eng.type_facts_cache.borrow_mut()
                .insert(key.clone(), (item.rid.clone(), Arc::new(facts)));
        }
        if item.mask.calls {
            let facts = item.bundle.calls.take().unwrap_or_default();
            eng.call_facts_cache.borrow_mut()
                .insert(key.clone(), (item.rid.clone(), Arc::new(facts)));
        }
        if item.mask.dataflow {
            let facts = item.bundle.dataflow.take().unwrap_or_default();
            eng.df_facts_cache.borrow_mut()
                .insert(key, (item.rid, Arc::new(facts)));
        }
    }
    Ok(())
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
        // No stored `norm` column (storage-diet Direction 3b): the third
        // column is folded at read time by the same scalar the `norm()`
        // builtin calls (src/db.rs `sprf_norm`, itself `spine::normalize`),
        // so `string(id, text, norm)` rows stay byte-identical to the old
        // stored-column values.
        let strings: Vec<Vec<Value>> = self.db.query_rows(
            "_strings",
            "SELECT id, content, sprf_norm(content) FROM _strings WHERE id != 0",
            &[],
            |r| Ok(vec![
                Value::Int(r.get::<_, i64>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
            ]),
        )?;
        let refs: Vec<Vec<Value>> = self.db.query_rows(
            "_where_bytes",
            "SELECT id, string_id, file_id, lo, hi FROM _where_bytes WHERE id != '0'",
            &[],
            |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Int(r.get::<_, i64>(1)?),
                Value::Text(r.get::<_, String>(2)?),
                Value::Int(r.get::<_, i64>(3)?),
                Value::Int(r.get::<_, i64>(4)?),
            ]),
        )?;
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
        let programs: Vec<Vec<Value>> = self.db.query_rows(
            "_program",
            "SELECT path, hash, mtime FROM _program",
            &[],
            |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Int(r.get::<_, i64>(2)?),
            ]),
        )?;
        let heads: Vec<Vec<Value>> = self.db.query_rows(
            "_ref",
            "SELECT repo, name, oid FROM _ref",
            &[],
            |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
            ]),
        )?;
        let advances: Vec<Vec<Value>> = self.db.query_rows(
            "_rev_log",
            "SELECT repo, name, old, new FROM _rev_log ORDER BY id",
            &[],
            |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
                Value::Text(r.get::<_, String>(3)?),
            ]),
        )?;
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
        let rows: Vec<Vec<Value>> = self.db.query_rows(
            "pending_effect",
            "SELECT id, kind, head_rel, state, args_json, req_tx \
             FROM pending_effect ORDER BY req_tx, id",
            &[],
            |r| Ok(vec![
                Value::Text(r.get::<_, String>(0)?),
                Value::Text(r.get::<_, String>(1)?),
                Value::Text(r.get::<_, String>(2)?),
                Value::Text(r.get::<_, String>(3)?),
                Value::Text(r.get::<_, String>(4)?),
                Value::Int(r.get::<_, i64>(5)?),
            ]),
        )?;
        self.refresh_rel("effect_log",
            &["id", "kind", "head", "state", "args", "req_tx"], &rows)?;
        Ok(())
    }

    fn module_files_by_rev(&self) -> Result<HashMap<String, Vec<(String, String)>>> {
        let mut by_rev: HashMap<String, Vec<(String, String)>> = HashMap::new();
        let rows = self.db.query_rows(
            "_file",
            "SELECT path, rev, hash FROM _file ORDER BY repo, path, rev",
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        )?;
        for row in rows { by_rev.entry(row.1).or_default().push((row.0, row.2)); }
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
            tracing::warn!(
                sys_path_mutators,
                rev = %rev,
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
            crate::budget::throttle_point();
            tracing::trace!(file = %path, "[extract] module resolve");
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
                    // module_binding: EVERY local binding, for EVERY resolution
                    // kind (File/External/Unresolved) — unlike `bindings` above,
                    // this is the rel a "which library" query joins, so it must
                    // not skip External (the common library-import case).
                    for (local_name, imported_name, kind) in &mref.module_bindings {
                        rows.module_bindings.push(vec![
                            t(path), t(local_name), t(&mref.specifier), t(imported_name),
                            Value::Text((*kind).to_string()), t(rev),
                        ]);
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
        self.insert_rel_rows("module_import", &["file", "rev", "specifier", "kind", "line"], &rows.imports)?;
        self.insert_rel_rows("module_edge_rev", &["src", "dst", "rev"], &rows.edges_rev)?;
        self.insert_rel_rows("module_unresolved_rev", &["file", "rev", "specifier", "reason", "line"], &rows.unresolved_rev)?;
        if include_crate_edges {
            self.insert_rel_rows("crate_edge", &["src", "dst", "kind", "rev"], &rows.crate_edges)?;
        }
        self.insert_rel_rows("module_binding_resolved_rev", &["file", "local", "source", "dst", "rev"], &rows.bindings)?;
        self.insert_rel_rows("module_binding_rev",
            &["file", "local_name", "source_module", "imported_name", "kind", "rev"], &rows.module_bindings)?;
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
        let binding = tbl("module_binding_resolved");
        let binding_rev = tbl("module_binding_resolved_rev");
        self.db.exec(&format!("DELETE FROM {binding}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {binding} (\"file\", \"local\", \"source\", \"dst\") \
             SELECT \"file\", \"local\", \"source\", \"dst\" FROM {binding_rev}"
        ))?;
        let module_binding = tbl("module_binding");
        let module_binding_rev = tbl("module_binding_rev");
        self.db.exec(&format!("DELETE FROM {module_binding}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {module_binding} (\"file\", \"local_name\", \"source_module\", \"imported_name\", \"kind\") \
             SELECT \"file\", \"local_name\", \"source_module\", \"imported_name\", \"kind\" FROM {module_binding_rev}"
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
    // ARCH {"url":"engine/30-extract","role":"extraction"}
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
            if self.load_extract_digest("module", rev)? == Some(d) { continue; }
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
        self.refresh_rel("module_binding_resolved_rev", &["file", "local", "source", "dst", "rev"], &rows.bindings)?;
        self.refresh_rel("module_binding_rev",
            &["file", "local_name", "source_module", "imported_name", "kind", "rev"], &rows.module_bindings)?;
        self.insert_module_spans(&rows)?;
        self.rebuild_legacy_module_rels()?;
        for (rev, d) in &moved {
            self.save_extract_digest("module", rev, d)?;
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
        fold(&mut acc, &blake3::hash(format!("module\0{rev}").as_bytes()));
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
        // The twins' `rev` column is interned (i64 StringId); `_module_refresh_rev`
        // holds raw text revs. Hash the text side into id-space (`sprf_sym`) so the
        // set-match runs in the same representation — else the stale rows are never
        // cleared before re-insert (same fix as `refresh_rel_for_revs`).
        let del_by_rev = "\"rev\" IN (SELECT sprf_sym(rev) FROM _module_refresh_rev)";
        self.db.exec(&format!("DELETE FROM {} WHERE {del_by_rev}", tbl("module_import")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE {del_by_rev}", tbl("module_edge_rev")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE {del_by_rev}", tbl("module_unresolved_rev")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE {del_by_rev}", tbl("crate_edge")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE {del_by_rev}", tbl("module_binding_resolved_rev")))?;
        self.db.exec(&format!("DELETE FROM {} WHERE {del_by_rev}", tbl("module_binding_rev")))?;

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
        // Interned `rev`/`file`/`src` columns: hash the raw literal rev and the
        // raw temp-table paths into id-space so the per-path DELETE matches.
        let rev_id = format!("sprf_sym('{rev}')");
        let paths_id = "(SELECT sprf_sym(path) FROM _module_refresh_path)";
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = {rev_id} AND \"file\" IN {paths_id}",
            tbl("module_import"),
        ))?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = {rev_id} AND \"src\" IN {paths_id}",
            tbl("module_edge_rev"),
        ))?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = {rev_id} AND \"file\" IN {paths_id}",
            tbl("module_unresolved_rev"),
        ))?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = {rev_id} AND \"file\" IN {paths_id}",
            tbl("module_binding_resolved_rev"),
        ))?;
        self.db.exec(&format!(
            "DELETE FROM {} WHERE \"rev\" = {rev_id} AND \"file\" IN {paths_id}",
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
    pub(crate) fn extract_file_set(&self) -> Result<Vec<ExtractFile>> {
        self.db.query_rows(
            "_file",
            "SELECT repo, path, rev, hash FROM _file WHERE path LIKE '%.rs' OR path LIKE '%.kt' OR path LIKE '%.kts' \
             OR path LIKE '%.ts' OR path LIKE '%.tsx' \
             OR path LIKE '%.js' OR path LIKE '%.jsx' OR path LIKE '%.mjs' OR path LIKE '%.cjs' \
             OR path LIKE '%.go' OR path LIKE '%.py' ORDER BY repo, path, rev",
            &[],
            |r| Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?.unwrap_or_default())),
        )
    }

    /// SCIP rows that can change call/type resolution for `rev`. SCIP indexes
    /// describe only the working tree, so committed revisions intentionally
    /// return the zero digest. The row framing is part of the persisted extract
    /// digest contract; keep it byte-for-byte aligned with the original fold.
    pub(crate) fn scip_resolution_dependency_digest(&self, rev: &str) -> [u8; 32] {
        let mut acc = [0u8; 32];
        if !self.is_worktree_rev(rev) {
            return acc;
        }
        let rows = self.db.query_rows(
            "scip_ref",
            &format!("SELECT file, symbol, def_file, repo FROM {}", txt_tbl("scip_ref")),
            &[],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            )),
        ).unwrap_or_default();
        for (file, symbol, def_file, repo) in rows {
            let hash = blake3::hash(
                format!("scip\0{file}\0{symbol}\0{def_file}\0{repo}").as_bytes(),
            );
            for (slot, byte) in acc.iter_mut().zip(hash.as_bytes()) {
                *slot ^= *byte;
            }
        }
        acc
    }

    /// Rev-scoped module rows that can change call/type resolution: import
    /// edges for ambiguity narrowing plus resolved alias bindings. Returns one
    /// XOR digest while retaining the two original row-domain prefixes.
    pub(crate) fn module_resolution_dependency_digest(&self, rev: &str) -> [u8; 32] {
        let mut acc = [0u8; 32];
        let edge_rows = self.db.query_rows(
            "module_edge_rev",
            &format!("SELECT src, dst FROM {} WHERE \"rev\" = ?1", txt_tbl("module_edge_rev")),
            &[SqlVal::from(rev)],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        ).unwrap_or_default();
        for (src, dst) in edge_rows {
            let hash = blake3::hash(format!("module\0{src}\0{dst}").as_bytes());
            for (slot, byte) in acc.iter_mut().zip(hash.as_bytes()) {
                *slot ^= *byte;
            }
        }
        let binding_rows = self.db.query_rows(
            "module_binding_resolved_rev",
            &format!(
                "SELECT file, local, source, dst FROM {} WHERE \"rev\" = ?1",
                txt_tbl("module_binding_resolved_rev")
            ),
            &[SqlVal::from(rev)],
            |r| Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            )),
        ).unwrap_or_default();
        for (file, local, source, dst) in binding_rows {
            let hash = blake3::hash(
                format!("binding\0{file}\0{local}\0{source}\0{dst}").as_bytes(),
            );
            for (slot, byte) in acc.iter_mut().zip(hash.as_bytes()) {
                *slot ^= *byte;
            }
        }
        acc
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
        fold(&mut acc, &blake3::hash(format!("{family}\0{rev}").as_bytes()));
        for (repo, path, frev, hash) in files {
            if hash.is_empty() {
                // An empty content hash means the file is unresolved (vanished,
                // unreadable, or a not-yet-scanned path still listed in a root).
                // Fold a STABLE token keyed on identity — NOT a wall-clock nonce.
                // A per-tick nonce made the digest non-deterministic, so an
                // unchanged corpus with even one persistently-empty file never
                // matched its prior digest and re-extracted every file every
                // tick (the 5-repo daemon CPU/battery storm, 2026-07-12). A
                // stable token still flips the digest the tick a file BECOMES or
                // STOPS being empty (its hash column changes), which is the only
                // transition that must re-tick.
                fold(&mut acc, &blake3::hash(format!("unresolved\0{repo}\0{path}\0{frev}").as_bytes()));
                continue;
            }
            fold(&mut acc, &blake3::hash(format!("{repo}\0{path}\0{frev}\0{hash}").as_bytes()));
        }
        if with_scip {
            for dependency in [
                self.scip_resolution_dependency_digest(rev),
                self.module_resolution_dependency_digest(rev),
            ] {
                for (slot, byte) in acc.iter_mut().zip(dependency) {
                    *slot ^= byte;
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
        let start = std::time::Instant::now();
        let mut by_rev: HashMap<&str, Vec<ExtractFile>> = HashMap::new();
        for f in files { by_rev.entry(f.2.as_str()).or_default().push(f.clone()); }
        let mut moved: Vec<(String, [u8; 32])> = Vec::new();
        for (rev, frev) in &by_rev {
            let d = self.extract_input_digest(family, rev, frev, with_scip);
            let prior = self.load_extract_digest(family, rev)?;
            if prior == Some(d) {
                crate::verdict::debug_verdict(
                    "extract-rebuild",
                    &format!("[extract] {family}: skipped (digest match)"),
                    &[("family", family), ("rev", rev), ("outcome", "skip")],
                );
                continue;
            }
            let reason = self.extract_rebuild_reason(family, rev, frev, with_scip, prior.is_none());
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            crate::verdict::verdict(
                "extract-rebuild",
                &format!("[extract] {family}: REBUILD ({reason}) — {} files parsed, {ms:.1}ms", frev.len()),
                &[
                    ("family", family), ("rev", rev), ("outcome", "rebuild"),
                    ("reason", &reason), ("files", &frev.len().to_string()),
                    ("ms", &format!("{ms:.1}")),
                ],
            );
            moved.push(((*rev).to_string(), d));
        }
        Ok(moved)
    }

    /// Cheap, honest attribution for WHY `moved_extract_revs` decided a
    /// family/rev needs a rebuild. Each check is a coarse proxy over one
    /// category of `extract_input_digest`'s inputs (exe identity / scip row
    /// count / an unhashed "nonce" file signaling a rev gaining or losing a
    /// file), checked in that priority order; the common case — some
    /// scanned file's content hash moved — falls through to
    /// `corpus-changed (n paths)`. No category here re-derives a full
    /// content hash of the prior corpus (that would mean keeping a second
    /// copy of the file list around just to explain a digest, which this
    /// stays honest about not doing).
    fn extract_rebuild_reason(&self, family: &str, rev: &str, files: &[ExtractFile], with_scip: bool, first_run: bool) -> String {
        if first_run {
            return "first-run".to_string();
        }
        if files.iter().any(|f| f.3.is_empty()) {
            return "rev-set-changed".to_string();
        }
        if self.exe_identity_changed_since_last_run() {
            return "exe-identity-changed".to_string();
        }
        if with_scip && self.is_worktree_rev(rev) {
            let n: i64 = self.db
                .query_one("scip_ref", &format!("SELECT COUNT(*) FROM {}", txt_tbl("scip_ref")), &[], |r| Ok(r.get(0)?))
                .unwrap_or(0);
            let key = format!("extract:scip-rows:{family}");
            let prior = self.load_rel_digest(&key).ok().flatten();
            let cur = blake3::hash(n.to_string().as_bytes());
            if prior != Some(*cur.as_bytes()) {
                let _ = self.save_rel_digest(&key, cur.as_bytes());
                return "scip-index-changed".to_string();
            }
        }
        format!("corpus-changed ({} paths)", files.len())
    }

    /// Per-Engine, per-tick check of whether the running binary's identity
    /// changed since the last recorded run (the exe stamp `extract_input_
    /// digest` folds into every family's digest). The answer is memoized in
    /// `self.exe_identity_changed` so every family's reason lookup within ONE
    /// tick reads the same answer instead of racing the persisted key, and the
    /// memo is cleared at tick completion so a real binary swap causes exactly
    /// one full-rebuild cycle per Engine. A process-global `OnceLock<bool>` was
    /// wrong: the compared stamp is persisted per-Engine/per-root database, and
    /// a long-lived daemon process serves multiple roots and survives binary
    /// swaps — the global cache pinned `true` for the whole process after the
    /// first mismatch, forcing every tick of every root to rebuild forever.
    fn exe_identity_changed_since_last_run(&self) -> bool {
        if let Some(changed) = self.exe_identity_changed.get() {
            return changed;
        }
        let cur = blake3::hash(format!("{:032x}", exe_stamp()).as_bytes());
        let prior = self.load_rel_digest("extract:exe-stamp").ok().flatten();
        let changed = prior.is_some() && prior != Some(*cur.as_bytes());
        let _ = self.save_rel_digest("extract:exe-stamp", cur.as_bytes());
        self.exe_identity_changed.set(Some(changed));
        changed
    }

    /// Clear the per-tick `exe_identity_changed` memo. Called at the end of
    /// every tick so the next tick re-evaluates the persisted stamp against the
    /// current binary (after a swap the saved stamp now matches, so only the
    /// first post-swap tick reports `true`).
    pub(crate) fn clear_exe_identity_cache(&self) {
        self.exe_identity_changed.set(None);
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
    /// Returns whether the scoped rows actually moved (same contract and same
    /// identical-content skip as `refresh_rel`; the count guard is scoped to
    /// the named revs so out-of-scope rows never mask an external delete).
    fn refresh_rel_for_revs(&self, rel: &str, cols: &[&str], rows: &[Vec<Value>], revs: &[&str]) -> Result<bool> {
        if revs.is_empty() { return Ok(false); }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _rel_refresh_rev(rev TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _rel_refresh_rev")?;
        let rev_rows: Vec<Vec<Value>> = revs.iter().map(|rev| vec![Value::Text((*rev).to_string())]).collect();
        self.db.insert_rows("_rel_refresh_rev", &["rev"], &rev_rows)?;
        let encoded = self.encode_rel_rows(rel, cols, rows)?;
        let mut scope: Vec<&str> = revs.to_vec();
        scope.sort_unstable();
        let content_key = format!("rows:{rel}");
        let digest = crate::engine::rows_content_digest(cols, &encoded, &scope);
        if self.load_rel_digest(&content_key)? == Some(digest) {
            let live: i64 = self.db
                .query_one(
                    rel,
                    &format!("SELECT COUNT(*) FROM {} WHERE \"rev\" IN (SELECT sprf_sym(rev) FROM _rel_refresh_rev)", tbl(rel)),
                    &[], |r| Ok(r.get(0)?),
                )
                .unwrap_or(-1);
            if live == encoded.len() as i64 {
                return Ok(false);
            }
        }
        // The twin's `rev` column is interned (i64 StringId); `_rel_refresh_rev`
        // holds the raw text revs. Hash the text side into id-space so the
        // set-match happens in the same representation (else i64 IN (text…)
        // never matches and the stale rows are never cleared before re-insert).
        self.db.exec(&format!("DELETE FROM {} WHERE \"rev\" IN (SELECT sprf_sym(rev) FROM _rel_refresh_rev)", tbl(rel)))?;
        self.db.insert_rows(&tbl(rel), cols, &encoded)
            .map_err(|error| anyhow::anyhow!("refresh relation {rel}: {error}"))?;
        self.save_rel_digest(&content_key, &digest)?;
        Ok(true)
    }

    /// Every `_rev` twin an extraction family writes: type (node/link/edge),
    /// call (def/edge), df (node/node_repo/arg/field), and the module family's
    /// own pair. One shared list so `sweep_gone_revs` and any future twin
    /// addition touch a single place.
    /// `call_def_rev`/`call_edge_rev` are deliberately absent (P4, capstone
    /// cutover): the call family router is the sole writer of every public
    /// call rel, and a gone rev's call-graph data is retracted at the OWNED
    /// input layer by `Engine::sweep_gone_call_inputs` (below), not by a
    /// public-rel twin delete + legacy rebuild.
    const REV_TWINS: &[&str] = &[
        "type_entity_rev", "type_link_rev", "type_edge_rev", "const_value_rev",
        "df_node_rev", "df_node_repo_rev", "df_arg_rev", "df_field_rev", "df_lit_rev",
        "module_edge_rev", "module_unresolved_rev", "module_binding_resolved_rev", "module_binding_rev",
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
    /// END of `refresh_type_rels`, gated behind the same per-rev digest early
    /// return (`moved.is_empty()`) that skips the whole family when every
    /// currently-live rev's digest is unchanged. A rev that just disappeared
    /// moves nothing in that check, so a family with no other reason to run
    /// this tick would never reach its internal legacy rebuild — the gone
    /// rev's rows would vanish from the twin but linger in legacy for another
    /// tick or more. Rebuilding legacy directly here is what makes a gone rev
    /// disappear from twin AND legacy within the SAME tick, independent of
    /// whether any family's digest moved. `module` now has its own digest gate
    /// (`refresh_module_rels`), so its `extract:module:<rev>` digest is swept
    /// here like the rest — without that, a gone rev's surviving digest would
    /// make the next tick skip repopulating the wiped module rels. `call` has
    /// no legacy rebuild anymore (P4, capstone cutover): its gone-rev sweep
    /// (`sweep_gone_call_inputs`, below) retracts straight from the owned
    /// `_call_*` tables and flips the family router instead.
    ///
    /// Called once per tick, right after `refresh_builtin_rels` settles this
    /// tick's `_file` set and before the extraction families run — see the
    /// call sites in `tick.rs`.
    pub(crate) fn sweep_gone_revs(&self) -> Result<()> {
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _live_rev_scope(rev TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _live_rev_scope")?;
        self.db.exec("INSERT OR IGNORE INTO _live_rev_scope SELECT DISTINCT rev FROM _file")?;

        // `_live_rev_scope` holds raw text revs (from `_file.rev`, which is a
        // hand-rolled table that never interns); each twin's `rev` column is
        // interned (i64). Hash the live text revs into id-space (`sprf_sym`,
        // hash-only, no `_strings` insert) so `i64 NOT IN (id…)` matches the
        // live set — otherwise every twin row reads as gone and the sweep wipes
        // the rows the extraction just wrote.
        for twin in Self::REV_TWINS {
            self.db.exec(&format!(
                "DELETE FROM {} WHERE \"rev\" NOT IN (SELECT sprf_sym(rev) FROM _live_rev_scope)",
                tbl(twin),
            ))?;
        }

        // Extract skip-state for a gone rev, every family at once. A rev that
        // disappeared MUST take its digest with it: the digest certifies "I
        // built and STORED this rev's outputs," and the twin DELETE above just
        // wiped those outputs, so a surviving digest would make the next tick's
        // gate skip repopulating them (the lint-imports/diag_mute regression).
        // `rev` is a column, so this is one indexed set difference rather than
        // the per-family `LIKE` scan with a `substr` projection it replaces, and
        // it can no longer miss a family the old hardcoded list forgot.
        // Only this binary's namespace is swept; a prior binary's stale digests
        // are harmless (its skip-state, never read by this exe).
        self.db.exec_params(
            "_extract_digest",
            "DELETE FROM _extract_digest WHERE exe = ?1 \
             AND rev NOT IN (SELECT sprf_sym(rev) FROM _live_rev_scope)",
            &[extract_digest_exe().into()],
        )?;

        self.rebuild_legacy_type_rels()?;
        self.sweep_gone_call_inputs()?;
        self.rebuild_legacy_module_rels()?;
        Ok(())
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
        let rows = self.db.query_rows(
            "scip_ref",
            &format!("SELECT file, symbol, def_file, repo FROM {}", txt_tbl("scip_ref")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)),
        ).unwrap_or_default();
        for row in rows {
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
        // Definition file per symbol (the def sites the resolver joins into
        // sym_at). Absent scip tables => empty index, name path carries.
        let def_rows = self.db.query_rows(
            "scip_def",
            &format!("SELECT symbol, file, repo FROM {}", txt_tbl("scip_def")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        );
        let Ok(def_rows) = def_rows else {
            return Ok(idx);
        };
        for row in def_rows {
            let (symbol, file, repo) = row;
            idx.def_file_of.entry((repo, symbol)).or_insert(file);
        }
        // Occurrences: (repo, file, 0-based line) -> symbols; cache descriptor
        // names as we go (the moniker parse is repo-independent).
        let occ_rows = self.db.query_rows(
            "scip_occurrence",
            &format!("SELECT file, symbol, line, repo FROM {}", txt_tbl("scip_occurrence")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)),
        ).unwrap_or_default();
        for row in occ_rows {
            let (file, symbol, line, repo) = row;
            if !idx.desc_name.contains_key(&symbol) {
                if let Some(name) = scip_descriptor_name(&symbol) {
                    idx.desc_name.insert(symbol.clone(), name);
                }
            }
            idx.occ_at.entry((repo, file, line)).or_default().push(symbol);
        }
        // Aliased-import local bindings: (repo, file, symbol) -> {local name}.
        let binding_rows = self.db.query_rows(
            "scip_binding",
            &format!("SELECT file, symbol, local_name, repo FROM {}", txt_tbl("scip_binding")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)),
        ).unwrap_or_default();
        for row in binding_rows {
            let (file, symbol, local, repo) = row;
            idx.binding_names.entry((repo, file, symbol)).or_default().insert(local);
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
        let rows = self.db.query_rows(
            "module_edge_rev",
            &format!("SELECT src, dst, \"rev\" FROM {}", txt_tbl("module_edge_rev")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)),
        );
        let Ok(rows) = rows else {
            return Ok(out);
        };
        for row in rows {
            let (src, dst, rev) = row;
            out.entry((rev, src)).or_default().insert(dst);
        }
        Ok(out)
    }

    /// Alias-hop input: `module_binding_resolved_rev(file, local, source, dst, rev)`
    /// read once per family refresh (never per lookup, same shape as
    /// `module_import_map`) into (rev, importing file) -> {local binding name
    /// -> (source name at dst, dst file)}. Empty when the module graph hasn't
    /// populated yet (the hop is then simply a no-op, same as an absent
    /// import map). Index-free equivalent of `scip_binding`: resolves an
    /// aliased import (`use x::y as z`, TS `import { a as b }`/default,
    /// Kotlin `import a.b.C as D`) that the name-keyed `by_name` bucket has
    /// no bucket for (the def's real name is `y`/`a`/`C`, not the local `z`/
    /// `b`/`D`).
    fn module_binding_resolved_map(&self) -> Result<HashMap<(String, String), HashMap<String, (String, String)>>> {
        let mut out: HashMap<(String, String), HashMap<String, (String, String)>> = HashMap::new();
        let rows = self.db.query_rows(
            "module_binding_resolved_rev",
            &format!("SELECT file, local, source, dst, \"rev\" FROM {}", txt_tbl("module_binding_resolved_rev")),
            &[],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, String>(3)?, r.get::<_, String>(4)?)),
        );
        let Ok(rows) = rows else {
            return Ok(out);
        };
        for row in rows {
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
        let const_value = tbl("const_value");
        let const_value_rev = tbl("const_value_rev");
        self.db.exec(&format!("DELETE FROM {const_value}"))?;
        self.db.exec(&format!(
            "INSERT OR IGNORE INTO {const_value} (\"repo\", \"sym\", \"field\", \"text\", \"kind\", \"file\", \"line\") \
             SELECT \"repo\", \"sym\", \"field\", \"text\", \"kind\", \"file\", \"line\" FROM {const_value_rev}"
        ))?;
        Ok(())
    }













}
