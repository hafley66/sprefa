use super::*;

impl Engine {
    /// Refresh `doc_node` from the document-grammar registry. Same shape as the
    /// source-lang refreshers but simpler: no corpus-global resolution pass, no
    /// legacy mirror -- each DocNode is self-contained. Files come from `_file`
    /// (a source rule scanning `**/*.md` feeds it, exactly as `**/*.rs` feeds
    /// the type graph); the SQL prefilter narrows to document extensions, then
    /// the registry's `matches` decides the real dispatch.
    /// Change-reporting contract mirrors `refresh_type_rels`. The `doc_ref`
    /// bridge reads `type_entity`, so the input digest folds the md corpus
    /// PLUS the stored `extract:type` digest (which identifies the type
    /// family's inputs; type refresh runs before doc in both tick paths).
    pub(crate) fn refresh_doc_rels(&self) -> Result<bool> {
        let mut files: Vec<ExtractFile> = Vec::new();
        {
            let mut sel = self.db.conn().prepare(
                "SELECT repo, path, rev, hash FROM _file WHERE path LIKE '%.md' OR path LIKE '%.markdown' ORDER BY repo, path, rev")?;
            let rows = sel.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                ))
            })?;
            for row in rows.flatten() {
                files.push(row);
            }
        }
        // Per-rev skip, riding the type family per rev: the doc_ref bridge reads
        // type_entity, so each rev's doc digest folds the SAME rev's stored
        // `extract:type:<rev>` (type refresh runs before doc in both tick paths).
        // Single-rev (WORK-only) programs see the old whole-family behavior.
        let mut moved: Vec<(String, [u8; 32])> = Vec::new();
        {
            let mut by_rev: HashMap<&str, Vec<ExtractFile>> = HashMap::new();
            for f in &files {
                by_rev.entry(f.2.as_str()).or_default().push(f.clone());
            }
            for (rev, frev) in &by_rev {
                let mut digest = self.extract_input_digest("doc", rev, frev, false);
                let ty = self
                    .load_rel_digest(&extract_digest_key("type", rev))?
                    .map(|d| d.iter().map(|b| format!("{b:02x}")).collect::<String>())
                    .unwrap_or_default();
                for (a, b) in digest
                    .iter_mut()
                    .zip(blake3::hash(format!("type\0{ty}").as_bytes()).as_bytes())
                {
                    *a ^= *b;
                }
                if self.load_rel_digest(&extract_digest_key("doc", rev))? == Some(digest) {
                    continue;
                }
                moved.push(((*rev).to_string(), digest));
            }
        }
        if moved.is_empty() {
            return Ok(false);
        }
        let root = self.root.clone();
        let roots = self.repo_roots();
        let facts: Vec<(String, String, ingest::DocFacts)> = files
            .par_iter()
            .filter_map(|(repo, path, rev, _)| {
                let lang = ingest::ingest_langs().iter().find(|l| l.matches(path))?;
                let froot = roots.get(repo).map(|p| p.as_path()).unwrap_or(&root);
                let content = read_content(froot, rev, path).unwrap_or_default();
                let rid = repo_id_of(froot, path, repo);
                Some((rid, path.clone(), lang.extract_docs(path, &content)))
            })
            .collect();
        let t = |s: &str| Value::Text(s.to_string());
        let i = |n: u32| Value::Int(n as i64);
        let mut rows: Vec<Vec<Value>> = Vec::new();
        let mut seen: HashSet<(String, String, u32, &str, String)> = HashSet::new();
        for (repo, path, f) in &facts {
            for n in &f.nodes {
                if seen.insert((repo.clone(), path.clone(), n.line, n.kind, n.name.clone())) {
                    rows.push(vec![
                        t(repo),
                        t(path),
                        i(n.line),
                        t(n.kind),
                        t(&n.name),
                        t(&n.parent),
                    ]);
                }
            }
        }
        self.refresh_rel(
            "doc_node",
            &["repo", "file", "line", "kind", "name", "parent"],
            &rows,
        )?;

        // doc_ref bridge: doc_node -> type_entity, three sources.
        //   (1) heading exact:    doc_node.name == type_entity.name
        //   (2) heading norm:     normalize(doc_node.name) == type_entity.name
        //       (strips leading articles + trailing kind words; lowercase).
        //   (3) code_block text:  identifiers in doc_node.text matched against
        //       type_entity.name (lowercase). The whole block counts as one
        //       doc position (the fence line); dedup collapses repeats.
        //
        // Normalization lives in `normalize_doc_name` (below). type_entity names
        // are already clean identifiers, so the symbol side only lowercases.
        // Empty if the program doesn't use type relations (type_entity table
        // exists but is unpopulated -> empty map -> no rows).
        let type_rows: Vec<(String, String)> = {
            let mut sel = self.db.prepare(&format!(
                "SELECT sym, name FROM {}",
                crate::lower::txt_tbl("type_entity")
            ))?;
            let rows: Vec<(String, String)> = sel
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
                .filter_map(|x| x.ok())
                .collect();
            rows
        };
        // Map lowercase type name -> Vec of (sym, original_name) so multiple
        // symbols of the same name all bridge (e.g. two `Engine` in different
        // files).
        let mut by_name: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for (sym, name) in &type_rows {
            by_name
                .entry(name.to_ascii_lowercase())
                .or_default()
                .push((sym.clone(), name.clone()));
        }
        let mut ref_rows: Vec<Vec<Value>> = Vec::new();
        let mut ref_seen: HashSet<(String, u32, String, &'static str, String)> = HashSet::new();
        let push_ref =
            |repo: &str,
             file: &str,
             line: u32,
             sym: &str,
             kind: &'static str,
             matched: &str,
             rows: &mut Vec<Vec<Value>>,
             seen: &mut HashSet<(String, u32, String, &'static str, String)>| {
                if seen.insert((
                    file.to_string(),
                    line,
                    sym.to_string(),
                    kind,
                    matched.to_string(),
                )) {
                    rows.push(vec![t(repo), t(file), i(line), t(sym), t(kind), t(matched)]);
                }
            };
        for (repo, path, f) in &facts {
            for n in &f.nodes {
                match n.kind {
                    "heading" => {
                        // Try exact name first, then normalized. The original
                        // heading text is recorded as matched_name so a rule
                        // can cross-reference doc_node directly.
                        let exact = by_name.get(&n.name.to_ascii_lowercase());
                        if let Some(hits) = exact {
                            for (sym, _orig) in hits {
                                push_ref(
                                    repo,
                                    path,
                                    n.line,
                                    sym,
                                    "heading",
                                    &n.name,
                                    &mut ref_rows,
                                    &mut ref_seen,
                                );
                            }
                            continue;
                        }
                        let norm = normalize_doc_name(&n.name);
                        if !norm.is_empty() {
                            if let Some(hits) = by_name.get(&norm) {
                                for (sym, _orig) in hits {
                                    push_ref(
                                        repo,
                                        path,
                                        n.line,
                                        sym,
                                        "heading",
                                        &n.name,
                                        &mut ref_rows,
                                        &mut ref_seen,
                                    );
                                }
                            }
                        }
                    }
                    "code_block" => {
                        // Scan the block body for identifiers that match a type
                        // name. Each unique (sym, token) pair emits one row.
                        for tok in identifiers_in(&n.text) {
                            if let Some(hits) = by_name.get(&tok.to_ascii_lowercase()) {
                                for (sym, _orig) in hits {
                                    push_ref(
                                        repo,
                                        path,
                                        n.line,
                                        sym,
                                        "code_block",
                                        tok,
                                        &mut ref_rows,
                                        &mut ref_seen,
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        self.refresh_rel(
            "doc_ref",
            &["repo", "file", "line", "sym", "kind", "matched_name"],
            &ref_rows,
        )?;
        // Persisted only after the writes land, so a failed refresh retries.
        for (rev, d) in &moved {
            self.save_rel_digest(&extract_digest_key("doc", rev), d)?;
        }
        Ok(true)
    }
}
