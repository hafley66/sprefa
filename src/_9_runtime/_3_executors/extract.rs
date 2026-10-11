//! `(extract ?Root ?Family ?Kind ?Payload)`: one `ryi --resolve --arms <arms>
//! <files>` run per root+family, one row per JSONL record; `?Kind` is its
//! `record` field. `ryi` is the thin client of the warm `ryii --daemon`, so a
//! run costs a process start and a socket request, not a cold index.

use super::{handle, message, text_at, wake};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IService};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

pub const RELATION: &str = "extract";
pub const ERROR: &str = "extract_error";

/// The 10-second law: a slower run is killed and answered as an error row.
pub(super) const RUN_BUDGET: Duration = Duration::from_secs(10);

/// The `ryi` flags a run passes; `ryi help flags` must list each one.
pub const FLAGS: [&str; 2] = ["--resolve", "--arms"];

/// The stderr tail an error row carries when `ryi` exits non-zero.
const STDERR_TAIL: usize = 2000;

pub struct Extract {
    relation: TermId,
    error: TermId,
    /// Finished runs, plain data until `poll` mints terms.
    done: Receiver<Finished>,
    sender: Sender<Finished>,
    /// Runs started and not yet drained.
    tasks: usize,
}

/// One finished run: its `(kind, line)` records or its failure.
struct Finished {
    root: String,
    family: String,
    outcome: Result<Vec<(String, String)>, String>,
}

impl Extract {
    pub fn new(relation: TermId, error: TermId) -> Extract {
        let (sender, done) = channel();
        Extract {
            relation,
            error,
            done,
            sender,
            tasks: 0,
        }
    }

    fn record_row(
        &self,
        u: &mut Universe,
        root: &str,
        family: &str,
        kind: &str,
        payload: &str,
    ) -> Row {
        let root = u.string(root);
        let family = u.string(family);
        let kind = u.string(kind);
        let payload = u.string(payload);
        Row {
            rel: self.relation,
            args: vec![
                u.compound("const", vec![root]),
                u.compound("const", vec![family]),
                u.compound("const", vec![kind]),
                u.compound("const", vec![payload]),
            ],
        }
    }

    fn error_row(&self, u: &mut Universe, root: &str, family: &str, failure: &str) -> Row {
        let root = u.string(root);
        let family = u.string(family);
        let failure = u.string(failure);
        Row {
            rel: self.error,
            args: vec![
                u.compound("const", vec![root]),
                u.compound("const", vec![family]),
                u.compound("const", vec![failure]),
            ],
        }
    }
}

/// The resolve arms one family asks of `ryi`.
pub(super) fn arms(family: &str) -> Result<&'static str, String> {
    match family {
        "type" => Ok("type"),
        "call" => Ok("call"),
        "diet_scip" => Ok("call,type"),
        _ => Err(format!("unknown extract family {family}")),
    }
}

/// One `ryi` run in `root`. Every stdout line carrying a `record` field is a
/// record; a non-zero exit is a failure carrying stderr's tail. The read
/// loop is a drain bounded by the child's stdout.
pub(super) async fn run(
    binary: PathBuf,
    root: &str,
    arms: &str,
    files: Vec<String>,
) -> Result<Vec<(String, String)>, String> {
    let mut child = Command::new(&binary)
        .current_dir(root)
        .args(FLAGS)
        .arg(arms)
        .args(&files)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|failure| format!("spawn {}: {failure}", binary.display()))?;
    let stdout = child.stdout.take().ok_or("extract stdout")?;
    let mut stderr = child.stderr.take().ok_or("extract stderr")?;
    let complaint = tokio::spawn(async move {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text).await;
        text
    });
    let mut records = Vec::new();
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await.map_err(message)? {
        let kind = serde_json::from_str::<serde_json::Value>(&line)
            .ok()
            .and_then(|value| value.get("record")?.as_str().map(str::to_string));
        if let Some(kind) = kind {
            records.push((kind, line));
        }
    }
    let status = child.wait().await.map_err(message)?;
    let complaint = complaint.await.unwrap_or_default();
    if !status.success() {
        let start = complaint.len().saturating_sub(STDERR_TAIL);
        let start = (start..complaint.len())
            .find(|index| complaint.is_char_boundary(*index))
            .unwrap_or(complaint.len());
        return Err(format!(
            "ryi exited {status}: {}",
            complaint[start..].trim()
        ));
    }
    Ok(records)
}

/// `$SPREFA_EXTRACT_BIN`, then `ryi` on `PATH`.
pub fn extract_bin() -> Result<PathBuf, String> {
    if let Some(named) = std::env::var_os("SPREFA_EXTRACT_BIN") {
        let path = PathBuf::from(named);
        return match path.is_file() {
            true => Ok(path),
            false => Err(format!(
                "SPREFA_EXTRACT_BIN names {}, which is not a file",
                path.display()
            )),
        };
    }
    which::which("ryi").map_err(|_| "no ryi on PATH; set SPREFA_EXTRACT_BIN".to_string())
}

/// Files under `root`, relative to it: git's tracked set when `root` sits in a
/// checkout, else every regular file.
pub(super) fn files_under(root: &Path) -> Result<Vec<String>, String> {
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

impl IService for Extract {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn call(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) {
        for &application in pending {
            let (Some((_, root)), Some((_, family))) =
                (text_at(u, application, 0), text_at(u, application, 1))
            else {
                continue;
            };
            let sender = self.sender.clone();
            self.tasks += 1;
            let started = arms(&family)
                .and_then(|arms| Ok((extract_bin()?, arms, files_under(Path::new(&root))?)));
            let (binary, arms, files) = match started {
                Ok(started) => started,
                Err(failure) => {
                    let _ = sender.send(Finished {
                        root,
                        family,
                        outcome: Err(failure),
                    });
                    continue;
                }
            };
            handle().spawn(async move {
                let outcome =
                    match tokio::time::timeout(RUN_BUDGET, run(binary, &root, arms, files)).await {
                        Ok(outcome) => outcome,
                        Err(_) => Err("timeout".to_string()),
                    };
                let _ = sender.send(Finished {
                    root,
                    family,
                    outcome,
                });
                wake();
            });
        }
    }

    fn poll(&mut self, u: &mut Universe) -> Vec<Row> {
        let mut rows = Vec::new();
        while let Ok(finished) = self.done.try_recv() {
            self.tasks -= 1;
            match finished.outcome {
                Ok(records) => {
                    for (kind, line) in records {
                        rows.push(self.record_row(
                            u,
                            &finished.root,
                            &finished.family,
                            &kind,
                            &line,
                        ));
                    }
                }
                Err(failure) => {
                    tracing::warn!(target: "dl8::extract", root = %finished.root, family = %finished.family, "{failure}");
                    rows.push(self.error_row(u, &finished.root, &finished.family, &failure));
                }
            }
        }
        rows
    }

    fn in_flight(&self) -> usize {
        self.tasks
    }

    fn armed(&self) -> bool {
        false
    }
}
