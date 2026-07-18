use super::*;

impl Engine {
    // ARCH {"url":"engine/20-reconcile","role":"source-pass"}
    #[tracing::instrument(skip_all, fields(n_rules = source_rules.len()), level = "debug")]
    pub(crate) fn reconcile_sources(
        &mut self,
        source_rules: &[&Rule],
        source_rels: &[String],
        consumed: &HashSet<String>,
    ) -> Result<Reconcile> {
        // Load prior file metadata first so enumerate can use the mtime fast-path.
        let prev = self.load_file_meta()?;
        // Reference second from the walk that produced `prev`, for the
        // racy-window guard in `enumerate_with_hash` (see its doc comment).
        // `now_secs` becomes the NEW reference, persisted below once this
        // tick's walk has actually happened, so a quiet daemon still keeps
        // the guard advancing tick over tick.
        let prev_walk_ref_secs = self.load_walk_ref_secs()?;
        let now_secs = unix_secs();

        let mut current: FileMeta = HashMap::new();
        // (rule idx, repo slug, path, rev, hash) for every enumerated file. A
        // single rule scanning `"*"` fans out to one batch of rows per config
        // repo, all carrying the same rule idx but distinct repo slugs.
        let mut rule_files: Vec<(usize, String, String, String, String, Vec<(String, String)>)> =
            Vec::new();
        // slug -> on-disk root for every repo touched this tick; parse_file reads
        // content from the matching root and the slug stamps `_file`/`_prov` so
        // two repos sharing a path stay distinct.
        let mut root_by_repo: HashMap<String, PathBuf> = HashMap::new();
        // Group rules by (slug, rev) so one repo×rev walks/ls-trees ONCE no
        // matter how many rules scan it (the old shape re-walked per rule —
        // rules × repos walks across a big config). Clones + rev-parse stay in
        // this serial loop (rev_cache needs &mut self); the walks parallelize.
        // Each entry carries `head_binds`: the data-driven coord values that
        // produced this (slug, rev), so the rule head can reference the repo/rev
        // variable each file was scanned under (empty for a literal-coord scan).
        let mut groups: BTreeMap<
            (String, String),
            (PathBuf, Vec<(usize, String, Vec<(String, String)>)>),
        > = BTreeMap::new();
        for (idx, rule) in source_rules.iter().enumerate() {
            for b in self.resolve_scan_bindings(rule)? {
                root_by_repo.insert(b.slug.clone(), b.root.clone());
                groups
                    .entry((b.slug, b.rev))
                    .or_insert_with(|| (b.root, Vec::new()))
                    .1
                    .push((idx, b.glob, b.head_binds));
            }
        }
        let group_list: Vec<(
            &(String, String),
            &(PathBuf, Vec<(usize, String, Vec<(String, String)>)>),
        )> = groups.iter().collect();
        let enumerated: Vec<Result<Vec<(String, String, i64, i64, i64)>>> = group_list
            .par_iter()
            .map(|((slug, rev), (repo_root, rules))| {
                let t = std::time::Instant::now();
                let mut union = globset::GlobSetBuilder::new();
                for (_, g, _) in rules {
                    union.add(globset::Glob::new(g)?);
                }
                let files = enumerate_with_hash(
                    slug,
                    repo_root,
                    rev,
                    &union.build()?,
                    &prev,
                    prev_walk_ref_secs,
                )?;
                if crate::db::profiling() {
                    eprintln!(
                        "[scan {slug}@{}] {} file(s) in {:.1}ms",
                        if rev == "WORK" {
                            "WORK"
                        } else {
                            &rev[..rev.len().min(8)]
                        },
                        files.len(),
                        t.elapsed().as_secs_f64() * 1000.0
                    );
                }
                Ok(files)
            })
            .collect();
        for (((slug, rev), (_, rules)), files) in group_list.iter().zip(enumerated) {
            let matchers: Vec<(usize, globset::GlobMatcher, Vec<(String, String)>)> = rules
                .iter()
                .map(|(idx, g, hb)| {
                    Ok((*idx, globset::Glob::new(g)?.compile_matcher(), hb.clone()))
                })
                .collect::<Result<_>>()?;
            for (path, h, mt, sz, lines) in files? {
                current.insert(
                    (slug.clone(), path.clone(), rev.clone()),
                    (h.clone(), mt, sz, lines),
                );
                for (idx, m, hb) in &matchers {
                    if m.is_match(&path) {
                        rule_files.push((
                            *idx,
                            slug.clone(),
                            path.clone(),
                            rev.clone(),
                            h.clone(),
                            hb.clone(),
                        ));
                    }
                }
            }
        }
        // Promote only after every parsed row and the short source transaction
        // commit. A failed preparation must not make in-memory coordinates run
        // ahead of the live database.
        let next_rev_index: HashSet<(String, String, String)> = current
            .keys()
            .map(|(repo, p, r)| (repo.clone(), r.clone(), p.clone()))
            .collect();

        // Zero-match diagnostic (v3/v4 parity): a scan rule that matched no files
        // is almost always a glob/root mismatch, which otherwise fails silently as
        // "0 rows" far downstream. Warn with the rule, glob, and where it looked so
        // the miss is self-diagnosing instead of a mystery.
        //
        // Softened for two expected-empty shapes so a helper-in-progress isn't
        // noisy: (a) POLYGLOT SIBLING — another scan heading the SAME rel matched
        // (e.g. `seen` scanned for both Rust and `{ts,tsx}`, and this repo has no
        // TS); the rel already has rows, so the empty glob is intentional
        // fan-out → silent. (b) CONSUMED — the rel feeds a downstream rule; the
        // author wired it up, an empty tick is transient → one quiet line, no
        // fix-it note. Only a genuinely dead scan (unmatched, no sibling, unread)
        // gets the loud two-line "check your glob/root" warning.
        let matched: HashSet<usize> = rule_files.iter().map(|(idx, ..)| *idx).collect();
        let rel_matched: HashSet<&str> = source_rules
            .iter()
            .enumerate()
            .filter(|(idx, _)| matched.contains(idx))
            .map(|(_, r)| r.head.rel.as_str())
            .collect();
        for (idx, rule) in source_rules.iter().enumerate() {
            if matched.contains(&idx) {
                continue;
            }
            let rel = rule.head.rel.as_str();
            if rel_matched.contains(rel) {
                continue;
            } // (a) sibling glob matched — silent
            let Ok(spec) = scan_spec_of(rule) else {
                continue;
            };
            let Term::Str(glob) = &spec.glob else {
                continue;
            };
            let targets: Vec<String> = groups
                .iter()
                .filter(|(_, (_, rules))| rules.iter().any(|(i, _, _)| *i == idx))
                .map(|((slug, rev), (root, _))| {
                    let r = if rev == "WORK" {
                        "WORK"
                    } else {
                        &rev[..rev.len().min(8)]
                    };
                    format!("{slug}@{r} ({})", root.display())
                })
                .collect();
            let where_ = if targets.is_empty() {
                "no repo/rev resolved".into()
            } else {
                targets.join(", ")
            };
            if consumed.contains(rel) {
                // (b) consumed helper — quiet, no fix-it note.
                eprintln!("[dl] source `{rel}` matched 0 files this tick: scan(\"{glob}\") under {where_} (feeds a rule — transient if mid-edit)");
                continue;
            }
            eprintln!("[dl] source `{rel}` matched 0 files: scan(\"{glob}\") under {where_}",);
            // The glob matches paths relative to the working root (the cwd `dl`
            // ran in). The usual miss is an anchored glob (`src/…`) run from ABOVE
            // the repo, or a rev with no such path. `*` already crosses `/`, so
            // recursion is not the issue.
            eprintln!("       note: the glob matches paths relative to the working root; run `dl` from the repo (its cwd is the root — there is no --root) and check the leading path segments match");
        }

        let hash_of = |m: &FileMeta, repo: &str, p: &str, r: &str| {
            m.get(&(repo.to_string(), p.to_string(), r.to_string()))
                .map(|t| t.0.clone())
        };

        // An edited source rule must re-extract files whose content did not
        // change. A dirty rel widens retraction to its whole file set; the new
        // digests persist only after the re-extraction lands (end of this fn).
        let (dirty_rels, pending_digests) = self.source_rule_digests(source_rules)?;

        // Retraction key is (repo, path): `_prov` prunes by that pair, so two
        // repos at the same path do not retract each other's source rows.
        let mut to_retract: HashSet<(String, String)> = HashSet::new();
        for ((repo, path, rev), (h, _, _, _)) in &current {
            if hash_of(&prev, repo, path, rev).as_ref() != Some(h) {
                to_retract.insert((repo.clone(), path.clone()));
            }
        }
        for (repo, path, _rev) in prev.keys() {
            if !current.contains_key(&(repo.clone(), path.clone(), _rev.clone())) {
                to_retract.insert((repo.clone(), path.clone()));
            }
        }
        for (idx, repo, path, _rev, _h, _hb) in &rule_files {
            if dirty_rels.contains(&source_rules[*idx].head.rel) {
                to_retract.insert((repo.clone(), path.clone()));
            }
        }

        // Extract any file whose path was retracted, not just hash-moved ones:
        // retraction is path-grain across ALL source rels, so a clean rule
        // sharing a path with a dirty one must re-provide its rows too.
        let to_extract: Vec<crate::engine::source_prepare::SourceExtractJob> = rule_files
            .iter()
            .filter(|(_, repo, p, r, h, _)| {
                hash_of(&prev, repo, p, r).as_ref() != Some(h)
                    || to_retract.contains(&(repo.clone(), p.clone()))
            })
            .map(|(idx, repo, path, rev, hash, head_binds)| {
                crate::engine::source_prepare::SourceExtractJob {
                    rule_idx: *idx,
                    repo: repo.clone(),
                    path: path.clone(),
                    rev: rev.clone(),
                    hash: hash.clone(),
                    head_binds: head_binds.clone(),
                }
            })
            .collect();
        let parsed = to_extract.len();

        let (stage_generation, stage_base) =
            crate::engine::pipeline::source_stage_base(self, source_rules)?;
        let prepared_batch = crate::engine::source_prepare::prepare_source_batch(
            self,
            source_rules,
            &to_extract,
            &root_by_repo,
            &next_rev_index,
            stage_generation,
            stage_base,
        )?;
        let crate::engine::source_prepare::PreparedSourceBatch {
            facts: prepared,
            dropped: prepared_dropped,
            drop_diags: prepared_drop_diags,
        } = prepared_batch;
        let retract_owned: Vec<(String, String)> = to_retract.into_iter().collect();
        let outcome = self.with_semantic_generation(|engine| {
            let (_, current_base) =
                crate::engine::pipeline::source_stage_base(engine, source_rules)?;
            prepared.verify(engine, current_base)?;
            let retract_refs: Vec<(&str, &str)> = retract_owned
                .iter()
                .map(|(repo, path)| (repo.as_str(), path.as_str()))
                .collect();
            let retracted = engine.retract_paths(&retract_refs, source_rels)?;
            let extracted = prepared.apply(engine, current_base)?;
            engine.save_file_meta(&current, &prev)?;
            engine.save_walk_ref_secs(now_secs)?;
            for (key, digest) in &pending_digests {
                engine.save_rel_digest(key, digest)?;
            }
            Ok(Reconcile {
                changed: retracted > 0 || extracted > 0,
                extracted,
                retracted,
                parsed,
                total: rule_files.len(),
            })
        });
        let cleanup = prepared.discard(self.db.conn());
        match outcome {
            Ok(report) => {
                self.rev_index = next_rev_index;
                self.dropped += prepared_dropped;
                self.extraction_drops.extend(prepared_drop_diags);
                if let Err(error) = cleanup {
                    eprintln!(
                        "[stage] committed source generation; TEMP cleanup deferred: {error}"
                    );
                }
                Ok(report)
            }
            Err(error) => {
                if let Err(cleanup_error) = cleanup {
                    eprintln!("[stage] source rollback cleanup failed: {cleanup_error}");
                }
                Err(error)
            }
        }
    }

    pub(crate) fn retract_path(
        &self,
        repo: &str,
        path: &str,
        source_rels: &[String],
    ) -> Result<usize> {
        self.retract_paths(&[(repo, path)], source_rels)
    }

    /// Retract every row sourced only from these `(repo, path)` pairs. Prune
    /// `_prov` for all pairs first, then run the orphan sweep once per relation
    /// (not once per pair): a row survives iff some remaining path still provides
    /// its `__src`. Turns the old O(paths x rels x table) into O(rels x table).
    /// Keying by `(repo, path)` keeps two repos sharing a path from retracting
    /// each other's source rows.
    pub(crate) fn retract_paths(
        &self,
        paths: &[(&str, &str)],
        source_rels: &[String],
    ) -> Result<usize> {
        if paths.is_empty() {
            return Ok(0);
        }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _retract_path(repo TEXT, path TEXT, PRIMARY KEY (repo, path))")?;
        self.db.exec("DELETE FROM _retract_path")?;
        let path_rows: Vec<Vec<Value>> = paths
            .iter()
            .map(|(repo, p)| {
                vec![
                    Value::Text((*repo).to_string()),
                    Value::Text((*p).to_string()),
                ]
            })
            .collect();
        self.db
            .insert_rows("_retract_path", &["repo", "path"], &path_rows)?;
        self.db.exec(
            "DELETE FROM _prov WHERE (repo, path) IN (SELECT repo, path FROM _retract_path)",
        )?;
        // Drop located rows attributed to these (repo, path) pairs; fresh spans
        // re-insert on reparse. Sentinel row has path '' and is never retracted.
        // Keying by (repo, path) keeps two config repos sharing a path from
        // retracting each other's located rows.
        self.db.exec(
            "DELETE FROM _where_bytes WHERE (repo, path) IN (SELECT repo, path FROM _retract_path)",
        )?;
        let mut removed = 0usize;
        for rel in source_rels {
            let rel_lit = rel.replace('\'', "''");
            let sql = format!(
                "DELETE FROM {} WHERE __src NOT IN (SELECT src FROM _prov WHERE rel = '{rel_lit}')",
                tbl(rel),
            );
            removed += self.db.exec(&sql)?;
        }
        Ok(removed)
    }
    /// Evaluate the TERM-form `json`/`jsonp` rules — the hybrid join+extract. A
    /// rule like `star(repo,n) <- page(repo,200,_,body), jsonp(body,"stars",n).`
    /// joins relations in SQL (binding the content var `body`), then runs the
    /// tree-sitter extractor over each joined row's bound string, fanning the
    /// extracted bindings into head rows. This is the only path that parses a
    /// value held in a relation (a response body, a column) rather than a file.
    /// Runs after sources/responses are present and before the derived fixpoint
    /// (so derived rules see the output). Returns whether any head rel changed,
    /// which the caller ORs into the rebuild gate.
    ///
    /// @recompute unguarded: re-runs each tick — its inputs (response/source rels)
    /// move off the file-source-digest path, so a digest skip here would miss a
    /// freshly-drained body. The join is bounded by the read relations (the
    /// response/page set), not the repo; the downstream rebuild is gated on the
    /// returned changed flag, so a steady state does not re-run the fixpoint.

    pub(crate) fn eval_extract_rules(&self, extract_rules: &[&Rule]) -> Result<bool> {
        if extract_rules.is_empty() {
            return Ok(false);
        }
        let mut heads: Vec<String> = Vec::new();
        for r in extract_rules {
            if !heads.contains(&r.head.rel) {
                heads.push(r.head.rel.clone());
            }
        }
        let mut any_changed = false;
        // Per-head-rel wall time (parse/extract + before/after row-set diff +
        // the DELETE/insert flush) into `_stmt_ms` under `extract:<rel>` — this
        // hybrid join+extract pass (term-form json/jsonp/sg) ran untimed before,
        // part of the derived-phase attribution gap.
        let mut stmt_ms: HashMap<String, (i64, i64)> = HashMap::new();
        for head_rel in &heads {
            let t = std::time::Instant::now();
            let cols: Vec<String> = {
                let meta = self.rels.get(head_rel).ok_or_else(|| {
                    anyhow::anyhow!("term-extract head rel `{head_rel}` is not declared")
                })?;
                meta.cols.iter().map(|c| c.name.clone()).collect()
            };
            let mut rows: Vec<Vec<Value>> = Vec::new();
            for r in extract_rules.iter().filter(|r| &r.head.rel == head_rel) {
                self.extract_rule_rows(r, &mut rows)?;
            }
            let col_refs: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
            let encoded_rows = self.encode_rel_rows(head_rel, &col_refs, &rows)?;
            // Changed iff the head row SET differs from what is stored (sorted
            // compare): only then does the downstream fixpoint need to re-run.
            // `Value` is not Ord/Eq; compare the row SETS via a string projection.
            let key = |row: &[Value]| -> Vec<String> {
                row.iter()
                    .map(|v| match v {
                        Value::Int(n) => format!("i{n}"),
                        Value::Text(s) => format!("t{s}"),
                        Value::Null => "n".to_string(),
                    })
                    .collect()
            };
            let mut before: Vec<Vec<String>> = {
                let n = cols.len();
                let sql = format!("SELECT * FROM {}", tbl(head_rel));
                let mut stmt = self.db.conn().prepare(&sql)?;
                let v = stmt
                    .query_map([], |row| {
                        let mut r = Vec::with_capacity(n);
                        for i in 0..n {
                            r.push(match row.get::<_, rusqlite::types::Value>(i)? {
                                rusqlite::types::Value::Integer(x) => format!("i{x}"),
                                rusqlite::types::Value::Text(s) => format!("t{s}"),
                                rusqlite::types::Value::Null => "t".to_string(),
                                other => format!("{other:?}"),
                            });
                        }
                        Ok(r)
                    })?
                    .filter_map(|x| x.ok())
                    .collect();
                v
            };
            let mut after: Vec<Vec<String>> = encoded_rows.iter().map(|r| key(r)).collect();
            before.sort();
            after.sort();
            if before != after {
                any_changed = true;
            }
            self.db
                .conn()
                .execute(&format!("DELETE FROM {}", tbl(head_rel)), [])?;
            self.db
                .insert_rows(&tbl(head_rel), &col_refs, &encoded_rows)?;
            stmt_ms.insert(
                format!("extract:{head_rel}"),
                (t.elapsed().as_millis() as i64, 1),
            );
        }
        self.save_stmt_ms(&stmt_ms)?;
        Ok(any_changed)
    }

    /// One term-extract rule: project the relational join to bind the content var,
    /// then fan the extractor (`run_data` for jsonp, `run_pattern` for json) over
    /// each joined row's bound string into head rows. Cmps over both join vars AND
    /// the extracted vars are post-filtered with `eval_cmp`.
    pub(crate) fn extract_rule_rows(&self, r: &Rule, out_rows: &mut Vec<Vec<Value>>) -> Result<()> {
        let extracts: Vec<&BodyItem> = r
            .body
            .iter()
            .filter(|b| {
                matches!(
                    b,
                    BodyItem::JsonP { rev: None, .. }
                        | BodyItem::Json { rev: None, .. }
                        | BodyItem::Sg { rev: None, .. }
                )
            })
            .collect();
        if extracts.len() != 1 {
            bail!(
                "rule `{}`: a term-form json/jsonp/sg rule must have exactly one extract op \
                   (split a multi-extract rule into chained rules)",
                r.head.rel
            );
        }
        let cmps: Vec<&Constraint> = r
            .body
            .iter()
            .filter_map(|b| {
                if let BodyItem::Cmp(c) = b {
                    Some(c)
                } else {
                    None
                }
            })
            .collect();
        // The relational join binds the content var (and the head's join vars).
        let vars = async_bound_vars(r);
        if vars.is_empty() {
            bail!(
                "rule `{}`: a term-extract rule needs a positive atom binding the content var",
                r.head.rel
            );
        }
        let sql = crate::lower::lower_body_projection(&r.body, &self.rels, &vars)?;
        let join_rows: Vec<Bind> = {
            let mut stmt = self.db.conn().prepare(&sql)?;
            let v = stmt
                .query_map([], |row| {
                    let mut b: Bind = HashMap::new();
                    for (i, v) in vars.iter().enumerate() {
                        let val = match row.get::<_, rusqlite::types::Value>(i)? {
                            rusqlite::types::Value::Integer(x) => Value::Int(x),
                            rusqlite::types::Value::Text(s) => Value::Text(s),
                            rusqlite::types::Value::Null => Value::Text(String::new()),
                            other => Value::Text(format!("{other:?}")),
                        };
                        b.insert(v.clone(), val);
                    }
                    Ok(b)
                })?
                .filter_map(|x| x.ok())
                .collect();
            v
        };
        // A term source has no extension to dispatch on (response bodies are
        // json); the synthetic name routes `run_data`/`run_pattern` to the json
        // walker. yaml/toml-in-a-string is not supported (v1).
        let synth = "_.json";
        let emit = |env: &Bind, out: &mut Vec<Vec<Value>>| -> Result<()> {
            for c in &cmps {
                if !eval_cmp(c, env)? {
                    return Ok(());
                }
            }
            let mut row = Vec::with_capacity(r.head.terms.len());
            for t in &r.head.terms {
                row.push(val_of(t, env)?);
            }
            out.push(row);
            Ok(())
        };
        match extracts[0] {
            BodyItem::JsonP {
                src,
                jpath,
                out,
                id,
                ..
            } => {
                let srcvar = var_of(src)?;
                let outvar = var_of(out)?;
                if id.is_some() {
                    bail!(
                        "rule `{}`: a term-form jsonp has no file to locate — drop the `id` arg",
                        r.head.rel
                    );
                }
                for jr in &join_rows {
                    let content = match jr.get(&srcvar) {
                        Some(Value::Text(s)) => s.clone(),
                        Some(Value::Int(n)) => n.to_string(),
                        _ => continue,
                    };
                    for (v, _lo, _hi) in crate::datapath::run_data(synth, &content, jpath) {
                        let mut env = jr.clone();
                        env.insert(outvar.clone(), Value::Text(v));
                        emit(&env, out_rows)?;
                    }
                }
            }
            BodyItem::Json { src, pat, .. } => {
                let srcvar = var_of(src)?;
                let (steps, _) = crate::datapath::parse_pattern(pat)
                    .map_err(|e| anyhow::anyhow!("json pattern error: {e}"))?;
                for jr in &join_rows {
                    let content = match jr.get(&srcvar) {
                        Some(Value::Text(s)) => s.clone(),
                        Some(Value::Int(n)) => n.to_string(),
                        _ => continue,
                    };
                    for m in crate::datapath::run_pattern(synth, &content, &steps) {
                        let mut env = jr.clone();
                        for (cap, text, _lo, _hi) in m {
                            env.insert(cap, Value::Text(text));
                        }
                        emit(&env, out_rows)?;
                    }
                }
            }
            // Term-form `sg(:lang, src, "pat", line, col, end_line, end_col)`:
            // run the ast-grep pattern over each joined row's bound string. Metavar
            // captures bind by name (like the file form); the span outputs bind the
            // match's line/col RELATIVE to the bound string (byte 0 = start of the
            // value). No file, no located id — the caller adds the enclosing
            // region's own line to reach file coordinates.
            BodyItem::Sg {
                src,
                lang,
                pattern,
                line,
                col,
                end_line,
                end_col,
                ..
            } => {
                let srcvar = var_of(src)?;
                let slv = opt_var(line)?;
                let clv = opt_var(col)?;
                let ellv = opt_var(end_line)?;
                let eclv = opt_var(end_col)?;
                for jr in &join_rows {
                    let content = match jr.get(&srcvar) {
                        Some(Value::Text(s)) => s.clone(),
                        Some(Value::Int(n)) => n.to_string(),
                        _ => continue,
                    };
                    for (ln, c, eln, ec, _mlo, _mhi, caps) in
                        crate::sg::run_sg(&content, lang, pattern)?
                    {
                        let mut env = jr.clone();
                        if let Some(v) = &slv {
                            env.insert(v.clone(), Value::Int(ln));
                        }
                        if let Some(v) = &clv {
                            env.insert(v.clone(), Value::Int(c));
                        }
                        if let Some(v) = &ellv {
                            env.insert(v.clone(), Value::Int(eln));
                        }
                        if let Some(v) = &eclv {
                            env.insert(v.clone(), Value::Int(ec));
                        }
                        for (name, text, _lo, _hi) in caps {
                            env.insert(name, Value::Text(text));
                        }
                        emit(&env, out_rows)?;
                    }
                }
            }
            _ => unreachable!("extracts filtered to JsonP/Json/Sg"),
        }
        Ok(())
    }

    pub(crate) fn insert_source_rows(
        &self,
        rel: &str,
        meta: &RelMeta,
        repo: &str,
        path: &str,
        rows: &[Vec<Value>],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        let path_rows: Vec<(String, String, Vec<Value>)> = rows
            .iter()
            .cloned()
            .map(|row| (repo.to_string(), path.to_string(), row))
            .collect();
        self.insert_source_rows_for_paths(rel, meta, &path_rows)
    }

    /// Insert source facts plus their `_prov` map rows. Each input is
    /// `(repo slug, path, row)`; `_prov` records `(rel, repo, path, __src)` so
    /// retraction can prune by `(repo, path)` without cross-repo collision.
    pub(crate) fn insert_source_rows_for_paths(
        &self,
        rel: &str,
        meta: &RelMeta,
        rows: &[(String, String, Vec<Value>)],
    ) -> Result<usize> {
        if rows.is_empty() {
            return Ok(0);
        }
        self.insert_spine_strings(rows)?;
        let col_names: Vec<&str> = meta.cols.iter().map(|col| col.name.as_str()).collect();
        let plain_rows: Vec<Vec<Value>> = rows.iter().map(|(_, _, row)| row.clone()).collect();
        let encoded_rows = self.encode_rel_rows(rel, &col_names, &plain_rows)?;
        let mut fact_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
        let mut prov_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
        for ((repo, path, row), mut encoded) in rows.iter().zip(encoded_rows) {
            let src = row_hash(row);
            encoded.push(Value::Text(src.clone()));
            fact_rows.push(encoded);
            prov_rows.push(vec![
                Value::Text(rel.to_string()),
                Value::Text(repo.to_string()),
                Value::Text(path.to_string()),
                Value::Text(src),
            ]);
        }
        let mut cols: Vec<String> = meta.cols.iter().map(|c| c.name.clone()).collect();
        cols.push("__src".to_string());
        let col_refs: Vec<&str> = cols.iter().map(|c| c.as_str()).collect();
        let table = tbl(rel);
        let inserted = self.db.insert_rows(&table, &col_refs, &fact_rows)?;
        self.db
            .insert_rows("_prov", &["rel", "repo", "path", "src"], &prov_rows)?;
        Ok(inserted)
    }
}
