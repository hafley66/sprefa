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

    /// Assign a DENSE surrogate id to each distinct dataflow-node coordinate
    /// `(file, line, col, kind)` via the persistent `_df_node_dict`, and return
    /// the coordinate-key -> surrogate map the row writers probe. This replaces
    /// the former `StringId::of(format!("{file}:{line}:{col}:{kind}"))` naked
    /// hash as the node identity: the surrogate is dense (AUTOINCREMENT, zero
    /// hash-collision risk across the corpus's coordinates) and the dict's
    /// `UNIQUE(file, line, col, kind)` is the real key over the coordinate
    /// columns. Content-keyed (`INSERT OR IGNORE` on the tuple), so a re-run
    /// cold-chunk slice or a later wholesale refresh resolves an unchanged
    /// coordinate to the SAME surrogate — the property the cold-slice append
    /// relies on, kept without a corpus-global sequence.
    ///
    /// Core: assign a dense `_df_node_dict` surrogate to each distinct
    /// coordinate TUPLE `(file_text, line, col, kind_text)` and return the
    /// `(file_sid, line, col, kind_sid) -> surrogate` map. `file`/`kind` are
    /// keyed in the dict by their `StringId` (the SAME id the df rels store),
    /// and their TEXT is interned into `_strings` here so `coord_reconstruct`
    /// resolves the coordinate even when no rel row otherwise carries it (a
    /// module-level template's `"template"` kind, say). Batched N+1-safe: one
    /// `_strings` flush + one TEMP-probe load + one `INSERT OR IGNORE` + one
    /// `SELECT` per call, regardless of coordinate count. The dataflow row
    /// writer keys by tuple directly (each node's columns); `resolve_coord_
    /// surrogates` is a thin string-keyed wrapper for the template folder.
    pub(crate) fn resolve_coord_surrogates_tuples(
        &self,
        coords: &[(String, u32, u32, String)],
    ) -> Result<std::collections::HashMap<(i64, u32, u32, i64), i64>> {
        use crate::spine::StringId;
        use std::collections::HashMap;
        if coords.is_empty() {
            return Ok(HashMap::new());
        }
        // Intern every distinct file/kind text so the dict's StringId keys decode
        // back to text at display time (`coord_reconstruct`). Dedup is the
        // sink's + `_strings` cache's job; re-offering an already-interned file
        // (the common df case) is a cache hit.
        let mut sink = crate::spine::SymSink::new();
        let mut probe_rows: Vec<Vec<Value>> = Vec::with_capacity(coords.len());
        let mut seen_tuple: std::collections::HashSet<(i64, u32, u32, i64)> =
            std::collections::HashSet::new();
        for (file_text, line, col, kind_text) in coords {
            let file_sid = StringId::of(file_text).sqlite();
            let kind_sid = StringId::of(kind_text).sqlite();
            if seen_tuple.insert((file_sid, *line, *col, kind_sid)) {
                sink.sym(file_text);
                sink.sym(kind_text);
                probe_rows.push(vec![
                    Value::Int(file_sid),
                    Value::Int(*line as i64),
                    Value::Int(*col as i64),
                    Value::Int(kind_sid),
                ]);
            }
        }
        self.db
            .flush_syms_keyed(&mut sink, "INSERT _strings (spine/source)")?;
        self.db.exec_on(
            "_df_coord_probe",
            "CREATE TEMP TABLE IF NOT EXISTS _df_coord_probe \
             (file INTEGER, line INTEGER, col INTEGER, kind INTEGER)",
        )?;
        self.db
            .exec_on("_df_coord_probe", "DELETE FROM _df_coord_probe")?;
        self.db.insert_rows_keyed(
            "_df_coord_probe",
            "INSERT _df_coord_probe",
            &["file", "line", "col", "kind"],
            &probe_rows,
        )?;
        self.db.exec_on(
            "_df_node_dict",
            "INSERT OR IGNORE INTO _df_node_dict (file, line, col, kind) \
             SELECT DISTINCT file, line, col, kind FROM _df_coord_probe",
        )?;
        let mut tuple_id: HashMap<(i64, u32, u32, i64), i64> =
            HashMap::with_capacity(probe_rows.len());
        self.db.query_rows(
            "_df_node_dict",
            "SELECT p.file, p.line, p.col, p.kind, d.id FROM _df_coord_probe p \
             JOIN _df_node_dict d ON d.file = p.file AND d.line = p.line \
               AND d.col = p.col AND d.kind = p.kind",
            &[],
            |row| {
                let file: i64 = row.get(0)?;
                let line: i64 = row.get(1)?;
                let col: i64 = row.get(2)?;
                let kind: i64 = row.get(3)?;
                let id: i64 = row.get(4)?;
                tuple_id.insert((file, line as u32, col as u32, kind), id);
                Ok(())
            },
        )?;
        Ok(tuple_id)
    }

    /// String-keyed wrapper over `resolve_coord_surrogates_tuples`: maps each
    /// input `(coord_key, file, line, col, kind)` to its coordinate's surrogate.
    /// The template folder (`extract/text.rs`) carries an opaque `coord_key` per
    /// occurrence and needs the string-keyed shape; the dataflow row writer keys
    /// by tuple directly and calls the core.
    pub(crate) fn resolve_coord_surrogates(
        &self,
        coords: &[(String, String, u32, u32, String)],
    ) -> Result<std::collections::HashMap<String, i64>> {
        use crate::spine::StringId;
        use std::collections::HashMap;
        if coords.is_empty() {
            return Ok(HashMap::new());
        }
        let tuples: Vec<(String, u32, u32, String)> = coords
            .iter()
            .map(|(_key, file_text, line, col, kind_text)| {
                (file_text.clone(), *line, *col, kind_text.clone())
            })
            .collect();
        let tuple_id = self.resolve_coord_surrogates_tuples(&tuples)?;
        let mut surrogate: HashMap<String, i64> = HashMap::with_capacity(coords.len());
        for (coord_key, file_text, line, col, kind_text) in coords {
            let key = (
                StringId::of(file_text).sqlite(),
                *line,
                *col,
                StringId::of(kind_text).sqlite(),
            );
            if let Some(&id) = tuple_id.get(&key) {
                surrogate.insert(coord_key.clone(), id);
            }
        }
        Ok(surrogate)
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
        // twins) carry a DENSE surrogate assigned per distinct `(file, line,
        // col, kind)` coordinate by `_df_node_dict` (identity normalization,
        // 2026-07-20) — replacing the former
        // `StringId::of(format!("{file}:{line}:{col}:{kind}"))` naked hash. The
        // coordinate TEXT is still never interned into `_strings`; on display
        // every coord id reconstructs `file:line:col:kind` from the dict
        // (`coord_reconstruct`). `nid(coord_string)` looks the coordinate up in
        // the surrogate map built below (every referenced id IS a df_node, so it
        // is always present; a miss is an identity-invariant break, hence the
        // `?`). The text these coordinates are BUILT from (fn_sym, var, kind,
        // file, ...) still interns normally via `encode_rel_rows`.
        // Node identity is now the dense in-memory INDEX (`DfNode.id: NodeIdx`),
        // never a coordinate string. Resolve each distinct coordinate TUPLE to
        // its `_df_node_dict` surrogate in one batched pass, then look up per
        // node by its columns. A node's in-file index maps to its surrogate via
        // `node_sid` (built per file below); every edge/arg/field/lit reference
        // is an index into the same file's `nodes`, so it resolves through the
        // same vector — no coordinate string is ever minted here.
        use crate::spine::StringId;
        let mut tuple_coords: Vec<(String, u32, u32, String)> = Vec::new();
        for (_, _, _, f) in &facts {
            for n in &f.nodes {
                tuple_coords.push((n.file.clone(), n.line, n.col, n.kind.clone()));
            }
        }
        let tuple_id = self.resolve_coord_surrogates_tuples(&tuple_coords)?;
        let sid_of = |n: &typegraph::DfNode| -> Result<i64> {
            let key = (
                StringId::of(&n.file).sqlite(),
                n.line,
                n.col,
                StringId::of(&n.kind).sqlite(),
            );
            tuple_id.get(&key).copied().ok_or_else(|| {
                anyhow::anyhow!(
                    "dataflow node {}:{}:{}:{} has no _df_node_dict surrogate \
                     (identity invariant: every node coordinate resolves)",
                    n.file,
                    n.line,
                    n.col,
                    n.kind
                )
            })
        };
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
        let mut seen_param: HashSet<i64> = HashSet::new();
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
        // Dedup keys carry the dense SURROGATE (globally unique across files) in
        // place of the former coordinate string; the remaining discriminants
        // (kind/var/fn, rev, repo, text) match each rel's declared PRIMARY KEY.
        let mut seen_node: HashSet<(i64, &str, &str, &str, &str, u32)> = HashSet::new();
        let mut seen_lit: HashSet<(i64, &str, &str)> = HashSet::new();
        let mut seen_edge: HashSet<(i64, i64)> = HashSet::new();
        let mut seen_loop: HashSet<(&str, u32)> = HashSet::new();
        let mut seen_nest: HashSet<(i64, &str)> = HashSet::new();
        let mut seen_node_rev: HashSet<(i64, &str)> = HashSet::new();
        let mut seen_node_repo_rev: HashSet<(i64, &str, &str)> = HashSet::new();
        let mut seen_arg_rev: HashSet<(i64, i64, i64, &str)> = HashSet::new();
        let mut seen_field_rev: HashSet<(i64, &str, i64, &str)> = HashSet::new();
        let mut seen_lit_rev: HashSet<(i64, &str)> = HashSet::new();
        for (repo, _, rev, f) in &facts {
            // Per-file map: node in-file index -> its `_df_node_dict` surrogate.
            let node_sid: Vec<i64> = f
                .nodes
                .iter()
                .map(|n| sid_of(n))
                .collect::<Result<Vec<_>>>()?;
            for n in &f.nodes {
                let id_sid = node_sid[n.id as usize];
                if seen_node.insert((
                    id_sid,
                    n.kind.as_str(),
                    n.var.as_str(),
                    n.fn_sym.as_str(),
                    n.file.as_str(),
                    n.line,
                )) {
                    node_rows.push(vec![
                        Value::Int(id_sid),
                        t(&n.kind),
                        t(&n.var),
                        t(&n.fn_sym),
                        t(&n.file),
                        i(n.line),
                        i(n.col),
                    ]);
                }
                if seen_node_rev.insert((id_sid, rev.as_str())) {
                    node_rev_rows.push(vec![
                        Value::Int(id_sid),
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
                if seen_node_repo_rev.insert((id_sid, repo.as_str(), rev.as_str())) {
                    node_repo_rev_rows.push(vec![Value::Int(id_sid), t(repo), t(rev)]);
                }
            }
            for e in &f.edges {
                let from_sid = node_sid[e.from as usize];
                let to_sid = node_sid[e.to as usize];
                if seen_edge.insert((from_sid, to_sid)) {
                    edge_rows.push(vec![Value::Int(from_sid), Value::Int(to_sid)]);
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
                let call_sid = node_sid[ns.call_id as usize];
                if seen_nest.insert((call_sid, ns.loop_id.as_str())) {
                    nest_rows.push(vec![
                        Value::Int(call_sid),
                        t(&ns.loop_id),
                        i(ns.depth),
                        t(&ns.collection),
                    ]);
                }
            }
            for (id, pos) in &f.param_pos {
                let id_sid = node_sid[*id as usize];
                if seen_param.insert(id_sid) {
                    param_rows.push(vec![Value::Int(id_sid), i(*pos)]);
                }
            }
            for (call, pos, arg) in &f.args {
                // legacy df_arg is now a VIEW over rel_df_arg_rev
                // (`SELECT DISTINCT call, pos, arg`, src/engine/decls.rs); only
                // the `_rev` twin is written. call/arg carry the df_node
                // surrogate (matching df_node_rev.id); rev is its own trailing
                // dedup column.
                let call_sid = node_sid[*call as usize];
                let arg_sid = node_sid[*arg as usize];
                if seen_arg_rev.insert((call_sid, *pos, arg_sid, rev.as_str())) {
                    arg_rev_rows.push(vec![
                        Value::Int(call_sid),
                        Value::Int(*pos),
                        Value::Int(arg_sid),
                        t(rev),
                    ]);
                }
            }
            for (id, field, value) in &f.fields {
                // legacy df_field is now a VIEW over rel_df_field_rev
                // (`SELECT DISTINCT id, field, value`, src/engine/decls.rs); only
                // the `_rev` twin is written. id/value carry the df_node
                // surrogate (matching df_node_rev.id); value is always a value
                // df_node id (never a literal); field is a plain string, never
                // interned.
                let id_sid = node_sid[*id as usize];
                let value_sid = node_sid[*value as usize];
                if seen_field_rev.insert((id_sid, field.as_str(), value_sid, rev.as_str())) {
                    field_rev_rows.push(vec![
                        Value::Int(id_sid),
                        t(field),
                        Value::Int(value_sid),
                        t(rev),
                    ]);
                }
            }
            for (id, text, kind) in &f.lits {
                let id_sid = node_sid[*id as usize];
                if seen_lit.insert((id_sid, text.as_str(), *kind)) {
                    lit_rows.push(vec![Value::Int(id_sid), t(text), t(kind)]);
                }
                // id is the df_node surrogate, matching df_node_rev.id (D5
                // pattern, same shape as df_field_rev); rev is a plain trailing
                // column.
                if seen_lit_rev.insert((id_sid, rev.as_str())) {
                    lit_rev_rows.push(vec![Value::Int(id_sid), t(text), t(kind), t(rev)]);
                }
            }
        }

        // No `_strings` flush for the df ids: they are dense `_df_node_dict`
        // surrogates now, never interned text. The remaining text columns
        // (kind/var/fn/file/lit-text) intern normally in `encode_rel_rows`.

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
        self.append_rel(
            "df_node",
            &["id", "kind", "var", "fn", "file", "line", "col"],
            &rows.node,
        )?;
        // df_node_repo, df_arg, df_field are VIEW-backed (see refresh_dataflow_rows);
        // no base-table append, only their `_rev` twins.
        self.append_rel("df_edge", &["from", "to"], &rows.edge)?;
        self.append_rel(
            "loop_over",
            &["file", "start", "end", "var", "collection", "fn"],
            &rows.loop_over,
        )?;
        self.append_rel("allocates", &["fn"], &rows.alloc)?;
        self.append_rel(
            "nest",
            &["call_id", "loop_id", "depth", "collection"],
            &rows.nest,
        )?;
        self.append_rel("df_param", &["id", "pos"], &rows.param)?;
        self.append_rel("df_lit", &["id", "text", "kind"], &rows.lit)?;
        self.append_rel(
            "df_node_rev",
            &["id", "kind", "var", "fn", "file", "line", "col", "rev"],
            &rows.node_rev,
        )?;
        self.append_rel(
            "df_node_repo_rev",
            &["id", "repo", "rev"],
            &rows.node_repo_rev,
        )?;
        self.append_rel("df_arg_rev", &["call", "pos", "arg", "rev"], &rows.arg_rev)?;
        self.append_rel(
            "df_field_rev",
            &["id", "field", "value", "rev"],
            &rows.field_rev,
        )?;
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
