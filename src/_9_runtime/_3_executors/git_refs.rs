//! `(git.refs ?Root ?Name ?Sha)`: one ref watcher per bound root. The opening
//! snapshot is rows; each wake's `diff_refs` delta is new rows; a removed ref writes nothing.

use super::{message, text_at};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IService};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const RELATION: &str = "git.refs";
pub const ERROR: &str = "git.refs_error";
/// How often a root whose watcher failed to open is re-read.
const RESNAPSHOT_EVERY: Duration = Duration::from_secs(1);

pub struct GitRefs {
    relation: TermId,
    error: TermId,
    continuing: bool,
    roots: BTreeMap<String, WatchedRoot>,
    /// Opening snapshots read inside `call`, drained by `poll`.
    done: Vec<Row>,
}
/// One root's last snapshot and the wake source that says when to re-read it.
struct WatchedRoot {
    root_cell: TermId,
    refs: soopy::Refs,
    query: soopy::RefQuery,
    last: soopy::RefSnapshot,
    wake: Wake,
}

enum Wake {
    Watcher(Box<soopy::RepositoryWatcher>),
    Every(Instant),
}

impl Wake {
    /// Blocks up to `slice`. A closed watcher degrades to the one-second re-read.
    fn woke(&mut self, root: &str, slice: Duration) -> bool {
        if let Wake::Watcher(watcher) = self {
            match watcher.recv_timeout(slice) {
                Ok(batch) => return batch.is_some(),
                Err(error) => {
                    tracing::warn!(target: "dl8::git_refs", root, error = %error, "watcher closed, re-reading every second")
                }
            }
            *self = Wake::Every(Instant::now() + RESNAPSHOT_EVERY);
            return true;
        }
        let Wake::Every(due) = self else {
            return false;
        };
        let wait = due.saturating_duration_since(Instant::now());
        if wait > slice {
            std::thread::sleep(slice);
            return false;
        }
        std::thread::sleep(wait);
        *due = Instant::now() + RESNAPSHOT_EVERY;
        true
    }
}

impl GitRefs {
    pub fn new(relation: TermId, error: TermId) -> GitRefs {
        GitRefs {
            relation,
            error,
            continuing: true,
            roots: BTreeMap::new(),
            done: Vec::new(),
        }
    }

    fn open(
        root: &str,
    ) -> Result<(soopy::Refs, soopy::RefQuery, soopy::RefSnapshot, Wake), String> {
        let repository = soopy::discover(root).map_err(message)?;
        let query = soopy::RefQuery {
            repository: repository.identity.clone(),
            namespace: Arc::from(""),
            name: None,
            pattern: None,
        };
        let watch = soopy::WatchQuery {
            source: None,
            refs: Some(query.clone()),
            index: false,
            linked_worktrees: false,
            coalescing: soopy::WatchCoalescing::default(),
        };
        let wake = match soopy::SourceTree::open(repository.clone()).watch_repository(watch) {
            Ok(watcher) => Wake::Watcher(Box::new(watcher)),
            Err(error) => {
                tracing::warn!(target: "dl8::git_refs", root, error = %error, "watcher unavailable, re-reading every second");
                Wake::Every(Instant::now() + RESNAPSHOT_EVERY)
            }
        };
        let refs = soopy::Refs::open(repository);
        let last = refs.snapshot(&query).map_err(message)?;
        Ok((refs, query, last, wake))
    }

    /// One opening snapshot with no repository watcher.
    pub fn snapshot(relation: TermId, error: TermId) -> GitRefs {
        GitRefs {
            relation,
            error,
            continuing: false,
            roots: BTreeMap::new(),
            done: Vec::new(),
        }
    }

    fn snapshot_at(
        root: &str,
    ) -> Result<(soopy::Refs, soopy::RefQuery, soopy::RefSnapshot), String> {
        let repository = soopy::discover(root).map_err(message)?;
        let query = soopy::RefQuery {
            repository: repository.identity.clone(),
            namespace: Arc::from(""),
            name: None,
            pattern: None,
        };
        let refs = soopy::Refs::open(repository);
        let snapshot = refs.snapshot(&query).map_err(message)?;
        Ok((refs, query, snapshot))
    }

    fn row(&self, u: &mut Universe, root_cell: TermId, name: &str, sha: &str) -> Row {
        let name = u.string(name);
        let sha = u.string(sha);
        Row {
            rel: self.relation,
            args: vec![
                root_cell,
                u.compound("const", vec![name]),
                u.compound("const", vec![sha]),
            ],
        }
    }

    fn snapshot_rows(
        &self,
        u: &mut Universe,
        root_cell: TermId,
        snapshot: &soopy::RefSnapshot,
    ) -> Vec<Row> {
        let mut rows = Vec::with_capacity(snapshot.refs.len() + 1);
        for observation in &snapshot.refs {
            rows.push(self.row(u, root_cell, &observation.name, peeled(observation)));
        }
        if let Some(head) = &snapshot.head_target {
            rows.push(self.row(u, root_cell, "HEAD", &head.0));
        }
        rows
    }

    fn delta_rows(
        &self,
        u: &mut Universe,
        root_cell: TermId,
        before: &soopy::RefSnapshot,
        after: &soopy::RefSnapshot,
    ) -> Vec<Row> {
        let mut rows = Vec::new();
        for delta in soopy::diff_refs(before, after) {
            match delta {
                soopy::RefDelta::Added(observation)
                | soopy::RefDelta::Changed {
                    after: observation, ..
                } => rows.push(self.row(u, root_cell, &observation.name, peeled(&observation))),
                soopy::RefDelta::HeadChanged { .. } | soopy::RefDelta::Removed(_) => {}
            }
        }
        if after.head_target != before.head_target {
            if let Some(head) = &after.head_target {
                rows.push(self.row(u, root_cell, "HEAD", &head.0));
            }
        }
        rows
    }

    fn error_row(&self, u: &mut Universe, root_cell: TermId, failure: &str) -> Row {
        let message = u.string(failure);
        Row {
            rel: self.error,
            args: vec![root_cell, u.compound("const", vec![message])],
        }
    }
}

fn peeled(observation: &soopy::RefObservation) -> &str {
    &observation.peeled.as_ref().unwrap_or(&observation.direct).0
}

impl IService for GitRefs {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        if self.continuing {
            Cadence::Continuing
        } else {
            Cadence::Once
        }
    }

    fn call(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) {
        for &application in pending {
            let Some((root, root_text)) = text_at(u, application, 0) else {
                continue;
            };
            if self.roots.contains_key(&root_text) {
                continue;
            }
            let root_cell = u.compound("const", vec![root]);
            if !self.continuing {
                match Self::snapshot_at(&root_text) {
                    Ok((_, _, snapshot)) => {
                        self.done.extend(self.snapshot_rows(u, root_cell, &snapshot));
                        tracing::info!(target: "dl8::git_refs", root = %root_text, refs = snapshot.refs.len(), watcher = false, "snapshot");
                    }
                    Err(failure) => self.done.push(self.error_row(u, root_cell, &failure)),
                }
                continue;
            }
            match Self::open(&root_text) {
                Ok((refs, query, last, wake)) => {
                    self.done.extend(self.snapshot_rows(u, root_cell, &last));
                    tracing::info!(target: "dl8::git_refs", root = %root_text, refs = last.refs.len(), watcher = matches!(wake, Wake::Watcher(_)), "armed");
                    self.roots.insert(
                        root_text,
                        WatchedRoot {
                            root_cell,
                            refs,
                            query,
                            last,
                            wake,
                        },
                    );
                }
                Err(failure) => self.done.push(self.error_row(u, root_cell, &failure)),
            }
        }
    }

    /// Non-blocking: each root is checked once without waiting, so a move is
    /// seen at the answering loop's next bounded wait instead of inside poll.
    fn poll(&mut self, u: &mut Universe) -> Vec<Row> {
        let mut rows = std::mem::take(&mut self.done);
        let mut changed: Vec<(String, Result<soopy::RefSnapshot, String>)> = Vec::new();
        for (root_text, watched) in self.roots.iter_mut() {
            if watched.wake.woke(root_text, Duration::ZERO) {
                let snapshot = watched.refs.snapshot(&watched.query);
                changed.push((root_text.clone(), snapshot.map_err(message)));
            }
        }
        for (root_text, snapshot) in changed {
            let watched = &self.roots[&root_text];
            let root_cell = watched.root_cell;
            match snapshot {
                Ok(after) => {
                    let found = self.delta_rows(u, root_cell, &watched.last, &after);
                    tracing::debug!(target: "dl8::git_refs", root = %root_text, rows = found.len(), "re-read");
                    rows.extend(found);
                    if let Some(watched) = self.roots.get_mut(&root_text) {
                        watched.last = after;
                    }
                }
                Err(failure) => rows.push(self.error_row(u, root_cell, &failure)),
            }
        }
        rows
    }

    fn armed(&self) -> bool {
        !self.roots.is_empty()
    }
}
