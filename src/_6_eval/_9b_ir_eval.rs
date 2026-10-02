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
use std::hash::{Hash, Hasher};
use std::time::Duration;

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

pub struct SignedRow {
    pub row: Row,
    pub weight: i64,
}

pub struct NamedDelta {
    pub relation: String,
    pub args: Vec<TermId>,
    pub weight: i64,
}

pub struct FrontierResult {
    pub closure: Closure,
    pub deltas: Vec<NamedDelta>,
    pub counters: ivm_engine::Counters,
}

struct EngineSession<E: Engine> {
    engine: E,
    mapped: HashMap<TermId, Cell>,
    answered: HashMap<(TermId, TermId), Vec<(ir::RelId, Vec<Cell>, Row)>>,
    last_seeds: BTreeSet<(ir::RelId, Vec<Cell>)>,
    initialized: bool,
    fingerprint: u64,
    structure: ir::Program,
}

impl<E: Engine> EngineSession<E> {
    fn install(
        lowered: &Lowered,
        host: &mut Raw<'_>,
        fingerprint: u64,
    ) -> Result<Self, ivm_engine::EngineError> {
        let census_start = std::time::Instant::now();
        let engine = E::install(&lowered.ir, host)?;
        tracing::info!(
            target: "dl8::ir",
            install_ns = census_start.elapsed().as_nanos(),
            engine = std::any::type_name::<E>(),
            "ir install"
        );
        Ok(Self {
            engine,
            mapped: HashMap::new(),
            answered: HashMap::new(),
            last_seeds: BTreeSet::new(),
            initialized: false,
            fingerprint,
            structure: lowered.ir.clone(),
        })
    }

    fn matches(&self, lowered: &Lowered, fingerprint: u64) -> bool {
        self.fingerprint == fingerprint && self.structure == lowered.ir
    }
}

pub struct IrSession {
    backend: Backend,
    dd: Option<EngineSession<ivm_dd::Dd>>,
    sqlite: Option<(rusqlite::Connection, EngineSession<ivm_sqlite::Sqlite>)>,
}

impl IrSession {
    pub fn new(backend: Backend) -> Self {
        Self {
            backend,
            dd: None,
            sqlite: None,
        }
    }

    pub fn evaluate(
        &mut self,
        u: &mut Universe,
        program: &Program,
        answerer: Option<&mut Answerer>,
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
        let fingerprint = structural_fingerprint(&lowered.ir);
        let result = match self.backend {
            Backend::Dd => {
                let mut host = Raw::default();
                if self
                    .dd
                    .as_ref()
                    .is_none_or(|session| !session.matches(&lowered, fingerprint))
                {
                    self.dd = match EngineSession::install(&lowered, &mut host, fingerprint) {
                        Ok(session) => Some(session),
                        Err(error) => {
                            return Closure {
                                rows: vec![],
                                diagnostics: vec![diagnostic(u, "engine", error)],
                            }
                        }
                    };
                }
                run_session(
                    u,
                    &lowered,
                    &mut host,
                    self.dd.as_mut().unwrap(),
                    answerer,
                    &[],
                    None,
                    None,
                    fx,
                )
            }
            Backend::Sqlite => {
                if self
                    .sqlite
                    .as_ref()
                    .is_none_or(|(_, session)| !session.matches(&lowered, fingerprint))
                {
                    let connection = match rusqlite::Connection::open_in_memory() {
                        Ok(connection) => connection,
                        Err(error) => {
                            return Closure {
                                rows: vec![],
                                diagnostics: vec![diagnostic(u, "sqlite_open", error)],
                            }
                        }
                    };
                    let mut host = Raw::with_connection(&connection);
                    let session = match EngineSession::install(&lowered, &mut host, fingerprint) {
                        Ok(session) => session,
                        Err(error) => {
                            return Closure {
                                rows: vec![],
                                diagnostics: vec![diagnostic(u, "engine", error)],
                            }
                        }
                    };
                    if let Ok(creates) = sqlite_create_count(&connection) {
                        tracing::info!(target: "dl8::ir", sqlite_create_count = creates, "ir schema");
                    }
                    self.sqlite = Some((connection, session));
                }
                let (connection, session) = self.sqlite.as_mut().unwrap();
                let mut host = Raw::with_connection(connection);
                run_session(
                    u,
                    &lowered,
                    &mut host,
                    session,
                    answerer,
                    &[],
                    None,
                    None,
                    fx,
                )
            }
        };
        match result {
            Ok((closure, _, counters)) => {
                LAST_COUNTERS.with(|last| last.set(Some(counters)));
                closure
            }
            Err(error) => {
                self.dd = None;
                self.sqlite = None;
                Closure {
                    rows: vec![],
                    diagnostics: vec![diagnostic(u, "engine", error)],
                }
            }
        }
    }
}

fn structural_fingerprint(program: &ir::Program) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // IR has structural equality but no Hash implementation. Hash its canonical
    // serialized structure, then confirm equality when the hash matches.
    serde_json::to_vec(program)
        .expect("IR serializes")
        .hash(&mut hasher);
    hasher.finish()
}

struct LiveRun<'a> {
    max_settles: usize,
    poll: &'a mut dyn FnMut(&mut Answerer, &mut Universe, Duration) -> Option<Vec<SignedRow>>,
    on_settle: &'a mut dyn FnMut(&Universe, &Closure, &[NamedDelta], &ivm_engine::Counters),
}

fn snapshot_closure<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    engine: &E,
    host: &mut Raw<'_>,
    mapped: &HashMap<TermId, Cell>,
) -> Result<Closure, ivm_engine::EngineError> {
    let mut decoded = decode_terms(u, lowered, engine, host, mapped)?;
    let mut rows = Vec::new();
    for &(rel, original, arity) in &lowered.visible {
        for (cells, weight) in engine.snapshot(rel, host)? {
            if weight <= 0 {
                continue;
            }
            let mut args = Vec::with_capacity(arity);
            for &cell in &cells[..arity] {
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
    let mut diagnostics = Vec::new();
    if let Some(program) = &lowered.fold_program {
        for spec in &lowered.declared_folds {
            let mut groups: BTreeMap<Vec<Cell>, Vec<(Cell, TermId)>> = BTreeMap::new();
            for (cells, weight) in engine.snapshot(spec.ranked, host)? {
                if weight <= 0 {
                    continue;
                }
                let subject_cell = cells[spec.key_count];
                let subject = if let Some(&term) = decoded.get(&subject_cell) {
                    term
                } else if let Some(value) = engine.text(subject_cell, host)? {
                    let term = u.string(&value);
                    decoded.insert(subject_cell, term);
                    term
                } else {
                    return Err(ivm_engine::EngineError::new(
                        ivm_engine::Stage::Snapshot,
                        Some(spec.ranked),
                        ivm_engine::ErrorKind::Unsupported("undecodable fold subject"),
                    ));
                };
                groups
                    .entry(cells[..spec.key_count].to_vec())
                    .or_default()
                    .push((cells[spec.key_count + 1], subject));
            }
            for values in groups.values_mut() {
                values.sort_by_key(|(rank, _)| *rank);
                let values: Vec<_> = values.iter().map(|(_, value)| *value).collect();
                if let Err(diagnostic) =
                    super::evaluate::fold_generic_rows(u, program, &rows, &spec.fold, &values)
                {
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
    diagnostics.sort_by_key(|diagnostic| (diagnostic.phase, diagnostic.payload.0));
    diagnostics.dedup();
    if !diagnostics.is_empty() {
        rows.clear();
    }
    Ok(Closure { rows, diagnostics })
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

fn run_session<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    host: &mut Raw<'_>,
    session: &mut EngineSession<E>,
    mut answerer: Option<&mut Answerer>,
    frontiers: &[Vec<SignedRow>],
    public_names: Option<&HashMap<TermId, String>>,
    mut live: Option<&mut LiveRun<'_>>,
    fx: &mut dyn FnMut(Trace),
) -> Result<(Closure, Vec<FrontierResult>, ivm_engine::Counters), ivm_engine::EngineError> {
    let engine = &mut session.engine;
    let mapped = &mut session.mapped;
    let answered = &mut session.answered;
    for (term, value) in &lowered.texts {
        mapped.insert(*term, engine.intern_text(value, host)?);
    }
    let mut pending: BTreeSet<usize> = (0..lowered.terms.len())
        .filter(|&index| !mapped.contains_key(&lowered.terms[index].original))
        .collect();
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
        settle(engine, host, changes)?;
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
    for (&original, &id) in mapped.iter() {
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
    let current_seeds: BTreeSet<_> = changes
        .iter()
        .map(|change| (change.rel, change.row.clone()))
        .collect();
    let frontier = if session.initialized {
        let mut diff = Vec::new();
        for (rel, row) in session.last_seeds.difference(&current_seeds) {
            diff.push(ir::SourceChange {
                rel: *rel,
                row: row.clone(),
                w: -1,
            });
        }
        for (rel, row) in current_seeds.difference(&session.last_seeds) {
            diff.push(ir::SourceChange {
                rel: *rel,
                row: row.clone(),
                w: 1,
            });
        }
        diff
    } else {
        changes
    };
    session.last_seeds = current_seeds;
    session.initialized = true;
    let mut delta = settle(engine, host, frontier)?;
    let request_relations: HashMap<_, _> = lowered
        .served
        .iter()
        .map(|served| (served.request, served.relation))
        .collect();
    let mut request_count = 0;
    while answerer.is_some() {
        let decoded = decode_terms(u, lowered, engine, host, mapped)?;
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
        let mut answers = Vec::new();
        for (relation, row, weight) in request_changes {
            let Some(&application) = row.last().and_then(|cell| decoded.get(cell)) else {
                continue;
            };
            let key = (relation, application);
            if weight < 0 {
                answerer
                    .as_deref_mut()
                    .unwrap()
                    .cancel_request(u, relation, application);
                if let Some(rows) = answered.remove(&key) {
                    next.extend(rows.into_iter().map(|(rel, row, _)| ir::SourceChange {
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
            answers.push((key, relation, rows));
        }
        let answer_terms: Vec<_> = answers
            .iter()
            .flat_map(|(_, _, rows)| rows.iter().flat_map(|answer| answer.args.iter().copied()))
            .collect();
        install_terms(u, lowered, engine, host, mapped, &answer_terms)?;
        for (key, _, rows) in answers {
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
                    for term in &answer.args {
                        cells.push(mapped[term]);
                    }
                }
                next.push(ir::SourceChange {
                    rel: source,
                    row: cells.clone(),
                    w: 1,
                });
                recorded.push((source, cells, answer));
            }
            answered.insert(key, recorded);
        }
        if next.is_empty() {
            break;
        }
        delta = settle(engine, host, next)?;
    }
    let mut frontier_results = Vec::with_capacity(frontiers.len());
    for frontier in frontiers {
        let terms: Vec<_> = frontier
            .iter()
            .flat_map(|change| change.row.args.iter().copied())
            .collect();
        install_terms(u, lowered, engine, host, mapped, &terms)?;
        let mut source_changes = Vec::with_capacity(frontier.len());
        for change in frontier {
            let Some(&source) = lowered
                .sources
                .get(&(change.row.rel.0, change.row.args.len()))
            else {
                return Err(ivm_engine::EngineError::new(
                    ivm_engine::Stage::Settle,
                    None,
                    ivm_engine::ErrorKind::Unsupported("frontier relation is not a source"),
                ));
            };
            source_changes.push(ir::SourceChange {
                rel: source,
                row: change.row.args.iter().map(|term| mapped[term]).collect(),
                w: change.weight,
            });
        }
        let mut delta = settle(engine, host, source_changes)?;
        let mut receipts = Vec::new();
        let mut stable = false;
        for _ in 0..TERM_BUDGET {
            let decoded = decode_terms(u, lowered, engine, host, mapped)?;
            let store_closure = snapshot_closure(u, lowered, engine, host, mapped)?;
            let mut store = Store::default();
            for row in &store_closure.rows {
                store.insert(row.rel, row.args.clone().into_boxed_slice());
            }
            let request_changes: Vec<_> = delta
                .changes
                .iter()
                .filter_map(|(rel, row, weight)| {
                    request_relations
                        .get(rel)
                        .map(|relation| (*relation, row.clone(), *weight))
                })
                .collect();
            let mut next = Vec::new();
            let mut answers = Vec::new();
            for (relation, row, weight) in request_changes {
                let Some(&application) = row.last().and_then(|cell| decoded.get(cell)) else {
                    continue;
                };
                receipts.push(NamedDelta {
                    relation: format!(
                        "{}.request",
                        public_names
                            .and_then(|names| names.get(&relation))
                            .cloned()
                            .unwrap_or_else(|| u.display(relation).to_string())
                    ),
                    args: vec![application],
                    weight,
                });
                let key = (relation, application);
                if weight < 0 {
                    answerer
                        .as_deref_mut()
                        .unwrap()
                        .cancel_request(u, relation, application);
                    if let Some(rows) = answered.remove(&key) {
                        for (source, cells, answer) in rows {
                            receipts.push(NamedDelta {
                                relation: format!(
                                    "{}.answer",
                                    public_names
                                        .and_then(|names| names.get(&relation))
                                        .cloned()
                                        .unwrap_or_else(|| u.display(relation).to_string())
                                ),
                                args: answer.args,
                                weight: -1,
                            });
                            next.push(ir::SourceChange {
                                rel: source,
                                row: cells,
                                w: -1,
                            });
                        }
                    }
                    continue;
                }
                let (rows, asked) =
                    answerer
                        .as_deref_mut()
                        .unwrap()
                        .answer_requests(u, &store, [key]);
                request_count += asked;
                answers.push((key, relation, rows));
            }
            let answer_terms: Vec<_> = answers
                .iter()
                .flat_map(|(_, _, rows)| rows.iter().flat_map(|answer| answer.args.iter().copied()))
                .collect();
            install_terms(u, lowered, engine, host, mapped, &answer_terms)?;
            for (key, relation, rows) in answers {
                let mut recorded = Vec::new();
                for answer in rows {
                    let Some(&source) = lowered.sources.get(&(answer.rel.0, answer.args.len()))
                    else {
                        continue;
                    };
                    let cells: Vec<_> = answer.args.iter().map(|term| mapped[term]).collect();
                    receipts.push(NamedDelta {
                        relation: format!(
                            "{}.answer",
                            public_names
                                .and_then(|names| names.get(&relation))
                                .cloned()
                                .unwrap_or_else(|| u.display(relation).to_string())
                        ),
                        args: answer.args.clone(),
                        weight: 1,
                    });
                    next.push(ir::SourceChange {
                        rel: source,
                        row: cells.clone(),
                        w: 1,
                    });
                    recorded.push((source, cells, answer));
                }
                answered.insert(key, recorded);
            }
            if next.is_empty() {
                stable = true;
                break;
            }
            delta = settle(engine, host, next)?;
        }
        if !stable {
            return Err(ivm_engine::EngineError::new(
                ivm_engine::Stage::Settle,
                None,
                ivm_engine::ErrorKind::Unsupported("effect frontier budget"),
            ));
        }
        frontier_results.push(FrontierResult {
            closure: snapshot_closure(u, lowered, engine, host, mapped)?,
            deltas: receipts,
            counters: engine.counters(),
        });
    }

    if let Some(live) = live.as_deref_mut() {
        let closure = snapshot_closure(u, lowered, engine, host, mapped)?;
        let counters = engine.counters();
        (live.on_settle)(u, &closure, &[], &counters);

        let mut event_settles = 0;
        while event_settles < live.max_settles {
            let Some(frontier) = (live.poll)(
                answerer.as_deref_mut().unwrap(),
                u,
                Duration::from_millis(50),
            ) else {
                break;
            };
            if frontier.is_empty() {
                continue;
            }
            event_settles += 1;

            let terms: Vec<_> = frontier
                .iter()
                .flat_map(|change| change.row.args.iter().copied())
                .collect();
            install_terms(u, lowered, engine, host, mapped, &terms)?;
            let mut source_changes = Vec::with_capacity(frontier.len());
            let mut receipts = Vec::new();
            for change in frontier {
                let Some(&source) = lowered
                    .sources
                    .get(&(change.row.rel.0, change.row.args.len()))
                else {
                    return Err(ivm_engine::EngineError::new(
                        ivm_engine::Stage::Settle,
                        None,
                        ivm_engine::ErrorKind::Unsupported("live relation is not a source"),
                    ));
                };
                let cells: Vec<_> = change.row.args.iter().map(|term| mapped[term]).collect();
                if let Some(served) = lowered.served.iter().find(|served| {
                    served.relation == change.row.rel && served.arity == change.row.args.len()
                }) {
                    if let Some(application) = answerer
                        .as_deref()
                        .unwrap()
                        .matching_requests(u, &change.row)
                        .into_iter()
                        .next()
                    {
                        let key = (served.relation, application);
                        let owned = answered.entry(key).or_default();
                        if change.weight > 0 {
                            owned.push((source, cells.clone(), change.row.clone()));
                        } else if change.weight < 0 {
                            if let Some(index) =
                                owned.iter().position(|(_, _, row)| row == &change.row)
                            {
                                owned.remove(index);
                            }
                            if owned.is_empty() {
                                answered.remove(&key);
                            }
                        }
                        receipts.push(NamedDelta {
                            relation: format!(
                                "{}.answer",
                                public_names
                                    .and_then(|names| names.get(&served.relation))
                                    .cloned()
                                    .unwrap_or_else(|| u.display(served.relation).to_string())
                            ),
                            args: change.row.args.clone(),
                            weight: change.weight,
                        });
                    }
                }
                source_changes.push(ir::SourceChange {
                    rel: source,
                    row: cells,
                    w: change.weight,
                });
            }

            let mut delta = settle(engine, host, source_changes)?;
            let mut stable = false;
            for _ in 0..TERM_BUDGET {
                let decoded = decode_terms(u, lowered, engine, host, mapped)?;
                let store_closure = snapshot_closure(u, lowered, engine, host, mapped)?;
                let mut store = Store::default();
                for row in &store_closure.rows {
                    store.insert(row.rel, row.args.clone().into_boxed_slice());
                }
                let request_changes: Vec<_> = delta
                    .changes
                    .iter()
                    .filter_map(|(rel, row, weight)| {
                        request_relations
                            .get(rel)
                            .map(|relation| (*relation, row.clone(), *weight))
                    })
                    .collect();
                let mut next = Vec::new();
                let mut answers = Vec::new();
                for (relation, row, weight) in request_changes {
                    let Some(&application) = row.last().and_then(|cell| decoded.get(cell)) else {
                        continue;
                    };
                    receipts.push(NamedDelta {
                        relation: format!(
                            "{}.request",
                            public_names
                                .and_then(|names| names.get(&relation))
                                .cloned()
                                .unwrap_or_else(|| u.display(relation).to_string())
                        ),
                        args: vec![application],
                        weight,
                    });
                    let key = (relation, application);
                    if weight < 0 {
                        answerer
                            .as_deref_mut()
                            .unwrap()
                            .cancel_request(u, relation, application);
                        if let Some(rows) = answered.remove(&key) {
                            for (source, cells, answer) in rows {
                                receipts.push(NamedDelta {
                                    relation: format!(
                                        "{}.answer",
                                        public_names
                                            .and_then(|names| names.get(&relation))
                                            .cloned()
                                            .unwrap_or_else(|| u.display(relation).to_string())
                                    ),
                                    args: answer.args,
                                    weight: -1,
                                });
                                next.push(ir::SourceChange {
                                    rel: source,
                                    row: cells,
                                    w: -1,
                                });
                            }
                        }
                        continue;
                    }
                    let (rows, asked) =
                        answerer
                            .as_deref_mut()
                            .unwrap()
                            .answer_requests(u, &store, [key]);
                    request_count += asked;
                    answers.push((key, relation, rows));
                }
                let answer_terms: Vec<_> = answers
                    .iter()
                    .flat_map(|(_, _, rows)| rows.iter().flat_map(|answer| answer.args.iter().copied()))
                    .collect();
                install_terms(u, lowered, engine, host, mapped, &answer_terms)?;
                for (key, relation, rows) in answers {
                    let mut recorded = Vec::new();
                    for answer in rows {
                        let Some(&source) = lowered.sources.get(&(answer.rel.0, answer.args.len()))
                        else {
                            continue;
                        };
                        let cells: Vec<_> = answer.args.iter().map(|term| mapped[term]).collect();
                        receipts.push(NamedDelta {
                            relation: format!(
                                "{}.answer",
                                public_names
                                    .and_then(|names| names.get(&relation))
                                    .cloned()
                                    .unwrap_or_else(|| u.display(relation).to_string())
                            ),
                            args: answer.args.clone(),
                            weight: 1,
                        });
                        next.push(ir::SourceChange {
                            rel: source,
                            row: cells.clone(),
                            w: 1,
                        });
                        recorded.push((source, cells, answer));
                    }
                    answered.insert(key, recorded);
                }
                if next.is_empty() {
                    stable = true;
                    break;
                }
                delta = settle(engine, host, next)?;
            }
            if !stable {
                return Err(ivm_engine::EngineError::new(
                    ivm_engine::Stage::Settle,
                    None,
                    ivm_engine::ErrorKind::Unsupported("live effect frontier budget"),
                ));
            }
            let closure = snapshot_closure(u, lowered, engine, host, mapped)?;
            let counters = engine.counters();
            (live.on_settle)(u, &closure, &receipts, &counters);
        }
    }
    LAST_REQUESTS.with(|count| count.set(request_count));
    for rows in answered.values() {
        for (rel, row, _) in rows {
            session.last_seeds.insert((*rel, row.clone()));
        }
    }
    let counters = engine.counters();

    let closure = snapshot_closure(u, lowered, engine, host, mapped)?;
    fx(Trace::Closure {
        rows: closure.rows.len(),
    });
    Ok((closure, frontier_results, counters))
}

fn run<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    host: &mut Raw<'_>,
    answerer: Option<&mut Answerer>,
    frontiers: &[Vec<SignedRow>],
    public_names: Option<&HashMap<TermId, String>>,
    live: Option<&mut LiveRun<'_>>,
    fx: &mut dyn FnMut(Trace),
) -> Result<(Closure, Vec<FrontierResult>, ivm_engine::Counters), ivm_engine::EngineError> {
    let mut session =
        EngineSession::<E>::install(lowered, host, structural_fingerprint(&lowered.ir))?;
    run_session(
        u,
        lowered,
        host,
        &mut session,
        answerer,
        frontiers,
        public_names,
        live,
        fx,
    )
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

pub fn evaluate_ir_frontiers(
    u: &mut Universe,
    program: &Program,
    backend: Backend,
    answerer: &mut Answerer,
    frontiers: &[Vec<SignedRow>],
    public_names: &HashMap<TermId, String>,
    fx: &mut dyn FnMut(Trace),
) -> Result<(Closure, Vec<FrontierResult>), Diagnostic> {
    let lowered = ir_lower::lower(u, program)?;
    let result = match backend {
        Backend::Dd => {
            let mut host = Raw::default();
            run::<ivm_dd::Dd>(
                u,
                &lowered,
                &mut host,
                Some(answerer),
                frontiers,
                Some(public_names),
                None,
                fx,
            )
        }
        Backend::Sqlite => {
            let connection = rusqlite::Connection::open_in_memory()
                .map_err(|error| diagnostic(u, "sqlite_open", error))?;
            let mut host = Raw::with_connection(&connection);
            run::<ivm_sqlite::Sqlite>(
                u,
                &lowered,
                &mut host,
                Some(answerer),
                frontiers,
                Some(public_names),
                None,
                fx,
            )
        }
    };
    match result {
        Ok((closure, results, counters)) => {
            LAST_COUNTERS.with(|last| last.set(Some(counters)));
            Ok((closure, results))
        }
        Err(error) => Err(diagnostic(u, "engine", error)),
    }
}

pub fn evaluate_ir_live(
    u: &mut Universe,
    program: &Program,
    backend: Backend,
    answerer: &mut Answerer,
    public_names: &HashMap<TermId, String>,
    max_settles: usize,
    poll: &mut dyn FnMut(&mut Answerer, &mut Universe, Duration) -> Option<Vec<SignedRow>>,
    on_settle: &mut dyn FnMut(&Universe, &Closure, &[NamedDelta], &ivm_engine::Counters),
    fx: &mut dyn FnMut(Trace),
) -> Result<Closure, Diagnostic> {
    LAST_COUNTERS.with(|counters| counters.set(None));
    LAST_ANSWERS.with(|rows| rows.borrow_mut().clear());
    LAST_REQUESTS.with(|count| count.set(0));
    let lowered = ir_lower::lower(u, program)?;
    let mut live = LiveRun {
        max_settles,
        poll,
        on_settle,
    };
    let result = match backend {
        Backend::Dd => {
            let mut host = Raw::default();
            run::<ivm_dd::Dd>(
                u,
                &lowered,
                &mut host,
                Some(answerer),
                &[],
                Some(public_names),
                Some(&mut live),
                fx,
            )
        }
        Backend::Sqlite => {
            let connection = rusqlite::Connection::open_in_memory()
                .map_err(|error| diagnostic(u, "sqlite_open", error))?;
            let mut host = Raw::with_connection(&connection);
            run::<ivm_sqlite::Sqlite>(
                u,
                &lowered,
                &mut host,
                Some(answerer),
                &[],
                Some(public_names),
                Some(&mut live),
                fx,
            )
        }
    };
    match result {
        Ok((closure, _, counters)) => {
            LAST_COUNTERS.with(|last| last.set(Some(counters)));
            Ok(closure)
        }
        Err(error) => Err(diagnostic(u, "engine", error)),
    }
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
            run::<ivm_dd::Dd>(
                u,
                &lowered,
                &mut host,
                answerer.as_deref_mut(),
                &[],
                None,
                None,
                fx,
            )
        }
        Backend::Sqlite => match rusqlite::Connection::open_in_memory() {
            Ok(connection) => {
                let mut host = Raw::with_connection(&connection);
                let result = run::<ivm_sqlite::Sqlite>(
                    u,
                    &lowered,
                    &mut host,
                    answerer.as_deref_mut(),
                    &[],
                    None,
                    None,
                    fx,
                );
                if let Ok(creates) = sqlite_create_count(&connection) {
                    tracing::info!(target: "dl8::ir", sqlite_create_count = creates, "ir schema");
                }
                result
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
        Ok((closure, _, counters)) => {
            LAST_COUNTERS.with(|last| last.set(Some(counters)));
            closure
        }
        Err(error) => Closure {
            rows: vec![],
            diagnostics: vec![diagnostic(u, "engine", error)],
        },
    }
}

fn sqlite_create_count(connection: &rusqlite::Connection) -> rusqlite::Result<i64> {
    connection.query_row(
        "SELECT (SELECT count(*) FROM sqlite_master WHERE sql LIKE 'CREATE %') + (SELECT count(*) FROM sqlite_temp_master WHERE sql LIKE 'CREATE %')",
        [],
        |row| row.get(0),
    )
}
