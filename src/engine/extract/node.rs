use super::*;

impl Engine {
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
        let (node_rows, child_rows, path_by_id, mut str_by_id, wb_by_id) =
            self.node_rows_from_walk(&parsed);

        // Node ids are content-addressed and kind-salted, so each node row is
        // already unique within a tick (a node can't appear twice in one walk;
        // path folds into the id across files). Early-out by comparing the stored
        // id set to the computed one — if identical, no file's tree moved.
        let computed: std::collections::HashSet<String> = node_rows
            .iter()
            .filter_map(|row| {
                if let Value::Text(s) = &row[0] {
                    Some(s.clone())
                } else {
                    None
                }
            })
            .collect();
        let stored: std::collections::HashSet<String> = {
            let conn = self.db.conn();
            let mut s =
                conn.prepare(&format!("SELECT id FROM {}", crate::lower::txt_tbl("node")))?;
            let set: std::collections::HashSet<String> = s
                .query_map([], |r| r.get::<_, String>(0))?
                .filter_map(|x| x.ok())
                .collect();
            set
        };
        if stored == computed {
            // Same node id set means the same content, already interned by a
            // prior run — drop the sink's queued interns unflushed (harmless
            // redundant text) rather than pay a no-op write; drain marks it
            // flushed so the debug Drop guard doesn't fire.
            let _ = str_by_id.drain();
            return Ok(false);
        }

        // Full replace: spine first (so node ids resolve), then a whole-table
        // wipe + reinsert of node/child via refresh_rel. `_node_path` (id->path
        // attribution, not a public rel column) is rebuilt wholesale too so the
        // delta refresh can later prune one file's rows.
        self.flush_node_spine(str_by_id, wb_by_id)?;
        self.refresh_rel(
            "node",
            &["id", "kind", "file", "lo", "hi", "parent"],
            &node_rows,
        )?;
        self.refresh_rel("child", &["parent", "child"], &child_rows)?;
        self.db.exec("DELETE FROM _node_path")?;
        self.db
            .insert_rows("_node_path", &["id", "path"], &path_by_id)?;
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
        if paths.is_empty() {
            return Ok(false);
        }
        let files = self.node_file_set(Some(paths))?;
        let parsed = self.node_walk(&files);
        self.last_node_files_walked.set(parsed.len());
        let (node_rows, child_rows, path_by_id, str_by_id, wb_by_id) =
            self.node_rows_from_walk(&parsed);

        // Prune this tick's changed files' OLD rows: `node` rows whose id is
        // attributed to a changed path (via `_node_path`), plus the `_node_path`
        // rows themselves. `node.file` is a content FileId shared by
        // byte-identical files, so it can't key the prune; `_node_path` keys by
        // the real source path. Other files' node rows stay untouched.
        self.db
            .exec("CREATE TEMP TABLE IF NOT EXISTS _node_refresh_path(path TEXT PRIMARY KEY)")?;
        self.db.exec("DELETE FROM _node_refresh_path")?;
        let path_rows: Vec<Vec<Value>> =
            paths.iter().map(|p| vec![Value::Text(p.clone())]).collect();
        self.db
            .insert_rows("_node_refresh_path", &["path"], &path_rows)?;
        let node_tbl = tbl("node");
        let child_tbl = tbl("child");
        let changed_ids_sql =
            "SELECT id FROM _node_path WHERE path IN (SELECT path FROM _node_refresh_path)";
        // child edges of the changed files: their `child` endpoint id is in the
        // changed-path id set (CST is per-file, so an edge never crosses files).
        // Delete BEFORE pruning `_node_path` so the id subquery still resolves.
        self.db.exec(&format!(
            "DELETE FROM {child_tbl} WHERE \"child\" IN ({changed_ids_sql})"
        ))?;
        self.db.exec(&format!(
            "DELETE FROM {node_tbl} WHERE \"id\" IN ({changed_ids_sql})"
        ))?;
        self.db
            .exec("DELETE FROM _node_path WHERE path IN (SELECT path FROM _node_refresh_path)")?;

        // Spine first (so the new node ids resolve through `ref`/`string`), then
        // the node rows + their `_node_path` attribution, then re-derive `child`
        // so it never references a node id that no longer exists.
        self.flush_node_spine(str_by_id, wb_by_id)?;
        let nodes_changed = !node_rows.is_empty();
        if nodes_changed {
            self.insert_rel_rows(
                "node",
                &["id", "kind", "file", "lo", "hi", "parent"],
                &node_rows,
            )?;
            self.db
                .insert_rows("_node_path", &["id", "path"], &path_by_id)?;
        }
        // Re-insert the fresh walk's child edges (the stale ones were deleted
        // above by the changed-path id set). One plural write; other files'
        // edges untouched (no whole-corpus child rebuild).
        if !child_rows.is_empty() {
            self.insert_rel_rows("child", &["parent", "child"], &child_rows)?;
        }
        Ok(nodes_changed)
    }

    /// The scanned file set for a node walk, keyed by (repo, path, rev, hash).
    /// `only` restricts to a changed-path subset (the delta refresh); `None`
    /// reads every `_file` row (the cold/full refresh).
    pub(crate) fn node_file_set(
        &self,
        only: Option<&HashSet<String>>,
    ) -> Result<Vec<(String, String, String, String)>> {
        let mut files: Vec<(String, String, String, String)> = Vec::new();
        let conn = self.db.conn();
        let mut sel = conn.prepare("SELECT repo, path, rev, hash FROM _file ORDER BY repo, path, rev")?;
        let rows = sel.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        for row in rows.flatten() {
            if let Some(set) = only {
                if !set.contains(&row.1) {
                    continue;
                }
            }
            files.push(row);
        }
        Ok(files)
    }

    /// Per-file parse + tree-sitter walk in parallel (no DB touch). Each yields
    /// the file's node records plus the repo id + path + FileId its spans key off.
    fn node_walk(&self, files: &[(String, String, String, String)]) -> Vec<FileNodes> {
        let root = self.root.clone();
        let roots = self.repo_roots();
        files
            .par_iter()
            .filter_map(|(repo, path, rev, hash)| {
                let label = crate::cst::lang_label_for_path(path)?;
                let lang = ts_lang(label).ok()?;
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let file = spine::FileId::from_content_address(hash, content.len() as i64)
                    .filter(|f| *f != spine::FileId::SYNTHETIC)?;
                let nodes = crate::cst::walk_cst(&content, &lang).ok()?;
                if nodes.is_empty() {
                    return None;
                }
                let rid = repo_id_of(froot, path, repo);
                Some(FileNodes {
                    repo: rid,
                    path: path.clone(),
                    file,
                    content,
                    nodes,
                })
            })
            .collect()
    }

    /// Build the node/child rel rows + the spine (`_strings`/`_where_bytes`)
    /// interns from a parsed walk. Collect-then-flush; no DB touch.
    fn node_rows_from_walk(
        &self,
        parsed: &[FileNodes],
    ) -> (
        Vec<Vec<Value>>,
        Vec<Vec<Value>>,
        Vec<Vec<Value>>,
        spine::SymSink,
        BTreeMap<String, Vec<Value>>,
    ) {
        let mut node_rows: Vec<Vec<Value>> = Vec::new();
        let mut child_rows: Vec<Vec<Value>> = Vec::new();
        // (node id, path) attribution rows for the `_node_path` side table.
        let mut path_by_id: Vec<Vec<Value>> = Vec::new();
        let mut sink = spine::SymSink::new();
        let mut wb_by_id: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for fln in parsed {
            let FileNodes {
                repo,
                path,
                file,
                content,
                nodes,
            } = fln;
            // Pre-compute each node's salted id (an index-aligned Vec) so the
            // child edges reference the parent's id without recomputing.
            let mut ids: Vec<String> = Vec::with_capacity(nodes.len());
            for n in nodes {
                let slice = content.get(n.lo..n.hi).unwrap_or("");
                let raw_sid = spine::StringId::of(slice);
                let raw_wb = spine::WhereBytes {
                    string: raw_sid,
                    file: *file,
                    lo: n.lo as u32,
                    hi: n.hi as u32,
                    ..Default::default()
                };
                let base = spine::WhereBytesId::of_located(raw_wb, repo, path);
                let node_id = base.salted(&n.kind).to_string();
                ids.push(node_id.clone());
                // Spine rows: the `_where_bytes` row uses the SALTED id but the
                // RAW StringId, so ref(node_id) -> string(raw_sid) = raw slice.
                if !slice.is_empty() {
                    let raw = sink.sym(slice);
                    wb_by_id.entry(node_id.clone()).or_insert_with(|| {
                        vec![
                            Value::Text(node_id.clone()),
                            Value::Int(raw.cell()),
                            Value::Text(file.to_string()),
                            Value::Int(n.lo as i64),
                            Value::Int(n.hi as i64),
                            Value::Text(repo.clone()),
                            Value::Text(spine::RevId::default().to_string()),
                            Value::Text(path.clone()),
                        ]
                    });
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
                path_by_id.push(vec![
                    Value::Text(ids[ix].clone()),
                    Value::Text(path.clone()),
                ]);
                if let Some(p) = n.parent_ix {
                    child_rows.push(vec![
                        Value::Text(ids[p].clone()),
                        Value::Text(ids[ix].clone()),
                    ]);
                }
            }
        }
        (node_rows, child_rows, path_by_id, sink, wb_by_id)
    }

    /// Flush the node walk's spine interns: `_strings` (via `Db::flush_syms`)
    /// then `_where_bytes` (INSERT OR IGNORE, content-addressed), so a node id
    /// resolves through `ref`/`string`. One plural write each.
    fn flush_node_spine(
        &self,
        mut sink: spine::SymSink,
        wb_by_id: BTreeMap<String, Vec<Value>>,
    ) -> Result<()> {
        self.db.flush_syms(&mut sink)?;
        if !wb_by_id.is_empty() {
            let wb_rows: Vec<Vec<Value>> = wb_by_id.into_values().collect();
            self.db.insert_rows(
                "_where_bytes",
                &[
                    "id",
                    "string_id",
                    "file_id",
                    "lo",
                    "hi",
                    "repo",
                    "rev",
                    "path",
                ],
                &wb_rows,
            )?;
        }
        Ok(())
    }
}
