//! The scheduler/reconciler — ReactDOM for code facts. Everything is a scheduler
//! (React fiber, RxJS Scheduler, axum = dispatch over a mergeMap threadpool); this
//! is ours. Inputs arrive as `Change`s; a tick reconciles the affected rels to
//! fixpoint and commits deltas for the MATERIALIZED ones, skips VIEW rels, wakes
//! fired CLOCK rels. Effects are a registered seam like Extractor, with the
//! cache/cancel/clock machinery SHARED (a new effect gets it unwritten).
//!
//! (bidi / backpressure aren't here yet — same words apply, embed later: a bounded
//! channel = the dam; mergeMap concurrency = the pool.)

use crate::_1_feldera::Delta;
use crate::_0_key::{FileId, RelId, RevId, SymId};

/// Input sources into the reactive graph (json-rx `bindings.sources`, generalized).
pub enum Change {
    FileChanged(FileId),        // fs watch (notify)
    RevChanged(RevId),          // git checkout — NOT pure fs; watch needs gix
    ClockFired(RelId),          // a Clock eval-strategy rel's timer
    UserAnswer(SymId),          // user asking question(s) with select = an input source
    EffectResult(RelId),        // an async effect completed -> re-enters next tick
}

// --- HOOKS: the forward-authored dependency surface (React hooks / Salsa queries) ---

/// A recorded READ = a dependency edge + an early-cutoff key. An op that called
/// `use_file(p)` gets a `Read { File(p), digest }`; the runtime re-invokes the op
/// only when the LIVE digest differs from `digest` — "changed from the last time i
/// saw it" = Salsa early-cutoff / backdating = the makeSwitchMapCached keyFn.
pub struct Read {
    pub on: ReadTarget,
    pub digest: u64,     // value-at-read; re-run iff the live digest differs
}

/// Read targets are STREAMS (things that change), never interned strings (values
/// that don't). A file stream, a relation stream — both are a StreamId underneath.
pub enum ReadTarget {
    File(FileId),
    Rel(RelId),
}

/// Forward authoring (React hooks, Solid signals, Salsa queries) — NOT backward
/// RxJS pipe wiring. An op body calls `use_*` in natural reading order; each call
/// PUSHES a `Read` into the op's dep-set and returns the current value. The
/// dependency GRAPH is INFERRED from the reads (like `_4_analyze` infers the static
/// ref graph), not authored. Independent reads in one round FLATTEN + batch (Haxl
/// applicative = a static graph we can do math over); a read whose value picks the
/// next read is the monadic case (can't flatten) — so PREFER applicative. This is
/// the fine-grained (file/row) DYNAMIC twin of analyze's coarse (rel) STATIC graph.
pub trait Hooks {
    fn read_file(&mut self, path: SymId) -> FileId;  // tracks a read of this file
    fn read_rel(&mut self, rel: RelId);              // tracks a read of this rel (rows in the real crate)
    // interned strings are VALUES not streams -> intern(&str), NOT a tracked read.
    // the collected dep-set drives invalidation AND feeds _4_analyze's RefGraph.
}

/// Effects have a DIRECTION. sprefa's original feature was `--move`, a MUTATION —
/// so the reconciler was born writing, not reading.
pub enum EffectDir {
    Ingest,   // World -> Facts (http/cmd read): produce facts from outside.
    Mutate,   // Facts -> World (--move/rename/codemod WRITE): apply a derived change.
}

/// A registered effect — PUBLIC and OPEN like Extractor (React component contract):
/// we ship builtins (http/cmd/clock/--move), users `impl Effect` in their own Rust
/// crate. Native vs Composed mirrors host vs composite components; both referenced
/// by name. Ingest = http/cmd read; Mutate = --move/codemod write. The runtime
/// supplies skip-if-same (`cache_key`), cancel-stale (switchMap), and clock
/// scheduling — the makeSwitchMapCached triplet, once, for both directions.
pub trait Effect {
    fn dir(&self) -> EffectDir;
    // async fn run_ingest(&self, inputs: &[SymId]) -> RowSet;   // Ingest
    // async fn run_mutate(&self, inputs: &[SymId]) -> Vec<Edit>; // Mutate
    /// the makeSwitchMapCached keyFn: digest(command + bound inputs). For Mutate
    /// effects this is ALSO the idempotency key: a codemod already applied (digest
    /// unchanged) must not re-apply.
    fn cache_key(&self, inputs: &[SymId]) -> u64;
}

/// A file edit produced by a Mutate effect (a codemod hunk). `--move` emits these.
pub struct Edit {
    pub file: FileId,
    pub range: (u32, u32),   // byte span to replace
    pub replacement: String,
}

/// THE LOOP CLOSES THROUGH THE FILESYSTEM. Applying a Mutate effect writes files,
/// which emits Change::FileChanged, which re-extracts, which converges actual ->
/// desired. Fixpoint = desired == actual = ReactDOM committing until vDOM == DOM.
/// `--move` terminating is this reconcile reaching a stable state; it demands
/// idempotent codemods and the effect quarantine (mutate in commit, never mid-fixpoint).
pub struct Reconcile;

pub struct TickReport {
    pub committed: Vec<(RelId, usize)>,   // rel -> delta size
    pub woke: Vec<RelId>,
}

/// One reconcile tick (ReactDOM commit phase). Async in the real crate.
pub trait Runtime {
    fn tick(&mut self, changes: Vec<Change>) -> TickReport;
    // fn subscribe(&mut self, rel: RelId) -> ...;   // demand: a reader makes a rel hot
}

/// A committed change to a rel, produced by a tick. Ties runtime -> feldera Delta.
pub struct Commit<K, W> {
    pub rel: RelId,
    pub delta: Delta<K, W>,
}
