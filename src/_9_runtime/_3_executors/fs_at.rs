//! `(fs.at ?Root ?Sha ?Path ?Blob)`: one row per tracked file at the revision,
//! with the git blob sha. `?Sha` is any revision git names: a sha, a branch, `HEAD`.

use super::{message, text_at};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IExecutor};
use std::time::Duration;

pub const RELATION: &str = "fs.at";
pub const ERROR: &str = "fs.at_error";

pub struct FsAt {
    relation: TermId,
    error: TermId,
}

impl FsAt {
    pub fn new(relation: TermId, error: TermId) -> FsAt {
        FsAt { relation, error }
    }
}

/// `(path, blob sha)` per tracked file.
fn files_at(root: &str, revision: &str) -> Result<Vec<(String, String)>, String> {
    let repository = soopy::discover(root).map_err(message)?;
    let entries = soopy::SourceTree::open(repository)
        .git_files(&soopy::GitFilesQuery {
            revision: soopy::Revision::Named(revision.into()),
            pathspecs: Vec::new(),
        })
        .map_err(message)?;
    entries
        .into_iter()
        .map(|entry| match entry.content {
            soopy::ContentId::GitBlob(blob) => {
                Ok((entry.source.path.0.to_string(), blob.0.to_string()))
            }
            soopy::ContentId::Blake3(_) => {
                Err(format!("{} carries no git blob", entry.source.path.0))
            }
        })
        .collect()
}

impl IExecutor for FsAt {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn answer(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) -> Vec<Row> {
        let mut rows = Vec::new();
        for &application in pending {
            let (Some((root, root_text)), Some((sha, sha_text))) =
                (text_at(u, application, 0), text_at(u, application, 1))
            else {
                continue;
            };
            let root_cell = u.compound("const", vec![root]);
            let sha_cell = u.compound("const", vec![sha]);
            match files_at(&root_text, &sha_text) {
                Ok(files) => {
                    tracing::info!(target: "dl8::fs_at", root = %root_text, sha = %sha_text, files = files.len());
                    for (path, blob) in files {
                        let path = u.string(&path);
                        let blob = u.string(&blob);
                        rows.push(Row {
                            rel: self.relation,
                            args: vec![
                                root_cell,
                                sha_cell,
                                u.compound("const", vec![path]),
                                u.compound("const", vec![blob]),
                            ],
                        });
                    }
                }
                Err(failure) => {
                    let message = u.string(&failure);
                    rows.push(Row {
                        rel: self.error,
                        args: vec![root_cell, sha_cell, u.compound("const", vec![message])],
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
