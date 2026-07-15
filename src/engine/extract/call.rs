use super::*;
use crate::storage::call::{
    CallDefBucketBaseline, CallDeltaOutcome, CallFamilyWrite, CallOwnerBaseline,
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
        let (rid, facts) = if let Some((rid, facts)) = self.call_facts_cache.borrow().get(&key) {
            (rid.clone(), facts.clone())
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

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut def_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut site_rows: Vec<Vec<Value>> = Vec::new();
        let mut edge_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut name_rows: Vec<Vec<Value>> = Vec::new();
        let mut owners: Vec<CallOwnerBaseline> = Vec::with_capacity(facts.len());
        // call_kind is keyed by (caller, kind); a fn with both a read and a
        // write emits two rows. Accumulate in a set so multiple write sites in
        // the same fn collapse to one (fn, "write") row.
        let mut kind_set: HashSet<(String, &'static str)> = HashSet::new();
        // Dedup carries the rev, so one def present at two revs emits its
        // call_def_rev row once PER rev — rev is a column, not folded into the
        // sym (same crux as type_entity_rev).
        let mut seen_def: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_edge: HashSet<(String, String, &str)> = HashSet::new();
        let mut seen_owner: HashSet<(&str, &str, &str)> = HashSet::new();
        for (repo, path, rev, f) in &facts {
            let mut owned_sites = Vec::with_capacity(f.sites.len());
            let mut occurrence_ordinals: HashMap<(&str, u32), u32> = HashMap::new();
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
                let classification = classify_call_kind(&s.callee);
                if !caller.is_empty() {
                    if let Some(k) = classification {
                        kind_set.insert((caller.clone(), k));
                    }
                }
                let ordinal = occurrence_ordinals.entry((s.file.as_str(), s.line)).or_default();
                let occurrence = format!("{}:{}:{}", s.file, s.line, *ordinal);
                *ordinal += 1;
                // call_edge is the resolved graph: emit only when both endpoints
                // resolve to def syms, so closure(call_edge) walks one identity
                // space (same contract as type_link). Unresolved calls stay in
                // call_site with their bare callee.
                let callee_sym = resolve_callee(repo, rev, &s.file, &s.callee, s.line);
                let edge = if let Some(callee_sym) = callee_sym {
                    if !caller.is_empty() && seen_edge.insert((caller.clone(), callee_sym.clone(), rev)) {
                        edge_rev_rows.push(vec![t(&caller), t(&callee_sym), t("call"), t(rev)]);
                    }
                    (!caller.is_empty()).then(|| CallResolvedEdge {
                        caller: caller.clone(),
                        callee: callee_sym,
                    })
                } else {
                    None
                };
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
        let def_rev_rows = self.encode_rel_rows(
            "call_def_rev",
            &["repo", "sym", "kind", "file", "line", "end", "rev"],
            &def_rev_rows,
        )?;
        let site_rows = self.encode_rel_rows(
            "call_site",
            &["repo", "caller", "callee", "file", "line"],
            &site_rows,
        )?;
        let edge_rev_rows = self.encode_rel_rows(
            "call_edge_rev",
            &["caller", "callee", "kind", "rev"],
            &edge_rev_rows,
        )?;
        let name_rows = self.encode_rel_rows("call_name", &["sym", "name"], &name_rows)?;
        let kind_rows = self.encode_rel_rows("call_kind", &["fn", "kind"], &kind_rows)?;
        self.db.persist_call_family(CallFamilyWrite {
            def_rev_rows: &def_rev_rows,
            site_rows: &site_rows,
            edge_rev_rows: &edge_rev_rows,
            name_rows: &name_rows,
            kind_rows: &kind_rows,
            revs: &all_revs,
            owners: &owners,
            def_buckets: &def_buckets,
            scip_dependency_digest: self.scip_resolution_dependency_digest("WORK"),
            module_dependency_digest: self.module_resolution_dependency_digest("WORK"),
        })?;
        // Persisted only after the writes land, so a failed refresh retries.
        for (rev, d) in &moved { self.save_rel_digest(&extract_digest_key("call", rev), d)?; }
        Ok(true)
    }

    /// Keep rev-sweep callers on the same storage-owned compatibility rebuild
    /// used by the bundled call-family write.
    pub(crate) fn rebuild_legacy_call_rels(&self) -> Result<()> {
        CallStore::rebuild_legacy_call_rels(&self.db)
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
