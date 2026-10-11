//! `(ryi.resolve ?Root ?Family)`: one `ryi --resolve` run per root+family; each
//! record a row on its `@std/ryi` product, or one error row for the whole run.

use super::extract::{arms, extract_bin, files_under, run, RUN_BUDGET};
use super::ryi_schema::RECORDS;
use super::{handle, text_at, wake};
use crate::_6_eval::evaluate::Store;
use crate::_6_eval::{Row, TermId, Universe};
use crate::_9_runtime::reconcile::{Cadence, IService};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{channel, Receiver, Sender};

pub const RELATION: &str = "ryi.resolve";
pub const ERROR: &str = "ryi.resolve_error";

/// The namespace every record product carries in `names`.
pub const NAMESPACE: &str = "ryi.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Int,
    Bool,
    Str,
    Json,
}

/// One product column: its key path into the JSONL record and its type.
pub struct Column {
    pub name: &'static str,
    pub path: &'static [&'static str],
    pub kind: Kind,
    pub optional: bool,
}

pub struct RyiResolve {
    relation: TermId,
    error: TermId,
    none: TermId,
    /// Record name to its product, for every record the program's names hold.
    products: HashMap<&'static str, (TermId, &'static [Column])>,
    done: Receiver<Finished>,
    sender: Sender<Finished>,
    tasks: usize,
}

struct Finished {
    root: String,
    family: String,
    outcome: Result<Vec<(String, String)>, String>,
}

impl RyiResolve {
    pub fn new(
        relation: TermId,
        error: TermId,
        none: TermId,
        names: &HashMap<String, TermId>,
    ) -> RyiResolve {
        let products = RECORDS
            .iter()
            .filter_map(|(record, columns)| {
                let product = names.get(&format!("{NAMESPACE}{record}"))?;
                Some((*record, (*product, *columns)))
            })
            .collect();
        let (sender, done) = channel();
        RyiResolve {
            relation,
            error,
            none,
            products,
            done,
            sender,
            tasks: 0,
        }
    }

    fn cell(&self, u: &mut Universe, column: &Column, record: &Value) -> Result<TermId, String> {
        let value = column
            .path
            .iter()
            .try_fold(record, |value, key| value.get(key));
        let value = match value {
            None | Some(Value::Null) if column.optional => return Ok(self.none),
            None | Some(Value::Null) => return Err(format!("column {} is null", column.name)),
            Some(value) => value,
        };
        let term = match (column.kind, value) {
            (Kind::Int, Value::Number(number)) => match number.as_i64() {
                Some(int) => u.int(int),
                None => return Err(format!("column {} is not an i64", column.name)),
            },
            (Kind::Bool, Value::Bool(boolean)) => u.boolean(*boolean),
            (Kind::Str, Value::String(text)) => u.string(text),
            (Kind::Json, value) => u.string(&value.to_string()),
            _ => return Err(format!("column {} is not {:?}", column.name, column.kind)),
        };
        Ok(u.compound("const", vec![term]))
    }

    fn record_row(
        &self,
        u: &mut Universe,
        root: &str,
        kind: &str,
        line: &str,
    ) -> Result<Option<Row>, String> {
        let Some((product, columns)) = self.products.get(kind) else {
            return Ok(None);
        };
        let record: Value =
            serde_json::from_str(line).map_err(|e| format!("record {kind}: {e}"))?;
        let root = u.string(root);
        let mut args = vec![u.compound("const", vec![root])];
        for column in columns.iter() {
            args.push(
                self.cell(u, column, &record)
                    .map_err(|e| format!("record {kind}: {e}"))?,
            );
        }
        Ok(Some(Row {
            rel: *product,
            args,
        }))
    }

    fn pair_row(&self, u: &mut Universe, rel: TermId, cells: &[&str]) -> Row {
        let args = cells
            .iter()
            .map(|cell| {
                let text = u.string(cell);
                u.compound("const", vec![text])
            })
            .collect();
        Row { rel, args }
    }
}

impl IService for RyiResolve {
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
            let decoded = finished.outcome.and_then(|records| {
                let mut decoded = Vec::with_capacity(records.len());
                for (kind, line) in &records {
                    decoded.extend(self.record_row(u, &finished.root, kind, line)?);
                }
                Ok(decoded)
            });
            match decoded {
                Ok(decoded) => {
                    rows.extend(decoded);
                    rows.push(self.pair_row(u, self.relation, &[&finished.root, &finished.family]))
                }
                Err(failure) => {
                    tracing::warn!(target: "dl8::ryi_resolve", root = %finished.root, family = %finished.family, "{failure}");
                    rows.push(self.pair_row(
                        u,
                        self.error,
                        &[&finished.root, &finished.family, &failure],
                    ));
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
