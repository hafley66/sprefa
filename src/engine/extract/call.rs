use super::*;
use crate::storage::call::{
    CallDefBaseline, CallDefBucketBaseline, CallDeltaOutcome, CallFamilyWrite, CallOwnerBaseline,
    CallOwnerDelta, CallResolvedEdge, CallSiteBaseline, CallStore,
};
use crate::rels::{CallPathRefreshOutcome, PathRefreshContext};

impl Engine {
    pub(crate) fn refresh_call_rels_delta(
        &self,
        context: &PathRefreshContext<'_>,
    ) -> Result<CallPathRefreshOutcome> {
        if context.module_full_refresh {
            return Ok(CallPathRefreshOutcome::Unsupported("call-module-full-refresh"));
        }
        if context.module_dependency_changed {
            return Ok(CallPathRefreshOutcome::Unsupported("call-module-dependency-changed"));
        }
        if context.changed_paths.len() != 1 {
            return Ok(CallPathRefreshOutcome::Unsupported("call-path-count"));
        }
        let path = context.changed_paths.iter().next().unwrap();
        if !self.root.join(path).is_file() {
            return Ok(CallPathRefreshOutcome::Unsupported("call-path-not-existing"));
        }

        let files = self.extract_file_set()?;
        let work_files: Vec<ExtractFile> = files.iter()
            .filter(|file| file.2 == "WORK")
            .cloned()
            .collect();
        let repos: HashSet<&str> = work_files.iter().map(|file| file.0.as_str()).collect();
        if repos.len() != 1 {
            return Ok(CallPathRefreshOutcome::Unsupported("call-multi-repo-corpus"));
        }
        let matching: Vec<&ExtractFile> = work_files.iter()
            .filter(|file| file.1 == *path)
            .collect();
        if matching.len() != 1 {
            return Ok(CallPathRefreshOutcome::Unsupported("call-owner-coordinate"));
        }
        let (repo, path, rev, fact_digest) = matching[0];
        let digest = self.extract_input_digest("call", "WORK", &work_files, true);
        if self.load_rel_digest(&extract_digest_key("call", "WORK"))? == Some(digest) {
            return Ok(CallPathRefreshOutcome::Unchanged);
        }

        let roots = self.repo_roots();
        let froot = roots.get(repo).map(|root| root.as_path()).unwrap_or(&self.root);
        let key = (repo.clone(), path.clone(), fact_digest.clone());
        // Clone out of the cache in its own statement so the immutable borrow is
        // released before the miss branch takes `borrow_mut` (an `if let`
        // scrutinee holds its temporary across the whole `else`).
        let cached = self.call_facts_cache.borrow().get(&key).cloned();
        let (rid, facts) = if let Some((rid, facts)) = cached {
            (rid, facts)
        } else {
            let Some(lang) = typegraph::type_langs().iter().find(|lang| lang.matches(path)) else {
                return Ok(CallPathRefreshOutcome::Unsupported("call-language-unsupported"));
            };
            let content = read_content(froot, rev, path)?;
            let rid = repo_id_of(froot, path, repo);
            let facts = Arc::new(lang.extract_calls(path, &content));
            let mut cache = self.call_facts_cache.borrow_mut();
            cache.retain(|(cached_repo, cached_path, cached_hash), _| {
                cached_repo != repo || cached_path != path || cached_hash == fact_digest
            });
            cache.insert(key, (rid.clone(), facts.clone()));
            (rid, facts)
        };

        let mut sites = Vec::with_capacity(facts.sites.len());
        let mut ordinals: HashMap<(&str, u32), u32> = HashMap::new();
        for site in &facts.sites {
            let caller = facts.defs.iter()
                .filter(|def| def.file == site.file && site.line >= def.line && site.line <= def.end)
                .min_by_key(|def| def.end - def.line)
                .map(|def| format!("{rid}::{}", def.sym))
                .unwrap_or_default();
            let ordinal = ordinals.entry((site.file.as_str(), site.line)).or_default();
            let occurrence = format!("{}:{}:{}", site.file, site.line, *ordinal);
            *ordinal += 1;
            sites.push(CallSiteBaseline {
                occurrence,
                caller,
                callee: site.callee.clone(),
                classification: classify_call_kind(&site.callee).map(str::to_string),
                file: site.file.clone(),
                line: site.line,
                edge: None,
            });
        }
        let owner = CallOwnerBaseline {
            repo: rid,
            rev: rev.clone(),
            path: path.clone(),
            fact_digest: fact_digest.clone(),
            def_digest: call_def_digest(&facts.defs),
            sites,
        };
        let outcome = self.db.apply_call_owner_delta(CallOwnerDelta {
            owner,
            scip_dependency_digest: self.scip_resolution_dependency_digest("WORK"),
            module_dependency_digest: self.module_resolution_dependency_digest("WORK"),
        })?;
        match outcome {
            CallDeltaOutcome::Applied => {
                self.save_rel_digest(&extract_digest_key("call", "WORK"), &digest)?;
                // Flip: the family router is the SOLE writer of the public call
                // rels (P4, capstone cutover), so re-derive them from the
                // _call_* tables the delta just mutated. An owner delta
                // rewrites the owner's sites and their resolutions, touching
                // _call_owner/_call_raw_site/_call_resolution but NOT _call_def
                // (the def set is unchanged — the delta bails otherwise).
                // Passing that exact footprint reruns CallSite/CallEdge (both
                // read _call_raw_site) and SKIPS CallName (footprint
                // {_call_def}), so its call_name rows are kept from the memo
                // instead of rederived — the live skip.
                self.flip_call_rels_via_router(&crate::engine::family::call_owner_delta_rels())?;
                Ok(CallPathRefreshOutcome::Applied)
            }
            CallDeltaOutcome::Unsupported(reason) => {
                Ok(CallPathRefreshOutcome::Unsupported(reason))
            }
        }
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
        let mut fact_digest_by_owner: HashMap<(String, String, String), String> = HashMap::new();
        for (repo, path, rev, digest) in &files {
            let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
            let rid = repo_id_of(froot, path, repo);
            fact_digest_by_owner.insert((rid, rev.clone(), path.clone()), digest.clone());
        }
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
        let mut def_buckets: Vec<CallDefBucketBaseline> = by_name
            .iter()
            .map(|(&(repo, rev, name), syms)| CallDefBucketBaseline {
                repo: repo.to_string(),
                rev: rev.to_string(),
                name: name.to_string(),
                candidate_count: syms.len(),
                unique_sym: (syms.len() == 1).then(|| format!("{repo}::{}", syms[0])),
            })
            .collect();
        def_buckets.sort_by(|a, b| (&a.repo, &a.rev, &a.name).cmp(&(&b.repo, &b.rev, &b.name)));
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
        let aliases = self.module_binding_resolved_map().unwrap_or_default();
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

        let mut defs: Vec<CallDefBaseline> = Vec::new();
        let mut owners: Vec<CallOwnerBaseline> = Vec::with_capacity(facts.len());
        // Dedup carries the rev, so one def present at two revs emits its
        // _call_def row once PER rev — rev is a column, not folded into the
        // sym (same crux as type_entity_rev).
        let mut seen_def: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_owner: HashSet<(&str, &str, &str)> = HashSet::new();
        for (repo, path, rev, f) in &facts {
            let mut owned_sites = Vec::with_capacity(f.sites.len());
            let mut occurrence_ordinals: HashMap<(&str, u32), u32> = HashMap::new();
            for d in &f.defs {
                if seen_def.insert((repo.as_str(), d.sym.as_str(), rev.as_str())) {
                    defs.push(CallDefBaseline {
                        repo: repo.clone(),
                        sym: format!("{repo}::{}", d.sym),
                        name: d.name.clone(),
                        kind: d.kind.tag().to_string(),
                        file: d.file.clone(),
                        line: d.line,
                        end: d.end,
                        rev: rev.clone(),
                    });
                }
            }
            for s in &f.sites {
                // caller resolved when a def encloses the site (repo-qualified);
                // callee kept as written. Both feed the owned `_call_raw_site`
                // row the CallSite/CallEdge/CallKind families project from.
                let caller = resolve_caller(repo, rev, &s.file, s.line).unwrap_or_default();
                // classify the callee's bare name as read/write. The fn-aggregate
                // is the precision axis the conn-loop-reachable rail needs (a fn
                // that only reads through its .conn() does not fire). Heuristic
                // by name: $R.execute(...) is a write on any receiver; the rail's
                // conn_fn join narrows to db-shaped sites.
                let classification = classify_call_kind(&s.callee);
                let ordinal = occurrence_ordinals.entry((s.file.as_str(), s.line)).or_default();
                let occurrence = format!("{}:{}:{}", s.file, s.line, *ordinal);
                *ordinal += 1;
                // The resolved edge: set only when both endpoints resolve to def
                // syms, so closure(call_edge) walks one identity space (same
                // contract as type_link). Unresolved calls stay in call_site
                // (via CallSite's projection) with their bare callee.
                let callee_sym = resolve_callee(repo, rev, &s.file, &s.callee, s.line);
                let edge = callee_sym.and_then(|callee_sym| {
                    (!caller.is_empty()).then(|| CallResolvedEdge {
                        caller: caller.clone(),
                        callee: callee_sym,
                    })
                });
                owned_sites.push(CallSiteBaseline {
                    occurrence,
                    caller,
                    callee: s.callee.clone(),
                    classification: classification.map(str::to_string),
                    file: s.file.clone(),
                    line: s.line,
                    edge,
                });
            }
            if seen_owner.insert((repo.as_str(), rev.as_str(), path.as_str())) {
                owners.push(CallOwnerBaseline {
                    repo: repo.clone(),
                    rev: rev.clone(),
                    path: path.clone(),
                    fact_digest: fact_digest_by_owner
                        .get(&(repo.clone(), rev.clone(), path.clone()))
                        .cloned()
                        .unwrap_or_default(),
                    def_digest: call_def_digest(&f.defs),
                    sites: owned_sites,
                });
            }
        }

        self.db.persist_call_family(CallFamilyWrite {
            owners: &owners,
            def_buckets: &def_buckets,
            defs: &defs,
            scip_dependency_digest: self.scip_resolution_dependency_digest("WORK"),
            module_dependency_digest: self.module_resolution_dependency_digest("WORK"),
        })?;
        // Persisted only after the writes land, so a failed refresh retries.
        for (rev, d) in &moved { self.save_rel_digest(&extract_digest_key("call", rev), d)?; }
        // The family router is the SOLE writer of the public call rels (P4,
        // capstone cutover): derive every one of them from the owned _call_*
        // tables the write above just populated. A full refresh rewrote every
        // input, so pass the whole input set (nothing to skip).
        self.flip_call_rels_via_router(&crate::engine::family::call_input_rels())?;
        Ok(true)
    }

    /// The persistent reactive router flip: `react_deltas` against the
    /// engine's cross-tick memo, then apply each rerun family's `RowDelta`
    /// incrementally (retract + insert) instead of overwriting the relation —
    /// a retracted input row now surfaces as a retracted output row (P5,
    /// capstone cutover: retraction goes live). The one exception is a family
    /// with no memo yet this process (a fresh DB, or the first flip after a
    /// daemon restart): `INSERT OR IGNORE` can never remove stale rows a
    /// prior process already committed to disk, so that family gets an
    /// authoritative `reload_rel` instead — the cold set is the guard.
    /// Returns the rerun rel names (every family `react_deltas` reran,
    /// including ones whose delta came back empty). The SOLE writer of every
    /// public call rel (P4, capstone cutover) — no direct extracted-row write
    /// competes with it anymore. One transaction around the whole render.
    pub(crate) fn flip_call_rels_via_router(
        &self,
        changed: &std::collections::HashSet<&'static str>,
    ) -> Result<Vec<&'static str>> {
        use crate::engine::family;
        use crate::lower::tbl;
        use crate::storage::Storage;

        let mut guard = self.call_router.borrow_mut();
        let router = guard.get_or_insert_with(|| family::FamilyRouter::new(family::call_families()));

        let cold: std::collections::HashSet<&'static str> = family::call_families()
            .iter()
            .filter(|family| router.rows(family.name()).is_none())
            .map(|family| family.name())
            .collect();

        let owns_transaction = self.db.is_autocommit();
        if owns_transaction {
            self.db.begin_immediate()?;
        }
        let result: Result<Vec<&'static str>> = (|| {
            let deltas = router.react_deltas(&self.db, changed)?;
            let mut rerun = Vec::with_capacity(deltas.len());
            for (name, delta) in &deltas {
                // Generic render: the family declares its output columns, so a
                // new family needs no arm here. `out_cols` and `tbl(name)` are
                // the whole routing contract.
                let cols = router
                    .family(name)
                    .ok_or_else(|| anyhow::anyhow!("router produced unrouted family `{name}`"))?
                    .out_cols();
                if cold.contains(name) {
                    self.db.reload_rel(&tbl(name), cols, router.rows(name).unwrap_or(&[]))?;
                } else if !delta.is_empty() {
                    self.db.retract_rows(&tbl(name), cols, &delta.retracted)?;
                    self.db.insert_rows(&tbl(name), cols, &delta.inserted)?;
                }
                rerun.push(*name);
            }
            Ok(rerun)
        })();
        if owns_transaction {
            match &result {
                Ok(_) => self.db.commit()?,
                Err(_) => { let _ = self.db.rollback(); }
            }
        }
        result
    }

    /// P4's call-family half of the rev-retraction sweep (`sweep_gone_revs`,
    /// `src/engine/extract/mod.rs`): a gone rev's call-graph data now
    /// disappears by deleting it straight from the 6 owned `_call_*` tables
    /// (`CallStore::sweep_gone_call_inputs`) rather than through the old
    /// REV_TWINS-delete-then-legacy-rebuild path — there is no legacy path
    /// left to rebuild. Flips the call families through the router only when
    /// a row actually moved, so a no-op sweep costs nothing.
    pub(crate) fn sweep_gone_call_inputs(&self) -> Result<()> {
        let moved = self.db.sweep_gone_call_inputs()?;
        if moved > 0 {
            self.flip_call_rels_via_router(&crate::engine::family::call_input_rels())?;
        }
        Ok(())
    }
}

fn call_def_digest(defs: &[typegraph::CallDef]) -> [u8; 32] {
    let mut identities: Vec<(&str, &str, &str, &str, u32, u32)> = defs
        .iter()
        .map(|def| (
            def.sym.as_str(),
            def.name.as_str(),
            def.kind.tag(),
            def.file.as_str(),
            def.line,
            def.end,
        ))
        .collect();
    identities.sort_unstable();
    let mut hash = blake3::Hasher::new();
    for (sym, name, kind, file, line, end) in identities {
        for value in [sym, name, kind, file] {
            hash.update(&(value.len() as u64).to_le_bytes());
            hash.update(value.as_bytes());
        }
        hash.update(&line.to_le_bytes());
        hash.update(&end.to_le_bytes());
    }
    *hash.finalize().as_bytes()
}
