//! `(fs.at ?Root ?Sha ?Path ?Blob)`: one row per tracked file at the revision,
//! with the git blob sha. `?Sha` is any revision git names: a sha, a branch, `HEAD`.

use super::{message, text_at};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IService};

pub const RELATION: &str = "fs.at";
pub const ERROR: &str = "fs.at_error";

pub struct FsAt {
    relation: TermId,
    error: TermId,
    /// Files answered inside `call`, drained by `poll`.
    done: Vec<Row>,
}

impl FsAt {
    pub fn new(relation: TermId, error: TermId) -> FsAt {
        FsAt {
            relation,
            error,
            done: Vec::new(),
        }
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

impl IService for FsAt {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn call(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) {
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
                        self.done.push(Row {
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
                    self.done.push(Row {
                        rel: self.error,
                        args: vec![root_cell, sha_cell, u.compound("const", vec![message])],
                    });
                }
            }
        }
    }

    fn poll(&mut self, _u: &mut Universe) -> Vec<Row> {
        std::mem::take(&mut self.done)
    }

    fn armed(&self) -> bool {
        false
    }
}
