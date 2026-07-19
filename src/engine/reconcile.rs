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
                    let rev_short = if rev == "WORK" {
                        "WORK"
                    } else {
                        &rev[..rev.len().min(8)]
                    };
                    let file_count = files.len();
                    let ms = t.elapsed().as_secs_f64() * 1000.0;
                    tracing::debug!(
                        slug = %slug,
                        rev = %rev_short,
                        file_count,
                        ms,
                        "[scan {slug}@{rev_short}] {file_count} file(s) in {ms:.1}ms"
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
                // (b) consumed helper — quiet, no fix-it note, but still visible by default.
                tracing::warn!(
                    rel = %rel,
                    glob = %glob,
                    where_ = %where_,
                    "[dl] source `{rel}` matched 0 files this tick: scan(\"{glob}\") under {where_} (feeds a rule — transient if mid-edit)"
                );
                continue;
            }
            tracing::warn!(
                rel = %rel,
                glob = %glob,
                where_ = %where_,
                "[dl] source `{rel}` matched 0 files: scan(\"{glob}\") under {where_}\n       note: the glob matches paths relative to the working root; run `dl` from the repo (its cwd is the root — there is no --root) and check the leading path segments match"
            );
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
        // scan-narrowing guard (the SOME-scan-rules twin of `tick.rs`'s
        // no-scan-rules guard — read that one first): a program with scan
        // rules narrows an existing db to its own glob/root scope every tick.
        // That is honest reconcile semantics (unlike the no-scan-rules case,
        // there is no ambiguity to soften), but a program whose scope is far
        // smaller than what the db already knows about can silently prune
        // most of the corpus in one tick — the 618->68-file smashy incident
        // (docs/arch-measures-review.md). Warn, never block, when the files
        // falling outside this tick's scan scope exceed a threshold share of
        // the db's total known files. Path-grain (repo, path), ignoring rev,
        // matches `to_retract`'s own key and the incident's "N files" framing.
        // `DL_SCAN_NARROW_THRESHOLD` (0.0-1.0, default 0.5) tunes the trigger.
        {
            let prev_paths: HashSet<(&str, &str)> = prev
                .keys()
                .map(|(repo, path, _rev)| (repo.as_str(), path.as_str()))
                .collect();
            let before = prev_paths.len();
            if before > 0 {
                let current_paths: HashSet<(&str, &str)> = current
                    .keys()
                    .map(|(repo, path, _rev)| (repo.as_str(), path.as_str()))
                    .collect();
                let out_of_scope = prev_paths.iter().filter(|k| !current_paths.contains(*k)).count();
                let threshold: f64 = std::env::var("DL_SCAN_NARROW_THRESHOLD")
                    .ok()
                    .and_then(|s| s.parse::<f64>().ok())
                    .filter(|t| *t > 0.0 && *t <= 1.0)
                    .unwrap_or(0.5);
                let share = out_of_scope as f64 / before as f64;
                if share > threshold {
                    let after = current_paths.len();
                    self.shape_diags.push(DiagRow {
                        path: "(scan)".into(), line: 1, col: 0, end_line: 1, end_col: 0,
                        severity: "warn".into(), code: "scan-narrowing".into(),
                        msg: format!(
                            "this tick's scan scope covers {after} of the {before} file(s) this \
                             db already knows about — {out_of_scope} ({:.0}%) fall outside it and \
                             reconcile is about to drop their source rows. This is reconcile \
                             semantics (the db narrows to whatever the program scans this tick), \
                             not a bug; if that is unintended, broaden the program's scan \
                             glob/root or start a fresh --db instead of reconciling this one.",
                            share * 100.0,
                        ),
                        hint: None,
                    });
                }
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
        // Jobs are per FILE, each carrying every matching rule: the preparer
        // reads the content once and shares one tree cache across the rules
        // (one parse per grammar, not one per rule). `rule_files` lists rules
        // contiguously per file, so first-occurrence grouping keeps the flat
        // (file, rule) order — and therefore the staged ordinals — unchanged.
        let mut to_extract: Vec<crate::engine::source_prepare::SourceExtractJob> = Vec::new();
        let mut job_index: HashMap<(String, String, String), usize> = HashMap::new();
        for (idx, repo, path, rev, hash, head_binds) in rule_files.iter() {
            let stale = hash_of(&prev, repo, path, rev).as_ref() != Some(hash)
                || to_retract.contains(&(repo.clone(), path.clone()));
            if !stale {
                continue;
            }
            let rule = crate::engine::source_prepare::SourceExtractRule {
                rule_idx: *idx,
                head_binds: head_binds.clone(),
            };
            match job_index.get(&(repo.clone(), path.clone(), rev.clone())) {
                Some(&slot) => to_extract[slot].rules.push(rule),
                None => {
                    job_index.insert((repo.clone(), path.clone(), rev.clone()), to_extract.len());
                    to_extract.push(crate::engine::source_prepare::SourceExtractJob {
                        repo: repo.clone(),
                        path: path.clone(),
                        rev: rev.clone(),
                        hash: hash.clone(),
                        rules: vec![rule],
                    });
                }
            }
        }
        // `parsed` keeps its historical meaning: (file, rule) extractions run.
        let parsed = to_extract.iter().map(|job| job.rules.len()).sum();

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
        let cleanup = prepared.discard(&self.db);
        match outcome {
            Ok(report) => {
                self.rev_index = next_rev_index;
                self.dropped += prepared_dropped;
                self.extraction_drops.extend(prepared_drop_diags);
                if let Err(error) = cleanup {
                    tracing::warn!(
                        error = %error,
                        "[stage] committed source generation; TEMP cleanup deferred: {error}"
                    );
                }
                Ok(report)
            }
            Err(error) => {
                if let Err(cleanup_error) = cleanup {
                    tracing::warn!(
                        error = %cleanup_error,
                        "[stage] source rollback cleanup failed: {cleanup_error}"
                    );
                }
                Err(error)
            }
        }
    }

    // `retract_path`/`retract_paths` and `insert_source_rows{,_for_paths}`
    // (the two sides of the `_prov` map) live in `source_rows.rs`; the
    // term-form json/jsonp/sg pass lives in `term_extract.rs` (file-budget
    // decomp, 2026-07-18). All stay methods on `Engine`.
}
