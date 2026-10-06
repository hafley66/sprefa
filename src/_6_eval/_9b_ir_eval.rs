//! Runtime selection and term transport for the shared IVM algebra.
use super::evaluate::Store;
use super::evaluate::{Closure, Trace};
use super::ir_lower::{self, Lowered};
use super::program::{Diagnostic, Program, Row};
use super::rule_interp::{self, RulesMode};
use super::term::{Term, TermId, Universe};
use crate::_9_runtime::Answerer;
use ivm_engine::Engine;
use ivm_ir::{self as ir, Cell};
use std::cell::{Cell as ThreadCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::rc::Rc;
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

#[derive(Clone)]
pub struct SignedRow {
    pub row: Row,
    pub weight: i64,
}

/// What one `prepare` or `apply` settled: the signed changes of every
/// visible relation, decoded to terms.
#[derive(Clone)]
pub struct Applied {
    pub rows: Vec<SignedRow>,
    pub diagnostics: Vec<Diagnostic>,
    /// Source rows the settle was given.
    pub sent: usize,
    /// Interned rows read to decode cells the session had not seen.
    pub intern_read: usize,
}

pub enum ApplyError {
    /// The prepared IR has no source or constructor for a changed row; the
    /// caller prepares again from its whole row set.
    Unprepared(&'static str),
    Failed(Diagnostic),
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

/// The engine and the term caches every program installed on it shares.
struct Core<E: Engine> {
    engine: E,
    mapped: HashMap<TermId, Cell>,
    /// Cells decoded so far; `apply` decodes only what this misses.
    decoded: HashMap<Cell, TermId>,
    /// Constructors declared after install (`Engine::declare_constructors`), by shape.
    dynamic: BTreeMap<(String, usize, bool), ir::RelId>,
}

struct EngineSession<E: Engine> {
    core: Rc<RefCell<Core<E>>>,
    /// A part of a composed install: its relation ids to the engine's (`ivm_ir::compose`).
    relocation: Option<Rc<BTreeMap<ir::RelId, ir::RelId>>>,
    /// Terms whose term-map rows this program holds.
    known: HashSet<TermId>,
    answered: HashMap<(TermId, TermId), Vec<(ir::RelId, Vec<Cell>, Row)>>,
    last_seeds: BTreeSet<(ir::RelId, Vec<Cell>)>,
    initialized: bool,
    fingerprint: u64,
    structure: ir::Program,
    /// The session caches at the engine's last `mark`; `rewind` restores them with the engine,
    /// since cells interned after the mark are freed and reissued.
    marked: Option<Marked>,
}

struct Marked {
    mapped: HashMap<TermId, Cell>,
    known: HashSet<TermId>,
    answered: HashMap<(TermId, TermId), Vec<(ir::RelId, Vec<Cell>, Row)>>,
    last_seeds: BTreeSet<(ir::RelId, Vec<Cell>)>,
    decoded: HashMap<Cell, TermId>,
    initialized: bool,
}

/// Joins that read no constructor relation (a constructor join is a decode, never arranged),
/// and the column count of their inputs: what the engines arrange.
fn arranged_join_inputs(program: &ir::Program) -> (usize, usize) {
    let mut memo = Vec::new();
    let mut joins = 0;
    let mut cols = 0;
    for op in &program.nodes {
        let ir::Op::Join { inputs, .. } = op else { continue };
        let constructor = inputs.iter().any(|input| {
            matches!(program.nodes.get(*input as usize), Some(ir::Op::Get(rel))
                if program.rel(*rel).is_some_and(|rel| rel.kind == ir::RelKind::Constructor))
        });
        if constructor {
            continue;
        }
        joins += 1;
        cols += inputs
            .iter()
            .map(|input| program.node_types_memo(*input, &mut memo).map_or(0, |types| types.len()))
            .sum::<usize>();
    }
    (joins, cols)
}

impl<E: Engine> EngineSession<E> {
    fn install(
        lowered: &Lowered,
        fingerprint: u64,
        measured: bool,
    ) -> Result<Self, ivm_engine::EngineError> {
        dump_ir(&lowered.ir, fingerprint);
        let census_start = std::time::Instant::now();
        let engine = if measured {
            E::install(&lowered.ir)?
        } else {
            E::install_unmeasured(&lowered.ir)?
        };
        let (joins, join_input_cols) = arranged_join_inputs(&lowered.ir);
        tracing::info!(
            target: "dl8::ir",
            install_ns = census_start.elapsed().as_nanos(),
            engine = std::any::type_name::<E>(),
            nodes = lowered.ir.nodes.len(),
            joins,
            join_input_cols,
            "ir install"
        );
        Ok(Self::on(Core::new(engine), None, lowered, fingerprint))
    }

    fn on(
        core: Rc<RefCell<Core<E>>>,
        relocation: Option<Rc<BTreeMap<ir::RelId, ir::RelId>>>,
        lowered: &Lowered,
        fingerprint: u64,
    ) -> Self {
        Self {
            core,
            relocation,
            known: HashSet::new(),
            answered: HashMap::new(),
            last_seeds: BTreeSet::new(),
            initialized: false,
            fingerprint,
            structure: lowered.ir.clone(),
            marked: None,
        }
    }

    /// `lowered` in this session's engine relation ids.
    fn local(&self, lowered: &Lowered) -> Option<Lowered> {
        self.relocation.as_ref().map(|map| relocate(lowered, map))
    }
}

impl<E: Engine> Core<E> {
    fn new(engine: E) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            engine,
            mapped: HashMap::new(),
            decoded: HashMap::new(),
            dynamic: BTreeMap::new(),
        }))
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
    fn start(
        &mut self,
        u: &mut Universe,
        lowered: &Lowered,
    ) -> Result<Applied, ivm_engine::EngineError>;
    fn apply(
        &mut self,
        u: &mut Universe,
        lowered: &Lowered,
        changes: &[SignedRow],
    ) -> Result<Applied, ApplyError>;
    /// The whole closure of `lowered`'s visible relations.
    fn snapshot(&mut self, u: &mut Universe, lowered: &Lowered) -> Result<Closure, ivm_engine::EngineError>;
    /// `lowered`'s bootstrap rows in engine relation ids; the session holds them from now on.
    fn bootstrap(&mut self, lowered: &Lowered) -> Result<Vec<ir::SourceChange>, ivm_engine::EngineError>;
    /// One settle of raw source changes on the engine.
    fn settle_raw(&mut self, changes: Vec<ir::SourceChange>) -> Result<ir::Delta, ivm_engine::EngineError>;
    /// `delta`'s rows of `lowered`'s visible relations, and its fold diagnostics.
    fn decode(&mut self, u: &mut Universe, lowered: &Lowered, delta: &ir::Delta, sent: usize) -> Result<Applied, ivm_engine::EngineError>;
    fn rewinds(&self) -> bool;
    fn mark(&mut self) -> Result<(), ivm_engine::EngineError>;
    fn rewind(&mut self) -> Result<(), ivm_engine::EngineError>;
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
        let local = self.local(lowered);
        run_session(
            u,
            local.as_ref().unwrap_or(lowered),
            self,
            answerer,
            frontiers,
            public_names,
            live,
            fx,
        )
    }

    fn start(
        &mut self,
        u: &mut Universe,
        lowered: &Lowered,
    ) -> Result<Applied, ivm_engine::EngineError> {
        let local = self.local(lowered);
        start_session(u, local.as_ref().unwrap_or(lowered), self)
    }

    fn apply(
        &mut self,
        u: &mut Universe,
        lowered: &Lowered,
        changes: &[SignedRow],
    ) -> Result<Applied, ApplyError> {
        let local = self.local(lowered);
        apply_session(u, local.as_ref().unwrap_or(lowered), self, changes)
    }

    fn snapshot(&mut self, u: &mut Universe, lowered: &Lowered) -> Result<Closure, ivm_engine::EngineError> {
        let local = self.local(lowered);
        let core = self.core.borrow();
        snapshot_closure(u, local.as_ref().unwrap_or(lowered), &core.engine, &core.mapped)
    }

    fn bootstrap(&mut self, lowered: &Lowered) -> Result<Vec<ir::SourceChange>, ivm_engine::EngineError> {
        let local = self.local(lowered);
        let changes = bootstrap_changes(local.as_ref().unwrap_or(lowered), &mut self.core.borrow_mut(), Some(&mut self.known))?;
        self.last_seeds = changes.iter().map(|change| (change.rel, change.row.clone())).collect();
        self.initialized = true;
        Ok(changes)
    }

    fn settle_raw(&mut self, changes: Vec<ir::SourceChange>) -> Result<ir::Delta, ivm_engine::EngineError> {
        let sent = changes.len();
        let delta = settle(&mut self.core.borrow_mut().engine, changes)?;
        tracing::info!(target: "dl8::ir", sent, changed = delta.changes.len(), "ir settle");
        Ok(delta)
    }

    fn decode(&mut self, u: &mut Universe, lowered: &Lowered, delta: &ir::Delta, sent: usize) -> Result<Applied, ivm_engine::EngineError> {
        let local = self.local(lowered);
        let lowered = local.as_ref().unwrap_or(lowered);
        let mut core = self.core.borrow_mut();
        let (rows, intern_read) = decode_delta(u, lowered, &mut core, delta)?;
        let diagnostics = fold_diagnostics(u, lowered, &core)?;
        Ok(Applied { rows, diagnostics, sent, intern_read })
    }

    /// A part of a composed install never rewinds: a rewind would return every part.
    fn rewinds(&self) -> bool {
        self.relocation.is_none() && self.core.borrow().engine.rewinds()
    }

    fn mark(&mut self) -> Result<(), ivm_engine::EngineError> {
        let mut core = self.core.borrow_mut();
        core.engine.mark()?;
        self.marked = Some(Marked {
            mapped: core.mapped.clone(),
            known: self.known.clone(),
            answered: self.answered.clone(),
            last_seeds: self.last_seeds.clone(),
            decoded: core.decoded.clone(),
            initialized: self.initialized,
        });
        Ok(())
    }

    fn rewind(&mut self) -> Result<(), ivm_engine::EngineError> {
        let Some(marked) = self.marked.as_ref() else {
            return Err(ivm_engine::EngineError::new(
                ivm_engine::Stage::Settle,
                None,
                ivm_engine::ErrorKind::Unsupported("rewind without mark"),
            ));
        };
        let mut core = self.core.borrow_mut();
        core.engine.rewind()?;
        core.mapped.clone_from(&marked.mapped);
        self.known.clone_from(&marked.known);
        self.answered.clone_from(&marked.answered);
        self.last_seeds.clone_from(&marked.last_seeds);
        core.decoded.clone_from(&marked.decoded);
        self.initialized = marked.initialized;
        Ok(())
    }
}

fn install_session<E: Engine + 'static>(
    u: &mut Universe,
    lowered: &Lowered,
    fingerprint: u64,
    measured: bool,
) -> Result<Box<dyn Session>, Diagnostic> {
    let session = EngineSession::<E>::install(lowered, fingerprint, measured)
        .map_err(|error| engine_diagnostic(u, error))?;
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

/// A session several `IrSession`s may hold: a part of a composed install.
type Slot = Rc<RefCell<Box<dyn Session>>>;

/// One program installed beside others (`ivm_ir::compose`), bootstrapped, waiting for an
/// `IrSession` whose `prepare` or `evaluate` lowers the same IR.
pub struct Part {
    pub name: String,
    session: Slot,
    lowered: Lowered,
    fingerprint: u64,
    applied: Applied,
}

/// Programs to install beside the next program an attached `IrSession` installs, and the parts
/// earlier installs left.
#[derive(Default)]
pub struct Pool {
    pub pending: Vec<(String, Lowered)>,
    pub parts: Vec<Part>,
}

pub type SharedPool = Rc<RefCell<Pool>>;

fn install_composed<E: Engine + 'static>(
    u: &mut Universe,
    composed: &ir::Composed,
    parts: &[(&str, &Lowered)],
) -> Result<Vec<Slot>, Diagnostic> {
    dump_ir(&composed.program, structural_fingerprint(&composed.program));
    let census_start = std::time::Instant::now();
    let engine = E::install_unmeasured(&composed.program).map_err(|error| engine_diagnostic(u, error))?;
    let (joins, join_input_cols) = arranged_join_inputs(&composed.program);
    tracing::info!(
        target: "dl8::ir",
        install_ns = census_start.elapsed().as_nanos(),
        engine = std::any::type_name::<E>(),
        nodes = composed.program.nodes.len(),
        joins,
        join_input_cols,
        parts = parts.len(),
        "ir install"
    );
    let core = Core::new(engine);
    Ok(parts
        .iter()
        .zip(&composed.rels)
        .map(|((_, lowered), map)| {
            let session = EngineSession::on(
                core.clone(),
                Some(Rc::new(map.clone())),
                lowered,
                structural_fingerprint(&lowered.ir),
            );
            Rc::new(RefCell::new(Box::new(session) as Box<dyn Session>))
        })
        .collect())
}

/// `parts` composed into one program and installed once; one session per part.
fn open_composed(
    u: &mut Universe,
    backend: Backend,
    parts: &[(&str, &Lowered)],
) -> Result<Vec<Slot>, Diagnostic> {
    let programs: Vec<(&str, &ir::Program)> = parts.iter().map(|(name, lowered)| (*name, &lowered.ir)).collect();
    let composed = ir::compose(&programs).map_err(|error| diagnostic(u, "engine", error))?;
    match backend {
        Backend::Dd => install_composed::<ivm_dd::Dd>(u, &composed, parts),
        Backend::Sqlite => install_composed::<ivm_sqlite::Sqlite>(u, &composed, parts),
    }
}

pub struct IrSession {
    backend: Result<Backend, String>,
    session: Option<Slot>,
    /// One-engine mode: installs compose with the pool's pending programs; `prepare` and
    /// `evaluate` adopt a part whose IR they lower.
    pool: Option<SharedPool>,
    /// One-engine mode: `evaluate` runs the stdlib rules on their part.
    stdlib: Option<Rc<RefCell<super::one_engine::Stdlib>>>,
    /// The rule interpreter installed for `evaluate` in `RulesMode::Interpret`.
    interp: Option<Box<dyn InterpRun>>,
    /// The program `prepare` lowered and installed; `apply` feeds it.
    prepared: Option<Lowered>,
    /// `structural_fingerprint` of the prepared IR.
    fingerprint: Option<u64>,
}

impl IrSession {
    pub fn new(backend: Backend) -> Self {
        Self {
            backend: Ok(backend),
            session: None,
            pool: None,
            stdlib: None,
            interp: None,
            prepared: None,
            fingerprint: None,
        }
    }

    /// The backend `DL8_ENGINE` names; an unknown name surfaces as a
    /// diagnostic on the first evaluation.
    pub fn configured() -> Self {
        Self {
            backend: configured_backend(),
            session: None,
            pool: None,
            stdlib: None,
            interp: None,
            prepared: None,
            fingerprint: None,
        }
    }

    pub fn with_pool(mut self, pool: SharedPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn with_stdlib(mut self, stdlib: Rc<RefCell<super::one_engine::Stdlib>>) -> Self {
        self.stdlib = Some(stdlib);
        self
    }

    /// A session for `lowered`: an adopted pool part (with its bootstrap rows), a composed install
    /// with the pool's pending programs, or a fresh install.
    fn open(
        &mut self,
        u: &mut Universe,
        backend: Backend,
        lowered: &Lowered,
        fingerprint: u64,
    ) -> Result<(Slot, Option<Applied>), Diagnostic> {
        let Some(pool) = self.pool.clone() else {
            return Ok((Rc::new(RefCell::new(open_session(u, backend, lowered, fingerprint, false)?)), None));
        };
        let mut pool = pool.borrow_mut();
        if let Some(part) = pool
            .parts
            .iter()
            .find(|part| part.fingerprint == fingerprint && part.lowered.ir == lowered.ir)
        {
            tracing::info!(target: "dl8::one", part = %part.name, "one engine adopt");
            return Ok((part.session.clone(), Some(part.applied.clone())));
        }
        if pool.pending.is_empty() {
            tracing::info!(target: "dl8::one", nodes = lowered.ir.nodes.len(), "one engine separate install");
            return Ok((Rc::new(RefCell::new(open_session(u, backend, lowered, fingerprint, false)?)), None));
        }
        let pending = std::mem::take(&mut pool.pending);
        let mut parts: Vec<(&str, &Lowered)> = vec![("p0.", lowered)];
        parts.extend(pending.iter().map(|(name, lowered)| (name.as_str(), lowered)));
        let slots = open_composed(u, backend, &parts)?;
        let failed = |u: &mut Universe, error| engine_diagnostic(u, error);
        let mut changes = Vec::new();
        for (slot, (_, part)) in slots[1..].iter().zip(&pending) {
            changes.extend(slot.borrow_mut().bootstrap(part).map_err(|e| failed(u, e))?);
        }
        let sent = changes.len();
        let delta = slots[1].borrow_mut().settle_raw(changes).map_err(|e| failed(u, e))?;
        for (slot, (name, part)) in slots[1..].iter().zip(pending) {
            let applied = slot.borrow_mut().decode(u, &part, &delta, sent).map_err(|e| failed(u, e))?;
            tracing::info!(target: "dl8::one", part = %name, nodes = part.ir.nodes.len(), rows = applied.rows.len(), "one engine part");
            pool.parts.push(Part {
                name,
                session: slot.clone(),
                fingerprint: structural_fingerprint(&part.ir),
                lowered: part,
                applied,
            });
        }
        Ok((slots[0].clone(), None))
    }

    /// The prepared program's whole closure.
    pub fn snapshot(&mut self, u: &mut Universe) -> Result<Closure, Diagnostic> {
        let (Some(session), Some(lowered)) = (self.session.as_ref(), self.prepared.as_ref()) else {
            return Err(diagnostic(u, "engine", "snapshot without a prepared program"));
        };
        let result = session.borrow_mut().snapshot(u, lowered);
        result.map_err(|error| engine_diagnostic(u, error))
    }

    /// Lower `program` with the `declared` sources and shapes, install it once,
    /// settle its own seeds, and keep both for `apply`.
    pub fn prepare(
        &mut self,
        u: &mut Universe,
        program: &Program,
        declared: &ir_lower::Declared,
    ) -> Result<Applied, Diagnostic> {
        self.session = None;
        self.prepared = None;
        self.fingerprint = None;
        let backend = match &self.backend {
            Ok(backend) => *backend,
            Err(message) => return Err(unknown_engine(u, message)),
        };
        let lowered = ir_lower::lower_declared(u, program, declared)?;
        let fingerprint = structural_fingerprint(&lowered.ir);
        let (session, adopted) = self.open(u, backend, &lowered, fingerprint)?;
        let applied = match adopted {
            Some(applied) => applied,
            None => {
                let started = session.borrow_mut().start(u, &lowered);
                started.map_err(|error| engine_diagnostic(u, error))?
            }
        };
        self.session = Some(session);
        self.prepared = Some(lowered);
        self.fingerprint = Some(fingerprint);
        Ok(applied)
    }

    /// The fingerprint of the IR `prepare` installed; `None` before it or after a failed apply.
    pub fn fingerprint(&self) -> Option<u64> {
        self.fingerprint
    }

    /// The prepared IR has a constructor for `shape` (`shape_of`).
    pub fn declares(&self, shape: &(String, usize, bool)) -> bool {
        self.prepared
            .as_ref()
            .is_some_and(|lowered| lowered.constructors.contains_key(shape))
    }

    /// The prepared engine can `mark` and `rewind`.
    pub fn rewinds(&self) -> bool {
        self.session.as_ref().is_some_and(|session| session.borrow().rewinds())
    }

    /// Record the prepared engine's state for `rewind`.
    pub fn mark(&mut self, u: &mut Universe) -> Result<(), Diagnostic> {
        let Some(session) = self.session.as_mut() else {
            return Err(diagnostic(u, "engine", "mark without a prepared program"));
        };
        let marked = session.borrow_mut().mark();
        marked.map_err(|error| engine_diagnostic(u, error))
    }

    /// Return the prepared engine to its last `mark`. A failed rewind drops the session.
    pub fn rewind(&mut self, u: &mut Universe) -> Result<(), Diagnostic> {
        let Some(session) = self.session.as_mut() else {
            return Err(diagnostic(u, "engine", "rewind without a prepared program"));
        };
        let rewound = session.borrow_mut().rewind();
        let result = rewound.map_err(|error| engine_diagnostic(u, error));
        if result.is_err() {
            self.session = None;
            self.prepared = None;
            self.fingerprint = None;
        }
        result
    }

    /// Settle signed seed rows on the prepared program.
    pub fn apply(
        &mut self,
        u: &mut Universe,
        changes: &[SignedRow],
    ) -> Result<Applied, ApplyError> {
        let (Some(session), Some(lowered)) = (self.session.as_mut(), self.prepared.as_ref()) else {
            return Err(ApplyError::Unprepared("no prepared program"));
        };
        let result = session.borrow_mut().apply(u, lowered, changes);
        if let Err(ApplyError::Failed(_)) = &result {
            self.session = None;
            self.prepared = None;
            self.fingerprint = None;
        }
        result
    }

    pub fn evaluate(
        &mut self,
        u: &mut Universe,
        program: &Program,
        answerer: Option<&mut Answerer>,
        fx: &mut dyn FnMut(Trace),
    ) -> Closure {
        match self.stdlib.clone() {
            Some(stdlib) => stdlib.borrow_mut().evaluate(u, program, answerer, fx, self),
            None => self.evaluate_plain(u, program, answerer, fx),
        }
    }

    /// `evaluate` without the stdlib split.
    pub fn evaluate_plain(
        &mut self,
        u: &mut Universe,
        program: &Program,
        answerer: Option<&mut Answerer>,
        fx: &mut dyn FnMut(Trace),
    ) -> Closure {
        LAST_COUNTERS.with(|counters| counters.set(None));
        LAST_ANSWERS.with(|rows| rows.borrow_mut().clear());
        LAST_REQUESTS.with(|count| count.set(0));
        self.prepared = None;
        self.fingerprint = None;
        let backend = match &self.backend {
            Ok(backend) => *backend,
            Err(message) => {
                return Closure {
                    rows: vec![],
                    diagnostics: vec![unknown_engine(u, message)],
                }
            }
        };
        match placement(u, program) {
            Placement::Compiled => self.evaluate_compiled(u, program, backend, answerer, fx),
            Placement::Interpreted(plan) => {
                let seeds = program.seeds.clone();
                self.evaluate_interpreted(u, backend, &plan, &seeds)
            }
            Placement::Split(compiled, plan, feeds) => {
                let closure = self.evaluate_compiled(u, &compiled, backend, answerer, fx);
                if !closure.diagnostics.is_empty() {
                    return closure;
                }
                let counters = LAST_COUNTERS.with(|last| last.take());
                let interpreted =
                    self.evaluate_interpreted(u, backend, &plan, &feed(&closure, &feeds));
                if !interpreted.diagnostics.is_empty() {
                    return interpreted;
                }
                let both = LAST_COUNTERS
                    .with(|last| last.take())
                    .map(|i| add_counters(counters.unwrap_or_default(), i));
                LAST_COUNTERS.with(|last| last.set(both));
                merge(u, closure, interpreted)
            }
        }
    }

    fn evaluate_interpreted(
        &mut self,
        u: &mut Universe,
        backend: Backend,
        plan: &rule_interp::Plan,
        seeds: &[Row],
    ) -> Closure {
        let wanted = rule_interp::bucket(plan.levels());
        if self
            .interp
            .as_ref()
            .is_none_or(|interp| interp.bucket() < wanted)
        {
            tracing::info!(
                target: "dl8::rules",
                bucket = wanted,
                previous = self.interp.as_ref().map_or(0, |interp| interp.bucket()),
                "rules interpreter install"
            );
            self.interp = match open_interp(u, backend, wanted, false) {
                Ok(interp) => Some(interp),
                Err(diag) => {
                    return Closure {
                        rows: vec![],
                        diagnostics: vec![diag],
                    }
                }
            };
        }
        match self.interp.as_mut().unwrap().settle_rows(u, plan, seeds) {
            Ok((closure, counters)) => {
                LAST_COUNTERS.with(|last| last.set(Some(counters)));
                closure
            }
            Err(error) => {
                self.interp = None;
                Closure {
                    rows: vec![],
                    diagnostics: vec![engine_diagnostic(u, error)],
                }
            }
        }
    }

    fn evaluate_compiled(
        &mut self,
        u: &mut Universe,
        program: &Program,
        backend: Backend,
        answerer: Option<&mut Answerer>,
        fx: &mut dyn FnMut(Trace),
    ) -> Closure {
        // A comptime round: rows enter only as the program's seeds and its answers.
        let sealed = ir_lower::Declared { sealed: true, ..ir_lower::Declared::default() };
        let lowered = match ir_lower::lower_declared(u, program, &sealed) {
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
            .is_none_or(|session| !session.borrow().matches(&lowered, fingerprint))
        {
            self.session = match self.open(u, backend, &lowered, fingerprint) {
                Ok((session, _)) => Some(session),
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
            .as_ref()
            .unwrap()
            .borrow_mut()
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
                    diagnostics: vec![engine_diagnostic(u, error)],
                }
            }
        }
    }
}

/// `lowered` with every relation id through `map`: a part of a composed install.
fn relocate(lowered: &Lowered, map: &BTreeMap<ir::RelId, ir::RelId>) -> Lowered {
    let rel = |id: ir::RelId| map[&id];
    let mut ir = lowered.ir.clone();
    for relation in &mut ir.rels {
        relation.id = rel(relation.id);
    }
    for term in &mut ir.terms {
        term.functor = rel(term.functor);
    }
    Lowered {
        ir,
        terms: lowered
            .terms
            .iter()
            .map(|spec| ir_lower::TermSpec {
                functor: rel(spec.functor),
                source: rel(spec.source),
                output: rel(spec.output),
                ..spec.clone()
            })
            .collect(),
        texts: lowered.texts.clone(),
        seeds: lowered.seeds.iter().map(|(id, row)| (rel(*id), row.clone())).collect(),
        term_map: rel(lowered.term_map),
        unit: rel(lowered.unit),
        visible: lowered
            .visible
            .iter()
            .map(|&(id, original, arity)| (rel(id), original, arity))
            .collect(),
        internal_seeds: lowered
            .internal_seeds
            .iter()
            .map(|(id, terms)| (rel(*id), terms.clone()))
            .collect(),
        plain_source: rel(lowered.plain_source),
        bad_lists: rel(lowered.bad_lists),
        served: lowered
            .served
            .iter()
            .map(|served| ir_lower::ServedSpec {
                request: rel(served.request),
                answer: rel(served.answer),
                ..served.clone()
            })
            .collect(),
        sources: lowered.sources.iter().map(|(key, id)| (*key, rel(*id))).collect(),
        installers: lowered
            .installers
            .iter()
            .map(|(key, (a, b))| (key.clone(), (rel(*a), rel(*b))))
            .collect(),
        constructors: lowered.constructors.iter().map(|(key, id)| (key.clone(), rel(*id))).collect(),
        atom_raws: lowered.atom_raws.clone(),
        dynamic_atom_base: lowered.dynamic_atom_base,
        fold_program: lowered.fold_program.clone(),
        declared_folds: lowered
            .declared_folds
            .iter()
            .map(|spec| ir_lower::DeclaredFoldSpec {
                ranked: rel(spec.ranked),
                ..spec.clone()
            })
            .collect(),
        literals: lowered.literals.clone(),
    }
}

/// `DL8_IR_DUMP=<dir>`: every installed IR as `<dir>/<pid>_<seq>_<fingerprint>.json`, in install
/// order. The `ivm-sqlite` install benchmark reads these.
fn dump_ir(program: &ir::Program, fingerprint: u64) {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let Some(dir) = std::env::var_os("DL8_IR_DUMP") else { return };
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::path::Path::new(&dir).join(format!("{}_{seq}_{fingerprint:016x}.json", std::process::id()));
    let written = serde_json::to_vec(program)
        .map_err(|e| e.to_string())
        .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
    match written {
        Ok(()) => tracing::info!(target: "dl8::ir", path = %path.display(), "ir dump"),
        Err(error) => tracing::warn!(target: "dl8::ir", path = %path.display(), %error, "ir dump failed"),
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
    tracing::info!(target: "dl8::ir", rows = rows.len(), "ir snapshot");
    let mut diagnostics = Vec::new();
    if let Some(program) = &lowered.fold_program {
        let mut folds = Vec::with_capacity(lowered.declared_folds.len());
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
            folds.push((&spec.fold, values));
        }
        let folds: Vec<_> = folds
            .iter()
            .map(|(fold, values)| (*fold, values.iter().map(Vec::as_slice).collect()))
            .collect();
        // One row store for every group of every declared fold.
        for result in super::evaluate::fold_generic_row_groups(u, program, &rows, &folds) {
            if let Err(diagnostic) = result {
                diagnostics.push(diagnostic);
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

/// `DL8_ENGINE`: unset is `ir:dd`.
pub fn configured_backend() -> Result<Backend, String> {
    match std::env::var("DL8_ENGINE").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("ir:dd") => Ok(Backend::Dd),
        Ok("ir:sqlite") => Ok(Backend::Sqlite),
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

/// An engine error as a diagnostic. A wave LetRec still changing at its limit is
/// `wave_limit(Limit)`.
fn engine_diagnostic(u: &mut Universe, error: ivm_engine::EngineError) -> Diagnostic {
    match error.kind {
        ivm_engine::ErrorKind::LetRecLimit(limit) => {
            let limit = u.int(i64::from(limit));
            Diagnostic {
                phase: "ir_eval",
                payload: u.compound("wave_limit", vec![limit]),
            }
        }
        _ => diagnostic(u, "engine", error),
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
    decode_terms_counted(u, lowered, engine, mapped).map(|(decoded, _)| decoded)
}

/// `decode_terms` and the number of interned rows it read.
fn decode_terms_counted<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    engine: &E,
    mapped: &HashMap<TermId, Cell>,
) -> Result<(HashMap<Cell, TermId>, usize), ivm_engine::EngineError> {
    let mut decoded: HashMap<Cell, TermId> = mapped.iter().map(|(&term, &id)| (id, term)).collect();
    // `(id, constructor name, args)` of every interned term not decoded yet, ascending id.
    let mut pending: Vec<(Cell, &str, Vec<Cell>)> = Vec::new();
    let constructors: Vec<&ir::Relation> = lowered
        .ir
        .rels
        .iter()
        .filter(|rel| rel.kind == ir::RelKind::Constructor)
        .collect();
    let ids: Vec<ir::RelId> = constructors.iter().map(|rel| rel.id).collect();
    let snapshots = engine.intern_snapshots(&ids)?;
    let read = snapshots.iter().map(Vec::len).sum::<usize>();
    tracing::info!(target: "dl8::ir", rows = read, "ir intern read");
    for (rel, rows) in constructors.iter().zip(snapshots) {
        for (mut row, weight) in rows {
            if weight > 0 && !decoded.contains_key(&row[0]) {
                let id = row.remove(0);
                pending.push((id, rel.name.as_str(), row));
            }
        }
    }
    pending.sort_unstable_by_key(|(id, ..)| *id);
    pending.dedup_by_key(|(id, ..)| *id);
    for (id, _, args) in pending.iter().filter(|(_, name, _)| *name == "2:str") {
        if let Some(value) = engine.text(args[0])? {
            decoded.insert(*id, u.string(&value));
        }
    }
    pending.retain(|(id, name, _)| !decoded.contains_key(id) && *name != "3:atom");
    // A term's arguments are interned before it, so one ascending pass decodes most terms.
    for _ in 0..pending.len().min(TERM_BUDGET) + 1 {
        let before = pending.len();
        pending.retain(|(id, functor, args)| {
            let term = match *functor {
                "0:int" => Some(u.int(args[0])),
                "0:float" => Some(u.float(f64::from_bits(args[0] as u64))),
                "1:bool" => Some(u.boolean(args[0] != 0)),
                name if name.starts_with("9:") => args
                    .iter()
                    .map(|arg| decoded.get(arg).copied())
                    .collect::<Option<Vec<_>>>()
                    .map(|items| u.compound(name.splitn(3, ':').nth(2).unwrap(), items)),
                _ => return false,
            };
            match term {
                Some(term) => {
                    decoded.insert(*id, term);
                    false
                }
                None => true,
            }
        });
        if pending.len() == before {
            break;
        }
    }
    Ok((decoded, read))
}

fn install_terms<E: Engine>(
    u: &Universe,
    lowered: &Lowered,
    engine: &mut E,
    mapped: &mut HashMap<TermId, Cell>,
    dynamic: &BTreeMap<(String, usize, bool), ir::RelId>,
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
            let Some(&functor) = lowered.constructors.get(&shape).or_else(|| dynamic.get(&shape)) else {
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

/// Intern every term the lowered program names and list its whole seed
/// frontier: unit, term map, seeds and internal seeds, sorted and deduplicated.
/// `known`: a part of a composed install lists term-map rows of its own terms and of `known`
/// only, never every term the shared engine maps; it gets them back extended.
fn bootstrap_changes<E: Engine>(
    lowered: &Lowered,
    core: &mut Core<E>,
    known: Option<&mut HashSet<TermId>>,
) -> Result<Vec<ir::SourceChange>, ivm_engine::EngineError> {
    let engine = &mut core.engine;
    let mapped = &mut core.mapped;
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
    match known {
        Some(known) => {
            known.extend(lowered.terms.iter().map(|spec| spec.original));
            for original in known.iter() {
                changes.push(ir::SourceChange {
                    rel: lowered.term_map,
                    row: vec![original.0 as i64, mapped[original]],
                    w: 1,
                });
            }
        }
        None => {
            for (&original, &id) in mapped.iter() {
                changes.push(ir::SourceChange {
                    rel: lowered.term_map,
                    row: vec![original.0 as i64, id],
                    w: 1,
                });
            }
        }
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
    Ok(changes)
}

/// The first settle of a prepared program: its whole seed frontier.
fn start_session<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    session: &mut EngineSession<E>,
) -> Result<Applied, ivm_engine::EngineError> {
    let changes = if session.relocation.is_some() {
        bootstrap_changes(lowered, &mut session.core.borrow_mut(), Some(&mut session.known))?
    } else {
        let changes = bootstrap_changes(lowered, &mut session.core.borrow_mut(), None)?;
        session.known = session.core.borrow().mapped.keys().copied().collect();
        changes
    };
    session.last_seeds = changes
        .iter()
        .map(|change| (change.rel, change.row.clone()))
        .collect();
    session.initialized = true;
    let sent = changes.len();
    let mut core = session.core.borrow_mut();
    let delta = settle(&mut core.engine, changes)?;
    tracing::info!(target: "dl8::ir", sent, changed = delta.changes.len(), "ir settle");
    let (rows, intern_read) = decode_delta(u, lowered, &mut core, &delta)?;
    let diagnostics = fold_diagnostics(u, lowered, &core)?;
    Ok(Applied {
        rows,
        diagnostics,
        sent,
        intern_read,
    })
}

/// Declared folds run over the whole closure (`snapshot_closure`); a program
/// without them reads nothing back.
fn fold_diagnostics<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    core: &Core<E>,
) -> Result<Vec<Diagnostic>, ivm_engine::EngineError> {
    if lowered.fold_program.is_none() {
        return Ok(Vec::new());
    }
    Ok(snapshot_closure(u, lowered, &core.engine, &core.mapped)?.diagnostics)
}

/// The visible relation changes of `delta` as terms. Cells the session has
/// not decoded yet cost one read of the interned terms.
fn decode_delta<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    session: &mut Core<E>,
    delta: &ir::Delta,
) -> Result<(Vec<SignedRow>, usize), ivm_engine::EngineError> {
    let visible: HashMap<ir::RelId, (TermId, usize)> = lowered
        .visible
        .iter()
        .map(|&(rel, original, arity)| (rel, (original, arity)))
        .collect();
    let missing = delta.changes.iter().any(|(rel, row, _)| {
        visible.get(rel).is_some_and(|&(_, arity)| {
            row[..arity]
                .iter()
                .any(|cell| !session.decoded.contains_key(cell))
        })
    });
    let mut intern_read = 0;
    if missing {
        let (decoded, read) = decode_terms_counted(u, lowered, &session.engine, &session.mapped)?;
        session.decoded.extend(decoded);
        intern_read = read;
    }
    let mut rows = Vec::new();
    for (rel, cells, weight) in &delta.changes {
        let Some(&(original, arity)) = visible.get(rel) else {
            continue;
        };
        let mut args = Vec::with_capacity(arity);
        for &cell in &cells[..arity] {
            let term = if let Some(&term) = session.decoded.get(&cell) {
                term
            } else if let Some(value) = session.engine.text(cell)? {
                let term = u.string(&value);
                session.decoded.insert(cell, term);
                term
            } else {
                return Err(ivm_engine::EngineError::new(
                    ivm_engine::Stage::Snapshot,
                    Some(*rel),
                    ivm_engine::ErrorKind::Unsupported("undecodable term id"),
                ));
            };
            args.push(term);
        }
        rows.push(SignedRow {
            row: Row {
                rel: original,
                args,
            },
            weight: *weight,
        });
    }
    Ok((rows, intern_read))
}

/// The constructor key `install_terms` uses for one term.
pub fn shape_of(u: &Universe, term: TermId) -> (String, usize, bool) {
    match u.get(term) {
        Term::Int(_) => ("0:int".into(), 0, true),
        Term::Float(_) => ("0:float".into(), 0, true),
        Term::Str(_) => ("2:str".into(), 1, false),
        Term::Bool(_) => ("1:bool".into(), 0, true),
        Term::Atom(_) => ("3:atom".into(), 0, true),
        Term::Compound(sym, args) => (
            format!("9:{}:{}", args.len(), u.sym_str(*sym)),
            args.len(),
            false,
        ),
    }
}

/// Install the terms of `roots` the session has not mapped yet and return
/// them. A shape the prepared IR has no constructor for is `Unprepared`.
/// A part of a composed install (`dynamic`) declares a shape its IR lacks as a constructor
/// no operator reads; any other session reports it `Unprepared`.
fn install_fresh<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    session: &mut EngineSession<E>,
    roots: &[TermId],
) -> Result<Vec<TermId>, ApplyError> {
    let dynamic = session.relocation.is_some();
    let mut core = session.core.borrow_mut();
    let mut fresh = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut todo = roots.to_vec();
    let mut missing = BTreeSet::new();
    while let Some(term) = todo.pop() {
        if session.known.contains(&term) || !seen.insert(term) {
            continue;
        }
        let shape = shape_of(u, term);
        if !core.mapped.contains_key(&term)
            && !lowered.constructors.contains_key(&shape)
            && !core.dynamic.contains_key(&shape)
        {
            if !dynamic {
                return Err(ApplyError::Unprepared("term constructor"));
            }
            missing.insert(shape);
        }
        fresh.push(term);
        if let Term::Compound(_, args) = u.get(term) {
            todo.extend(args.iter().copied());
        }
    }
    if !missing.is_empty() {
        let missing: Vec<_> = missing.into_iter().collect();
        let declared: Vec<(String, Vec<ir::Ty>)> = missing
            .iter()
            .map(|(name, width, raw)| (name.clone(), constructor_columns(name, *width, *raw)))
            .collect();
        let ids = core
            .engine
            .declare_constructors(&declared)
            .map_err(|error| ApplyError::Failed(engine_diagnostic(u, error)))?;
        tracing::info!(target: "dl8::one", shapes = ?declared.iter().map(|(name, _)| name).collect::<Vec<_>>(), "one engine declare constructors");
        core.dynamic.extend(missing.into_iter().zip(ids));
    }
    let core = &mut *core;
    install_terms(u, lowered, &mut core.engine, &mut core.mapped, &core.dynamic, &fresh)
        .map_err(|error| ApplyError::Failed(engine_diagnostic(u, error)))?;
    for &term in &fresh {
        core.decoded.insert(core.mapped[&term], term);
        session.known.insert(term);
    }
    Ok(fresh)
}

/// Argument columns of a constructor shape, as `ir_lower::lower_declared` declares them.
fn constructor_columns(name: &str, width: usize, raw: bool) -> Vec<ir::Ty> {
    let column = if name == "0:float" {
        ir::Ty::Real
    } else if raw {
        ir::Ty::Int
    } else {
        ir::Ty::Id
    };
    vec![column; width.max(usize::from(raw))]
}

/// One settle of signed seed rows. A term new to the session is installed
/// with the term-map and list rows `ir_lower::lower` seeds for its terms.
fn apply_session<E: Engine>(
    u: &mut Universe,
    lowered: &Lowered,
    session: &mut EngineSession<E>,
    changes: &[SignedRow],
) -> Result<Applied, ApplyError> {
    let failed = |u: &mut Universe, error| ApplyError::Failed(engine_diagnostic(u, error));
    let mut sources = Vec::with_capacity(changes.len());
    for change in changes {
        let Some(&source) = lowered
            .sources
            .get(&(change.row.rel.0, change.row.args.len()))
        else {
            return Err(ApplyError::Unprepared("seed relation"));
        };
        sources.push(source);
    }
    let roots: Vec<TermId> = changes
        .iter()
        .flat_map(|change| change.row.args.iter().copied())
        .collect();
    let mut fresh = install_fresh(u, lowered, session, &roots)?;
    let mut plain_items = Vec::new();
    let mut bad_lists = Vec::new();
    for &term in &fresh {
        let Some(payload) = u.unary(term, "const") else {
            continue;
        };
        match u.as_list(payload) {
            Some(list) => {
                if let Some(plain) = list
                    .iter()
                    .map(|id| super::kernel::semantic(u, *id))
                    .collect::<Option<Vec<_>>>()
                {
                    plain_items.push((term, plain));
                }
            }
            None => bad_lists.push(term),
        }
    }
    let plain_pairs: Vec<(TermId, TermId)> = plain_items
        .into_iter()
        .map(|(tagged, plain)| (tagged, u.list(&plain)))
        .collect();
    let plains: Vec<TermId> = plain_pairs.iter().map(|(_, plain)| *plain).collect();
    fresh.extend(install_fresh(u, lowered, session, &plains)?);
    let core_rc = session.core.clone();
    let mut core = core_rc.borrow_mut();
    let mut frontier = Vec::with_capacity(changes.len() + fresh.len());
    for &term in &fresh {
        frontier.push(ir::SourceChange {
            rel: lowered.term_map,
            row: vec![term.0 as i64, core.mapped[&term]],
            w: 1,
        });
    }
    for (tagged, plain) in &plain_pairs {
        frontier.push(ir::SourceChange {
            rel: lowered.plain_source,
            row: vec![core.mapped[tagged], core.mapped[plain]],
            w: 1,
        });
    }
    for term in &bad_lists {
        frontier.push(ir::SourceChange {
            rel: lowered.bad_lists,
            row: vec![core.mapped[term]],
            w: 1,
        });
    }
    for (change, source) in changes.iter().zip(sources) {
        let row = if change.row.args.is_empty() {
            vec![1]
        } else {
            change
                .row
                .args
                .iter()
                .map(|term| core.mapped[term])
                .collect()
        };
        frontier.push(ir::SourceChange {
            rel: source,
            row,
            w: change.weight,
        });
    }
    frontier.sort_by(|a, b| (a.rel, &a.row).cmp(&(b.rel, &b.row)));
    let mut consolidated: Vec<ir::SourceChange> = Vec::with_capacity(frontier.len());
    for change in frontier {
        match consolidated.last_mut() {
            Some(last) if last.rel == change.rel && last.row == change.row => last.w += change.w,
            _ => consolidated.push(change),
        }
    }
    consolidated.retain(|change| change.w != 0);
    for change in &consolidated {
        let key = (change.rel, change.row.clone());
        if change.w > 0 {
            session.last_seeds.insert(key);
        } else {
            session.last_seeds.remove(&key);
        }
    }
    let sent = consolidated.len();
    let delta = settle(&mut core.engine, consolidated).map_err(|error| failed(u, error))?;
    tracing::info!(target: "dl8::ir", sent, changed = delta.changes.len(), "ir apply");
    let (rows, intern_read) =
        decode_delta(u, lowered, &mut core, &delta).map_err(|error| failed(u, error))?;
    let diagnostics = fold_diagnostics(u, lowered, &core).map_err(|error| failed(u, error))?;
    Ok(Applied {
        rows,
        diagnostics,
        sent,
        intern_read,
    })
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
    let core_rc = session.core.clone();
    let mut core_ref = core_rc.borrow_mut();
    let changes = if session.relocation.is_some() {
        bootstrap_changes(lowered, &mut core_ref, Some(&mut session.known))?
    } else {
        let changes = bootstrap_changes(lowered, &mut core_ref, None)?;
        session.known = core_ref.mapped.keys().copied().collect();
        changes
    };
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
    let Core {
        engine,
        mapped,
        dynamic,
        ..
    } = &mut *core_ref;
    let answered = &mut session.answered;
    let sent = frontier.len();
    let mut delta = settle(engine, frontier)?;
    tracing::info!(target: "dl8::ir", sent, changed = delta.changes.len(), "ir settle");
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
        install_terms(u, lowered, engine, mapped, dynamic, &answer_terms)?;
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
        install_terms(u, lowered, engine, mapped, dynamic, &terms)?;
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
            install_terms(u, lowered, engine, mapped, dynamic, &answer_terms)?;
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
            install_terms(u, lowered, engine, mapped, dynamic, &terms)?;
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
                install_terms(u, lowered, engine, mapped, dynamic, &answer_terms)?;
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
    let program = match placement(u, program) {
        Placement::Compiled => program,
        Placement::Interpreted(plan) => {
            let mut interp = open_interp(u, backend, rule_interp::bucket(plan.levels()), true)?;
            return run_interpreted(
                u,
                interp.as_mut(),
                &plan,
                &program.seeds,
                answerer,
                frontiers,
                live,
                fx,
            )
            .map_err(|error| engine_diagnostic(u, error));
        }
        Placement::Split(compiled, plan, feeds) => {
            return run_split(
                u,
                backend,
                &compiled,
                &plan,
                &feeds,
                answerer,
                frontiers,
                public_names,
                live,
                fx,
            );
        }
    };
    let lowered = ir_lower::lower(u, program)?;
    let fingerprint = structural_fingerprint(&lowered.ir);
    let mut session = open_session(u, backend, &lowered, fingerprint, true)?;
    match session.run(u, &lowered, answerer, frontiers, public_names, live, fx) {
        Ok((closure, results, counters)) => {
            LAST_COUNTERS.with(|last| last.set(Some(counters)));
            Ok((closure, results))
        }
        Err(error) => Err(engine_diagnostic(u, error)),
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

/// Where `DL8_RULES` and `rule_interp::split` put one program's rules.
enum Placement {
    Compiled,
    /// Every rule interpreted and no relation served: the interpreter alone on the seeds.
    Interpreted(rule_interp::Plan),
    /// The compiled rules first, then the interpreted rules over the compiled rows of `feeds`.
    Split(
        Program,
        rule_interp::Plan,
        std::collections::HashSet<(u32, usize)>,
    ),
}

fn placement(u: &mut Universe, program: &Program) -> Placement {
    if rule_interp::rules_mode() != RulesMode::Interpret {
        return Placement::Compiled;
    }
    let split = rule_interp::split(u, program).and_then(|split| {
        let plan = rule_interp::plan(u, &split.interpreted)?;
        Ok((split, plan))
    });
    match split {
        Ok((split, plan)) => {
            tracing::info!(
                target: "dl8::rules",
                mode = "interpret",
                rules = program.rules.len(),
                interpreted = plan.rule_count(),
                compiled = split.compiled.rules.len(),
                levels = plan.levels(),
                reasons = ?split.reasons,
                "rules mode"
            );
            if split.compiled.rules.is_empty() && program.served.is_empty() {
                Placement::Interpreted(plan)
            } else {
                Placement::Split(split.compiled, plan, split.feeds)
            }
        }
        Err(reason) => {
            tracing::info!(
                target: "dl8::rules",
                mode = "compile",
                rules = program.rules.len(),
                %reason,
                "rules mode"
            );
            Placement::Compiled
        }
    }
}

/// The rows of `closure` the interpreted rules read or head.
fn feed(closure: &Closure, feeds: &std::collections::HashSet<(u32, usize)>) -> Vec<Row> {
    closure
        .rows
        .iter()
        .filter(|row| feeds.contains(&(row.rel.0, row.args.len())))
        .cloned()
        .collect()
}

/// What `take_answers` returns next: the answers of every pass of one evaluation.
pub(crate) fn put_answers(requests: usize, rows: Vec<Row>) {
    LAST_REQUESTS.with(|count| count.set(requests));
    LAST_ANSWERS.with(|last| *last.borrow_mut() = rows);
}

pub(crate) fn reset_last() {
    LAST_COUNTERS.with(|counters| counters.set(None));
    LAST_ANSWERS.with(|rows| rows.borrow_mut().clear());
    LAST_REQUESTS.with(|count| count.set(0));
}

pub(crate) fn merge_closures(u: &Universe, first: Closure, second: Closure) -> Closure {
    merge(u, first, second)
}

/// The compiled closure with the interpreted rows, sorted as `snapshot_closure` sorts.
fn merge(u: &Universe, compiled: Closure, interpreted: Closure) -> Closure {
    let mut rows = compiled.rows;
    rows.extend(interpreted.rows);
    rows.sort_by(|a, b| {
        u.cmp(a.rel, b.rel)
            .then_with(|| u.cmp_rows(&a.args, &b.args))
    });
    rows.dedup();
    Closure {
        rows,
        diagnostics: compiled.diagnostics,
    }
}

fn add_counters(a: ivm_engine::Counters, b: ivm_engine::Counters) -> ivm_engine::Counters {
    let sum = |x: Option<u64>, y: Option<u64>| match (x, y) {
        (None, None) => None,
        (x, y) => Some(x.unwrap_or(0) + y.unwrap_or(0)),
    };
    ivm_engine::Counters {
        rows_written: a.rows_written + b.rows_written,
        delta_rows: ivm_engine::DeltaRows {
            filter: sum(a.delta_rows.filter, b.delta_rows.filter),
            join: sum(a.delta_rows.join, b.delta_rows.join),
            antijoin: sum(a.delta_rows.antijoin, b.delta_rows.antijoin),
            reduce: sum(a.delta_rows.reduce, b.delta_rows.reduce),
            topk: sum(a.delta_rows.topk, b.delta_rows.topk),
            window: sum(a.delta_rows.window, b.delta_rows.window),
            mint: sum(a.delta_rows.mint, b.delta_rows.mint),
        },
        rounds: sum(a.rounds, b.rounds),
        interned: sum(a.interned, b.interned),
        statements: sum(a.statements, b.statements),
    }
}

/// The interpreter alone: the seeds, then each frontier, then each live settle.
#[allow(clippy::too_many_arguments)]
fn run_interpreted(
    u: &mut Universe,
    interp: &mut dyn InterpRun,
    plan: &rule_interp::Plan,
    seeds: &[Row],
    mut answerer: Option<&mut Answerer>,
    frontiers: &[Vec<SignedRow>],
    mut live: Option<&mut LiveRun<'_>>,
    fx: &mut dyn FnMut(Trace),
) -> Result<(Closure, Vec<FrontierResult>), ivm_engine::EngineError> {
    interp.settle_rows(u, plan, seeds)?;
    let mut frontier_results = Vec::with_capacity(frontiers.len());
    for frontier in frontiers {
        interp.change(plan, frontier, "frontier relation is not a source")?;
        frontier_results.push(FrontierResult {
            closure: interp.closure(u)?,
            deltas: Vec::new(),
            counters: interp.counters(),
        });
    }
    if let Some(live) = live.as_deref_mut() {
        let closure = interp.closure(u)?;
        (live.on_settle)(u, &closure, &[], &interp.counters());
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
            interp.change(plan, &frontier, "live relation is not a source")?;
            let closure = interp.closure(u)?;
            (live.on_settle)(u, &closure, &[], &interp.counters());
        }
    }
    LAST_REQUESTS.with(|count| count.set(0));
    LAST_COUNTERS.with(|last| last.set(Some(interp.counters())));
    let closure = interp.closure(u)?;
    fx(Trace::Closure {
        rows: closure.rows.len(),
    });
    Ok((closure, frontier_results))
}

/// The compiled rules on one fresh engine, then the interpreted rules on another over each
/// compiled closure: the run's, each frontier's (after a leading empty frontier for the start),
/// and each live settle's.
#[allow(clippy::too_many_arguments)]
fn run_split(
    u: &mut Universe,
    backend: Backend,
    compiled: &Program,
    plan: &rule_interp::Plan,
    feeds: &std::collections::HashSet<(u32, usize)>,
    answerer: Option<&mut Answerer>,
    frontiers: &[Vec<SignedRow>],
    public_names: Option<&HashMap<TermId, String>>,
    live: Option<&mut LiveRun<'_>>,
    fx: &mut dyn FnMut(Trace),
) -> Result<(Closure, Vec<FrontierResult>), Diagnostic> {
    let mut interp = open_interp(u, backend, rule_interp::bucket(plan.levels()), true)?;
    let lowered = ir_lower::lower(u, compiled)?;
    let fingerprint = structural_fingerprint(&lowered.ir);
    let mut session = open_session(u, backend, &lowered, fingerprint, true)?;
    let mut failure: Option<ivm_engine::EngineError> = None;
    let live_run = live.is_some();
    let result = match live {
        Some(LiveRun {
            max_settles,
            poll,
            on_settle,
        }) => {
            let user = on_settle;
            let interp = interp.as_mut();
            let failure = &mut failure;
            let mut wrapped = |u: &Universe,
                               closure: &Closure,
                               deltas: &[NamedDelta],
                               counters: &ivm_engine::Counters| {
                if failure.is_some() {
                    return;
                }
                if !closure.diagnostics.is_empty() {
                    (user)(u, closure, deltas, counters);
                    return;
                }
                match interp.settle_rows(u, plan, &feed(closure, feeds)) {
                    Ok((rows, interp_counters)) => {
                        let merged = merge(u, closure.clone(), rows);
                        (user)(
                            u,
                            &merged,
                            deltas,
                            &add_counters(*counters, interp_counters),
                        );
                    }
                    Err(error) => *failure = Some(error),
                }
            };
            let mut run = LiveRun {
                max_settles: *max_settles,
                poll: &mut **poll,
                on_settle: &mut wrapped,
            };
            session.run(u, &lowered, answerer, &[], public_names, Some(&mut run), fx)
        }
        None => {
            let mut all = Vec::with_capacity(frontiers.len() + 1);
            all.push(Vec::new());
            all.extend(frontiers.iter().cloned());
            session.run(u, &lowered, answerer, &all, public_names, None, fx)
        }
    };
    let (closure, results, counters) = result.map_err(|error| engine_diagnostic(u, error))?;
    if let Some(error) = failure {
        return Err(engine_diagnostic(u, error));
    }
    if !closure.diagnostics.is_empty() {
        LAST_COUNTERS.with(|last| last.set(Some(counters)));
        return Ok((
            closure,
            results.into_iter().skip(usize::from(!live_run)).collect(),
        ));
    }
    let mut frontier_results = Vec::with_capacity(frontiers.len());
    for (index, result) in results.into_iter().enumerate() {
        let (rows, interp_counters) = interp
            .settle_rows(u, plan, &feed(&result.closure, feeds))
            .map_err(|error| engine_diagnostic(u, error))?;
        if index == 0 {
            continue;
        }
        frontier_results.push(FrontierResult {
            closure: if result.closure.diagnostics.is_empty() {
                merge(u, result.closure, rows)
            } else {
                result.closure
            },
            deltas: result.deltas,
            counters: add_counters(result.counters, interp_counters),
        });
    }
    let (rows, interp_counters) = interp
        .settle_rows(u, plan, &feed(&closure, feeds))
        .map_err(|error| engine_diagnostic(u, error))?;
    LAST_COUNTERS.with(|last| last.set(Some(add_counters(counters, interp_counters))));
    Ok((merge(u, closure, rows), frontier_results))
}

/// The rule interpreter on one engine: the installed program, the dictionary ids Rust interned,
/// and the source rows the engine holds.
struct InterpEngine<E: Engine> {
    engine: E,
    shape: rule_interp::Shape,
    interner: rule_interp::Interner,
    installed: BTreeSet<(ir::RelId, Vec<Cell>)>,
}

trait InterpRun {
    fn bucket(&self) -> u32;
    /// Settles the source rows of `plan` and `seeds` in place of the rows installed before.
    fn settle_rows(
        &mut self,
        u: &Universe,
        plan: &rule_interp::Plan,
        seeds: &[Row],
    ) -> Result<(Closure, ivm_engine::Counters), ivm_engine::EngineError>;
    /// Settles signed seed changes.
    fn change(
        &mut self,
        plan: &rule_interp::Plan,
        rows: &[SignedRow],
        what: &'static str,
    ) -> Result<(), ivm_engine::EngineError>;
    fn closure(&self, u: &Universe) -> Result<Closure, ivm_engine::EngineError>;
    fn counters(&self) -> ivm_engine::Counters;
}

fn open_interp(
    u: &mut Universe,
    backend: Backend,
    bucket: u32,
    measured: bool,
) -> Result<Box<dyn InterpRun>, Diagnostic> {
    fn install<E: Engine + 'static>(
        bucket: u32,
        measured: bool,
    ) -> Result<Box<dyn InterpRun>, ivm_engine::EngineError> {
        let shape = rule_interp::shape(bucket);
        let fingerprint = structural_fingerprint(&shape.ir);
        dump_ir(&shape.ir, fingerprint);
        let engine = if measured {
            E::install(&shape.ir)?
        } else {
            E::install_unmeasured(&shape.ir)?
        };
        let (joins, join_input_cols) = arranged_join_inputs(&shape.ir);
        tracing::info!(
            target: "dl8::ir",
            engine = std::any::type_name::<E>(),
            nodes = shape.ir.nodes.len(),
            joins,
            join_input_cols,
            "ir install"
        );
        Ok(Box::new(InterpEngine {
            engine,
            shape,
            interner: rule_interp::Interner::default(),
            installed: BTreeSet::new(),
        }))
    }
    match backend {
        Backend::Dd => install::<ivm_dd::Dd>(bucket, measured),
        Backend::Sqlite => install::<ivm_sqlite::Sqlite>(bucket, measured),
    }
    .map_err(|error| engine_diagnostic(u, error))
}

impl<E: Engine> InterpRun for InterpEngine<E> {
    fn bucket(&self) -> u32 {
        self.shape.bucket
    }

    fn settle_rows(
        &mut self,
        u: &Universe,
        plan: &rule_interp::Plan,
        seeds: &[Row],
    ) -> Result<(Closure, ivm_engine::Counters), ivm_engine::EngineError> {
        let wanted = self
            .interner
            .rows(&mut self.engine, &self.shape, plan, seeds)?;
        let changes = rule_interp::diff(&self.installed, &wanted);
        self.installed = wanted;
        let sent = changes.len();
        let delta = settle(&mut self.engine, changes)?;
        tracing::info!(target: "dl8::ir", sent, changed = delta.changes.len(), "ir settle");
        Ok((self.closure(u)?, self.engine.counters()))
    }

    fn change(
        &mut self,
        plan: &rule_interp::Plan,
        rows: &[SignedRow],
        what: &'static str,
    ) -> Result<(), ivm_engine::EngineError> {
        let mut changes: Vec<ir::SourceChange> = Vec::with_capacity(rows.len());
        for change in rows {
            if !plan
                .keys
                .contains(&(change.row.rel.0, change.row.args.len()))
            {
                return Err(ivm_engine::EngineError::new(
                    ivm_engine::Stage::Settle,
                    None,
                    ivm_engine::ErrorKind::Unsupported(what),
                ));
            }
            let (rel, row) = self
                .interner
                .seed_row(&mut self.engine, &self.shape, &change.row)?;
            changes.push(ir::SourceChange {
                rel,
                row,
                w: change.weight,
            });
        }
        changes.sort_by(|a, b| (a.rel, &a.row).cmp(&(b.rel, &b.row)));
        let mut consolidated: Vec<ir::SourceChange> = Vec::with_capacity(changes.len());
        for change in changes {
            match consolidated.last_mut() {
                Some(last) if last.rel == change.rel && last.row == change.row => {
                    last.w += change.w
                }
                _ => consolidated.push(change),
            }
        }
        consolidated.retain(|change| change.w != 0);
        for change in &consolidated {
            let key = (change.rel, change.row.clone());
            if change.w > 0 {
                self.installed.insert(key);
            } else {
                self.installed.remove(&key);
            }
        }
        let sent = consolidated.len();
        let delta = settle(&mut self.engine, consolidated)?;
        tracing::info!(target: "dl8::ir", sent, changed = delta.changes.len(), "ir apply");
        Ok(())
    }

    fn closure(&self, u: &Universe) -> Result<Closure, ivm_engine::EngineError> {
        let rows = rule_interp::decode_facts(u, &self.engine, &self.shape)?;
        tracing::info!(target: "dl8::ir", rows = rows.len(), "ir snapshot");
        Ok(Closure {
            rows,
            diagnostics: Vec::new(),
        })
    }

    fn counters(&self) -> ivm_engine::Counters {
        self.engine.counters()
    }
}
