//! Runtime selection and term transport for the shared IVM algebra.
use super::evaluate::Store;
use super::evaluate::{Closure, Trace};
use super::ir_lower::{self, Lowered};
use super::program::{Diagnostic, Program, Row};
use super::term::{Term, TermId, Universe};
use crate::_9_runtime::Answerer;
use ivm_engine::{Engine, Raw};
use ivm_ir::{self as ir, Cell};
use std::cell::{Cell as ThreadCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap};

thread_local! {
    static LAST_COUNTERS: ThreadCell<Option<ivm_engine::Counters>> = const { ThreadCell::new(None) };
    static LAST_ANSWERS: RefCell<Vec<Row>> = const { RefCell::new(Vec::new()) };
    static LAST_REQUESTS: ThreadCell<usize> = const { ThreadCell::new(0) };
}

pub fn take_counters() -> Option<ivm_engine::Counters> {
    LAST_COUNTERS.with(|counters| counters.take())
}

pub fn take_answers() -> (usize, Vec<Row>) {
    let requests = LAST_REQUESTS.with(|count| count.replace(0));
    let rows = LAST_ANSWERS.with(|rows| std::mem::take(&mut *rows.borrow_mut()));
    (requests, rows)
}

#[derive(Copy, Clone, Debug)]
pub enum Backend {
    Dd,
    Sqlite,
}

pub fn configured_backend() -> Option<Backend> {
    match std::env::var("DL8_ENGINE").as_deref() {
        Ok("ir:dd") => Some(Backend::Dd),
        Ok("ir:sqlite") => Some(Backend::Sqlite),
        _ => None,
    }
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
) -> Result<ir::Delta, ivm_engine::EngineError> {
    engine.settle(ir::Frontier { changes }, host)
}

fn decode_terms<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    engine: &E,
    host: &mut Raw<'_>,
    mapped: &HashMap<TermId, Cell>,
) -> Result<HashMap<Cell, TermId>, ivm_engine::EngineError> {
    let mut decoded: HashMap<Cell, TermId> = mapped.iter().map(|(&term, &id)| (id, term)).collect();
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
    for (_, args) in dictionary
        .values()
        .filter(|(name, _)| name.starts_with("9:"))
    {
        for &id in args {
            if !decoded.contains_key(&id) {
                if let Some(value) = engine.text(id, host)? {
                    decoded.insert(id, u.string(&value));
                }
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
                "3:atom" => None,
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
    Ok(decoded)
}

fn install_terms<E: Engine>(
    u: &Universe,
    lowered: &Lowered,
    engine: &mut E,
    host: &mut Raw<'_>,
    mapped: &mut HashMap<TermId, Cell>,
    roots: &[TermId],
) -> Result<(), ivm_engine::EngineError> {
    let mut pending = BTreeSet::new();
    let mut todo = roots.to_vec();
    while let Some(term) = todo.pop() {
        if mapped.contains_key(&term) || !pending.insert(term.0) {
            continue;
        }
        match u.get(term) {
            Term::Str(sym) => {
                let cell = engine.intern_text(u.sym_str(*sym), host)?;
                mapped.insert(term, cell);
                pending.remove(&term.0);
            }
            Term::Compound(_, args) => todo.extend(args.iter().copied()),
            _ => {}
        }
    }
    for _ in 0..pending.len().min(TERM_BUDGET) + 1 {
        if pending.is_empty() {
            return Ok(());
        }
        let ready: Vec<_> = pending
            .iter()
            .copied()
            .map(TermId)
            .filter(|term| match u.get(*term) {
                Term::Compound(_, args) => args.iter().all(|arg| mapped.contains_key(arg)),
                _ => true,
            })
            .collect();
        if ready.is_empty() {
            break;
        }
        let mut changes = Vec::new();
        let mut outputs = BTreeSet::new();
        for term in &ready {
            let value = u.get(*term);
            let (shape, raw, args): ((String, usize, bool), Option<i64>, &[TermId]) = match value {
                Term::Int(n) => (("0:int".into(), 0, true), Some(*n), &[]),
                Term::Bool(v) => (("1:bool".into(), 0, true), Some(i64::from(*v)), &[]),
                Term::Atom(_) => (
                    ("3:atom".into(), 0, true),
                    Some(
                        lowered
                            .atom_raws
                            .get(term)
                            .copied()
                            .unwrap_or(lowered.dynamic_atom_base + i64::from(term.0)),
                    ),
                    &[],
                ),
                Term::Compound(sym, args) => (
                    (
                        format!("9:{}:{}", args.len(), u.sym_str(*sym)),
                        args.len(),
                        false,
                    ),
                    None,
                    args,
                ),
                Term::Float(_) | Term::Str(_) => continue,
            };
            let Some(&(source, output)) = lowered.installers.get(&shape) else {
                return Err(ivm_engine::EngineError::new(
                    ivm_engine::Stage::Settle,
                    None,
                    ivm_engine::ErrorKind::Unsupported("dynamic term constructor"),
                ));
            };
            let mut row = vec![term.0 as i64];
            if let Some(raw) = raw {
                row.push(raw);
            } else {
                row.extend(args.iter().map(|arg| mapped[arg]));
            }
            changes.push(ir::SourceChange {
                rel: source,
                row,
                w: 1,
            });
            outputs.insert(output);
        }
        settle(engine, host, changes)?;
        for output in outputs {
            for (row, weight) in engine.snapshot(output, host)? {
                if weight > 0 {
                    if let (Some(&original), Some(&cell)) = (row.first(), row.last()) {
                        let term = TermId(original as u32);
                        if pending.contains(&term.0) {
                            mapped.insert(term, cell);
                        }
                    }
                }
            }
        }
        for term in ready {
            if mapped.contains_key(&term) {
                pending.remove(&term.0);
            }
        }
    }
    if pending.is_empty() {
        Ok(())
    } else {
        Err(ivm_engine::EngineError::new(
            ivm_engine::Stage::Settle,
            None,
            ivm_engine::ErrorKind::Unsupported("dynamic term install budget or cycle"),
        ))
    }
}

fn run<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    host: &mut Raw<'_>,
    mut answerer: Option<&mut Answerer>,
    fx: &mut dyn FnMut(Trace),
) -> Result<(Closure, ivm_engine::Counters), ivm_engine::EngineError> {
    let mut engine = E::install(&lowered.ir, host)?;
    let mut mapped: HashMap<TermId, Cell> = HashMap::new();
    for (term, value) in &lowered.texts {
        mapped.insert(*term, engine.intern_text(value, host)?);
    }
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
    let mut delta = settle(&mut engine, host, changes)?;
    let request_relations: HashMap<_, _> = lowered
        .served
        .iter()
        .map(|served| (served.request, served.relation))
        .collect();
    let mut answered = HashMap::<(TermId, TermId), Vec<(ir::RelId, Vec<Cell>)>>::new();
    let mut request_count = 0;
    while answerer.is_some() {
        let decoded = decode_terms(u, lowered, &engine, host, &mapped)?;
        let mut store = Store::default();
        for &(rel, original, arity) in &lowered.visible {
            for (row, weight) in engine.snapshot(rel, host)? {
                if weight <= 0 {
                    continue;
                }
                let args = row[..arity]
                    .iter()
                    .filter_map(|cell| decoded.get(cell).copied())
                    .collect::<Vec<_>>();
                if args.len() == arity {
                    store.insert(original, args.into_boxed_slice());
                }
            }
        }
        let mut next = Vec::new();
        let request_changes: Vec<_> = delta
            .changes
            .iter()
            .filter_map(|(rel, row, weight)| {
                request_relations
                    .get(rel)
                    .map(|relation| (*relation, row.clone(), *weight))
            })
            .collect();
        for (relation, row, weight) in request_changes {
            let Some(&application) = row.last().and_then(|cell| decoded.get(cell)) else {
                continue;
            };
            let key = (relation, application);
            if weight < 0 {
                answerer
                    .as_deref_mut()
                    .unwrap()
                    .cancel_request(relation, application);
                if let Some(rows) = answered.remove(&key) {
                    next.extend(rows.into_iter().map(|(rel, row)| ir::SourceChange {
                        rel,
                        row,
                        w: -1,
                    }));
                }
                continue;
            }
            if weight == 0 {
                continue;
            }
            let (rows, asked) = answerer
                .as_deref_mut()
                .unwrap()
                .answer_requests(u, &store, [key]);
            request_count += asked;
            let answer_terms: Vec<_> = rows
                .iter()
                .flat_map(|answer| answer.args.iter().copied())
                .collect();
            install_terms(u, lowered, &mut engine, host, &mut mapped, &answer_terms)?;
            let mut recorded = Vec::new();
            for answer in rows {
                LAST_ANSWERS.with(|rows| rows.borrow_mut().push(answer.clone()));
                let Some(&source) = lowered.sources.get(&(answer.rel.0, answer.args.len())) else {
                    continue;
                };
                let mut cells = Vec::with_capacity(answer.args.len().max(1));
                if answer.args.is_empty() {
                    cells.push(1);
                } else {
                    for term in answer.args {
                        cells.push(mapped[&term]);
                    }
                }
                next.push(ir::SourceChange {
                    rel: source,
                    row: cells.clone(),
                    w: 1,
                });
                recorded.push((source, cells));
            }
            answered.insert(key, recorded);
        }
        if next.is_empty() {
            break;
        }
        delta = settle(&mut engine, host, next)?;
    }
    LAST_REQUESTS.with(|count| count.set(request_count));
    let counters = engine.counters();

    let mut decoded = decode_terms(u, lowered, &engine, host, &mapped)?;
    let mut rows = Vec::new();
    for &(rel, original, arity) in &lowered.visible {
        for (cells, weight) in engine.snapshot(rel, host)? {
            if weight <= 0 {
                continue;
            }
            let cells = &cells[..arity];
            let mut args = Vec::with_capacity(cells.len());
            for &cell in cells {
                let term = if let Some(&term) = decoded.get(&cell) {
                    term
                } else if let Some(value) = engine.text(cell, host)? {
                    let term = u.string(&value);
                    decoded.insert(cell, term);
                    term
                } else {
                    return Err(ivm_engine::EngineError::new(
                        ivm_engine::Stage::Snapshot,
                        Some(rel),
                        ivm_engine::ErrorKind::Unsupported("undecodable term id"),
                    ));
                };
                args.push(term);
            }
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
    Ok((
        Closure {
            rows,
            diagnostics: Vec::new(),
        },
        counters,
    ))
}

pub fn evaluate_ir(
    u: &mut Universe,
    program: &Program,
    backend: Backend,
    fx: &mut dyn FnMut(Trace),
) -> Closure {
    evaluate_ir_with_answerer(u, program, backend, None, fx)
}

pub fn evaluate_ir_answered(
    u: &mut Universe,
    program: &Program,
    backend: Backend,
    answerer: &mut Answerer,
    fx: &mut dyn FnMut(Trace),
) -> Closure {
    evaluate_ir_with_answerer(u, program, backend, Some(answerer), fx)
}

fn evaluate_ir_with_answerer(
    u: &mut Universe,
    program: &Program,
    backend: Backend,
    mut answerer: Option<&mut Answerer>,
    fx: &mut dyn FnMut(Trace),
) -> Closure {
    LAST_COUNTERS.with(|counters| counters.set(None));
    LAST_ANSWERS.with(|rows| rows.borrow_mut().clear());
    LAST_REQUESTS.with(|count| count.set(0));
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
            run::<ivm_dd::Dd>(u, &lowered, &mut host, answerer.as_deref_mut(), fx)
        }
        Backend::Sqlite => match rusqlite::Connection::open_in_memory() {
            Ok(connection) => {
                let mut host = Raw::with_connection(&connection);
                run::<ivm_sqlite::Sqlite>(u, &lowered, &mut host, answerer.as_deref_mut(), fx)
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
        Ok((closure, counters)) => {
            LAST_COUNTERS.with(|last| last.set(Some(counters)));
            closure
        }
        Err(error) => Closure {
            rows: vec![],
            diagnostics: vec![diagnostic(u, "engine", error)],
        },
    }
}
