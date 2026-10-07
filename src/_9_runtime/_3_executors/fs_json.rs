//! `(fs.json ?Path ?Root)`: one file as `:` rows under one root node. An object
//! is a level of edges, an array a list, a scalar an `intern`, null `none`.

use super::text_at;
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::json::{rows_from_json, Json};
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IService};

pub const RELATION: &str = "fs.json";
pub const ERROR: &str = "fs.json_error";

/// The prelude's zero-column product, the target of a JSON null.
pub const NONE: &str = "none";

/// The 10-second law: a larger document is an error row, never a walk that
/// holds the tick. Depth is bounded by serde_json's own 128-level limit.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

pub struct FsJson {
    relation: TermId,
    error: TermId,
    colon: TermId,
    none: TermId,
    /// Documents read inside `call`, drained by `poll`.
    done: Vec<Row>,
}

impl FsJson {
    pub fn new(relation: TermId, error: TermId, colon: TermId, none: TermId) -> FsJson {
        FsJson {
            relation,
            error,
            colon,
            none,
            done: Vec::new(),
        }
    }
}

fn read(path: &str) -> Result<Json, String> {
    let size = std::fs::metadata(path)
        .map_err(|e| format!("{path}: {e}"))?
        .len();
    if size > MAX_BYTES {
        return Err(format!("{path} is {size} bytes, over the {MAX_BYTES} cap"));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))
}

impl IService for FsJson {
    fn relation(&self) -> &str {
        RELATION
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    fn call(&mut self, u: &mut Universe, _rows: &Store, pending: &[TermId]) {
        for &application in pending {
            let Some((path, path_text)) = text_at(u, application, 0) else {
                continue;
            };
            let path_cell = u.compound("const", vec![path]);
            match read(&path_text) {
                Ok(document) => {
                    let constructor = u.unary(self.relation, "ref").unwrap_or(self.relation);
                    let arguments = u.list(&[path]);
                    let identity = u.compound("application", vec![constructor, arguments]);
                    let (root, members) =
                        rows_from_json(u, self.colon, self.none, identity, &document);
                    tracing::info!(target: "dl8::fs_json", path = %path_text, edges = members.len());
                    self.done.push(Row {
                        rel: self.relation,
                        args: vec![path_cell, root],
                    });
                    self.done.extend(members);
                }
                Err(failure) => {
                    let failure = u.string(&failure);
                    self.done.push(Row {
                        rel: self.error,
                        args: vec![path_cell, u.compound("const", vec![failure])],
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
