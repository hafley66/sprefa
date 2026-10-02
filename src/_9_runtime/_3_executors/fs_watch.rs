//! `(fs.watch ?Path ?Body)`: the current UTF-8 contents of a watched file.
//! A request watches the parent directory so replace-on-save editors are seen.

use super::text_at;
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::ir_eval::SignedRow;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IExecutor};
use notify_debouncer_full::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::Duration;

pub const RELATION: &str = "fs.watch";
const DEBOUNCE: Duration = Duration::from_millis(100);

type Watcher = Debouncer<RecommendedWatcher, RecommendedCache>;

struct WatchedFile {
    path_cell: TermId,
    body: Option<Arc<str>>,
    body_cell: Option<TermId>,
    applications: HashSet<TermId>,
}

pub struct FsWatch {
    relation: TermId,
    watcher: Option<Watcher>,
    events: Option<Receiver<DebounceEventResult>>,
    files: HashMap<PathBuf, WatchedFile>,
    application_paths: HashMap<TermId, PathBuf>,
    parent_uses: HashMap<PathBuf, usize>,
}

impl FsWatch {
    pub fn new(relation: TermId) -> FsWatch {
        FsWatch {
            relation,
            watcher: None,
            events: None,
            files: HashMap::new(),
            application_paths: HashMap::new(),
            parent_uses: HashMap::new(),
        }
    }

    fn ensure_watcher(&mut self) -> Result<(), String> {
        if self.watcher.is_some() {
            return Ok(());
        }
        let (send, receive) = channel();
        let watcher = new_debouncer(DEBOUNCE, None, send).map_err(|error| error.to_string())?;
        self.watcher = Some(watcher);
        self.events = Some(receive);
        Ok(())
    }

    fn target(path: &str) -> Result<(PathBuf, PathBuf), String> {
        let path = Path::new(path);
        let file = path
            .file_name()
            .ok_or_else(|| format!("{path:?} has no file name"))?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let parent = std::fs::canonicalize(parent)
            .map_err(|error| format!("{}: {error}", parent.display()))?;
        Ok((parent.join(file), parent))
    }

    fn body(path: &Path) -> Option<Arc<str>> {
        std::fs::read_to_string(path).ok().map(Arc::from)
    }

    fn row(&self, path_cell: TermId, body_cell: TermId) -> Row {
        Row {
            rel: self.relation,
            args: vec![path_cell, body_cell],
        }
    }

    fn changed_paths(events: Vec<notify_debouncer_full::DebouncedEvent>) -> HashSet<PathBuf> {
        events
            .into_iter()
            .flat_map(|event| event.paths.iter().cloned().collect::<Vec<_>>())
            .collect()
    }

    fn receive_changed(&self, timeout: Duration) -> HashSet<PathBuf> {
        let Some(events) = &self.events else {
            return HashSet::new();
        };
        let result = if timeout.is_zero() {
            events.try_recv().ok()
        } else {
            match events.recv_timeout(timeout) {
                Ok(result) => Some(result),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => None,
            }
        };
        match result {
            Some(Ok(batch)) => {
                let paths = Self::changed_paths(batch);
                tracing::info!(target: "dl8::fs_watch", paths = ?paths, "debounced");
                paths
            }
            Some(Err(errors)) => {
                for error in errors {
                    tracing::warn!(target: "dl8::fs_watch", %error, "watch error");
                }
                HashSet::new()
            }
            None => HashSet::new(),
        }
    }
}

impl IExecutor for FsWatch {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Continuing
    }

    fn answer(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) -> Vec<Row> {
        let mut answers = Vec::new();
        for &application in pending {
            let Some((path, path_text)) = text_at(u, application, 0) else {
                continue;
            };
            let Ok((target, parent)) = Self::target(&path_text) else {
                tracing::warn!(target: "dl8::fs_watch", path = %path_text, "cannot resolve watch path");
                continue;
            };
            if let Some(watched) = self.files.get_mut(&target) {
                watched.applications.insert(application);
                self.application_paths.insert(application, target);
                if let Some(body_cell) = watched.body_cell {
                    answers.push(Row {
                        rel: self.relation,
                        args: vec![watched.path_cell, body_cell],
                    });
                }
                continue;
            }
            if let Err(error) = self.ensure_watcher() {
                tracing::warn!(target: "dl8::fs_watch", path = %path_text, %error, "cannot start watcher");
                continue;
            }
            if !self.parent_uses.contains_key(&parent) {
                let result = self
                    .watcher
                    .as_mut()
                    .expect("watcher was initialized")
                    .watch(&parent, RecursiveMode::Recursive);
                if let Err(error) = result {
                    tracing::warn!(target: "dl8::fs_watch", path = %path_text, %error, "cannot watch parent");
                    continue;
                }
            }
            *self.parent_uses.entry(parent).or_insert(0) += 1;
            let path_cell = u.compound("const", vec![path]);
            let body = Self::body(&target);
            let body_cell = body.as_ref().map(|body| {
                let body = u.blob_string(body.clone());
                u.compound("const", vec![body])
            });
            let mut applications = HashSet::new();
            applications.insert(application);
            self.files.insert(
                target.clone(),
                WatchedFile {
                    path_cell,
                    body,
                    body_cell,
                    applications,
                },
            );
            self.application_paths.insert(application, target);
            tracing::info!(target: "dl8::fs_watch", path = %path_text, "armed");
            if let Some(body_cell) = body_cell {
                answers.push(self.row(path_cell, body_cell));
            }
        }
        answers
    }

    fn poll(&mut self, _u: &mut Universe, _timeout: Duration) -> Vec<Row> {
        Vec::new()
    }

    fn poll_signed(&mut self, u: &mut Universe, timeout: Duration) -> Vec<SignedRow> {
        let changed = self.receive_changed(timeout);
        if !changed.is_empty() {
            tracing::info!(target: "dl8::fs_watch", paths = ?changed, watched = ?self.files.keys().collect::<Vec<_>>(), "checking paths");
        }
        let targets: Vec<PathBuf> = self
            .files
            .keys()
            .filter(|target| changed.contains(*target))
            .cloned()
            .collect();
        let relation = self.relation;
        let mut rows = Vec::new();
        for target in targets {
            let after = Self::body(&target);
            let watched = self.files.get_mut(&target).expect("target came from files");
            if after == watched.body {
                continue;
            }
            if let Some(old_cell) = watched.body_cell {
                rows.push(SignedRow {
                    row: Row {
                        rel: relation,
                        args: vec![watched.path_cell, old_cell],
                    },
                    weight: -1,
                });
            }
            watched.body = after;
            watched.body_cell = watched.body.as_ref().map(|body| {
                let body = u.blob_string(body.clone());
                u.compound("const", vec![body])
            });
            if let Some(new_cell) = watched.body_cell {
                rows.push(SignedRow {
                    row: Row {
                        rel: relation,
                        args: vec![watched.path_cell, new_cell],
                    },
                    weight: 1,
                });
            }
        }
        rows
    }

    fn cancel(&mut self, _u: &Universe, application: TermId) {
        let Some(target) = self.application_paths.remove(&application) else {
            return;
        };
        let Some(watched) = self.files.get_mut(&target) else {
            return;
        };
        watched.applications.remove(&application);
        if !watched.applications.is_empty() {
            return;
        }
        self.files.remove(&target);
        let Some(parent) = target.parent().map(Path::to_path_buf) else {
            return;
        };
        let Some(uses) = self.parent_uses.get_mut(&parent) else {
            return;
        };
        *uses -= 1;
        if *uses == 0 {
            self.parent_uses.remove(&parent);
            if let Some(watcher) = &mut self.watcher {
                if let Err(error) = watcher.unwatch(&parent) {
                    tracing::warn!(target: "dl8::fs_watch", path = %parent.display(), %error, "cannot stop watcher");
                }
            }
        }
    }

    fn armed(&self) -> bool {
        !self.files.is_empty()
    }
}
