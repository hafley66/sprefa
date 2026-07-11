use super::*;

impl Engine {
    pub(crate) fn reconcile_sources(&mut self, source_rules: &[&Rule], source_rels: &[String],
        consumed: &HashSet<String>) -> Result<Reconcile> {
        // Load prior file metadata first so enumerate can use the mtime fast-path.
        let prev = self.load_file_meta()?;

        let mut current: FileMeta = HashMap::new();
        // (rule idx, repo slug, path, rev, hash) for every enumerated file. A
        // single rule scanning `"*"` fans out to one batch of rows per config
        // repo, all carrying the same rule idx but distinct repo slugs.
        let mut rule_files: Vec<(usize, String, String, String, String, Vec<(String, String)>)> = Vec::new();
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
        let mut groups: BTreeMap<(String, String), (PathBuf, Vec<(usize, String, Vec<(String, String)>)>)> = BTreeMap::new();
        for (idx, rule) in source_rules.iter().enumerate() {
            for b in self.resolve_scan_bindings(rule)? {
                root_by_repo.insert(b.slug.clone(), b.root.clone());
                groups.entry((b.slug, b.rev)).or_insert_with(|| (b.root, Vec::new()))
                    .1.push((idx, b.glob, b.head_binds));
            }
        }
        let group_list: Vec<(&(String, String), &(PathBuf, Vec<(usize, String, Vec<(String, String)>)>))> = groups.iter().collect();
        let enumerated: Vec<Result<Vec<(String, String, i64, i64, i64)>>> = group_list.par_iter()
            .map(|((slug, rev), (repo_root, rules))| {
                let t = std::time::Instant::now();
                let mut union = globset::GlobSetBuilder::new();
                for (_, g, _) in rules { union.add(globset::Glob::new(g)?); }
                let files = enumerate_with_hash(slug, repo_root, rev, &union.build()?, &prev)?;
                if crate::db::profiling() {
                    eprintln!("[scan {slug}@{}] {} file(s) in {:.1}ms",
                        if rev == "WORK" { "WORK" } else { &rev[..rev.len().min(8)] },
                        files.len(), t.elapsed().as_secs_f64() * 1000.0);
                }
                Ok(files)
            }).collect();
        for (((slug, rev), (_, rules)), files) in group_list.iter().zip(enumerated) {
            let matchers: Vec<(usize, globset::GlobMatcher, Vec<(String, String)>)> = rules.iter()
                .map(|(idx, g, hb)| Ok((*idx, globset::Glob::new(g)?.compile_matcher(), hb.clone())))
                .collect::<Result<_>>()?;
            for (path, h, mt, sz, lines) in files? {
                current.insert((slug.clone(), path.clone(), rev.clone()), (h.clone(), mt, sz, lines));
                for (idx, m, hb) in &matchers {
                    if m.is_match(&path) {
                        rule_files.push((*idx, slug.clone(), path.clone(), rev.clone(), h.clone(), hb.clone()));
                    }
                }
            }
        }
        self.rev_index = current.keys().map(|(repo, p, r)| (repo.clone(), r.clone(), p.clone())).collect();

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
        let rel_matched: HashSet<&str> = source_rules.iter().enumerate()
            .filter(|(idx, _)| matched.contains(idx))
            .map(|(_, r)| r.head.rel.as_str()).collect();
        for (idx, rule) in source_rules.iter().enumerate() {
            if matched.contains(&idx) { continue; }
            let rel = rule.head.rel.as_str();
            if rel_matched.contains(rel) { continue; } // (a) sibling glob matched — silent
            let Ok(spec) = scan_spec_of(rule) else { continue };
            let Term::Str(glob) = &spec.glob else { continue };
            let targets: Vec<String> = groups.iter()
                .filter(|(_, (_, rules))| rules.iter().any(|(i, _, _)| *i == idx))
                .map(|((slug, rev), (root, _))| {
                    let r = if rev == "WORK" { "WORK" } else { &rev[..rev.len().min(8)] };
                    format!("{slug}@{r} ({})", root.display())
                })
                .collect();
            let where_ = if targets.is_empty() { "no repo/rev resolved".into() } else { targets.join(", ") };
            if consumed.contains(rel) {
                // (b) consumed helper — quiet, no fix-it note.
                eprintln!("[dl] source `{rel}` matched 0 files this tick: scan(\"{glob}\") under {where_} (feeds a rule — transient if mid-edit)");
                continue;
            }
            eprintln!("[dl] source `{rel}` matched 0 files: scan(\"{glob}\") under {where_}", );
            // The glob matches paths relative to the working root (the cwd `dl`
            // ran in). The usual miss is an anchored glob (`src/…`) run from ABOVE
            // the repo, or a rev with no such path. `*` already crosses `/`, so
            // recursion is not the issue.
            eprintln!("       note: the glob matches paths relative to the working root; run `dl` from the repo (its cwd is the root — there is no --root) and check the leading path segments match");
        }

        let hash_of = |m: &FileMeta, repo: &str, p: &str, r: &str|
            m.get(&(repo.to_string(), p.to_string(), r.to_string())).map(|t| t.0.clone());

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

        let retract_list: Vec<(&str, &str)> = to_retract.iter()
            .map(|(repo, p)| (repo.as_str(), p.as_str())).collect();
        let retracted = self.retract_paths(&retract_list, source_rels)?;

        // Extract any file whose path was retracted, not just hash-moved ones:
        // retraction is path-grain across ALL source rels, so a clean rule
        // sharing a path with a dirty one must re-provide its rows too.
        let to_extract: Vec<(usize, String, String, String, String, Vec<(String, String)>)> = rule_files.iter()
            .filter(|(_, repo, p, r, h, _)| hash_of(&prev, repo, p, r).as_ref() != Some(h)
                || to_retract.contains(&(repo.clone(), p.clone())))
            .map(|(idx, repo, p, r, h, hb)| (*idx, repo.clone(), p.clone(), r.clone(), h.clone(), hb.clone()))
            .collect();
        let parsed = to_extract.len();

        // Parse + extract in parallel across files (CPU-bound, no DB touch),
        // then insert serially (SQLite is single-writer).
        let results: Vec<Result<(String, String, Vec<Vec<Value>>, Vec<(spine::WhereBytes, String)>, usize)>> = {
            let Engine { rels, rev_index, .. } = &*self;
            to_extract.par_iter().map(|(idx, repo, path, rev, hash, hb)| {
                let root = root_by_repo.get(repo)
                    .ok_or_else(|| anyhow::anyhow!("no root for repo {repo}"))?;
                let (rows, where_bytes, dropped) =
                    parse_file(source_rules[*idx], repo, path, rev, hash, root, rels, rev_index, hb)?;
                let rel = source_rules[*idx].head.rel.clone();
                Ok((rel, path.clone(), rows, where_bytes, dropped))
            }).collect()
        };

        let mut by_rel: HashMap<String, Vec<(String, String, Vec<Value>)>> = HashMap::new();
        let mut where_bytes: Vec<(String, String, spine::WhereBytes, Option<String>)> = Vec::new();
        for (res, (_, repo, _, _, _, _)) in results.into_iter().zip(to_extract.iter()) {
            let (rel, path, rows, wheres, dropped) = res?;
            self.dropped += dropped;
            if dropped > 0 { self.record_extraction_drop(&path, &rel, dropped); }
            where_bytes.extend(wheres.into_iter().map(|(w, t)| (repo.clone(), path.clone(), w, Some(t))));
            by_rel.entry(rel).or_default()
                .extend(rows.into_iter().map(|row| (repo.clone(), path.clone(), row)));
        }

        let mut extracted = 0usize;
        for (rel, rows) in by_rel {
            let meta = self.rels.get(&rel)
                .ok_or_else(|| anyhow::anyhow!("unknown head relation {}", rel))?.clone();
            extracted += self.insert_source_rows_for_paths(&rel, &meta, &rows)?;
        }
        self.insert_spine_where_bytes(&where_bytes)?;

        self.save_file_meta(&current, &prev)?;
        for (key, d) in &pending_digests { self.save_rel_digest(key, d)?; }
        Ok(Reconcile {
            changed: retracted > 0 || extracted > 0,
            extracted,
            retracted,
            parsed,
            total: rule_files.len(),
        })
    }

    pub(crate) fn retract_path(&self, repo: &str, path: &str, source_rels: &[String]) -> Result<usize> {
        self.retract_paths(&[(repo, path)], source_rels)
    }

    /// Retract every row sourced only from these `(repo, path)` pairs. Prune
    /// `_prov` for all pairs first, then run the orphan sweep once per relation
    /// (not once per pair): a row survives iff some remaining path still provides
    /// its `__src`. Turns the old O(paths x rels x table) into O(rels x table).
    /// Keying by `(repo, path)` keeps two repos sharing a path from retracting
    /// each other's source rows.
    pub(crate) fn retract_paths(&self, paths: &[(&str, &str)], source_rels: &[String]) -> Result<usize> {
        if paths.is_empty() { return Ok(0); }
        self.db.exec("CREATE TEMP TABLE IF NOT EXISTS _retract_path(repo TEXT, path TEXT, PRIMARY KEY (repo, path))")?;
        self.db.exec("DELETE FROM _retract_path")?;
        let path_rows: Vec<Vec<Value>> = paths.iter()
            .map(|(repo, p)| vec![Value::Text((*repo).to_string()), Value::Text((*p).to_string())]).collect();
        self.db.insert_rows("_retract_path", &["repo", "path"], &path_rows)?;
        self.db.exec(
            "DELETE FROM _prov WHERE (repo, path) IN (SELECT repo, path FROM _retract_path)")?;
        // Drop located rows attributed to these (repo, path) pairs; fresh spans
        // re-insert on reparse. Sentinel row has path '' and is never retracted.
        // Keying by (repo, path) keeps two config repos sharing a path from
        // retracting each other's located rows.
        self.db.exec(
            "DELETE FROM _where_bytes WHERE (repo, path) IN (SELECT repo, path FROM _retract_path)")?;
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

    pub(crate) fn eval_extract_rules(&self, extract_rules: &[&Rule]) -> Result<bool> {
        if extract_rules.is_empty() { return Ok(false); }
        let mut heads: Vec<String> = Vec::new();
        for r in extract_rules {
            if !heads.contains(&r.head.rel) { heads.push(r.head.rel.clone()); }
        }
        let mut any_changed = false;
        for head_rel in &heads {
            let cols: Vec<String> = {
                let meta = self.rels.get(head_rel)
                    .ok_or_else(|| anyhow::anyhow!("term-extract head rel `{head_rel}` is not declared"))?;
                meta.cols.iter().map(|c| c.name.clone()).collect()
            };
            let mut rows: Vec<Vec<Value>> = Vec::new();
            for r in extract_rules.iter().filter(|r| &r.head.rel == head_rel) {
                self.extract_rule_rows(r, &mut rows)?;
            }
            // Changed iff the head row SET differs from what is stored (sorted
            // compare): only then does the downstream fixpoint need to re-run.
            // `Value` is not Ord/Eq; compare the row SETS via a string projection.
            let key = |row: &[Value]| -> Vec<String> {
                row.iter().map(|v| match v {
                    Value::Int(n) => format!("i{n}"),
                    Value::Text(s) => format!("t{s}"),
                    Value::Null => "n".to_string(),
                }).collect()
            };
            let mut before: Vec<Vec<String>> = {
                let n = cols.len();
                let sql = format!("SELECT * FROM {}", tbl(head_rel));
                let mut stmt = self.db.conn().prepare(&sql)?;
                let v = stmt.query_map([], |row| {
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
                })?.filter_map(|x| x.ok()).collect();
                v
            };
            let mut after: Vec<Vec<String>> = rows.iter().map(|r| key(r)).collect();
            before.sort();
            after.sort();
            if before != after { any_changed = true; }
            self.db.conn().execute(&format!("DELETE FROM {}", tbl(head_rel)), [])?;
            let col_refs: Vec<&str> = cols.iter().map(|s| s.as_str()).collect();
            self.db.insert_rows(&tbl(head_rel), &col_refs, &rows)?;
        }
        Ok(any_changed)
    }

    /// One term-extract rule: project the relational join to bind the content var,
    /// then fan the extractor (`run_data` for jsonp, `run_pattern` for json) over
    /// each joined row's bound string into head rows. Cmps over both join vars AND
    /// the extracted vars are post-filtered with `eval_cmp`.
    pub(crate) fn extract_rule_rows(&self, r: &Rule, out_rows: &mut Vec<Vec<Value>>) -> Result<()> {
        let extracts: Vec<&BodyItem> = r.body.iter().filter(|b| matches!(b,
            BodyItem::JsonP { rev: None, .. } | BodyItem::Json { rev: None, .. }
            | BodyItem::Sg { rev: None, .. })).collect();
        if extracts.len() != 1 {
            bail!("rule `{}`: a term-form json/jsonp/sg rule must have exactly one extract op \
                   (split a multi-extract rule into chained rules)", r.head.rel);
        }
        let cmps: Vec<&Constraint> = r.body.iter()
            .filter_map(|b| if let BodyItem::Cmp(c) = b { Some(c) } else { None }).collect();
        // The relational join binds the content var (and the head's join vars).
        let vars = async_bound_vars(r);
        if vars.is_empty() {
            bail!("rule `{}`: a term-extract rule needs a positive atom binding the content var", r.head.rel);
        }
        let sql = crate::lower::lower_body_projection(&r.body, &self.rels, &vars)?;
        let join_rows: Vec<Bind> = {
            let mut stmt = self.db.conn().prepare(&sql)?;
            let v = stmt.query_map([], |row| {
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
            })?.filter_map(|x| x.ok()).collect();
            v
        };
        // A term source has no extension to dispatch on (response bodies are
        // json); the synthetic name routes `run_data`/`run_pattern` to the json
        // walker. yaml/toml-in-a-string is not supported (v1).
        let synth = "_.json";
        let emit = |env: &Bind, out: &mut Vec<Vec<Value>>| -> Result<()> {
            for c in &cmps { if !eval_cmp(c, env)? { return Ok(()); } }
            let mut row = Vec::with_capacity(r.head.terms.len());
            for t in &r.head.terms { row.push(val_of(t, env)?); }
            out.push(row);
            Ok(())
        };
        match extracts[0] {
            BodyItem::JsonP { src, jpath, out, id, .. } => {
                let srcvar = var_of(src)?;
                let outvar = var_of(out)?;
                if id.is_some() {
                    bail!("rule `{}`: a term-form jsonp has no file to locate — drop the `id` arg", r.head.rel);
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
                        for (cap, text, _lo, _hi) in m { env.insert(cap, Value::Text(text)); }
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
            BodyItem::Sg { src, lang, pattern, line, col, end_line, end_col, .. } => {
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
                    for (ln, c, eln, ec, _mlo, _mhi, caps) in crate::sg::run_sg(&content, lang, pattern)? {
                        let mut env = jr.clone();
                        if let Some(v) = &slv { env.insert(v.clone(), Value::Int(ln)); }
                        if let Some(v) = &clv { env.insert(v.clone(), Value::Int(c)); }
                        if let Some(v) = &ellv { env.insert(v.clone(), Value::Int(eln)); }
                        if let Some(v) = &eclv { env.insert(v.clone(), Value::Int(ec)); }
                        for (name, text, _lo, _hi) in caps { env.insert(name, Value::Text(text)); }
                        emit(&env, out_rows)?;
                    }
                }
            }
            _ => unreachable!("extracts filtered to JsonP/Json/Sg"),
        }
        Ok(())
    }

    /// Wipe derived tables and run the semi-naive fixpoint to convergence.
    pub(crate) fn insert_source_rows(&self, rel: &str, meta: &RelMeta, repo: &str, path: &str, rows: &[Vec<Value>]) -> Result<usize> {
        if rows.is_empty() { return Ok(0); }
        let path_rows: Vec<(String, String, Vec<Value>)> = rows.iter().cloned()
            .map(|row| (repo.to_string(), path.to_string(), row)).collect();
        self.insert_source_rows_for_paths(rel, meta, &path_rows)
    }

    /// Insert source facts plus their `_prov` map rows. Each input is
    /// `(repo slug, path, row)`; `_prov` records `(rel, repo, path, __src)` so
    /// retraction can prune by `(repo, path)` without cross-repo collision.
    pub(crate) fn insert_source_rows_for_paths(&self, rel: &str, meta: &RelMeta, rows: &[(String, String, Vec<Value>)]) -> Result<usize> {
        if rows.is_empty() { return Ok(0); }
        self.insert_spine_strings(rows)?;
        let mut fact_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
        let mut prov_rows: Vec<Vec<Value>> = Vec::with_capacity(rows.len());
        for (repo, path, row) in rows {
            let src = row_hash(row);
            let mut fact = row.clone();
            fact.push(Value::Text(src.clone()));
            fact_rows.push(fact);
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
        self.db.insert_rows("_prov", &["rel", "repo", "path", "src"], &prov_rows)?;
        Ok(inserted)
    }

}
