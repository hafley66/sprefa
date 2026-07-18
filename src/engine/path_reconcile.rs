//! Atomic source portion of a watcher-driven path tick.

use super::*;

pub(super) struct PathSourceReconcile {
    pub(super) changed_facts: bool,
    pub(super) changed_source_rels: HashSet<String>,
    pub(super) module_delta_paths: HashSet<String>,
    pub(super) module_full_work: bool,
    pub(super) node_delta_paths: HashSet<String>,
    pub(super) seen: HashSet<String>,
    pub(super) extracted: usize,
    pub(super) retracted: usize,
    pub(super) npaths: usize,
}

impl Engine {
    /// Keyed/merged source tables retain only the current winner, not every
    /// shadowed candidate. A path-local retract therefore cannot promote the
    /// next candidate honestly; route any matching watcher event through the
    /// full source inventory until candidate storage exists.
    pub(super) fn path_delta_needs_full_source_reconcile(
        &self,
        source_rules: &[&Rule],
        changed: &[PathBuf],
    ) -> Result<bool> {
        for rule in source_rules {
            let Some(meta) = self.rels.get(&rule.head.rel) else {
                continue;
            };
            if meta.key.is_none() && meta.merge.is_none() {
                continue;
            }
            let spec = scan_spec_of(rule)?;
            let (repo, rev, glob) = (str_of(&spec.repo)?, str_of(&spec.rev)?, str_of(&spec.glob)?);
            if rev != "WORK" || !(repo.is_empty() || repo == "." || repo == "self") {
                continue;
            }
            let matcher = globset::Glob::new(&glob)?.compile_matcher();
            if changed
                .iter()
                .filter_map(|path| path.strip_prefix(&self.root).ok())
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .any(|path| matcher.is_match(&path))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn reconcile_source_paths(
        &mut self,
        source_rules: &[&Rule],
        source_rels: &[String],
        changed: &[PathBuf],
        wants_module_rels: bool,
    ) -> Result<PathSourceReconcile> {
        let mut work_rules = Vec::new();
        for (idx, rule) in source_rules.iter().enumerate() {
            let spec = scan_spec_of(rule)?;
            let (repo, rev, glob) = (str_of(&spec.repo)?, str_of(&spec.rev)?, str_of(&spec.glob)?);
            if rev == "WORK" && (repo.is_empty() || repo == "." || repo == "self") {
                work_rules.push((idx, globset::Glob::new(&glob)?.compile_matcher()));
            }
        }

        let prev = self.load_file_meta()?;
        let mut current = prev.clone();
        let slug = self.self_slug();
        let mut result = PathSourceReconcile {
            changed_facts: false,
            changed_source_rels: HashSet::new(),
            module_delta_paths: HashSet::new(),
            module_full_work: false,
            node_delta_paths: HashSet::new(),
            seen: HashSet::new(),
            extracted: 0,
            retracted: 0,
            npaths: 0,
        };
        let mut jobs = Vec::new();
        let mut retracts = HashSet::new();
        let mut next_rev_index = self.rev_index.clone();

        for path in changed {
            let rel = match path.strip_prefix(&self.root) {
                Ok(path) => path.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            if !result.seen.insert(rel.clone()) {
                continue;
            }
            let matching: Vec<usize> = work_rules
                .iter()
                .filter(|(_, matcher)| matcher.is_match(&rel))
                .map(|(idx, _)| *idx)
                .collect();
            if matching.is_empty() {
                if wants_module_rels && module_manifest_path(&rel) {
                    result.module_full_work = true;
                }
                continue;
            }
            result.npaths += 1;
            let abs = self.root.join(&rel);
            if abs.is_file() {
                next_rev_index.insert((slug.clone(), "WORK".into(), rel.clone()));
                jobs.push(crate::engine::source_prepare::WorkPathExtractJob {
                    repo: slug.clone(),
                    path: rel,
                    rev: "WORK".into(),
                    rule_indices: matching,
                });
            } else {
                if prev.contains_key(&(slug.clone(), rel.clone(), "WORK".into())) {
                    result.module_full_work = true;
                }
                result.node_delta_paths.insert(rel.clone());
                retracts.insert((slug.clone(), rel.clone()));
                next_rev_index.remove(&(slug.clone(), "WORK".into(), rel.clone()));
                current.remove(&(slug.clone(), rel.clone(), "WORK".into()));
                for idx in matching {
                    result
                        .changed_source_rels
                        .insert(source_rules[idx].head.rel.clone());
                }
            }
        }

        if retracts.is_empty() && jobs.is_empty() {
            return Ok(result);
        }
        jobs.sort_by(|left, right| {
            (&left.repo, &left.rev, &left.path).cmp(&(&right.repo, &right.rev, &right.path))
        });
        let (generation, base) = crate::engine::pipeline::source_stage_base(self, source_rules)?;
        let prepared = crate::engine::source_prepare::prepare_work_path_batch(
            self,
            source_rules,
            &jobs,
            &prev,
            &next_rev_index,
            generation,
            base,
        )?;
        let crate::engine::source_prepare::PreparedWorkPathBatch {
            facts,
            changed: prepared_paths,
            dropped,
            drop_diags,
        } = prepared;
        for prepared in prepared_paths {
            let key = (
                prepared.repo.clone(),
                prepared.path.clone(),
                prepared.rev.clone(),
            );
            if prev.contains_key(&key) {
                result.module_delta_paths.insert(prepared.path.clone());
            } else {
                result.module_full_work = true;
            }
            result.node_delta_paths.insert(prepared.path.clone());
            retracts.insert((prepared.repo.clone(), prepared.path.clone()));
            current.insert(
                key,
                (prepared.hash, prepared.mtime, prepared.size, prepared.lines),
            );
            for idx in prepared.rule_indices {
                result
                    .changed_source_rels
                    .insert(source_rules[idx].head.rel.clone());
            }
        }
        if retracts.is_empty() {
            facts.discard(self.db.conn())?;
            return Ok(result);
        }
        let retract_owned: Vec<(String, String)> = retracts.into_iter().collect();
        let candidate_rels = result.changed_source_rels.clone();
        let outcome = self.with_semantic_generation(|engine| {
            let (_, current_base) =
                crate::engine::pipeline::source_stage_base(engine, source_rules)?;
            facts.verify(engine, current_base)?;
            let refs: Vec<(&str, &str)> = retract_owned
                .iter()
                .map(|(repo, path)| (repo.as_str(), path.as_str()))
                .collect();
            let retracted = engine.retract_paths(&refs, source_rels)?;
            let extracted = facts.apply(engine, current_base)?;
            engine.save_file_meta(&current, &prev)?;
            let changed_rels = engine.prune_unchanged_by_digest(candidate_rels)?;
            Ok((extracted, retracted, changed_rels))
        });
        let cleanup = facts.discard(self.db.conn());
        match outcome {
            Ok((extracted, retracted, changed_rels)) => {
                self.rev_index = next_rev_index;
                self.dropped += dropped;
                self.extraction_drops.extend(drop_diags);
                result.extracted = extracted;
                result.retracted = retracted;
                result.changed_source_rels = changed_rels;
                result.changed_facts = true;
                if let Err(error) = cleanup {
                    tracing::warn!(
                        error = %error,
                        "[stage] committed path generation; TEMP cleanup deferred: {error}"
                    );
                }
                Ok(result)
            }
            Err(error) => {
                if let Err(cleanup_error) = cleanup {
                    tracing::warn!(
                        error = %cleanup_error,
                        "[stage] path rollback cleanup failed: {cleanup_error}"
                    );
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyed_source_match_requires_full_reconcile() {
        let program = crate::parse::parse(crate::lex::lex(r#"
            rel winner(k: text, value: text) key(k).
            rel plain(value: text).
            winner("same", path) <- scan("WORK", "keyed.txt", path, rev), match(path, rev, /./, line).
            plain(path) <- scan("WORK", "plain.txt", path, rev), match(path, rev, /./, line).
        "#).unwrap()).unwrap();
        let root = std::env::temp_dir().join("sprefa-keyed-path-fallback");
        let mut engine = Engine::new(crate::db::open(None).unwrap(), root.clone());
        engine.ensure_meta().unwrap();
        engine.declare_all(&program, &HashMap::new()).unwrap();
        let source_rules: Vec<&Rule> = program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Rule(rule) if rule.is_source() => Some(rule),
                _ => None,
            })
            .collect();

        assert!(engine
            .path_delta_needs_full_source_reconcile(&source_rules, &[root.join("keyed.txt")],)
            .unwrap());
        assert!(!engine
            .path_delta_needs_full_source_reconcile(&source_rules, &[root.join("plain.txt")],)
            .unwrap());
    }
}
