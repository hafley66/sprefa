use super::*;

impl Engine {
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
        if moved.is_empty() {
            return Ok(false);
        }
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
        let facts: Vec<(String, String, String, Arc<typegraph::TypeFacts>)> = cached_facts(
            &self.type_facts_cache,
            &files,
            &self.extract_files_parsed,
            |repo, path, rev| {
                let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                Some((rid, lang.extract(path, &content)))
            },
        );

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
                let bucket = by_name
                    .entry((repo.as_str(), rev.as_str(), e.name.as_str()))
                    .or_default();
                if !bucket.iter().any(|s| *s == e.sym.as_str()) {
                    bucket.push(e.sym.as_str());
                }
                sym_at.insert(
                    (
                        repo.as_str(),
                        e.file.as_str(),
                        rev.as_str(),
                        e.name.as_str(),
                    ),
                    e.sym.as_str(),
                );
                sym_file.insert(
                    (repo.as_str(), rev.as_str(), e.sym.as_str()),
                    e.file.as_str(),
                );
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
        // once for the whole family, see `module_binding_resolved_map`.
        let aliases = self.module_binding_resolved_map().unwrap_or_default();
        // See `refresh_call_rels`: the SCIP override describes the working tree
        // only, and a rev is an oid after alias resolution.
        let work_revs = self.worktree_rev_texts.clone();
        let resolve = |repo: &str, rev: &str, file: &str, name: &str| -> Option<String> {
            if work_revs.contains(rev) {
                if let Some(def_file) =
                    scip.get(&(repo.to_string(), file.to_string(), name.to_string()))
                {
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
                if let Some((source, dst)) = aliases
                    .get(&(rev.to_string(), file.to_string()))
                    .and_then(|m| m.get(name))
                {
                    return sym_at
                        .get(&(repo, dst.as_str(), rev, source.as_str()))
                        .map(|sym| format!("{repo}::{sym}"));
                }
            }
            match by_name.get(&(repo, rev, name)) {
                Some(v) if v.len() == 1 => Some(format!("{repo}::{}", v[0])),
                // More than one candidate: narrow to the referencing file's own
                // import neighborhood (Win D) before giving up bare.
                Some(v) if v.len() > 1 => narrow_ambiguous(v, repo, rev, file, &sym_file, &imports)
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
        let mut const_rev_rows: Vec<Vec<Value>> = Vec::new();
        // (repo-qualified owning sym, field, rev): one row per string-valued
        // leaf per rev, same shape as `seen_entity`.
        let mut seen_const: HashSet<(String, &str, &str)> = HashSet::new();
        let mut const_spread_skips: usize = 0;
        let mut const_mutable_skips: usize = 0;
        for (repo, path, rev, f) in &facts {
            // historic name-keyed edges, now repo-tagged so two trees sharing a
            // type name don't collapse into one node when scanned together
            // (closure/scc still walk cols[0]/cols[1] = from/to, untouched).
            for edge in &f.edges {
                edge_rev_rows.push(vec![
                    t(&edge.from),
                    t(&edge.to),
                    t(edge.kind),
                    t(rev),
                    t(repo),
                ]);
                // SCIP-resolved graph: owner sym -> resolved target sym (or the
                // bare name when external/ambiguous, so leaf types still appear)
                let src = sym_at
                    .get(&(
                        repo.as_str(),
                        path.as_str(),
                        rev.as_str(),
                        edge.from.as_str(),
                    ))
                    .map(|s| format!("{repo}::{s}"))
                    .unwrap_or_else(|| edge.from.clone());
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
                    let qparent = ent
                        .parent
                        .as_deref()
                        .map(|p| {
                            let owner_name = p.rsplit("::").next().unwrap_or(p);
                            let same_file = sym_at.get(&(
                                repo.as_str(),
                                ent.file.as_str(),
                                rev.as_str(),
                                owner_name,
                            )) == Some(&p);
                            if same_file {
                                format!("{repo}::{p}")
                            } else {
                                resolve(repo, rev, &ent.file, owner_name)
                                    .unwrap_or_else(|| format!("{repo}::{p}"))
                            }
                        })
                        .unwrap_or_default();
                    entity_rev_rows.push(vec![
                        t(repo),
                        t(&qsym),
                        t(&ent.name),
                        t(ent.kind.tag()),
                        t(&qparent),
                        t(&ent.file),
                        i(ent.line),
                        t(rev),
                    ]);
                }
                // the arrow [...A] => B, one row per referenced type per slot
                if let Some(ty) = &ent.ty {
                    for (pos, slot) in ty.params.iter().enumerate() {
                        for r in slot {
                            let rf = resolve(repo, rev, path, r.name())
                                .unwrap_or_else(|| r.name().to_string());
                            sig_rows.push(vec![t(&qsym), t("param"), i(pos as u32), t(&rf)]);
                        }
                    }
                    for r in &ty.ret {
                        let rf = resolve(repo, rev, path, r.name())
                            .unwrap_or_else(|| r.name().to_string());
                        sig_rows.push(vec![t(&qsym), t("ret"), i(0), t(&rf)]);
                    }
                }
            }
            // Doc comments per entity (Tier 1) + their structured tags (Tier 2).
            // Same repo-qualified sym + first-seen dedup as the entity rows, so a
            // file present at two revs doesn't duplicate (doc_comment has no rev).
            for doc in &f.docs {
                if !seen_doc.insert((repo.as_str(), doc.sym.as_str())) {
                    continue;
                }
                let qsym = format!("{repo}::{}", doc.sym);
                doc_rows.push(vec![t(repo), t(&qsym), i(doc.line), t(&doc.text)]);
                for tag in &doc.tags {
                    tag_rows.push(vec![
                        t(repo),
                        t(&qsym),
                        t(&tag.tag),
                        t(&tag.arg),
                        t(&tag.text),
                    ]);
                }
            }
            // String values folded from const/as-const bindings (item 3). sym
            // is repo-qualified the same way entity syms are, so const_value
            // joins type_entity 1:1 on the const's own row (or the enum's row
            // for a string member).
            for c in &f.consts {
                let qsym = format!("{repo}::{}", c.sym);
                if seen_const.insert((qsym.clone(), c.field.as_str(), rev.as_str())) {
                    const_rev_rows.push(vec![
                        t(repo),
                        t(&qsym),
                        t(&c.field),
                        t(&c.text),
                        t(c.kind),
                        t(&c.file),
                        i(c.line),
                        t(rev),
                    ]);
                }
            }
            const_spread_skips += f.const_spread_skips;
            const_mutable_skips += f.const_mutable_skips;
        }
        if const_spread_skips > 0 {
            let suffix = if const_spread_skips == 1 { "y" } else { "ies" };
            tracing::warn!(
                const_spread_skips,
                suffix = %suffix,
                "[typegraph:const_value] {const_spread_skips} object-literal spread propert{suffix} skipped (never followed — the value is opaque without evaluating the spread source)"
            );
        }
        if const_mutable_skips > 0 {
            let suffix = if const_mutable_skips == 1 { "" } else { "s" };
            tracing::warn!(
                const_mutable_skips,
                suffix = %suffix,
                "[typegraph:const_value] {const_mutable_skips} let/var string initializer{suffix} skipped (soundness rule: only const/as const bindings are folded)"
            );
        }
        // type_edge_rev is the rev-carrying twin: write it through the rev-scoped
        // helper (the real in-tree consumer). Delete scope = every corpus rev;
        // the emit above is whole-corpus in D5.1, so wiping all corpus revs and
        // reinserting all rows is equivalent to a full `refresh_rel` wipe (a rev
        // absent from the corpus is D5.5's retraction sweep, not this path).
        let all_revs = Self::corpus_revs(&files);
        let all_rev_refs: Vec<&str> = all_revs.iter().map(|s| s.as_str()).collect();
        // OR of per-rel content movement: a rebuild that reproduced identical
        // rows (binary swap, spurious digest miss) marks nothing changed, so
        // the legacy rebuild and the derived cascade both stay skipped.
        let mut rows_changed = false;
        rows_changed |= self.refresh_rel_for_revs(
            "type_edge_rev",
            &["from", "to", "kind", "rev", "repo"],
            &edge_rev_rows,
            &all_rev_refs,
        )?;
        rows_changed |= self.refresh_rel_for_revs(
            "type_entity_rev",
            &[
                "repo", "sym", "name", "kind", "parent", "file", "line", "rev",
            ],
            &entity_rev_rows,
            &all_rev_refs,
        )?;
        rows_changed |= self.refresh_rel("type_sig", &["sym", "slot", "pos", "ref"], &sig_rows)?;
        rows_changed |= self.refresh_rel_for_revs(
            "type_link_rev",
            &["src", "dst", "kind", "rev"],
            &link_rev_rows,
            &all_rev_refs,
        )?;
        rows_changed |= self.refresh_rel("doc_comment", &["repo", "sym", "line", "text"], &doc_rows)?;
        rows_changed |= self.refresh_rel("doc_tag", &["repo", "sym", "tag", "arg", "text"], &tag_rows)?;
        rows_changed |= self.refresh_rel_for_revs(
            "const_value_rev",
            &[
                "repo", "sym", "field", "text", "kind", "file", "line", "rev",
            ],
            &const_rev_rows,
            &all_rev_refs,
        )?;
        if rows_changed {
            self.rebuild_legacy_type_rels()?;
        }
        // Persisted only after the writes land, so a failed refresh retries.
        for (rev, d) in &moved {
            self.save_extract_digest("type", rev, d)?;
        }
        Ok(rows_changed)
    }
}
