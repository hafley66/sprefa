use super::*;

/// `comment_node`'s declared column order, shared by the wholesale write
/// (`refresh_rel`) and the cold-chunk append (`append_rel`) so the two paths
/// can never drift.
const COMMENT_NODE_COLS: [&str; 7] = ["path", "line", "col", "end_line", "end_col", "text", "kind"];
/// `template_parts`' declared column order (see `COMMENT_NODE_COLS`).
const TEMPLATE_PARTS_COLS: [&str; 6] = ["file", "line", "node", "idx", "kind", "text"];
/// `unresolved`'s declared column order (see `COMMENT_NODE_COLS`).
const UNRESOLVED_COLS: [&str; 4] = ["file", "line", "reason", "detail"];

impl Engine {
    /// The comment corpus: every `_file` row whose path has a grammar the
    /// comment walk can parse — the oxc TS/TSX front-end (`.ts`/`.tsx`) or one of
    /// the tree-sitter grammars `cst::lang_label_for_path` recognizes (Rust,
    /// Kotlin, Python, Go, C, bash, ...). Scoping the set here (rather than
    /// reading all of `_file`) keeps the family's input digest from moving when
    /// an unparseable file (`.md`, `.json`) is edited.
    ///
    /// `pub(crate)` (not just this module): `cold_stage.rs`'s per-family chunk
    /// partitioning needs comment's OWN file set, which is a strict superset of
    /// `extract_file_set()` (adds `.md` and the broader `AST_LANG_TABLE`
    /// grammars) — using the corpus-wide set would silently under-chunk (and
    /// under-cover) this family.
    pub(crate) fn comment_file_set(&self) -> Result<Vec<ExtractFile>> {
        let mut files: Vec<ExtractFile> = Vec::new();
        let rows = self.db.query_rows(
            "_file",
            "SELECT repo, path, rev, hash FROM _file ORDER BY repo, path, rev",
            &[],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                ))
            },
        )?;
        for row in rows {
            let p = row.1.as_str();
            let ts = p.ends_with(".ts")
                || p.ends_with(".tsx")
                || p.ends_with(".mts")
                || p.ends_with(".cts");
            let md = p.ends_with(".md") || p.ends_with(".markdown");
            if ts || md || crate::cst::lang_label_for_path(p).is_some() {
                files.push(row);
            }
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
        if moved.is_empty() {
            return Ok(false);
        }
        let rows = self.collect_comment_rows(&files)?;
        self.refresh_rel("comment_node", &COMMENT_NODE_COLS, &rows)?;
        for (rev, d) in &moved {
            self.save_extract_digest("comment", rev, d)?;
        }
        Ok(true)
    }

    /// Cold-start chunk write (`cold_stage.rs`): parse `files` (one byte-bounded
    /// slice of `comment_file_set()`) and APPEND their comment rows via
    /// `append_rel` — no wholesale delete, no digest save. `comment_node` has no
    /// corpus-global resolver (each row is a pure function of its own file's
    /// bytes), so a per-file slice emits exactly the rows a whole-corpus pass
    /// would; the completion gate saves the family digest once every slice has
    /// landed. Mirrors `Engine::refresh_dataflow_rels_slice`.
    pub(crate) fn refresh_comment_rels_slice(&self, files: &[ExtractFile]) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let rows = self.collect_comment_rows(files)?;
        self.append_rel("comment_node", &COMMENT_NODE_COLS, &rows)
    }

    /// Cold completion gate: save comment's per-rev `extract:` digest over the
    /// whole `comment_file_set()` once every chunk slice has been appended, so
    /// the completion tick's wholesale `refresh_comment_rels` sees the family
    /// unchanged and SKIPS it. Mirrors `Engine::save_dataflow_cold_digest`.
    pub(crate) fn save_comment_cold_digest(&self) -> Result<()> {
        let files = self.comment_file_set()?;
        for (rev, d) in self.moved_extract_revs("comment", &files, false)? {
            self.save_extract_digest("comment", &rev, &d)?;
        }
        Ok(())
    }

    /// Parse `files` and build `comment_node`'s row set (dedup by span). Shared
    /// by the wholesale refresh and the cold-chunk slice append; the ONLY
    /// difference between them is the write path (`refresh_rel` vs `append_rel`).
    fn collect_comment_rows(&self, files: &[ExtractFile]) -> Result<Vec<Vec<Value>>> {
        let root = self.root.clone();
        let roots = self.repo_roots();
        // Per-file comment walk in parallel; TS/TSX via oxc, everything else via
        // the shared tree-sitter walk. Unchanged files come from the cache.
        let facts: Vec<(String, String, String, Arc<Vec<crate::cst::RawComment>>)> = cached_facts(
            &self.comment_facts_cache,
            files,
            &self.extract_files_parsed,
            |repo, path, rev| {
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                let comments = if path.ends_with(".ts") || path.ends_with(".tsx") {
                    typegraph::ts_comments(&content, path.ends_with(".tsx"))
                } else if path.ends_with(".md") || path.ends_with(".markdown") {
                    crate::cst::walk_md_comments(&content).unwrap_or_default()
                } else {
                    let label = crate::cst::lang_label_for_path(path)?;
                    let lang = ts_lang(label).ok()?;
                    crate::cst::walk_comments(&content, &lang).unwrap_or_default()
                };
                Some((rid, comments))
            },
        );

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        // Dedup by span across revs (no rev column): a file scanned at two revs
        // emits its comments once. Keyed on (path, span) — a comment can't occur
        // twice at one span in one file.
        let mut seen: HashSet<(String, u32, u32, u32, u32)> = HashSet::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for (_repo, path, _rev, comments) in &facts {
            tracing::trace!(path = %path, comments = comments.len(), "[extract] comment fold file");
            for c in comments.iter() {
                if !seen.insert((path.clone(), c.start_row, c.start_col, c.end_row, c.end_col)) {
                    continue;
                }
                let (kind, text) = crate::cst::classify_comment(&c.raw);
                rows.push(vec![
                    t(path),
                    i(c.start_row),
                    i(c.start_col),
                    i(c.end_row),
                    i(c.end_col),
                    t(&text),
                    t(kind),
                ]);
            }
        }
        Ok(rows)
    }

    /// The `template_parts` corpus: every `_file` row the oxc TS front-end
    /// parses (`.ts`/`.tsx`/`.js`/`.jsx`/`.mjs`/`.cjs` — the same extension set
    /// `TsTypes::matches` claims). Template literals are TS/JS-only syntax
    /// (Kotlin string templates and Rust's `format!`-style macros are a
    /// different grammar entirely and are explicitly OUT of scope), so scoping
    /// the set here keeps the family's input digest from moving when an
    /// unrelated file (`.rs`, `.kt`, `.md`) is edited.
    ///
    /// `pub(crate)` — `cold_stage.rs` chunks against this family-scoped set
    /// (see `comment_file_set`'s doc for why the corpus-wide `extract_file_set`
    /// is the wrong set to partition).
    pub(crate) fn template_file_set(&self) -> Result<Vec<ExtractFile>> {
        let mut files: Vec<ExtractFile> = Vec::new();
        let rows = self.db.query_rows(
            "_file",
            "SELECT repo, path, rev, hash FROM _file ORDER BY repo, path, rev",
            &[],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                ))
            },
        )?;
        for row in rows {
            let p = row.1.as_str();
            if p.ends_with(".ts")
                || p.ends_with(".tsx")
                || p.ends_with(".js")
                || p.ends_with(".jsx")
                || p.ends_with(".mjs")
                || p.ends_with(".cjs")
                || p.ends_with(".mts")
                || p.ends_with(".cts")
            {
                files.push(row);
            }
        }
        Ok(files)
    }

    /// Rebuild `template_parts` — every template-literal occurrence's ordered
    /// static/interpolated pieces, across every TS/JS-family file. Own family
    /// (not riding the `type`/`call`/`dataflow` TypeLang passes): a program
    /// reading only `template_parts` shouldn't pay for those. Same perf shape
    /// as `refresh_comment_rels`: per-rev input-digest skip, per-file fact
    /// cache, parallel parse, one batched `refresh_rel`.
    ///
    /// `template_parts` has no rev column (like `comment_node`): a file present
    /// at two revs unions its template occurrences, deduped by (path, node,
    /// idx) — a piece can't recur twice at the same slot of the same
    /// occurrence in one file.
    pub(crate) fn refresh_template_rels(&self) -> Result<bool> {
        let files = self.template_file_set()?;
        let moved = self.moved_extract_revs("template", &files, false)?;
        if moved.is_empty() {
            return Ok(false);
        }
        let rows = self.collect_template_rows(&files)?;
        self.refresh_rel("template_parts", &TEMPLATE_PARTS_COLS, &rows)?;
        for (rev, d) in &moved {
            self.save_extract_digest("template", rev, d)?;
        }
        Ok(true)
    }

    /// Cold-start chunk write (`cold_stage.rs`): parse `files` (one byte-bounded
    /// slice of `template_file_set()`) and APPEND their rows — no wholesale
    /// delete, no digest save. `template_parts` has no corpus-global resolver, so
    /// a per-file slice emits exactly the rows a whole-corpus pass would; the
    /// completion gate saves the family digest once every slice has landed.
    /// Mirrors `Engine::refresh_dataflow_rels_slice`.
    pub(crate) fn refresh_template_rels_slice(&self, files: &[ExtractFile]) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let rows = self.collect_template_rows(files)?;
        self.append_rel("template_parts", &TEMPLATE_PARTS_COLS, &rows)
    }

    /// Cold completion gate: save template's per-rev `extract:` digest over the
    /// whole `template_file_set()` once every chunk slice has been appended, so
    /// the completion tick's wholesale `refresh_template_rels` sees the family
    /// unchanged and SKIPS it. Mirrors `Engine::save_dataflow_cold_digest`.
    pub(crate) fn save_template_cold_digest(&self) -> Result<()> {
        let files = self.template_file_set()?;
        for (rev, d) in self.moved_extract_revs("template", &files, false)? {
            self.save_extract_digest("template", &rev, &d)?;
        }
        Ok(())
    }

    /// Parse `files` and build `template_parts`' row set (dedup by (node, idx)).
    /// Shared by the wholesale refresh and the cold-chunk slice append; the ONLY
    /// difference between them is the write path (`refresh_rel` vs `append_rel`).
    fn collect_template_rows(&self, files: &[ExtractFile]) -> Result<Vec<Vec<Value>>> {
        let root = self.root.clone();
        let roots = self.repo_roots();
        let facts: Vec<(
            String,
            String,
            String,
            Arc<Vec<crate::typegraph::TemplatePart>>,
        )> = cached_facts(
            &self.template_facts_cache,
            files,
            &self.extract_files_parsed,
            |repo, path, rev| {
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                let parts = typegraph::ts_template_parts(path, &content);
                Some((rid, parts))
            },
        );

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        // `template_parts.node` must carry the SAME `_df_node_dict` surrogate as
        // `df_lit.id` for the occurrence, so it joins the dataflow rels. Resolve
        // every part's coordinate `(path, line, col, "template")` through the
        // dict — the dataflow lift mints the identical coordinate, so INSERT OR
        // IGNORE converges both to one surrogate regardless of extract order.
        let coords: Vec<(String, String, u32, u32, String)> = facts
            .iter()
            .flat_map(|(_repo, path, _rev, parts)| {
                parts.iter().map(move |p| {
                    (
                        p.node.clone(),
                        path.clone(),
                        p.line,
                        p.col,
                        "template".to_string(),
                    )
                })
            })
            .collect();
        let surrogate = self.resolve_coord_surrogates(&coords)?;
        let mut seen: HashSet<(String, u32)> = HashSet::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for (_repo, path, _rev, parts) in &facts {
            tracing::trace!(path = %path, parts = parts.len(), "[extract] template fold file");
            for p in parts.iter() {
                if !seen.insert((p.node.clone(), p.idx)) {
                    continue;
                }
                let node_id = *surrogate.get(&p.node).ok_or_else(|| {
                    anyhow::anyhow!(
                        "template_parts coordinate {:?} did not resolve to a surrogate",
                        p.node
                    )
                })?;
                rows.push(vec![
                    t(path),
                    i(p.line),
                    Value::Int(node_id),
                    i(p.idx),
                    t(p.kind),
                    t(&p.text),
                ]);
            }
        }
        Ok(rows)
    }

    /// The `unresolved` corpus: same TS/JS-family extension set as
    /// `template_file_set` (v1 scope, see `typegraph::UnresolvedRef`).
    ///
    /// `pub(crate)` — same reason as `template_file_set`.
    pub(crate) fn unresolved_file_set(&self) -> Result<Vec<ExtractFile>> {
        let mut files: Vec<ExtractFile> = Vec::new();
        let rows = self.db.query_rows(
            "_file",
            "SELECT repo, path, rev, hash FROM _file ORDER BY repo, path, rev",
            &[],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                ))
            },
        )?;
        for row in rows {
            let p = row.1.as_str();
            if p.ends_with(".ts")
                || p.ends_with(".tsx")
                || p.ends_with(".js")
                || p.ends_with(".jsx")
                || p.ends_with(".mjs")
                || p.ends_with(".cjs")
                || p.ends_with(".mts")
                || p.ends_with(".cts")
            {
                files.push(row);
            }
        }
        Ok(files)
    }

    /// Rebuild `unresolved` — every runtime-computed edge marker, across every
    /// TS/JS-family file (see `typegraph::UnresolvedRef` for the reason
    /// vocabulary and why Python/`sys.path` stay out of v1). Own family, same
    /// perf shape as `refresh_template_rels`: per-rev input-digest skip,
    /// per-file fact cache, parallel parse, one batched `refresh_rel`.
    ///
    /// No rev column: a file present at two revs unions its markers, deduped
    /// by (file, line, reason, detail).
    pub(crate) fn refresh_unresolved_rel(&self) -> Result<bool> {
        let files = self.unresolved_file_set()?;
        let moved = self.moved_extract_revs("unresolved", &files, false)?;
        if moved.is_empty() {
            return Ok(false);
        }
        let rows = self.collect_unresolved_rows(&files)?;
        self.refresh_rel("unresolved", &UNRESOLVED_COLS, &rows)?;
        for (rev, d) in &moved {
            self.save_extract_digest("unresolved", rev, d)?;
        }
        Ok(true)
    }

    /// Cold-start chunk write (`cold_stage.rs`): parse `files` (one byte-bounded
    /// slice of `unresolved_file_set()`) and APPEND their rows — no wholesale
    /// delete, no digest save. `unresolved` has no corpus-global resolver, so a
    /// per-file slice emits exactly the rows a whole-corpus pass would; the
    /// completion gate saves the family digest once every slice has landed.
    /// Mirrors `Engine::refresh_dataflow_rels_slice`.
    pub(crate) fn refresh_unresolved_rel_slice(&self, files: &[ExtractFile]) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }
        let rows = self.collect_unresolved_rows(files)?;
        self.append_rel("unresolved", &UNRESOLVED_COLS, &rows)
    }

    /// Cold completion gate: save unresolved's per-rev `extract:` digest over the
    /// whole `unresolved_file_set()` once every chunk slice has been appended, so
    /// the completion tick's wholesale `refresh_unresolved_rel` sees the family
    /// unchanged and SKIPS it. Mirrors `Engine::save_dataflow_cold_digest`.
    pub(crate) fn save_unresolved_cold_digest(&self) -> Result<()> {
        let files = self.unresolved_file_set()?;
        for (rev, d) in self.moved_extract_revs("unresolved", &files, false)? {
            self.save_extract_digest("unresolved", &rev, &d)?;
        }
        Ok(())
    }

    /// Parse `files` and build `unresolved`'s row set (dedup by (file, line,
    /// reason, detail)). Shared by the wholesale refresh and the cold-chunk
    /// slice append; the ONLY difference between them is the write path
    /// (`refresh_rel` vs `append_rel`).
    fn collect_unresolved_rows(&self, files: &[ExtractFile]) -> Result<Vec<Vec<Value>>> {
        let root = self.root.clone();
        let roots = self.repo_roots();
        let facts: Vec<(
            String,
            String,
            String,
            Arc<Vec<crate::typegraph::UnresolvedRef>>,
        )> = cached_facts(
            &self.unresolved_facts_cache,
            files,
            &self.extract_files_parsed,
            |repo, path, rev| {
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                let refs = typegraph::ts_unresolved_refs(path, &content);
                Some((rid, refs))
            },
        );

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut seen: HashSet<(String, u32, String, String)> = HashSet::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for (_repo, path, _rev, refs) in &facts {
            tracing::trace!(path = %path, refs = refs.len(), "[extract] unresolved fold file");
            for r in refs.iter() {
                if !seen.insert((path.clone(), r.line, r.reason.to_string(), r.detail.clone())) {
                    continue;
                }
                rows.push(vec![t(path), i(r.line), t(r.reason), t(&r.detail)]);
            }
        }
        Ok(rows)
    }
}
