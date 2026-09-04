use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::json;

use sprefa_extract::tsi::{Arg, Mode, RunOut};
use sprefa_extract::{content_id_of, dispatch, flatten_each, FamilyMask, FlatFact};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Observation {
    relation: String,
    args: Vec<Arg>,
}

struct ReceiptObservation {
    content: String,
    observation: Observation,
}

#[derive(Serialize)]
struct SourceCoordinate<'a> {
    repository: &'a str,
    worktree: &'a str,
    path: &'a str,
    content: &'a str,
}

struct ReceiptStore {
    connection: Connection,
}

impl ReceiptStore {
    fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS _extract_watch_receipt_v1 (
                source_key TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                content TEXT NOT NULL,
                observation_json TEXT NOT NULL,
                PRIMARY KEY (source_key, ordinal)
            ) WITHOUT ROWID",
        )?;
        Ok(Self { connection })
    }

    fn clear(&mut self) -> Result<(), rusqlite::Error> {
        self.connection
            .execute("DELETE FROM _extract_watch_receipt_v1", [])?;
        Ok(())
    }

    fn take(
        &mut self,
        source_key: &str,
    ) -> Result<Vec<ReceiptObservation>, Box<dyn std::error::Error>> {
        let transaction = self.connection.transaction()?;
        let encoded = {
            let mut statement = transaction.prepare(
                "SELECT content, observation_json FROM _extract_watch_receipt_v1
                 WHERE source_key = ? ORDER BY ordinal",
            )?;
            let rows = statement
                .query_map([source_key], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        transaction.execute(
            "DELETE FROM _extract_watch_receipt_v1 WHERE source_key = ?",
            [source_key],
        )?;
        transaction.commit()?;
        encoded
            .into_iter()
            .map(|(content, row)| {
                Ok(ReceiptObservation {
                    content,
                    observation: serde_json::from_str(&row)?,
                })
            })
            .collect()
    }

    fn replace(
        &mut self,
        source_key: &str,
        content: &str,
        observations: &[Observation],
    ) -> Result<Vec<ReceiptObservation>, Box<dyn std::error::Error>> {
        let previous = self.take(source_key)?;
        let transaction = self.connection.transaction()?;
        {
            let mut insert = transaction.prepare_cached(
                "INSERT INTO _extract_watch_receipt_v1
                 (source_key, ordinal, content, observation_json)
                 VALUES (?, ?, ?, ?)",
            )?;
            for (ordinal, observation) in observations.iter().enumerate() {
                insert.execute(params![
                    source_key,
                    ordinal as i64,
                    content,
                    serde_json::to_string(observation)?
                ])?;
            }
        }
        transaction.commit()?;
        Ok(previous)
    }
}

struct Options {
    root: PathBuf,
    patterns: Vec<soopy::Pattern>,
    mask: FamilyMask,
    state: Option<PathBuf>,
    once: bool,
    poll_ms: u64,
}

enum ChangeInput {
    Events(soopy::SourceWatcher),
    Poll(soopy::SourceSnapshot),
}

pub fn run(arguments: impl Iterator<Item = String>) -> Result<(), Box<dyn std::error::Error>> {
    let options = parse(arguments)?;
    let repository = soopy::open(&options.root)?;
    let state = options
        .state
        .unwrap_or_else(|| default_state_path(repository.worktree.0.as_ref()));
    let query = soopy::SourceQuery {
        revision: soopy::Revision::Worktree,
        patterns: options.patterns,
    };

    let mut tree = soopy::SourceTree::open(repository);
    let mut receipts = ReceiptStore::open(&state)?;
    let stdout = std::io::stdout();
    let mut output = BufWriter::with_capacity(256 * 1024, stdout.lock());

    if options.once {
        emit_snapshot(
            &mut tree,
            &query,
            options.mask,
            0,
            &mut receipts,
            &mut output,
        )?;
        return Ok(());
    }

    // Registration precedes the snapshot when the platform watcher accepts
    // the tree. notify can reject a checkout containing dangling symlinks;
    // the snapshot-diff fallback retains the same logical SourceDelta seam.
    let watcher = soopy::SourceTree::open(tree.repository().clone()).watch(query.clone());
    let snapshot = emit_snapshot(
        &mut tree,
        &query,
        options.mask,
        0,
        &mut receipts,
        &mut output,
    )?;
    let mut input = match watcher {
        Ok(watcher) => ChangeInput::Events(watcher),
        Err(error) => {
            eprintln!(
                "extract watch: filesystem registration failed ({error:#}); polling every {}ms",
                options.poll_ms
            );
            ChangeInput::Poll(snapshot)
        }
    };

    let mut generation = 1u64;
    loop {
        let deltas = match &mut input {
            ChangeInput::Events(watcher) => watcher.recv()?,
            ChangeInput::Poll(previous) => {
                std::thread::sleep(Duration::from_millis(options.poll_ms));
                let after = tree.snapshot(&query)?;
                let deltas = diff_snapshots(previous, &after);
                *previous = after;
                deltas
            }
        };
        if deltas.is_empty() {
            continue;
        }
        if deltas
            .iter()
            .any(|delta| matches!(delta, soopy::SourceDelta::RescanRequired))
        {
            let snapshot = emit_snapshot(
                &mut tree,
                &query,
                options.mask,
                generation,
                &mut receipts,
                &mut output,
            )?;
            if let ChangeInput::Poll(previous) = &mut input {
                *previous = snapshot;
            }
            generation += 1;
            continue;
        }
        emit_json(
            &mut output,
            &json!({"record":"batch_start", "generation":generation, "mode":"delta"}),
        )?;
        for delta in deltas {
            match delta {
                soopy::SourceDelta::Added(entry) => emit_replacement(
                    &mut tree,
                    &entry,
                    options.mask,
                    generation,
                    &mut receipts,
                    &mut output,
                )?,
                soopy::SourceDelta::Changed { before: _, after } => emit_replacement(
                    &mut tree,
                    &after,
                    options.mask,
                    generation,
                    &mut receipts,
                    &mut output,
                )?,
                soopy::SourceDelta::Removed(source) => {
                    let key = source_key(&source);
                    let old = receipts.take(&key)?;
                    for receipt in old {
                        emit_change(
                            &mut output,
                            generation,
                            -1,
                            &source,
                            &receipt.content,
                            &receipt.observation,
                        )?;
                    }
                }
                soopy::SourceDelta::RevisionChanged { .. } | soopy::SourceDelta::RescanRequired => {
                }
            }
        }
        emit_json(
            &mut output,
            &json!({"record":"batch_end", "generation":generation}),
        )?;
        output.flush()?;
        generation += 1;
    }
}

fn emit_snapshot(
    tree: &mut soopy::SourceTree,
    query: &soopy::SourceQuery,
    mask: FamilyMask,
    generation: u64,
    receipts: &mut ReceiptStore,
    output: &mut impl Write,
) -> Result<soopy::SourceSnapshot, Box<dyn std::error::Error>> {
    let snapshot = tree.snapshot(query)?;
    receipts.clear()?;
    emit_json(
        output,
        &json!({"record":"batch_start", "generation":generation, "mode":"snapshot"}),
    )?;
    for entry in &snapshot.files {
        emit_replacement(tree, &entry, mask, generation, receipts, output)?;
    }
    emit_json(
        output,
        &json!({"record":"batch_end", "generation":generation}),
    )?;
    output.flush()?;
    Ok(snapshot)
}

fn diff_snapshots(
    before: &soopy::SourceSnapshot,
    after: &soopy::SourceSnapshot,
) -> Vec<soopy::SourceDelta> {
    let before_by_path = before
        .files
        .iter()
        .map(|entry| (entry.source.path.clone(), entry))
        .collect::<std::collections::BTreeMap<_, _>>();
    let after_by_path = after
        .files
        .iter()
        .map(|entry| (entry.source.path.clone(), entry))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut deltas = Vec::new();
    for (path, entry) in &before_by_path {
        match after_by_path.get(path) {
            None => deltas.push(soopy::SourceDelta::Removed(entry.source.clone())),
            Some(next) if entry.content != next.content => {
                deltas.push(soopy::SourceDelta::Changed {
                    before: (*entry).clone(),
                    after: (*next).clone(),
                })
            }
            Some(_) => {}
        }
    }
    for (path, entry) in after_by_path {
        if !before_by_path.contains_key(&path) {
            deltas.push(soopy::SourceDelta::Added(entry.clone()));
        }
    }
    deltas
}

fn emit_replacement(
    tree: &mut soopy::SourceTree,
    entry: &soopy::SourceEntry,
    mask: FamilyMask,
    generation: u64,
    receipts: &mut ReceiptStore,
    output: &mut impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
    let source_bytes = tree.read_many(&[soopy::ReadRequest {
        source: entry.source.clone(),
        expected: Some(entry.content.clone()),
    }])?;
    let Some(source_bytes) = source_bytes.first() else {
        return Ok(());
    };
    let observations = extract_observations(
        source_bytes.source.path.0.as_ref(),
        &source_bytes.bytes,
        mask,
    )?;
    let key = source_key(&entry.source);
    let content = entry.content.to_string();
    let previous = receipts.replace(&key, &content, &observations)?;
    for receipt in previous {
        emit_change(
            output,
            generation,
            -1,
            &entry.source,
            &receipt.content,
            &receipt.observation,
        )?;
    }
    for observation in observations {
        emit_change(output, generation, 1, &entry.source, &content, &observation)?;
    }
    Ok(())
}

fn extract_observations(
    path: &str,
    bytes: &[u8],
    mask: FamilyMask,
) -> Result<Vec<Observation>, std::convert::Infallible> {
    let mut observations = Vec::new();
    if let Some(extracted) = dispatch(path, bytes, mask) {
        let run = RunOut {
            run: 0,
            mode: Mode::Syntax,
            tool: "extract-watch".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            scope: vec![content_id_of(bytes).to_string()],
        };
        flatten_each(&extracted, Some(&run), &mut |row| {
            if let FlatFact::Fact(fact) = row {
                observations.push(Observation {
                    relation: fact.relation,
                    args: fact.args,
                });
            }
            Ok(())
        })?;
    }
    Ok(observations)
}

fn emit_change(
    output: &mut impl Write,
    generation: u64,
    sign: i8,
    source: &soopy::SourceRef,
    content: &str,
    observation: &Observation,
) -> Result<(), Box<dyn std::error::Error>> {
    let worktree = match &source.revision {
        soopy::RevisionId::Worktree { worktree, .. } => worktree.0.as_ref(),
        soopy::RevisionId::Commit(_) => "",
    };
    emit_json(
        output,
        &json!({
            "record":"change",
            "generation":generation,
            "sign":sign,
            "source":SourceCoordinate {
                repository:source.repository.0.as_ref(),
                worktree,
                path:source.path.0.as_ref(),
                content,
            },
            "relation":observation.relation,
            "args":observation.args,
        }),
    )?;
    Ok(())
}

fn emit_json(output: &mut impl Write, value: &serde_json::Value) -> std::io::Result<()> {
    serde_json::to_writer(&mut *output, value)?;
    output.write_all(b"\n")
}

fn source_key(source: &soopy::SourceRef) -> String {
    let worktree = match &source.revision {
        soopy::RevisionId::Worktree { worktree, .. } => worktree.0.as_ref(),
        soopy::RevisionId::Commit(_) => "commit",
    };
    format!("{}\0{}\0{}", source.repository.0, worktree, source.path.0)
}

fn default_state_path(worktree: &str) -> PathBuf {
    let root = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".local/state"))
        })
        .unwrap_or_else(std::env::temp_dir);
    root.join("sprefa")
        .join(format!("extract-watch-{worktree}.sqlite3"))
}

fn parse(
    mut arguments: impl Iterator<Item = String>,
) -> Result<Options, Box<dyn std::error::Error>> {
    let _command = arguments.next();
    let root = arguments.next().ok_or(
        "usage: extract watch ROOT [--pattern GLOB] [--family NAMES] [--state PATH] [--once]",
    )?;
    let mut patterns = Vec::new();
    let mut families = Vec::new();
    let mut state = None;
    let mut once = false;
    let mut poll_ms = 500;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--pattern" => patterns.push(soopy::Pattern(
                arguments.next().ok_or("--pattern requires a glob")?.into(),
            )),
            "--family" => families.extend(
                arguments
                    .next()
                    .ok_or("--family requires comma-separated names")?
                    .split(',')
                    .map(str::to_owned),
            ),
            "--state" => {
                state = Some(PathBuf::from(
                    arguments.next().ok_or("--state requires a path")?,
                ))
            }
            "--once" => once = true,
            "--poll-ms" => {
                poll_ms = arguments
                    .next()
                    .ok_or("--poll-ms requires milliseconds")?
                    .parse()?;
                if poll_ms == 0 {
                    return Err("--poll-ms must be greater than zero".into());
                }
            }
            unknown => return Err(format!("unknown extract watch argument {unknown}").into()),
        }
    }
    if patterns.is_empty() {
        patterns = default_patterns();
    }
    Ok(Options {
        root: PathBuf::from(root),
        patterns,
        mask: parse_mask(&families)?,
        state,
        once,
        poll_ms,
    })
}

fn default_patterns() -> Vec<soopy::Pattern> {
    [
        "**/*.rs", "**/*.ts", "**/*.tsx", "**/*.js", "**/*.jsx", "**/*.go", "**/*.py", "**/*.kt",
        "**/*.kts", "**/*.dl6", "**/*.dl7", "**/*.pl",
    ]
    .into_iter()
    .map(|pattern| soopy::Pattern(pattern.into()))
    .collect()
}

fn parse_mask(families: &[String]) -> Result<FamilyMask, Box<dyn std::error::Error>> {
    if families.is_empty() {
        return Ok(FamilyMask::ALL);
    }
    let mut mask = FamilyMask::NONE;
    for family in families {
        match family.as_str() {
            "cst" => mask.cst = true,
            "type" | "types" => mask.types = true,
            "call" => mask.call = true,
            "df" => mask.df = true,
            "data" => mask.data = true,
            other => return Err(format!("unknown extract watch family {other}").into()),
        }
    }
    Ok(mask)
}
