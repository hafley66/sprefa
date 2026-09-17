//! `(git.history ?Root ?Sha ?Parent)`: one row per parent edge reachable from
//! `?Sha`, or `HEAD` when unbound. A root commit has no parent and so no row.

use super::{message, text_at};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IExecutor};
use std::time::Duration;

pub const RELATION: &str = "git.history";
pub const ERROR: &str = "git.history_error";

pub struct SoopyHistory {
    relation: TermId,
    error: TermId,
}

impl SoopyHistory {
    pub fn new(relation: TermId, error: TermId) -> SoopyHistory {
        SoopyHistory { relation, error }
    }
}

/// Every commit reachable from `start` with its parents: one walk, then one
/// batched parent read over the walked commits.
fn walk(root: &str, start: &str) -> Result<Vec<soopy::CommitParents>, String> {
    let repository = soopy::discover(root).map_err(message)?;
    let graph = soopy::RevisionGraph::open(repository.clone());
    let query =
        |walks: Vec<soopy::Revision>, parents: Vec<soopy::ObjectId>| soopy::RevisionGraphQuery {
            repository: repository.identity.clone(),
            resolve: Vec::new(),
            parents,
            ancestry: Vec::new(),
            merge_bases: Vec::new(),
            ahead_behind: Vec::new(),
            walks,
        };
    let walked = graph
        .query(&query(
            vec![soopy::Revision::Named(start.into())],
            Vec::new(),
        ))
        .map_err(message)?;
    let commits = walked
        .walks
        .into_iter()
        .flat_map(|walk| walk.commits)
        .collect();
    let answer = graph.query(&query(Vec::new(), commits)).map_err(message)?;
    Ok(answer.parents)
}

impl IExecutor for SoopyHistory {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn answer(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) -> Vec<Row> {
        let mut rows = Vec::new();
        for &application in pending {
            let Some((root, root_text)) = text_at(u, application, 0) else {
                continue;
            };
            let start =
                text_at(u, application, 1).map_or_else(|| "HEAD".to_string(), |(_, sha)| sha);
            let root_cell = u.compound("const", vec![root]);
            match walk(&root_text, &start) {
                Ok(commits) => {
                    tracing::info!(target: "dl8::git_history", root = %root_text, start = %start, commits = commits.len());
                    for commit in commits {
                        let sha = u.string(&commit.commit.0);
                        let sha_cell = u.compound("const", vec![sha]);
                        for parent in commit.parents {
                            let parent = u.string(&parent.0);
                            rows.push(Row {
                                rel: self.relation,
                                args: vec![root_cell, sha_cell, u.compound("const", vec![parent])],
                            });
                        }
                    }
                }
                Err(failure) => {
                    let message = u.string(&failure);
                    rows.push(Row {
                        rel: self.error,
                        args: vec![root_cell, u.compound("const", vec![message])],
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
