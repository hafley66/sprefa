//! Bounded parallel source preparation feeding the single SQLite stage writer.

use super::*;

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::sync_channel,
    Arc,
};

const READY_FILE_SLOTS: usize = 2;

#[derive(Clone, Debug)]
pub(super) struct SourceExtractJob {
    pub(super) rule_idx: usize,
    pub(super) repo: String,
    pub(super) path: String,
    pub(super) rev: String,
    pub(super) hash: String,
    pub(super) head_binds: Vec<(String, String)>,
}
type ParsedFile = (
    String,
    String,
    Vec<Vec<Value>>,
    Vec<(spine::WhereBytes, String)>,
    usize,
);

#[derive(Clone, Debug)]
pub(super) struct WorkPathExtractJob {
    pub(super) repo: String,
    pub(super) path: String,
    pub(super) rev: String,
    pub(super) rule_indices: Vec<usize>,
}

pub(super) struct PreparedWorkPath {
    ordinal: usize,
    pub(super) repo: String,
    pub(super) path: String,
    pub(super) rev: String,
    pub(super) hash: String,
    pub(super) mtime: i64,
    pub(super) size: i64,
    pub(super) lines: i64,
    pub(super) rule_indices: Vec<usize>,
}

pub(super) struct PreparedWorkPathBatch {
    pub(super) facts: crate::engine::pipeline::PreparedSourceFacts,
    pub(super) changed: Vec<PreparedWorkPath>,
    pub(super) dropped: usize,
    pub(super) drop_diags: Vec<DiagRow>,
}

struct ParsedWorkRule {
    owner_ordinal: usize,
    parsed: ParsedFile,
}

enum ParsedWorkPath {
    Unchanged,
    Changed {
        meta: PreparedWorkPath,
        rules: Vec<ParsedWorkRule>,
    },
}

pub(super) struct PreparedSourceBatch {
    pub(super) facts: crate::engine::pipeline::PreparedSourceFacts,
    pub(super) dropped: usize,
    pub(super) drop_diags: Vec<DiagRow>,
}

pub(super) fn prepare_source_batch(
    engine: &Engine,
    source_rules: &[&Rule],
    jobs: &[SourceExtractJob],
    root_by_repo: &HashMap<String, PathBuf>,
    rev_index: &HashSet<(String, String, String)>,
    generation: i64,
    base: [u8; 32],
) -> Result<PreparedSourceBatch> {
    let mut stage =
        crate::engine::pipeline::FullSourceStageBuilder::new(engine.db.conn(), generation, base)?;
    let mut dropped = 0usize;
    let mut drop_diags = Vec::new();
    let (sender, receiver) = sync_channel::<(usize, String, Result<ParsedFile>)>(READY_FILE_SLOTS);
    let cancel = Arc::new(AtomicBool::new(false));
    let rels = &engine.rels;

    std::thread::scope(|scope| -> Result<()> {
        let producer = sender.clone();
        let producer_cancel = Arc::clone(&cancel);
        scope.spawn(move || {
            jobs.par_iter()
                .enumerate()
                .for_each_with(producer, |sender, (ordinal, job)| {
                    if producer_cancel.load(Ordering::Acquire) {
                        return;
                    }
                    crate::budget::throttle_point();
                    tracing::trace!(
                        file = %job.path, rule = job.rule_idx, repo = %job.repo,
                        "[source] extract file x rule"
                    );
                    // Cancellation is cooperative: parse_file runs to completion
                    // once entered, then its result is discarded if a peer failed.
                    let result = root_by_repo
                        .get(&job.repo)
                        .ok_or_else(|| anyhow::anyhow!("no root for repo {}", job.repo))
                        .and_then(|root| {
                            let (rows, wheres, dropped) = parse_file(
                                source_rules[job.rule_idx],
                                &job.repo,
                                &job.path,
                                &job.rev,
                                &job.hash,
                                root,
                                rels,
                                rev_index,
                                &job.head_binds,
                                None,
                            )?;
                            Ok((
                                source_rules[job.rule_idx].head.rel.clone(),
                                job.path.clone(),
                                rows,
                                wheres,
                                dropped,
                            ))
                        });
                    let result = match result {
                        Ok(parsed) => {
                            if producer_cancel.load(Ordering::Acquire) {
                                return;
                            }
                            Ok(parsed)
                        }
                        Err(error) => {
                            if producer_cancel.swap(true, Ordering::AcqRel) {
                                return;
                            }
                            Err(error)
                        }
                    };
                    if sender.send((ordinal, job.repo.clone(), result)).is_err() {
                        producer_cancel.store(true, Ordering::Release);
                    }
                });
        });
        drop(sender);

        while let Ok((file_ordinal, repo, parsed)) = receiver.recv() {
            let parsed = match parsed {
                Ok(parsed) => parsed,
                Err(error) => {
                    cancel.store(true, Ordering::Release);
                    drop(receiver);
                    return Err(error);
                }
            };
            if let Err(error) = stage_parsed_file(
                &mut stage,
                file_ordinal,
                &repo,
                Ok(parsed),
                &mut dropped,
                &mut drop_diags,
            ) {
                cancel.store(true, Ordering::Release);
                drop(receiver);
                return Err(error);
            }
        }
        Ok(())
    })?;

    Ok(PreparedSourceBatch {
        facts: stage.seal()?,
        dropped,
        drop_diags,
    })
}

/// Watcher-only preparation. Jobs retain path identities and rule indices;
/// each Rayon worker owns one content snapshot while it hashes, counts, and
/// evaluates every matching rule. The bounded channel limits ready PATH
/// bundles, not their bytes: an individual extractor result can still be large.
pub(super) fn prepare_work_path_batch(
    engine: &Engine,
    source_rules: &[&Rule],
    jobs: &[WorkPathExtractJob],
    prev: &FileMeta,
    rev_index: &HashSet<(String, String, String)>,
    generation: i64,
    base: [u8; 32],
) -> Result<PreparedWorkPathBatch> {
    prepare_work_path_batch_with_reader(
        engine,
        source_rules,
        jobs,
        prev,
        rev_index,
        generation,
        base,
        &|path| {
            std::fs::read_to_string(path)
                .with_context(|| format!("read changed source {}", path.display()))
        },
    )
}

fn prepare_work_path_batch_with_reader(
    engine: &Engine,
    source_rules: &[&Rule],
    jobs: &[WorkPathExtractJob],
    prev: &FileMeta,
    rev_index: &HashSet<(String, String, String)>,
    generation: i64,
    base: [u8; 32],
    reader: &(dyn Fn(&Path) -> Result<String> + Sync),
) -> Result<PreparedWorkPathBatch> {
    let mut stage =
        crate::engine::pipeline::FullSourceStageBuilder::new(engine.db.conn(), generation, base)?;
    let mut changed = Vec::new();
    let mut dropped = 0usize;
    let mut drop_diags = Vec::new();
    let (sender, receiver) = sync_channel::<Result<ParsedWorkPath>>(READY_FILE_SLOTS);
    let cancel = Arc::new(AtomicBool::new(false));
    let rels = &engine.rels;
    let root = &engine.root;
    let mut ordered_jobs: Vec<&WorkPathExtractJob> = jobs.iter().collect();
    ordered_jobs.sort_by(|left, right| {
        (&left.repo, &left.rev, &left.path).cmp(&(&right.repo, &right.rev, &right.path))
    });
    let mut next_owner_ordinal = 0usize;
    let mut owner_bases = Vec::with_capacity(ordered_jobs.len());
    for job in &ordered_jobs {
        owner_bases.push(next_owner_ordinal);
        next_owner_ordinal = next_owner_ordinal
            .checked_add(job.rule_indices.len())
            .ok_or_else(|| anyhow::anyhow!("too many watcher source owners"))?;
    }

    std::thread::scope(|scope| -> Result<()> {
        let producer = sender.clone();
        let producer_cancel = Arc::clone(&cancel);
        scope.spawn(move || {
            ordered_jobs.par_iter().enumerate().for_each_with(
                producer,
                |sender, (ordinal, job)| {
                    if producer_cancel.load(Ordering::Acquire) {
                        return;
                    }
                    crate::budget::throttle_point();
                    tracing::trace!(file = %job.path, "[source] extract WORK file");
                    let parsed = parse_work_path(
                        ordinal,
                        owner_bases[ordinal],
                        job,
                        root,
                        source_rules,
                        rels,
                        prev,
                        rev_index,
                        reader,
                        &producer_cancel,
                    );
                    let parsed = match parsed {
                        Ok(Some(parsed)) => {
                            if producer_cancel.load(Ordering::Acquire) {
                                return;
                            }
                            Ok(parsed)
                        }
                        Ok(None) => return,
                        Err(error) => {
                            if producer_cancel.swap(true, Ordering::AcqRel) {
                                return;
                            }
                            Err(error)
                        }
                    };
                    if sender.send(parsed).is_err() {
                        producer_cancel.store(true, Ordering::Release);
                    }
                },
            );
        });
        drop(sender);

        while let Ok(parsed) = receiver.recv() {
            let parsed = match parsed {
                Ok(parsed) => parsed,
                Err(error) => {
                    cancel.store(true, Ordering::Release);
                    drop(receiver);
                    return Err(error);
                }
            };
            match parsed {
                ParsedWorkPath::Unchanged => {}
                ParsedWorkPath::Changed { meta, rules } => {
                    for rule in rules {
                        if let Err(error) = stage_parsed_file(
                            &mut stage,
                            rule.owner_ordinal,
                            &meta.repo,
                            Ok(rule.parsed),
                            &mut dropped,
                            &mut drop_diags,
                        ) {
                            cancel.store(true, Ordering::Release);
                            drop(receiver);
                            return Err(error);
                        }
                    }
                    changed.push(meta);
                }
            }
        }
        Ok(())
    })?;

    changed.sort_by_key(|path| path.ordinal);
    Ok(PreparedWorkPathBatch {
        facts: stage.seal()?,
        changed,
        dropped,
        drop_diags,
    })
}

fn parse_work_path(
    ordinal: usize,
    owner_base: usize,
    job: &WorkPathExtractJob,
    root: &Path,
    source_rules: &[&Rule],
    rels: &Rels,
    prev: &FileMeta,
    rev_index: &HashSet<(String, String, String)>,
    reader: &(dyn Fn(&Path) -> Result<String> + Sync),
    cancel: &AtomicBool,
) -> Result<Option<ParsedWorkPath>> {
    if cancel.load(Ordering::Acquire) {
        return Ok(None);
    }
    let abs = root.join(&job.path);
    let content: Arc<str> = Arc::from(reader(&abs)?);
    let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
    let key = (job.repo.clone(), job.path.clone(), job.rev.clone());
    if prev.get(&key).is_some_and(|row| row.0 == hash) {
        return Ok(Some(ParsedWorkPath::Unchanged));
    }
    let mtime = std::fs::metadata(&abs)
        .ok()
        .map(|meta| mtime_secs(&meta))
        .unwrap_or(0);
    let Some(rules) = collect_work_rules(&job.rule_indices, cancel, |rule_offset, rule_idx| {
        let (rows, wheres, dropped) = parse_file(
            source_rules[rule_idx],
            &job.repo,
            &job.path,
            &job.rev,
            &hash,
            root,
            rels,
            rev_index,
            &[],
            Some(content.as_ref()),
        )?;
        Ok(ParsedWorkRule {
            owner_ordinal: owner_base
                .checked_add(rule_offset)
                .ok_or_else(|| anyhow::anyhow!("too many watcher source owners"))?,
            parsed: (
                source_rules[rule_idx].head.rel.clone(),
                job.path.clone(),
                rows,
                wheres,
                dropped,
            ),
        })
    })?
    else {
        return Ok(None);
    };
    let meta = PreparedWorkPath {
        ordinal,
        repo: job.repo.clone(),
        path: job.path.clone(),
        rev: job.rev.clone(),
        hash,
        mtime,
        size: content.len() as i64,
        lines: count_lines(content.as_bytes()),
        rule_indices: job.rule_indices.clone(),
    };
    Ok(Some(ParsedWorkPath::Changed { meta, rules }))
}

fn collect_work_rules<T>(
    rule_indices: &[usize],
    cancel: &AtomicBool,
    mut parse: impl FnMut(usize, usize) -> Result<T>,
) -> Result<Option<Vec<T>>> {
    let mut parsed = Vec::with_capacity(rule_indices.len());
    for (rule_offset, &rule_idx) in rule_indices.iter().enumerate() {
        // Cancellation is cooperative: an extractor already executing is not
        // interrupted, but no later matching rule starts after cancellation.
        if cancel.load(Ordering::Acquire) {
            return Ok(None);
        }
        parsed.push(parse(rule_offset, rule_idx)?);
    }
    Ok(Some(parsed))
}

fn stage_parsed_file(
    stage: &mut crate::engine::pipeline::FullSourceStageBuilder<'_>,
    file_ordinal: usize,
    repo: &str,
    parsed: Result<ParsedFile>,
    dropped_total: &mut usize,
    drop_diags: &mut Vec<DiagRow>,
) -> Result<()> {
    let (rel, path, rows, wheres, dropped) = parsed?;
    *dropped_total += dropped;
    if dropped > 0 {
        drop_diags.push(DiagRow {
            path: path.clone(),
            line: 1,
            col: 0,
            end_line: 1,
            end_col: 0,
            severity: "warn".into(),
            code: "checked-type".into(),
            msg: format!("{dropped} row(s) failing file/dir/path checks dropped from `{rel}`"),
            hint: None,
        });
    }
    let file_ordinal =
        u32::try_from(file_ordinal).map_err(|_| anyhow::anyhow!("too many staged source files"))?;
    if file_ordinal > i32::MAX as u32 {
        anyhow::bail!("too many staged source files");
    }
    for (span_ordinal, (span, text)) in wheres.into_iter().enumerate() {
        let span_ordinal = u32::try_from(span_ordinal)
            .map_err(|_| anyhow::anyhow!("too many staged spans for {path}"))?;
        let ordinal = (u64::from(file_ordinal) << 32) | u64::from(span_ordinal);
        stage.push_where(repo, &path, ordinal, span, text)?;
    }
    stage.complete_where_owner(repo, &path)?;
    for (row_ordinal, row) in rows.into_iter().enumerate() {
        let row_ordinal = u32::try_from(row_ordinal)
            .map_err(|_| anyhow::anyhow!("too many staged rows for {path}"))?;
        let ordinal = (u64::from(file_ordinal) << 32) | u64::from(row_ordinal);
        stage.push(&rel, repo, &path, ordinal, &row)?;
    }
    stage.complete_owner(&rel, repo, &path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn fixture() -> (Engine, Program) {
        let program = crate::parse::parse(
            crate::lex::lex(
                r#"
            rel left(path: text, line: int).
            rel right(path: text, line: int).
            rel winner(k: text, value: text) key(k).
            left(path, line) <- scan("WORK", "*.txt", path, rev), match(path, rev, /x/, line).
            right(path, line) <- scan("WORK", "*.txt", path, rev), match(path, rev, /x/, line).
            winner("same", path) <- scan("WORK", "*.txt", path, rev), match(path, rev, /x/, line).
        "#,
            )
            .unwrap(),
        )
        .unwrap();
        let root = std::env::temp_dir().join("sprefa-source-prepare-reader");
        let mut engine = Engine::new(crate::db::open(None).unwrap(), root);
        engine.ensure_meta().unwrap();
        engine.declare_all(&program, &HashMap::new()).unwrap();
        (engine, program)
    }

    fn source_rules(program: &Program) -> Vec<&Rule> {
        program
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Rule(rule) if rule.is_source() => Some(rule),
                _ => None,
            })
            .collect()
    }

    fn work_job() -> WorkPathExtractJob {
        WorkPathExtractJob {
            repo: "repo".into(),
            path: "a.txt".into(),
            rev: "WORK".into(),
            rule_indices: vec![0, 1],
        }
    }

    #[test]
    fn ready_file_queue_stays_tiny() {
        assert!(super::READY_FILE_SLOTS <= 2);
    }

    #[test]
    fn early_failure_or_cancellation_stops_remaining_work_rules_before_start() {
        let cancel = AtomicBool::new(false);
        let starts = AtomicUsize::new(0);
        let error = collect_work_rules(&[3, 5, 8], &cancel, |_, rule_idx| {
            starts.fetch_add(1, Ordering::SeqCst);
            if rule_idx == 3 {
                anyhow::bail!("injected parse failure");
            }
            Ok(rule_idx)
        })
        .err()
        .expect("the first rule must fail");
        assert!(error.to_string().contains("injected parse failure"));
        assert_eq!(
            starts.load(Ordering::SeqCst),
            1,
            "later rules must not start after failure"
        );

        starts.store(0, Ordering::SeqCst);
        let parsed = collect_work_rules(&[3, 5, 8], &cancel, |_, rule_idx| {
            starts.fetch_add(1, Ordering::SeqCst);
            // Model a failure reported by a peer while the current extractor
            // is completing a rule that was already allowed to start.
            cancel.store(true, Ordering::Release);
            Ok(rule_idx)
        })
        .unwrap();
        assert!(
            parsed.is_none(),
            "a cancelled path must not publish partial rules"
        );
        assert_eq!(
            starts.load(Ordering::SeqCst),
            1,
            "later rules must not start"
        );
    }

    #[test]
    fn work_path_reads_once_for_two_matching_rules() {
        let (mut engine, program) = fixture();
        let rules = source_rules(&program);
        assert_eq!(
            rules
                .iter()
                .map(|rule| rule.head.rel.as_str())
                .collect::<Vec<_>>(),
            ["left", "right", "winner"]
        );
        let (generation, base) =
            crate::engine::pipeline::source_stage_base(&engine, &rules).unwrap();
        let reads = AtomicUsize::new(0);
        let rev_index = HashSet::from([("repo".into(), "WORK".into(), "a.txt".into())]);
        let prepared = prepare_work_path_batch_with_reader(
            &engine,
            &rules,
            &[work_job()],
            &FileMeta::new(),
            &rev_index,
            generation,
            base,
            &|_| {
                reads.fetch_add(1, Ordering::SeqCst);
                Ok("x\n".into())
            },
        )
        .unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 1);
        assert_eq!(prepared.changed.len(), 1);
        let PreparedWorkPathBatch { facts, .. } = prepared;
        engine
            .with_semantic_generation(|engine| facts.apply(engine, base))
            .unwrap();
        facts.discard(engine.db.conn()).unwrap();
        for table in ["rel_left", "rel_right"] {
            let count: i64 = engine
                .db
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 1, "both matching rules parse the shared snapshot");
        }
    }

    #[test]
    fn work_path_read_error_leaves_live_state_unchanged() {
        let (engine, program) = fixture();
        let rules = source_rules(&program);
        let (generation, base) =
            crate::engine::pipeline::source_stage_base(&engine, &rules).unwrap();
        let before = engine.read_generation_watermark().unwrap();
        let tables = ["rel_left", "rel_right", "_file", "_prov", "_where_bytes"];
        let before_counts: Vec<i64> = tables
            .iter()
            .map(|table| {
                engine
                    .db
                    .conn()
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .unwrap()
            })
            .collect();
        let rev_index = HashSet::from([("repo".into(), "WORK".into(), "a.txt".into())]);
        let error = prepare_work_path_batch_with_reader(
            &engine,
            &rules,
            &[work_job()],
            &FileMeta::new(),
            &rev_index,
            generation,
            base,
            &|_| anyhow::bail!("injected read failure"),
        )
        .err()
        .expect("read failure must abort preparation");
        assert!(error.to_string().contains("injected read failure"));
        assert_eq!(engine.read_generation_watermark().unwrap(), before);
        for (table, before_count) in tables.into_iter().zip(before_counts) {
            let count: i64 = engine
                .db
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, before_count, "read failure must not mutate {table}");
        }
        for table in [
            "_source_stage_row",
            "_source_stage_owner",
            "_source_stage_ready",
        ] {
            let count: i64 = engine
                .db
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "failed stage must clean up {table}");
        }
    }

    #[test]
    fn reversed_jobs_and_delayed_reader_keep_stable_path_order() {
        let (mut engine, program) = fixture();
        let rules = source_rules(&program);
        let (generation, base) =
            crate::engine::pipeline::source_stage_base(&engine, &rules).unwrap();
        let jobs = [
            WorkPathExtractJob {
                repo: "repo".into(),
                path: "z.txt".into(),
                rev: "WORK".into(),
                rule_indices: vec![0],
            },
            WorkPathExtractJob {
                repo: "repo".into(),
                path: "a.txt".into(),
                rev: "WORK".into(),
                rule_indices: vec![0],
            },
        ];
        let rev_index = HashSet::from([
            ("repo".into(), "WORK".into(), "a.txt".into()),
            ("repo".into(), "WORK".into(), "z.txt".into()),
        ]);
        let prepared = prepare_work_path_batch_with_reader(
            &engine,
            &rules,
            &jobs,
            &FileMeta::new(),
            &rev_index,
            generation,
            base,
            &|path| {
                if path.file_name().is_some_and(|name| name == "a.txt") {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Ok("x\n".into())
            },
        )
        .unwrap();
        assert_eq!(
            prepared
                .changed
                .iter()
                .map(|path| path.path.as_str())
                .collect::<Vec<_>>(),
            ["a.txt", "z.txt"],
        );
        let PreparedWorkPathBatch { facts, .. } = prepared;
        let inserted = engine
            .with_semantic_generation(|engine| facts.apply(engine, base))
            .unwrap();
        assert_eq!(inserted, 2);
        facts.discard(engine.db.conn()).unwrap();
        let count: i64 = engine
            .db
            .conn()
            .query_row("SELECT count(*) FROM rel_left", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
