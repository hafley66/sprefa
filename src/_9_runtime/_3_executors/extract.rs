//! `(extract ?Root ?Family ?Kind ?Payload)`: one `extract --family F --resolve` run
//! over the root's files, one row per JSONL record; `?Kind` is its `record` field.

use super::message;
use super::text_at;
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IExecutor};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub const RELATION: &str = "extract";
pub const ERROR: &str = "extract_error";

/// The 10-second law: a slower run is killed and answered as an error row.
const RUN_BUDGET: Duration = Duration::from_secs(10);

pub struct Extract {
    relation: TermId,
    error: TermId,
}

/// One JSONL record: its `record` field and the whole line.
struct Record {
    kind: String,
    payload: String,
}

impl Extract {
    pub fn new(relation: TermId, error: TermId) -> Extract {
        Extract { relation, error }
    }
}

/// `$SPREFA_EXTRACT_BIN`, then `$CARGO_TARGET_DIR`, then the two build sites under
/// the sibling `hafley-rs` link, the order `tests/_16_extract_tsi.rs` searches.
pub fn extract_bin() -> Result<PathBuf, String> {
    if let Some(named) = std::env::var_os("SPREFA_EXTRACT_BIN") {
        let path = PathBuf::from(named);
        return match path.exists() {
            true => Ok(path),
            false => Err(format!(
                "SPREFA_EXTRACT_BIN names {}, which does not exist",
                path.display()
            )),
        };
    }
    let sprefa = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut candidates = Vec::new();
    if let Some(shared) = std::env::var_os("CARGO_TARGET_DIR") {
        candidates.push(PathBuf::from(shared).join("debug/ryi"));
    }
    candidates.push(sprefa.join("hafley-rs/target/debug/ryi"));
    candidates.push(sprefa.join("hafley-rs/crates/sprefa-extract/target/debug/ryi"));
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| "no ryi binary; set SPREFA_EXTRACT_BIN".to_string())
}

/// Files under `root`, relative to it: git's tracked set when `root` sits in a
/// checkout, else every regular file.
fn files_under(root: &Path) -> Result<Vec<String>, String> {
    let root = std::fs::canonicalize(root).map_err(message)?;
    if let Ok(repository) = soopy::discover(&root) {
        let top = std::fs::canonicalize(&repository.root).map_err(message)?;
        let prefix = root
            .strip_prefix(&top)
            .map_err(message)?
            .to_string_lossy()
            .to_string();
        let pathspecs = match prefix.is_empty() {
            true => Vec::new(),
            false => vec![prefix.clone()],
        };
        let entries = soopy::enumerate(
            &repository,
            &soopy::GitFilesQuery {
                revision: soopy::Revision::Worktree,
                pathspecs,
            },
        )
        .map_err(message)?;
        let mut files: Vec<String> = entries
            .into_iter()
            .filter_map(|entry| {
                let path = entry.source.path.0.to_string();
                match prefix.is_empty() {
                    true => Some(path),
                    false => path.strip_prefix(&format!("{prefix}/")).map(str::to_string),
                }
            })
            .collect();
        files.sort();
        return Ok(files);
    }
    let snapshot = soopy::DirectoryRoot::open(&root)
        .and_then(|mut directory| directory.snapshot(&soopy::FileQuery::default()))
        .map_err(message)?;
    let mut files: Vec<String> = snapshot
        .files
        .into_iter()
        .map(|entry| entry.file.path.0.to_string())
        .collect();
    files.sort();
    Ok(files)
}

fn run(root: &str, family: &str) -> Result<Vec<Record>, String> {
    let binary = extract_bin()?;
    let files = files_under(Path::new(root))?;
    let started = Instant::now();
    let mut child = Command::new(&binary)
        .current_dir(root)
        .args(["--family", family, "--resolve"])
        .args(&files)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn {}: {error}", binary.display()))?;
    let stdout = child.stdout.take().ok_or("extract stdout")?;
    let mut stderr = child.stderr.take().ok_or("extract stderr")?;
    let lines = std::thread::spawn(move || {
        BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .collect::<Vec<String>>()
    });
    let complaint = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    let status = loop {
        if let Some(status) = child.try_wait().map_err(message)? {
            break status;
        }
        if started.elapsed() > RUN_BUDGET {
            let _ = child.kill();
            let _ = child.wait();
            tracing::warn!(target: "dl8::extract", root, family, files = files.len(), "killed at the budget");
            return Err("timeout".to_string());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let lines = lines.join().map_err(|_| "extract stdout reader panicked")?;
    let complaint = complaint.join().unwrap_or_default();
    if !status.success() {
        return Err(format!(
            "extract exited {:?}: {}",
            status.code(),
            complaint.trim()
        ));
    }
    tracing::info!(target: "dl8::extract", root, family, files = files.len(), records = lines.len(), millis = started.elapsed().as_millis() as u64);
    Ok(lines
        .into_iter()
        .filter_map(|line| {
            let value: serde_json::Value = serde_json::from_str(&line).ok()?;
            let kind = value.get("record")?.as_str()?.to_string();
            Some(Record {
                kind,
                payload: line,
            })
        })
        .collect())
}

impl IExecutor for Extract {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn answer(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) -> Vec<Row> {
        let mut rows = Vec::new();
        for &application in pending {
            let (Some((root, root_text)), Some((family, family_text))) =
                (text_at(u, application, 0), text_at(u, application, 1))
            else {
                continue;
            };
            let root_cell = u.compound("const", vec![root]);
            let family_cell = u.compound("const", vec![family]);
            match run(&root_text, &family_text) {
                Ok(records) => {
                    for record in records {
                        let kind = u.string(&record.kind);
                        let payload = u.string(&record.payload);
                        rows.push(Row {
                            rel: self.relation,
                            args: vec![
                                root_cell,
                                family_cell,
                                u.compound("const", vec![kind]),
                                u.compound("const", vec![payload]),
                            ],
                        });
                    }
                }
                Err(failure) => {
                    let message = u.string(&failure);
                    rows.push(Row {
                        rel: self.error,
                        args: vec![root_cell, family_cell, u.compound("const", vec![message])],
                    });
                }
            }
        }
        rows
    }

    fn poll(&mut self, _u: &mut Universe, _timeout: Duration) -> Vec<Row> {
        Vec::new()
    }

    fn armed(&self) -> bool {
        false
    }
}
