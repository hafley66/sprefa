use super::*;

impl Engine {
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
            let md = p.ends_with(".md") || p.ends_with(".markdown");
            if ts || md || crate::cst::lang_label_for_path(p).is_some() { files.push(row); }
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
                } else if path.ends_with(".md") || path.ends_with(".markdown") {
                    crate::cst::walk_md_comments(&content).unwrap_or_default()
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

    /// The `template_parts` corpus: every `_file` row the oxc TS front-end
    /// parses (`.ts`/`.tsx`/`.js`/`.jsx`/`.mjs`/`.cjs` — the same extension set
    /// `TsTypes::matches` claims). Template literals are TS/JS-only syntax
    /// (Kotlin string templates and Rust's `format!`-style macros are a
    /// different grammar entirely and are explicitly OUT of scope), so scoping
    /// the set here keeps the family's input digest from moving when an
    /// unrelated file (`.rs`, `.kt`, `.md`) is edited.
    fn template_file_set(&self) -> Result<Vec<ExtractFile>> {
        let mut files: Vec<ExtractFile> = Vec::new();
        let mut sel = self.db.conn().prepare("SELECT repo, path, rev, hash FROM _file")?;
        let rows = sel.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?.unwrap_or_default())))?;
        for row in rows.flatten() {
            let p = row.1.as_str();
            if p.ends_with(".ts") || p.ends_with(".tsx") || p.ends_with(".js")
                || p.ends_with(".jsx") || p.ends_with(".mjs") || p.ends_with(".cjs") {
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
        if moved.is_empty() { return Ok(false); }
        let root = self.root.clone();
        let roots = self.repo_roots();
        let facts: Vec<(String, String, String, Arc<Vec<crate::typegraph::TemplatePart>>)> =
            cached_facts(&self.template_facts_cache, &files, &self.extract_files_parsed, |repo, path, rev| {
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                let parts = typegraph::ts_template_parts(path, &content);
                Some((rid, parts))
            });

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut seen: HashSet<(String, u32)> = HashSet::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for (_repo, path, _rev, parts) in &facts {
            for p in parts.iter() {
                if !seen.insert((p.node.clone(), p.idx)) {
                    continue;
                }
                rows.push(vec![
                    t(path), i(p.line), t(&p.node), i(p.idx), t(p.kind), t(&p.text),
                ]);
            }
        }
        self.refresh_rel("template_parts",
            &["file", "line", "node", "idx", "kind", "text"], &rows)?;
        for (rev, d) in &moved { self.save_rel_digest(&format!("extract:template:{rev}"), d)?; }
        Ok(true)
    }

    /// The `unresolved` corpus: same TS/JS-family extension set as
    /// `template_file_set` (v1 scope, see `typegraph::UnresolvedRef`).
    fn unresolved_file_set(&self) -> Result<Vec<ExtractFile>> {
        let mut files: Vec<ExtractFile> = Vec::new();
        let mut sel = self.db.conn().prepare("SELECT repo, path, rev, hash FROM _file")?;
        let rows = sel.query_map([], |r| Ok((
            r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?.unwrap_or_default())))?;
        for row in rows.flatten() {
            let p = row.1.as_str();
            if p.ends_with(".ts") || p.ends_with(".tsx") || p.ends_with(".js")
                || p.ends_with(".jsx") || p.ends_with(".mjs") || p.ends_with(".cjs") {
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
        if moved.is_empty() { return Ok(false); }
        let root = self.root.clone();
        let roots = self.repo_roots();
        let facts: Vec<(String, String, String, Arc<Vec<crate::typegraph::UnresolvedRef>>)> =
            cached_facts(&self.unresolved_facts_cache, &files, &self.extract_files_parsed, |repo, path, rev| {
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                let refs = typegraph::ts_unresolved_refs(path, &content);
                Some((rid, refs))
            });

        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut seen: HashSet<(String, u32, String, String)> = HashSet::new();
        let mut rows: Vec<Vec<Value>> = Vec::new();
        for (_repo, path, _rev, refs) in &facts {
            for r in refs.iter() {
                if !seen.insert((path.clone(), r.line, r.reason.to_string(), r.detail.clone())) {
                    continue;
                }
                rows.push(vec![t(path), i(r.line), t(r.reason), t(&r.detail)]);
            }
        }
        self.refresh_rel("unresolved",
            &["file", "line", "reason", "detail"], &rows)?;
        for (rev, d) in &moved { self.save_rel_digest(&format!("extract:unresolved:{rev}"), d)?; }
        Ok(true)
    }
}
