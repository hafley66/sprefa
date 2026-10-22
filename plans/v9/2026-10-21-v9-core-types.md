# v9 core types: the IVM graph event loop

Draft for review. Principles: `2026-10-21-v9-principles.md`. Inventory source: `ryii --file-fact
--kinds types src` over sprefa at 4d75f231a (315 structs, enums and traits) and over
`hafley-rs/crates/redux/src`. DD names are from `differential-dataflow 0.25.1` / `timely 0.31.0`.

## 1. What carries over, what DD replaces

| today (sprefa) | v9 | why |
| --- | --- | --- |
| `TermId`, `Universe`, `Sym` (`_6_eval/_0_term.rs`) | `Id` (hash) + `Sid` (dense) + `Dict` | ids not strings; hash at boundaries, dense inside |
| `Row { rel, args: Vec<TermId> }` (`_1_program.rs`) | columnar rows of `Sid`, keyed by `ProductId` | no per-row `Vec` |
| `Program`, `Rule`, `Goal`, `Arg`, `Polarity`, `Fold` | same, as rows (rules are data, principle 8) | generics and specifics through one interpreter |
| `Slice`, `Never`, `reduce_then_apply` (`_7_effect.rs`; redux `0_slice.rs`, `2_effects.rs`) | kept as the host face (games, the redux API); the core's phases stop being `Slice` reducers | the four fixpoints (`Drive`, `Expand`, `Rounds`, `Evaluate`) become dataflows |
| `IService`, `Cadence` (`_9_runtime/_2_reconcile.rs`) | `IService` with batched `call`/`poll`; `Cadence` unchanged | batch per frontier |
| `Answerer`, `Reconciler`, `Held`, `answered`, `Kept`, `Decoded`, `Marked` | gone: DD input sessions, diffs as multiplicity, arrangements | no Rust side maps of rows |
| `IrSession`, `EngineSession`, `Core`, `Pool`, `Part`, `Stdlib`, `OneEngine` | `Core` (one worker) + `Program::build` per program | one engine, many dataflows |
| `IRowStore`, `SqliteRowStore`, `Watermark` (`_9_runtime/_0_store.rs`) | `ByteStore` under capture and spill | storage is bytes plus the update log |
| `ivm_engine::Engine` trait, `Backend` | gone (scope decision 5) | one engine; swap at storage |

## 2. Type sigs (bodies are comments)

```rust
// ---- ids (principles 5, 12) ----
/// 64-bit content hash: crosses processes, survives restarts.
pub struct Id(u64);
/// Dense id minted by the running interner; meaningless outside it.
pub struct Sid(u32);
pub struct ProductId(u32);
/// Id <-> Sid <-> bytes. Text enters once; an Id hit with different bytes is a loud error.
pub struct Dict { /* hashbrown maps, bytes in one arena */ }
impl Dict {
    pub fn intern(&mut self, bytes: &[u8]) -> (Id, Sid);   // hash, check, mint
    pub fn bytes(&self, sid: Sid) -> &[u8];                // only at print/send edges
}

// ---- time (principle 2) ----
pub struct Event(u64);   // outer: a save, a request, a git move, a timer
pub struct Round(u32);   // comptime iteration
pub struct Wave(u32);    // macrotime iteration
/// DD product order; rounds and waves are nested `iterate` scopes.
pub type Time = timely::order::Product<Event, timely::order::Product<Round, Wave>>;
pub type Diff = i64;     // multiplicity; readers that want a set apply `distinct`

// ---- the one engine (principle 13) ----
pub struct Core {
    // timely worker, one per thread; every program is a dataflow on it
    inputs: Inputs,                 // one InputSession<Event, Row, Diff> per source ProductId
    shared: Shared,                 // arrangements other dataflows import
    probe: timely::dataflow::ProbeHandle<Event>,
    dict: Dict,
    store: Box<dyn ByteStore>,      // the one swap point (env selects)
    budget: Budget,
}
pub struct Budget {
    max_epochs_in_flight: u32,      // backpressure: inputs wait while the probe lags
    max_resident_bytes: u64,        // arrangement chunks page out past this
}
/// Arrangements by relation, imported by any dataflow that reads them.
pub struct Shared { /* ProductId -> TraceAgent */ }

// ---- programs on the engine: compiler, macrotime, runtime ----
pub trait Program {
    /// Builds this program's dataflow once; reads shared arrangements, publishes its own.
    fn build<G: timely::dataflow::Scope<Timestamp = Time>>(
        &self, scope: &mut G, shared: &mut Shared,
    ) -> Outputs;
}
pub struct Outputs { /* ProductId -> Collection, published to Shared */ }

// ---- drivers (sync and async) ----
pub trait Drive {
    /// Sync: feed `batch` at `at`, step until the probe passes `at`.
    fn settle(&mut self, at: Event, batch: Vec<SourceBatch>) -> Delta;
}
/// Async: a tokio task that steps the worker on each wake; same Core.
pub struct AsyncDriver { /* core, wake channel */ }
pub struct SourceBatch { product: ProductId, rows: Columns, diffs: Vec<Diff> }
pub struct Delta { /* changed rows per output ProductId at one Event */ }

// ---- effects (principles 6, 10) ----
pub enum Cadence { Once, Continuing }
pub trait IService {
    fn product(&self) -> ProductId;
    fn cadence(&self) -> Cadence;
    /// Every request of one frontier, at once. No per-row round trip.
    fn call(&mut self, at: Event, requests: &Columns);
    /// Answers ready now, as signed rows; never blocks.
    fn poll(&mut self, out: &mut Vec<SourceBatch>);
}
pub enum Transport { InProcess, SharedMemory, Wasm, Stdio }   // one contract, four carriers

// ---- storage (principle 7) ----
pub trait ByteStore {
    fn put(&mut self, key: Id, bytes: &[u8]);
    fn get(&self, key: Id) -> Option<bytes::Bytes>;
}
// impls: Memory, File, Sqlite (blob table). Backs both:
//   the input update log (capture: save = append; load = replay), and
//   arrangement chunk paging (the columnar `Chunk` spill hook).

// ---- locks and effects colocated (principle 11) ----
// `locks.rs`: every lock as `Ranked<T, RANK>`; acquire in rank order; clippy bans the rest.
// `effects/`: process, fs, net, env, clock, thread spawn; `IService`s are the only callers.
```

## 3. Instance timelines

| instance | created | read | dropped |
| --- | --- | --- | --- |
| `Core` | process start (server) or one-shot command | every program, every event | process exit |
| `Dict` | with `Core`; reloaded from the log | edges only (print, send) | with `Core` |
| a `Program` dataflow | first time its source is loaded | each `Event` the probe passes | its source is removed (dataflow dropped, traces released) |
| `IService` | when a program declares its relation | `call` per frontier, `poll` per step | when no program reads its relation |
| arrangement chunk | a batch merges | joins and reads | logical compaction merges it, or paged out past `Budget` |

## 4. Storage and the order of reads and writes

1. Load: `ByteStore` -> replay the update log into `inputs` at their recorded `Event`s; the
   dataflows rebuild arrangements (paged chunks fetched on read).
2. Per event: a source (editor save, a `IService::poll` batch, a timer) becomes a `SourceBatch` at
   the next `Event`; append it to the log first, then feed the input.
3. Settle: step until the probe passes the `Event` (sync) or on wake (async); `Budget` holds new
   events while epochs in flight are at the cap.
4. Compaction: advance logical compaction to the oldest `Event` a reader still needs; old chunks
   merge, cold ones page to `ByteStore`.

Uniqueness: one `Id` per byte string (hash, collision checked); one `Sid` per `Id` per process;
one input session per source product; one arrangement per (relation, key) in `Shared`.

## 5. redux, thematically

`crates/redux` (hafley-rs; from hafley-games at `80ded24f`) is the pure host-side API: `Slice`
(`reduce(state, event, cx, fx) -> output`), `Never` for proven-pure slices, combinators `Then`,
`MapEffect`, `Zoom`/`Lens`, `Each`, a `Pool`/`Handle` arena, `Store`/`Sub` (scan plus subject).
In v9 it is the face a game or app writes against; a slice's events become `SourceBatch`es and its
subscriptions read `Delta`s. The core under it is the dataflow above.
