//! Runtime selection and term transport for the shared IVM algebra.
use super::evaluate::{Closure, Trace};
use super::ir_lower::{self, Lowered};
use super::program::{Diagnostic, Program, Row};
use super::term::{TermId, Universe};
use ivm_engine::{Engine, Raw};
use ivm_ir::{self as ir, Cell};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Copy, Clone, Debug)]
pub enum Backend {
    Dd,
    Sqlite,
}

const TERM_BUDGET: usize = 8192;

fn diagnostic(u: &mut Universe, name: &str, detail: impl std::fmt::Display) -> Diagnostic {
    let name = u.atom(name);
    let detail = u.string(&format!("{}:{}: {detail}", file!(), line!()));
    Diagnostic {
        phase: "ir_eval",
        payload: u.compound("ir_engine_error", vec![name, detail]),
    }
}

fn settle<E: Engine>(
    engine: &mut E,
    host: &mut Raw<'_>,
    changes: Vec<ir::SourceChange>,
) -> Result<(), ivm_engine::EngineError> {
    engine.settle(ir::Frontier { changes }, host).map(|_| ())
}

fn run<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    host: &mut Raw<'_>,
    fx: &mut dyn FnMut(Trace),
) -> Result<Closure, ivm_engine::EngineError> {
    let mut engine = E::install(&lowered.ir, host)?;
    let mut mapped: HashMap<TermId, Cell> = HashMap::new();
    let mut pending: BTreeSet<usize> = (0..lowered.terms.len()).collect();
    // Each pass adds at least one term. The cap also guards malformed cyclic arenas.
    for _ in 0..lowered.terms.len().min(TERM_BUDGET) + 1 {
        if pending.is_empty() {
            break;
        }
        let mut ready = Vec::new();
        let mut changes = Vec::new();
        for &index in &pending {
            let spec = &lowered.terms[index];
            if spec.args.iter().all(|id| mapped.contains_key(id)) {
                let mut row = vec![spec.original.0 as i64];
                if let Some(raw) = spec.raw {
                    row.push(raw);
                } else {
                    row.extend(spec.args.iter().map(|id| mapped[id]));
                }
                changes.push(ir::SourceChange {
                    rel: spec.source,
                    row,
                    w: 1,
                });
                ready.push(index);
            }
        }
        if ready.is_empty() {
            break;
        }
        settle(&mut engine, host, changes)?;
        let mut outputs = BTreeSet::new();
        for &index in &ready {
            outputs.insert(lowered.terms[index].output);
        }
        for output in outputs {
            for (row, weight) in engine.snapshot(output, host)? {
                if weight > 0 {
                    if let (Some(&original), Some(&id)) = (row.first(), row.last()) {
                        mapped.insert(TermId(original as u32), id);
                    }
                }
            }
        }
        for index in ready {
            pending.remove(&index);
        }
    }
    if !pending.is_empty() {
        return Err(ivm_engine::EngineError::new(
            ivm_engine::Stage::Settle,
            None,
            ivm_engine::ErrorKind::Unsupported("term bootstrap budget or cycle"),
        ));
    }
    let mut changes = vec![ir::SourceChange {
        rel: lowered.unit,
        row: vec![1],
        w: 1,
    }];
    for (&original, &id) in &mapped {
        changes.push(ir::SourceChange {
            rel: lowered.term_map,
            row: vec![original.0 as i64, id],
            w: 1,
        });
    }
    for (rel, row) in &lowered.seeds {
        let args = if row.args.is_empty() {
            vec![1]
        } else {
            row.args.iter().map(|id| mapped[id]).collect()
        };
        changes.push(ir::SourceChange {
            rel: *rel,
            row: args,
            w: 1,
        });
    }
    for (rel, terms) in &lowered.internal_seeds {
        changes.push(ir::SourceChange {
            rel: *rel,
            row: terms.iter().map(|id| mapped[id]).collect(),
            w: 1,
        });
    }
    changes.sort_by(|a, b| (a.rel, &a.row).cmp(&(b.rel, &b.row)));
    changes.dedup_by(|a, b| a.rel == b.rel && a.row == b.row);
    settle(&mut engine, host, changes)?;

    let mut decoded: HashMap<Cell, TermId> =
        mapped.into_iter().map(|(term, id)| (id, term)).collect();
    let mut dictionary = BTreeMap::<Cell, (String, Vec<Cell>)>::new();
    for rel in lowered
        .ir
        .rels
        .iter()
        .filter(|rel| rel.kind == ir::RelKind::Constructor)
    {
        for (row, weight) in engine.intern_snapshot(rel.id, host)? {
            if weight > 0 {
                dictionary.insert(row[0], (rel.name.clone(), row[1..].to_vec()));
            }
        }
    }
    for _ in 0..dictionary.len().min(TERM_BUDGET) + 1 {
        let mut changed = false;
        for (&id, (functor, args)) in &dictionary {
            if decoded.contains_key(&id) {
                continue;
            }
            let term = match functor.as_str() {
                "0:int" => Some(u.int(args[0])),
                "1:bool" => Some(u.boolean(args[0] != 0)),
                name if name.starts_with("9:") => args
                    .iter()
                    .map(|arg| decoded.get(arg).copied())
                    .collect::<Option<Vec<_>>>()
                    .map(|items| u.compound(name.splitn(3, ':').nth(2).unwrap(), items)),
                _ => None,
            };
            if let Some(term) = term {
                decoded.insert(id, term);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut rows = Vec::new();
    for &(rel, original, arity) in &lowered.visible {
        for (cells, weight) in engine.snapshot(rel, host)? {
            if weight <= 0 {
                continue;
            }
            let cells = &cells[..arity];
            let args = cells
                .iter()
                .map(|cell| decoded.get(cell).copied())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    ivm_engine::EngineError::new(
                        ivm_engine::Stage::Snapshot,
                        Some(rel),
                        ivm_engine::ErrorKind::Unsupported("undecodable term id"),
                    )
                })?;
            rows.push(Row {
                rel: original,
                args,
            });
        }
    }
    rows.sort_by(|a, b| {
        u.cmp(a.rel, b.rel)
            .then_with(|| u.cmp_rows(&a.args, &b.args))
    });
    rows.dedup();
    fx(Trace::Closure { rows: rows.len() });
    Ok(Closure {
        rows,
        diagnostics: Vec::new(),
    })
}

pub fn evaluate_ir(
    u: &mut Universe,
    program: &Program,
    backend: Backend,
    fx: &mut dyn FnMut(Trace),
) -> Closure {
    let lowered = match ir_lower::lower(u, program) {
        Ok(lowered) => lowered,
        Err(diag) => {
            return Closure {
                rows: vec![],
                diagnostics: vec![diag],
            }
        }
    };
    let result = match backend {
        Backend::Dd => {
            let mut host = Raw::default();
            run::<ivm_dd::Dd>(u, &lowered, &mut host, fx)
        }
        Backend::Sqlite => match rusqlite::Connection::open_in_memory() {
            Ok(connection) => {
                let mut host = Raw::with_connection(&connection);
                run::<ivm_sqlite::Sqlite>(u, &lowered, &mut host, fx)
            }
            Err(error) => {
                return Closure {
                    rows: vec![],
                    diagnostics: vec![diagnostic(u, "sqlite_open", error)],
                }
            }
        },
    };
    match result {
        Ok(closure) => closure,
        Err(error) => Closure {
            rows: vec![],
            diagnostics: vec![diagnostic(u, "engine", error)],
        },
    }
}
