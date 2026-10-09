//! `(extract ?Root ?Family ?Kind ?Payload)`: one long-lived `ryi --resolve`
//! child, one request per root+family over its stdin, one row per JSONL
//! record; `?Kind` is its `record` field. A request is
//! `{"id":N,"root":...,"family":...,"files":[...]}`; its records end with
//! `{"id":N,"done":true}` or fail with `{"id":N,"error":"..."}`.

use super::{handle, message, text_at};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IService};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

pub const RELATION: &str = "extract";
pub const ERROR: &str = "extract_error";

/// The 10-second law: a slower resolve is dropped and answered as an error
/// row; its late records are dropped with it.
const RUN_BUDGET: Duration = Duration::from_secs(10);

/// The child dies this often before error rows replace respawn.
const RESPAWNS: usize = 1;

pub struct Extract {
    relation: TermId,
    error: TermId,
    /// The one resolve child; `None` until the first request and after a death.
    child: Option<ChildHandle>,
    deaths: usize,
    /// Requests handed to the child, waiting for their answer.
    waiting: HashMap<u64, Pending>,
    next_id: u64,
    /// Finished records, request ends, and child death, plain data until
    /// `poll` mints terms.
    done: Receiver<Finished>,
    sender: Sender<Finished>,
    /// Failures answered without a request: a missing binary, an unknown
    /// family, an unreadable root.
    errors: Vec<(String, String, String)>,
}

/// The child's request line: the writer task drains it into the child's
/// stdin; dropping the last sender closes that stdin and ends the child.
struct ChildHandle {
    requests: UnboundedSender<String>,
}

/// One in-flight request: what a respawn re-sends, the row it answers, and
/// when its 10-second law runs out.
struct Pending {
    request: String,
    root: String,
    family: String,
    sent: Instant,
}

/// One stdout event. Plain data; terms are minted in `poll` on the tick
/// thread.
enum Finished {
    Record { id: u64, line: String },
    Done { id: u64 },
    Failed { id: u64, message: String },
    ChildGone,
}

impl Extract {
    pub fn new(relation: TermId, error: TermId) -> Extract {
        let (sender, done) = channel();
        Extract {
            relation,
            error,
            child: None,
            deaths: 0,
            waiting: HashMap::new(),
            next_id: 0,
            done,
            sender,
            errors: Vec::new(),
        }
    }

    /// Spawns the one child and its three tasks: a writer into its stdin, a
    /// diagnostics logger on its stderr, and a reader that owns the process.
    fn spawn_child(&mut self) -> Result<(), String> {
        let binary = extract_bin()?;
        let mut child = Command::new(&binary)
            .arg("--resolve")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|failure| format!("spawn {}: {failure}", binary.display()))?;
        let stdin = child.stdin.take().ok_or("extract stdin")?;
        let stdout = child.stdout.take().ok_or("extract stdout")?;
        let stderr = child.stderr.take().ok_or("extract stderr")?;
        let (requests, request_lines) = unbounded_channel();
        let done = self.sender.clone();
        self.child = Some(ChildHandle { requests });

        // The writer drains requests for the child's whole life; the loop
        // ends when the channel closes, and the dropped stdin ends the child.
        handle().spawn(async move {
            let mut stdin = stdin;
            let mut request_lines = request_lines;
            while let Some(request) = request_lines.recv().await {
                if stdin.write_all(request.as_bytes()).await.is_err()
                    || stdin.write_all(b"\n").await.is_err()
                    || stdin.flush().await.is_err()
                {
                    return;
                }
            }
        });

        // Diagnostics only: the protocol's failures travel on stdout. The
        // loop ends when the child exits and its stderr closes.
        handle().spawn(async move {
            let mut stderr = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = stderr.next_line().await {
                tracing::warn!(target: "dl8::extract", "{line}");
            }
        });

        // The reader owns the child: every stdout line is one protocol
        // event, and the stream's end is the child's death. The loop ends at
        // EOF, which is also how the wait and the death notice run.
        handle().spawn(async move {
            let mut child = child;
            let mut stdout = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = stdout.next_line().await {
                if let Some(event) = parse_line(&line) {
                    let _ = done.send(event);
                }
            }
            let _ = child.wait().await;
            let _ = done.send(Finished::ChildGone);
        });
        Ok(())
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

/// One stdout line: a record, a request's end, its failure. Anything else is
/// noise and skipped.
fn parse_line(line: &str) -> Option<Finished> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let id = value.get("id")?.as_u64()?;
    if value.get("record").is_some() {
        return Some(Finished::Record {
            id,
            line: line.to_string(),
        });
    }
    if value.get("done").and_then(|done| done.as_bool()) == Some(true) {
        return Some(Finished::Done { id });
    }
    let message = value
        .get("error")
        .and_then(|error| error.as_str())
        .map(str::to_string)?;
    Some(Finished::Failed { id, message })
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

impl IService for Extract {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn call(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) {
        for &application in pending {
            let (Some((_, root_text)), Some((_, family_text))) =
                (text_at(u, application, 0), text_at(u, application, 1))
            else {
                continue;
            };
            let files = match family_text.as_str() {
                "type" | "call" | "diet_scip" => files_under(Path::new(&root_text)),
                _ => Err(format!("unknown extract family {family_text}")),
            };
            let files = match files {
                Ok(files) => files,
                Err(failure) => {
                    self.errors.push((root_text, family_text, failure));
                    continue;
                }
            };
            if self.child.is_none() {
                if let Err(failure) = self.spawn_child() {
                    self.errors.push((root_text, family_text, failure));
                    continue;
                }
            }
            let id = self.next_id;
            self.next_id += 1;
            let request = json!({
                "id": id,
                "root": root_text,
                "family": family_text,
                "files": files,
            })
            .to_string();
            let Some(child) = &self.child else {
                continue;
            };
            if child.requests.send(request.clone()).is_err() {
                self.errors
                    .push((root_text, family_text, "extract stdin closed".to_string()));
                continue;
            }
            self.waiting.insert(
                id,
                Pending {
                    request,
                    root: root_text,
                    family: family_text,
                    sent: Instant::now(),
                },
            );
        }
    }

    fn poll(&mut self, u: &mut Universe) -> Vec<Row> {
        let mut rows = Vec::new();
        self.expire(u, &mut rows);
        self.absorb(u, &mut rows);
        for (root, family, failure) in std::mem::take(&mut self.errors) {
            rows.push(self.error_row(u, &root, &family, &failure));
        }
        rows
    }

    fn in_flight(&self) -> usize {
        self.waiting.len()
    }

    fn armed(&self) -> bool {
        false
    }
}

impl Extract {
    /// Requests past the 10-second law become error rows now; their late
    /// records drop on arrival. The loop is bounded by the waiting map, one
    /// entry per live request.
    fn expire(&mut self, u: &mut Universe, rows: &mut Vec<Row>) {
        let now = Instant::now();
        let stale: Vec<u64> = self
            .waiting
            .iter()
            .filter(|(_, pending)| now.duration_since(pending.sent) >= RUN_BUDGET)
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some(pending) = self.waiting.remove(&id) {
                rows.push(self.error_row(u, &pending.root, &pending.family, "timeout"));
            }
        }
    }

    /// Drains the finished channel: records become rows, a request's end
    /// frees it, and a death respawns once. The loop is bounded by the
    /// events the child produced.
    fn absorb(&mut self, u: &mut Universe, rows: &mut Vec<Row>) {
        while let Ok(event) = self.done.try_recv() {
            match event {
                Finished::Record { id, line } => {
                    let kind = serde_json::from_str::<serde_json::Value>(&line)
                        .ok()
                        .and_then(|value| {
                            value
                                .get("record")
                                .and_then(|kind| kind.as_str().map(str::to_string))
                        });
                    let (Some(kind), Some(pending)) = (kind, self.waiting.get(&id)) else {
                        continue;
                    };
                    rows.push(self.record_row(u, &pending.root, &pending.family, &kind, &line));
                }
                Finished::Done { id } => {
                    self.waiting.remove(&id);
                }
                Finished::Failed { id, message } => {
                    if let Some(pending) = self.waiting.remove(&id) {
                        rows.push(self.error_row(u, &pending.root, &pending.family, &message));
                    }
                }
                Finished::ChildGone => self.child_gone(u, rows),
            }
        }
    }

    /// The child died: one respawn re-sends the waiting requests, the next
    /// death answers every waiting request with an error row.
    fn child_gone(&mut self, u: &mut Universe, rows: &mut Vec<Row>) {
        self.child = None;
        self.deaths += 1;
        if self.deaths > RESPAWNS {
            for (_, pending) in std::mem::take(&mut self.waiting) {
                rows.push(self.error_row(
                    u,
                    &pending.root,
                    &pending.family,
                    "extract child exited",
                ));
            }
            return;
        }
        match self.spawn_child() {
            Ok(()) => {
                if let Some(child) = &self.child {
                    for (_, pending) in &self.waiting {
                        let _ = child.requests.send(pending.request.clone());
                    }
                }
            }
            Err(failure) => {
                for (_, pending) in std::mem::take(&mut self.waiting) {
                    rows.push(self.error_row(u, &pending.root, &pending.family, &failure));
                }
            }
        }
    }
}
