//! Runtime selection and term transport for the shared IVM algebra.
use super::evaluate::Store;
use super::evaluate::{Closure, Trace};
use super::ir_lower::{self, Lowered};
use super::program::{Diagnostic, Program, Row};
use super::term::{Term, TermId, Universe};
use crate::_9_runtime::Answerer;
use ivm_engine::Engine;
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
        fingerprint: u64,
        measured: bool,
    ) -> Result<Self, ivm_engine::EngineError> {
        let census_start = std::time::Instant::now();
        let engine = if measured {
            E::install(&lowered.ir)?
        } else {
            E::install_unmeasured(&lowered.ir)?
        };
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
}

type RunResult =
    Result<(Closure, Vec<FrontierResult>, ivm_engine::Counters), ivm_engine::EngineError>;


trait Session {
    fn matches(&self, lowered: &Lowered, fingerprint: u64) -> bool;
    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        u: &mut Universe,
        lowered: &Lowered,
        answerer: Option<&mut Answerer>,
        frontiers: &[Vec<SignedRow>],
        public_names: Option<&HashMap<TermId, String>>,
        live: Option<&mut LiveRun<'_>>,
        fx: &mut dyn FnMut(Trace),
    ) -> RunResult;
}

impl<E: Engine> Session for EngineSession<E> {
    fn matches(&self, lowered: &Lowered, fingerprint: u64) -> bool {
        self.fingerprint == fingerprint && self.structure == lowered.ir
    }

    fn run(
        &mut self,
        u: &mut Universe,
        lowered: &Lowered,
        answerer: Option<&mut Answerer>,
        frontiers: &[Vec<SignedRow>],
        public_names: Option<&HashMap<TermId, String>>,
        live: Option<&mut LiveRun<'_>>,
        fx: &mut dyn FnMut(Trace),
    ) -> RunResult {
        run_session(
            u,
            lowered,
            self,
            answerer,
            frontiers,
            public_names,
            live,
            fx,
        )
    }
}

fn install_session<E: Engine + 'static>(
    u: &mut Universe,
    lowered: &Lowered,
    fingerprint: u64,
    measured: bool,
) -> Result<Box<dyn Session>, Diagnostic> {
    let session = EngineSession::<E>::install(lowered, fingerprint, measured)
        .map_err(|error| diagnostic(u, "engine", error))?;
    Ok(Box::new(session))
}

/// The one place a backend type is named. `measured`: the caller reads per-operator
/// counters (`take_counters` after `dl8 eval`).
fn open_session(
    u: &mut Universe,
    backend: Backend,
    lowered: &Lowered,
    fingerprint: u64,
    measured: bool,
) -> Result<Box<dyn Session>, Diagnostic> {
    match backend {
        Backend::Dd => install_session::<ivm_dd::Dd>(u, lowered, fingerprint, measured),
        Backend::Sqlite => install_session::<ivm_sqlite::Sqlite>(u, lowered, fingerprint, measured),
    }
}

pub struct IrSession {
    backend: Result<Backend, String>,
    session: Option<Box<dyn Session>>,
}

impl IrSession {
    pub fn new(backend: Backend) -> Self {
        Self {
            backend: Ok(backend),
            session: None,
        }
    }

    /// The backend `DL8_ENGINE` names; an unknown name surfaces as a
    /// diagnostic on the first evaluation.
    pub fn configured() -> Self {
        Self {
            backend: configured_backend(),
            session: None,
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
        let backend = match &self.backend {
            Ok(backend) => *backend,
            Err(message) => {
                return Closure {
                    rows: vec![],
                    diagnostics: vec![unknown_engine(u, message)],
                }
            }
        };
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
        if self
            .session
            .as_ref()
            .is_none_or(|session| !session.matches(&lowered, fingerprint))
        {
            self.session = match open_session(u, backend, &lowered, fingerprint, false) {
                Ok(session) => Some(session),
                Err(diag) => {
                    return Closure {
                        rows: vec![],
                        diagnostics: vec![diag],
                    }
                }
            };
        }
        let result = self
            .session
            .as_mut()
            .unwrap()
            .run(u, &lowered, answerer, &[], None, None, fx);
        match result {
            Ok((closure, _, counters)) => {
                LAST_COUNTERS.with(|last| last.set(Some(counters)));
                closure
            }
            Err(error) => {
                self.session = None;
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
    mapped: &HashMap<TermId, Cell>,
) -> Result<Closure, ivm_engine::EngineError> {
    let mut decoded = decode_terms(u, lowered, engine, mapped)?;
    let mut rows = Vec::new();
    for &(rel, original, arity) in &lowered.visible {
        for (cells, weight) in engine.snapshot(rel)? {
            if weight <= 0 {
                continue;
            }
            let mut args = Vec::with_capacity(arity);
            for &cell in &cells[..arity] {
                let term = if let Some(&term) = decoded.get(&cell) {
                    term
                } else if let Some(value) = engine.text(cell)? {
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
            for (cells, weight) in engine.snapshot(spec.ranked)? {
                if weight <= 0 {
                    continue;
                }
                let subject_cell = cells[spec.key_count];
                let subject = if let Some(&term) = decoded.get(&subject_cell) {
                    term
                } else if let Some(value) = engine.text(subject_cell)? {
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
            let values: Vec<Vec<TermId>> = groups
                .into_values()
                .map(|mut values| {
                    values.sort_by_key(|(rank, _)| *rank);
                    values.into_iter().map(|(_, value)| value).collect()
                })
                .collect();
            let groups: Vec<&[TermId]> = values.iter().map(Vec::as_slice).collect();
            // One row store for every group of the fold.
            for result in
                super::evaluate::fold_generic_row_groups(u, program, &rows, &spec.fold, &groups)
            {
                if let Err(diagnostic) = result {
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

/// `DL8_ENGINE`: unset is `ir:sqlite`.
pub fn configured_backend() -> Result<Backend, String> {
    match std::env::var("DL8_ENGINE").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("ir:sqlite") => Ok(Backend::Sqlite),
        Ok("ir:dd") => Ok(Backend::Dd),
        Ok(other) => Err(format!(
            "unknown DL8_ENGINE {other:?}; expected ir:dd or ir:sqlite"
        )),
        Err(error) => Err(format!("DL8_ENGINE {error}; expected ir:dd or ir:sqlite")),
    }
}

pub fn unknown_engine(u: &mut Universe, message: &str) -> Diagnostic {
    let detail = u.string(message);
    Diagnostic {
        phase: "ir_eval",
        payload: u.compound("unknown_engine", vec![detail]),
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
    changes: Vec<ir::SourceChange>,
) -> Result<ir::Delta, ivm_engine::EngineError> {
    engine.settle(ir::Frontier { changes })
}

fn decode_terms<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    engine: &E,
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
        for (row, weight) in engine.intern_snapshot(rel.id)? {
            if weight > 0 {
                dictionary.insert(row[0], (rel.name.clone(), row[1..].to_vec()));
            }
        }
    }
    for (&id, (_, args)) in dictionary.iter().filter(|(_, (name, _))| name == "2:str") {
        if !decoded.contains_key(&id) {
            if let Some(value) = engine.text(args[0])? {
                decoded.insert(id, u.string(&value));
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
                "0:float" => Some(u.float(f64::from_bits(args[0] as u64))),
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
    mapped: &mut HashMap<TermId, Cell>,
    roots: &[TermId],
) -> Result<(), ivm_engine::EngineError> {
    let mut pending = BTreeSet::new();
    let mut text_cells = HashMap::new();
    let mut todo = roots.to_vec();
    while let Some(term) = todo.pop() {
        if mapped.contains_key(&term) || !pending.insert(term.0) {
            continue;
        }
        match u.get(term) {
            Term::Str(sym) => {
                text_cells.insert(term, engine.intern_text(u.sym_str(*sym))?);
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
        let mut terms = Vec::with_capacity(ready.len());
        for term in &ready {
            let value = u.get(*term);
            let (shape, raw, args): ((String, usize, bool), Option<i64>, &[TermId]) = match value {
                Term::Int(n) => (("0:int".into(), 0, true), Some(*n), &[]),
                Term::Float(f) => (("0:float".into(), 0, true), Some(f.0.to_bits() as i64), &[]),
                Term::Str(_) => (("2:str".into(), 1, false), None, &[]),
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
            };
            let Some(&functor) = lowered.constructors.get(&shape) else {
                return Err(ivm_engine::EngineError::new(
                    ivm_engine::Stage::Settle,
                    None,
                    ivm_engine::ErrorKind::Unsupported("dynamic term constructor"),
                ));
            };
            let row = if let Some(raw) = raw {
                vec![raw]
            } else if let Some(&cell) = text_cells.get(term) {
                vec![cell]
            } else {
                args.iter().map(|arg| mapped[arg]).collect()
            };
            terms.push((functor, row));
        }
        let cells = engine.intern_terms(&terms)?;
        for (term, cell) in ready.into_iter().zip(cells) {
            mapped.insert(term, cell);
            pending.remove(&term.0);
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
    let mut text_cells = Vec::with_capacity(lowered.texts.len());
    for (_, value) in &lowered.texts {
        text_cells.push(engine.intern_text(value)?);
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
        let mut terms = Vec::new();
        for &index in &pending {
            let spec = &lowered.terms[index];
            if spec.args.iter().all(|id| mapped.contains_key(id)) {
                let row = if let Some(raw) = spec.raw {
                    vec![raw]
                } else if let Some(text) = spec.text {
                    vec![text_cells[text]]
                } else {
                    spec.args.iter().map(|id| mapped[id]).collect()
                };
                terms.push((spec.functor, row));
                ready.push(index);
            }
        }
        if ready.is_empty() {
            break;
        }
        let cells = engine.intern_terms(&terms)?;
        for (index, cell) in ready.into_iter().zip(cells) {
            mapped.insert(lowered.terms[index].original, cell);
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
    let mut delta = settle(engine, frontier)?;
    let request_relations: HashMap<_, _> = lowered
        .served
        .iter()
        .map(|served| (served.request, served.relation))
        .collect();
    let mut request_count = 0;
    while answerer.is_some() {
        let decoded = decode_terms(u, lowered, engine, mapped)?;
        let mut store = Store::default();
        for &(rel, original, arity) in &lowered.visible {
            for (row, weight) in engine.snapshot(rel)? {
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
        install_terms(u, lowered, engine, mapped, &answer_terms)?;
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
        delta = settle(engine, next)?;
    }
    let mut frontier_results = Vec::with_capacity(frontiers.len());
    for frontier in frontiers {
        let terms: Vec<_> = frontier
            .iter()
            .flat_map(|change| change.row.args.iter().copied())
            .collect();
        install_terms(u, lowered, engine, mapped, &terms)?;
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
        let mut delta = settle(engine, source_changes)?;
        let mut receipts = Vec::new();
        let mut stable = false;
        for _ in 0..TERM_BUDGET {
            let decoded = decode_terms(u, lowered, engine, mapped)?;
            let store_closure = snapshot_closure(u, lowered, engine, mapped)?;
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
            install_terms(u, lowered, engine, mapped, &answer_terms)?;
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
            delta = settle(engine, next)?;
        }
        if !stable {
            return Err(ivm_engine::EngineError::new(
                ivm_engine::Stage::Settle,
                None,
                ivm_engine::ErrorKind::Unsupported("effect frontier budget"),
            ));
        }
        frontier_results.push(FrontierResult {
            closure: snapshot_closure(u, lowered, engine, mapped)?,
            deltas: receipts,
            counters: engine.counters(),
        });
    }

    if let Some(live) = live.as_deref_mut() {
        let closure = snapshot_closure(u, lowered, engine, mapped)?;
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
            install_terms(u, lowered, engine, mapped, &terms)?;
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

            let mut delta = settle(engine, source_changes)?;
            let mut stable = false;
            for _ in 0..TERM_BUDGET {
                let decoded = decode_terms(u, lowered, engine, mapped)?;
                let store_closure = snapshot_closure(u, lowered, engine, mapped)?;
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
                    .flat_map(|(_, _, rows)| {
                        rows.iter().flat_map(|answer| answer.args.iter().copied())
                    })
                    .collect();
                install_terms(u, lowered, engine, mapped, &answer_terms)?;
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
                delta = settle(engine, next)?;
            }
            if !stable {
                return Err(ivm_engine::EngineError::new(
                    ivm_engine::Stage::Settle,
                    None,
                    ivm_engine::ErrorKind::Unsupported("live effect frontier budget"),
                ));
            }
            let closure = snapshot_closure(u, lowered, engine, mapped)?;
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

    let closure = snapshot_closure(u, lowered, engine, mapped)?;
    fx(Trace::Closure {
        rows: closure.rows.len(),
    });
    Ok((closure, frontier_results, counters))
}

/// One fresh install of `program` on `backend`, run to its closure.
#[allow(clippy::too_many_arguments)]
fn run_fresh(
    u: &mut Universe,
    program: &Program,
    backend: Backend,
    answerer: Option<&mut Answerer>,
    frontiers: &[Vec<SignedRow>],
    public_names: Option<&HashMap<TermId, String>>,
    live: Option<&mut LiveRun<'_>>,
    fx: &mut dyn FnMut(Trace),
) -> Result<(Closure, Vec<FrontierResult>), Diagnostic> {
    let lowered = ir_lower::lower(u, program)?;
    let fingerprint = structural_fingerprint(&lowered.ir);
    let mut session = open_session(u, backend, &lowered, fingerprint, true)?;
    match session.run(u, &lowered, answerer, frontiers, public_names, live, fx) {
        Ok((closure, results, counters)) => {
            LAST_COUNTERS.with(|last| last.set(Some(counters)));
            Ok((closure, results))
        }
        Err(error) => Err(diagnostic(u, "engine", error)),
    }
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
    run_fresh(
        u,
        program,
        backend,
        Some(answerer),
        frontiers,
        Some(public_names),
        None,
        fx,
    )
}

#[allow(clippy::too_many_arguments)]
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
    let mut live = LiveRun {
        max_settles,
        poll,
        on_settle,
    };
    run_fresh(
        u,
        program,
        backend,
        Some(answerer),
        &[],
        Some(public_names),
        Some(&mut live),
        fx,
    )
    .map(|(closure, _)| closure)
}

fn evaluate_ir_with_answerer(
    u: &mut Universe,
    program: &Program,
    backend: Backend,
    answerer: Option<&mut Answerer>,
    fx: &mut dyn FnMut(Trace),
) -> Closure {
    LAST_COUNTERS.with(|counters| counters.set(None));
    LAST_ANSWERS.with(|rows| rows.borrow_mut().clear());
    LAST_REQUESTS.with(|count| count.set(0));
    match run_fresh(u, program, backend, answerer, &[], None, None, fx) {
        Ok((closure, _)) => closure,
        Err(diagnostic) => Closure {
            rows: vec![],
            diagnostics: vec![diagnostic],
        },
    }
}
