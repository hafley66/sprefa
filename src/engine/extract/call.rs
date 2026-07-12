use super::*;

impl Engine {
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
        for (rev, d) in &moved { self.save_rel_digest(&extract_digest_key("call", rev), d)?; }
        Ok(true)
    }

    /// Rebuild the convenient rev-less `call_edge` / `call_def` from their
    /// rev-aware twins, deduped across revs. Same shape as
    /// `rebuild_legacy_type_rels`: the `_rev` table is the source of truth, the
    /// legacy rel is the closure/point-query target for the single-rev daemon.
    pub(crate) fn rebuild_legacy_call_rels(&self) -> Result<()> {
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
}
