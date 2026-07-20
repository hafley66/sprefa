use super::*;

impl Engine {
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
        if moved.is_empty() {
            return Ok(false);
        }
        let rows = self.collect_dataflow_rows(&files)?;
        let changed = self.write_dataflow_wholesale(&files, &rows)?;
        // Persisted only after the writes land, so a failed refresh retries.
        for (rev, digest) in &moved {
            self.save_extract_digest("dataflow", rev, digest)?;
        }
        Ok(changed)
    }

    /// Cold-start chunk write (`cold_stage.rs`): parse `files` (one byte-bounded
    /// slice of the corpus) and APPEND their dataflow rows via `append_rel`
    /// (`INSERT OR IGNORE`) — no wholesale delete, no digest save. dataflow has
    /// no corpus-global resolver (node ids are `file:line:col`-derived and
    /// self-contained), so a per-file slice emits exactly the rows it would in a
    /// whole-corpus pass; the completion gate saves the family digest once every
    /// slice has landed. See the plan's "Addendum 2026-07-18".
    pub(crate) fn refresh_dataflow_rels_slice(&self, files: &[ExtractFile]) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let rows = self.collect_dataflow_rows(files)?;
        self.append_dataflow_rows(&rows)
    }

    /// Cold completion gate: save dataflow's per-rev `extract:` digest over the
    /// whole corpus once every chunk slice has been appended, so the completion
    /// tick's wholesale `refresh_dataflow_rels` sees the family unchanged and
    /// SKIPS it (the rows are already written). The deferred-digest pattern from
    /// the crash-window arc: the digest never lands before its rows.
    pub(crate) fn save_dataflow_cold_digest(&self) -> Result<()> {
        let files = self.extract_file_set()?;
        for (rev, digest) in self.moved_extract_revs("dataflow", &files, false)? {
            self.save_extract_digest("dataflow", &rev, &digest)?;
        }
        Ok(())
    }

    /// Parse `files` and build every dataflow rel's row set (dedup + intern),
    /// flushing the interned id strings to `_strings`. Shared by the wholesale
    /// refresh and the cold-chunk slice append; the ONLY difference between them
    /// is the write path (`write_dataflow_wholesale` vs `append_dataflow_rows`).
    fn collect_dataflow_rows(&self, files: &[ExtractFile]) -> Result<DataflowRowSet> {
        let root = self.root.clone();
        // Read each file from its OWN repo root (same as type/call), so a config
        // repo's WORK content lifts too; reading everything from `self.root`
        // stranded config-repo files at self-root/path (missing -> empty -> zero
        // df rows) or a git blob, never their working tree. The derived repo id
        // (nearest `.git` basename, like type/call) rides each fact so df_node_repo
        // can attribute every node to the folder it lives in.
        let roots = self.repo_roots();
        let facts: Vec<(String, String, String, Arc<typegraph::DataflowFacts>)> =
            cached_facts_profiled(
                &self.df_facts_cache,
                files,
                &self.extract_files_parsed,
                "dataflow",
                |repo, path, rev| {
                    let lang = typegraph::type_langs().iter().find(|l| l.matches(path))?;
                    let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                    let content = read_content(froot, rev, path).unwrap_or_default();
                    let rid = repo_id_of(froot, path, repo);
                    Some((rid, lang.extract_dataflow(path, &content)))
                },
            );

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        // Opaque df-coordinate id columns (df_node.id and every column that
        // carries one: df_edge.from/to, df_arg.call/arg, df_field.id/value,
        // df_param.id, df_lit.id, df_node_repo.id, nest.call_id, and the `_rev`
        // twins — the twin id columns carry the SAME raw id, never a rev-folded
        // string) store the id's blake3 `StringId` hash as the join handle, but
        // the `file:line:col:kind` TEXT is NEVER interned into `_strings` (it was
        // 91.7% of the dictionary). Every such column is declared `Col::node`
        // (coord), so on display it reconstructs from the df_node coordinate
        // columns (`coord_decode`) instead of a `_strings` lookup. `nid` is the
        // pure hash — no queue, no `_strings` row. The text these ids are BUILT
        // from (fn_sym, var, kind, file, ...) still interns normally via
        // `encode_rel_rows` (those columns stay `Value::Text`).
        let nid = |s: &str| Value::Int(crate::spine::StringId::of(s).sqlite());
        let mut node_rows: Vec<Vec<Value>> = Vec::new();
        let mut edge_rows: Vec<Vec<Value>> = Vec::new();
        let mut loop_rows: Vec<Vec<Value>> = Vec::new();
        let mut alloc_rows: Vec<Vec<Value>> = Vec::new();
        let mut nest_rows: Vec<Vec<Value>> = Vec::new();
        let mut param_rows: Vec<Vec<Value>> = Vec::new();
        let mut lit_rows: Vec<Vec<Value>> = Vec::new();
        // Rev-carrying twins (D5.4). `rev` is a plain trailing column and every
        // id-valued column carries the SAME raw id `df_node`/`df_arg`/... use
        // (never salted) — a downstream cross-rel join (`df_arg_rev.call =
        // df_node_rev.id`) matches the legacy join shape exactly, it just needs
        // `AND rev = rev` to stay rev-scoped. Twin dedup keys carry rev, so one
        // file at two revs emits its twin rows once PER rev (the `(id, rev)`
        // primary key on `df_node_rev` is exactly this dedup key).
        let mut node_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut node_repo_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut arg_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut field_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut lit_rev_rows: Vec<Vec<Value>> = Vec::new();
        let mut seen_param: HashSet<&str> = HashSet::new();
        // df_lit identity is the full row (id, text, kind), NOT id alone: across
        // the corpus's live revs the SAME node id can carry divergent text/kind
        // (10 measured), so keying on id would drop the second and make the
        // table narrower than a `SELECT DISTINCT id,text,kind` view. Key on the
        // full declared PRIMARY KEY so table dedup == PK == view-DISTINCT.
        // df_node identity is the full row (id, kind, var, fn, file, line), NOT
        // id alone. id interns `file:line:col:kind`, so var/fn are the only
        // discriminants beyond it — and they DIVERGE across revs (503 measured:
        // a position whose enclosing fn or bound var changed between the
        // committed rev and WORK). var/fn are therefore IDENTITY, not payload.
        // id-only dedup (first-seen wins) drops the second and leaves the table
        // narrower than `SELECT DISTINCT id,kind,var,fn,file,line`. Key on the
        // full PRIMARY KEY tuple so the two agree.
        // (df_node_repo, df_arg, df_field are now views over their _rev twin —
        // their table dedup sets were deleted in the view-backed-rel arc.)
        let mut seen_node: HashSet<(&str, &str, &str, &str, &str, u32)> = HashSet::new();
        let mut seen_lit: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_edge: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_loop: HashSet<(&str, u32)> = HashSet::new();
        let mut seen_nest: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_node_rev: HashSet<(&str, &str)> = HashSet::new();
        let mut seen_node_repo_rev: HashSet<(&str, &str, &str)> = HashSet::new();
        let mut seen_arg_rev: HashSet<(&str, i64, &str, &str)> = HashSet::new();
        let mut seen_field_rev: HashSet<(&str, &str, &str, &str)> = HashSet::new();
        let mut seen_lit_rev: HashSet<(&str, &str)> = HashSet::new();
        for (repo, _, rev, f) in &facts {
            for n in &f.nodes {
                if seen_node.insert((
                    n.id.as_str(),
                    n.kind.as_str(),
                    n.var.as_str(),
                    n.fn_sym.as_str(),
                    n.file.as_str(),
                    n.line,
                )) {
                    node_rows.push(vec![
                        nid(&n.id),
                        t(&n.kind),
                        t(&n.var),
                        t(&n.fn_sym),
                        t(&n.file),
                        i(n.line),
                        i(n.col),
                    ]);
                }
                if seen_node_rev.insert((n.id.as_str(), rev.as_str())) {
                    node_rev_rows.push(vec![
                        nid(&n.id),
                        t(&n.kind),
                        t(&n.var),
                        t(&n.fn_sym),
                        t(&n.file),
                        i(n.line),
                        i(n.col),
                        t(rev),
                    ]);
                }
                // df_node id is `file:line:col` (path only, no repo). Attribute
                // each node to EVERY repo it appears in so a downstream join
                // (member_node field/fill in flow-panel.dl) can scope a fill to
                // its own repo instead of fanning across every repo that shares
                // the constructed type's NAME. Emitted per (id, repo, rev) into
                // the `_rev` twin only; legacy `df_node_repo` is now a VIEW over
                // it (`SELECT DISTINCT id, repo FROM rel_df_node_repo_rev`,
                // src/engine/decls.rs). Two repos with a byte-identical file at a
                // rev share one df_node row but get TWO df_node_repo(_rev) rows,
                // so the join mints the field for BOTH (they both really have it).
                if seen_node_repo_rev.insert((n.id.as_str(), repo.as_str(), rev.as_str())) {
                    node_repo_rev_rows.push(vec![nid(&n.id), t(repo), t(rev)]);
                }
            }
            for e in &f.edges {
                if seen_edge.insert((e.from.as_str(), e.to.as_str())) {
                    edge_rows.push(vec![nid(&e.from), nid(&e.to)]);
                }
            }
            for l in &f.loops {
                if seen_loop.insert((l.file.as_str(), l.start)) {
                    loop_rows.push(vec![
                        t(&l.file),
                        i(l.start),
                        i(l.end),
                        t(&l.var),
                        t(&l.collection),
                        t(&l.fn_sym),
                    ]);
                }
            }
            for fn_sym in &f.allocators {
                alloc_rows.push(vec![t(fn_sym)]);
            }
            for ns in &f.nests {
                if seen_nest.insert((ns.call_id.as_str(), ns.loop_id.as_str())) {
                    nest_rows.push(vec![
                        nid(&ns.call_id),
                        t(&ns.loop_id),
                        i(ns.depth),
                        t(&ns.collection),
                    ]);
                }
            }
            for (id, pos) in &f.param_pos {
                if seen_param.insert(id.as_str()) {
                    param_rows.push(vec![nid(id), i(*pos)]);
                }
            }
            for (call, pos, arg) in &f.args {
                // legacy df_arg is now a VIEW over rel_df_arg_rev
                // (`SELECT DISTINCT call, pos, arg`, src/engine/decls.rs); only
                // the `_rev` twin is written. call/arg carry the raw df_node ids
                // (matching df_node_rev.id); rev is its own trailing dedup column.
                if seen_arg_rev.insert((call.as_str(), *pos, arg.as_str(), rev.as_str())) {
                    arg_rev_rows.push(vec![nid(call), Value::Int(*pos), nid(arg), t(rev)]);
                }
            }
            for (id, field, value) in &f.fields {
                // legacy df_field is now a VIEW over rel_df_field_rev
                // (`SELECT DISTINCT id, field, value`, src/engine/decls.rs); only
                // the `_rev` twin is written. id/value carry the raw df_node ids
                // (matching df_node_rev.id); value is always a value df_node id
                // (never a literal); field is a plain string, never interned.
                if seen_field_rev.insert((
                    id.as_str(),
                    field.as_str(),
                    value.as_str(),
                    rev.as_str(),
                )) {
                    field_rev_rows.push(vec![nid(id), t(field), nid(value), t(rev)]);
                }
            }
            for (id, text, kind) in &f.lits {
                if seen_lit.insert((id.as_str(), text.as_str(), *kind)) {
                    lit_rows.push(vec![nid(id), t(text), t(kind)]);
                }
                // id is the raw df_node id, matching df_node_rev.id (D5 pattern,
                // same shape as df_field_rev); rev is a plain trailing column.
                if seen_lit_rev.insert((id.as_str(), rev.as_str())) {
                    lit_rev_rows.push(vec![nid(id), t(text), t(kind), t(rev)]);
                }
            }
        }

        // No `_strings` flush for the df ids: `nid` hashes only (the coordinate
        // TEXT is never interned — that dictionary bloat is what this arc
        // deletes). The remaining text columns (kind/var/fn/file/lit-text)
        // intern normally in `encode_rel_rows` at write time.

        Ok(DataflowRowSet {
            node: node_rows,
            edge: edge_rows,
            loop_over: loop_rows,
            alloc: alloc_rows,
            nest: nest_rows,
            param: param_rows,
            lit: lit_rows,
            node_rev: node_rev_rows,
            node_repo_rev: node_repo_rev_rows,
            arg_rev: arg_rev_rows,
            field_rev: field_rev_rows,
            lit_rev: lit_rev_rows,
        })
    }

    /// Whole-corpus write: `refresh_rel` (delete + reinsert) for the raw rels and
    /// `refresh_rel_for_revs` (rev-scoped delete + reinsert) for the `_rev` twins.
    fn write_dataflow_wholesale(
        &self,
        files: &[ExtractFile],
        rows: &DataflowRowSet,
    ) -> Result<bool> {
        let mut rows_changed = false;
        rows_changed |= self.refresh_rel(
            "df_node",
            &["id", "kind", "var", "fn", "file", "line", "col"],
            &rows.node,
        )?;
        // df_node_repo, df_arg, df_field are VIEW-backed (VIEWs over their `_rev`
        // twins, src/engine/decls.rs); no base-table write. Their twin refreshes
        // below still flip `rows_changed`, so dependents re-derive.
        rows_changed |= self.refresh_rel("df_edge", &["from", "to"], &rows.edge)?;
        rows_changed |= self.refresh_rel(
            "loop_over",
            &["file", "start", "end", "var", "collection", "fn"],
            &rows.loop_over,
        )?;
        rows_changed |= self.refresh_rel("allocates", &["fn"], &rows.alloc)?;
        rows_changed |= self.refresh_rel(
            "nest",
            &["call_id", "loop_id", "depth", "collection"],
            &rows.nest,
        )?;
        rows_changed |= self.refresh_rel("df_param", &["id", "pos"], &rows.param)?;
        rows_changed |= self.refresh_rel("df_lit", &["id", "text", "kind"], &rows.lit)?;
        // Rev-carrying twins: same delete scope as type/call — wipe every corpus
        // rev and reinsert the whole-corpus rows (the emit above is whole-corpus;
        // a rev absent from the corpus is D5.5's retraction sweep, not this
        // path). Legacy df rels above stay a separate raw-id write (deduped
        // across ALL revs by id alone, first-seen wins) rather than being
        // derived from the twins, so this is not a rebuild-from-twin path.
        let all_revs = Self::corpus_revs(files);
        let all_rev_refs: Vec<&str> = all_revs.iter().map(|s| s.as_str()).collect();
        rows_changed |= self.refresh_rel_for_revs(
            "df_node_rev",
            &["id", "kind", "var", "fn", "file", "line", "col", "rev"],
            &rows.node_rev,
            &all_rev_refs,
        )?;
        rows_changed |= self.refresh_rel_for_revs(
            "df_node_repo_rev",
            &["id", "repo", "rev"],
            &rows.node_repo_rev,
            &all_rev_refs,
        )?;
        rows_changed |= self.refresh_rel_for_revs(
            "df_arg_rev",
            &["call", "pos", "arg", "rev"],
            &rows.arg_rev,
            &all_rev_refs,
        )?;
        rows_changed |= self.refresh_rel_for_revs(
            "df_field_rev",
            &["id", "field", "value", "rev"],
            &rows.field_rev,
            &all_rev_refs,
        )?;
        rows_changed |= self.refresh_rel_for_revs(
            "df_lit_rev",
            &["id", "text", "kind", "rev"],
            &rows.lit_rev,
            &all_rev_refs,
        )?;
        Ok(rows_changed)
    }

    /// Cold-chunk write: `append_rel` (`INSERT OR IGNORE`, no delete) for every
    /// rel, raw and `_rev` twin alike. The rel starts empty at cold start and the
    /// slices are disjoint by file, so an append is equivalent to the wholesale
    /// reinsert; `INSERT OR IGNORE` covers a crash-resumed slice and the rare
    /// content-addressed id shared across two slices.
    fn append_dataflow_rows(&self, rows: &DataflowRowSet) -> Result<()> {
        self.append_rel("df_node", &["id", "kind", "var", "fn", "file", "line", "col"], &rows.node)?;
        // df_node_repo, df_arg, df_field are VIEW-backed (see refresh_dataflow_rows);
        // no base-table append, only their `_rev` twins.
        self.append_rel("df_edge", &["from", "to"], &rows.edge)?;
        self.append_rel(
            "loop_over",
            &["file", "start", "end", "var", "collection", "fn"],
            &rows.loop_over,
        )?;
        self.append_rel("allocates", &["fn"], &rows.alloc)?;
        self.append_rel("nest", &["call_id", "loop_id", "depth", "collection"], &rows.nest)?;
        self.append_rel("df_param", &["id", "pos"], &rows.param)?;
        self.append_rel("df_lit", &["id", "text", "kind"], &rows.lit)?;
        self.append_rel(
            "df_node_rev",
            &["id", "kind", "var", "fn", "file", "line", "col", "rev"],
            &rows.node_rev,
        )?;
        self.append_rel("df_node_repo_rev", &["id", "repo", "rev"], &rows.node_repo_rev)?;
        self.append_rel("df_arg_rev", &["call", "pos", "arg", "rev"], &rows.arg_rev)?;
        self.append_rel("df_field_rev", &["id", "field", "value", "rev"], &rows.field_rev)?;
        self.append_rel("df_lit_rev", &["id", "text", "kind", "rev"], &rows.lit_rev)?;
        Ok(())
    }
}

/// Every dataflow rel's built rows from one `collect_dataflow_rows` pass, split
/// so the wholesale and cold-chunk write paths share the parse+emit exactly.
struct DataflowRowSet {
    // df_node_repo, df_arg, df_field are VIEW-backed (VIEWs over their `_rev`
    // twins, src/engine/decls.rs), so their non-rev rows are never collected or
    // written; only the `_rev` fields below feed them. df_node and df_lit stay
    // base tables (df_node_rev's (id,rev) key can't reproduce df_node's full-row
    // dedup — see decls.rs), so their non-rev rows ARE collected/written.
    node: Vec<Vec<Value>>,
    edge: Vec<Vec<Value>>,
    loop_over: Vec<Vec<Value>>,
    alloc: Vec<Vec<Value>>,
    nest: Vec<Vec<Value>>,
    param: Vec<Vec<Value>>,
    lit: Vec<Vec<Value>>,
    node_rev: Vec<Vec<Value>>,
    node_repo_rev: Vec<Vec<Value>>,
    arg_rev: Vec<Vec<Value>>,
    field_rev: Vec<Vec<Value>>,
    lit_rev: Vec<Vec<Value>>,
}
