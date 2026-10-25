//! The `ryi` service: request `(root text)`, answer `(root text, caller text, callee text)`, the
//! resolved call edges of `ryi fast .` run inside `root`.

use crate::_0_id::Dict;
use crate::_1_effects::{ryi_call_edges, CallEdge};
use crate::_3_plan::{Row, Value};
use crate::_5_ends::{Cadence, Change, Diag, IService};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// One `ryi fast` run at most; protects the Core thread, which waits on it.
pub const RYI_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

pub struct Ryi {
    /// The ryi binary, `_1_effects::v9_ryi` at the binary edge.
    pub binary: String,
    /// Edges per asked root, read by `call`, interned by `poll`.
    pub pending: Vec<(Value, Vec<CallEdge>)>,
    /// Rows last answered per root, seeded from the stored `ryi_edge` rows at open. A re-ask
    /// answers only the difference, so the engine's answer input stays a set.
    pub answered: HashMap<Value, BTreeSet<Row>>,
    /// `ryi` failures since the runtime last drained them; a failed root answers nothing.
    pub errors: Vec<String>,
}

impl Ryi {
    pub fn new(binary: String) -> Ryi {
        Ryi {
            binary,
            pending: Vec::new(),
            answered: HashMap::new(),
            errors: Vec::new(),
        }
    }
}

impl IService for Ryi {
    fn service(&self) -> &str {
        "ryi"
    }

    fn cadence(&self) -> Cadence {
        Cadence::Once
    }

    /// Runs ryi for every asserted root, in the caller's thread. A retracted root needs no call:
    /// the engine hides its answers.
    fn call(&mut self, requests: &[Change], dict: &Dict) {
        for request in requests.iter().filter(|request| request.diff > 0) {
            let root_id = request.row[0];
            let root = String::from_utf8_lossy(&dict.bytes[&root_id]).into_owned();
            match ryi_call_edges(&self.binary, Path::new(&root), RYI_TIMEOUT) {
                Ok(edges) => self.pending.push((root_id, edges)),
                Err(Diag::Service { message, .. }) => {
                    self.errors.push(format!("{root}: {message}"))
                }
                Err(diag) => self.errors.push(format!("{diag:?}")),
            }
        }
    }

    /// `rel` of each change is 0; the runtime sets the Answer rel.
    fn poll(&mut self, dict: &mut Dict, out: &mut Vec<Change>) {
        for (root, edges) in std::mem::take(&mut self.pending) {
            let rows: BTreeSet<Row> = edges
                .iter()
                .map(|edge| {
                    vec![
                        root,
                        dict.intern(edge.caller.as_bytes()),
                        dict.intern(edge.callee.as_bytes()),
                    ]
                })
                .collect();
            let before = self.answered.remove(&root).unwrap_or_default();
            for (rows_from, rows_minus, diff) in [(&rows, &before, 1), (&before, &rows, -1)] {
                out.extend(rows_from.difference(rows_minus).map(|row| Change {
                    rel: 0,
                    row: row.clone(),
                    diff,
                }));
            }
            self.answered.insert(root, rows);
        }
    }

    fn seed(&mut self, answers: &[Change]) {
        self.pending.clear();
        self.answered.clear();
        self.errors.clear();
        for answer in answers {
            self.answered
                .entry(answer.row[0])
                .or_default()
                .insert(answer.row.clone());
        }
    }

    fn errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.errors)
    }
}
