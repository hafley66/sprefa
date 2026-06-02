# DBSP (Feldera) as a reuse target for sprefa v5's incremental-retraction layer

Source read: `~/projects/ext/feldera/crates/dbsp` (the embeddable `dbsp` crate; not the
`crates/adapters` server). Paths below are relative to that crate unless noted.

DBSP = Database Stream Processor. Write a query as if over a full dataset; the engine runs it
incrementally so a change costs time proportional to the change, not the dataset
(`src/lib.rs:1-7`). Theory: Budiu/Chajed/McSherry/Ryzhyk/Tannen VLDB23.

Two-stage embedder model (`src/tutorial.rs:30-48`): (1) build a circuit defining inputs,
operators, outputs; (2) any number of times, push input changes, run the circuit, read output
changes.

---

## 1. Core types

### Weight / Z-set (`src/algebra/zset.rs`)

```rust
pub type ZWeight = i64;                          // zset.rs:39  the standard integer weight
pub type DynZWeight = DynWeightTyped<ZWeight>;   // zset.rs:43

pub type OrdZSet<K>          = OrdWSet<K, DynZWeight>;              // zset.rs:46
pub type OrdIndexedZSet<K,V> = OrdIndexedWSet<K, V, DynZWeight>;   // zset.rs:56
```

A Z-set is conceptually a set of `(key, weight)` tuples; an indexed Z-set is `(key, value, weight)`
(`src/algebra/zset.rs:1-12`). The weight is an integer; **positive = insertion, negative =
deletion, magnitude = multiplicity** (`src/tutorial.rs:151-158`). So a change to a multiset is
just a Z-set, and `+1`/`-1` *are* insert/retract. The "Z" is the ring of integers; batches are
generic over weight type `R` so weights can be any abelian group, but in practice `i64`
(`src/algebra/zset.rs:32-40`).

Record types must satisfy `DBData` (`src/trace.rs:98-113`): `Default + Clone + Eq + Ord + Hash +
SizeOf + Send + Sync + Debug + ArchivedDBData (rkyv) + IsNone + 'static`. Weights satisfy
`DBWeight: DBData + MonoidValue` (`src/trace.rs:177`). In examples this is a derive stack of
`Clone, Default, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, SizeOf, Archive, Serialize,
rkyv::Deserialize, serde::Deserialize, IsNone` plus `#[archive_attr(derive(...))]`
(`examples/tutorial/tutorial3.rs:9-30`). DBSP ships `Tup0..Tup10` because the orphan rule blocks
implementing its traits on std tuples (`src/tutorial.rs:291-296`).

### Stream (`src/circuit/circuit_builder.rs:700`)

```rust
pub struct Stream<C, D> {            // C = circuit type, D = value (usually a Z-set/batch)
    stream_id: StreamId,
    local_node_id: NodeId,
    origin_node_id: GlobalNodeId,
    circuit: C,
    val: RefStreamValue<D>,          // at most one value: circuits are synchronous
}
```

`Stream` is `Clone` (cheap; shares `val`). Operators are methods on `Stream` that wire a new node
into the circuit and return a new `Stream`.

### Circuit / Runtime / handles (re-exported in `src/lib.rs:102-117`)

```rust
pub use circuit::{ ChildCircuit, Circuit, CircuitHandle, DBSPHandle,
                   NestedCircuit, RootCircuit, Runtime, Stream, ... };
```

- `RootCircuit` is `ChildCircuit<()>`; `NestedCircuit` is the child built inside `recursive`.
- `RootCircuit::build` (`src/circuit/circuit_builder.rs:3068`) builds a **single-threaded**
  circuit:

  ```rust
  pub fn build<F, T>(constructor: F) -> Result<(CircuitHandle, T), DbspError>
  where F: FnOnce(&mut RootCircuit) -> Result<T, AnyError>;
  ```

  It spins a current-thread tokio runtime internally (`circuit_builder.rs:3088`); the engine's
  operator scheduling is async under the hood but the embedder API is synchronous.

- `Runtime::init_circuit` (`src/circuit/dbsp_handle.rs:641`) builds a **multi-worker** circuit and
  returns a `DBSPHandle`:

  ```rust
  pub fn init_circuit<F, T>(config: impl Into<CircuitConfig>, constructor: F)
      -> Result<(DBSPHandle, T), DbspError>
  where F: FnOnce(&mut RootCircuit) -> Result<T, AnyError> + Clone + Send + 'static,
        T: Send + 'static;
  ```

  `config` can be a worker count (`init_circuit(1, ...)` or `init_circuit(4, ...)`); the
  constructor closure runs once per worker thread, data is sharded round-robin across workers, and
  outputs are per-worker (hence `consolidate()` to merge).

### Input / output handles (`src/operator/input.rs`, `src/operator/output.rs`)

```rust
pub struct ZSetHandle<K> { ... }                          // input.rs:81
pub fn push(&self, k: K, w: ZWeight);                     // input.rs:114
pub fn append(&self, vals: &mut Vec<Tup2<K, ZWeight>>);   // input.rs:118  (drains the vec)

pub struct OutputHandle<T>(Arc<OutputHandleInternal<T>>); // output.rs:240
pub fn consolidate(&self) -> T;                           // output.rs:348  merges worker outputs
```

`add_input_zset` / `add_input_indexed_zset` return `(Stream, Handle)` (`src/operator/input.rs:389,
418`). There are also set-semantics (`SetHandle`) and upsert (`MapHandle`) input variants.

---

## 2. Minimal end-to-end embedder program

Smallest one that builds, feeds, steps, and reads output is the tutorial filter, `tutorial3.rs`
(quoted verbatim, `examples/tutorial/tutorial3.rs:31-69`):

```rust
fn build_circuit(
    circuit: &mut RootCircuit,
) -> Result<(ZSetHandle<Record>, OutputHandle<OrdZSet<Record>>)> {
    let (input_stream, input_handle) = circuit.add_input_zset::<Record>();
    let subset = input_stream.filter(|r| {
        r.location == "England" || r.location == "Northern Ireland"
            || r.location == "Scotland" || r.location == "Wales"
    });
    Ok((input_handle, subset.output()))
}

fn main() -> Result<()> {
    // Build circuit.
    let (circuit, (input_handle, output_handle)) = RootCircuit::build(build_circuit)?;

    // Feed data into circuit.  Each row paired with weight +1 = insert.
    let mut input_records = Reader::from_path(path)?
        .deserialize()
        .map(|result| result.map(|record| Tup2(record, 1)))
        .collect::<Result<Vec<Tup2<Record, ZWeight>>, _>>()?;
    input_handle.append(&mut input_records);

    // Execute circuit (one transaction = one logical clock tick).
    circuit.transaction()?;

    // Read output from circuit.
    println!("{}", output_handle.consolidate().weighted_count());
    Ok(())
}
```

The four steps are: `RootCircuit::build` (returns `CircuitHandle` + your handle tuple) →
`input_handle.append(&mut vec_of_(row, weight))` → `circuit.transaction()` →
`output_handle.consolidate()`. For incremental use you loop steps 2-4, each `append` carrying only
the delta (`tutorial9.rs` feeds 500 rows per `transaction()`, output carries `+1`/`-1` weights,
`examples/tutorial/tutorial9.rs:107-129`).

### Execution surface (`src/circuit/dbsp_handle.rs`)

- `transaction(&mut self)` — start + commit one tick, block until done (`dbsp_handle.rs:1282`).
- `start_transaction` / `step` / commit — a transaction is a sequence of `step()`s for one logical
  clock tick; the clock advances only between transactions (`dbsp_handle.rs:1300-1359`). `step`
  returns `bool` (commit complete). This split exists for bootstrap/replay and bounded per-step
  work.
- `CircuitHandle::transaction` / `step` are the single-thread equivalents
  (`src/circuit/circuit_builder.rs:7142, 7187`).

---

## 3. Retraction mechanics

Retraction is not a special path; it is a negative weight flowing through the same operators.

- **Insert vs delete**: a row appears in an input/output Z-set with weight `+n`/`-n`. Net contents
  = sum of weights per `(key,value)` (`src/operator/input.rs:438-444`). Updating a Z-set means
  adding a Z-set of changes.
- **`distinct`** (`src/operator/distinct.rs`) is where multiset weights collapse to set semantics:
  `stream_distinct` emits `(key,value,1)` for every tuple whose summed `weight > 0` and **drops
  `weight <= 0`** (`distinct.rs:9-24`). `distinct()` is the incremental version over a stream of
  changes (`distinct.rs:33-50`); `hash_distinct()` indexes by key hash for large keys
  (`distinct.rs:57-64`). This is the operator that turns "this row's weight went to 0" into an
  actual retraction in the output.
- **Consolidation**: `OutputHandle::consolidate()` (`src/operator/output.rs:348`) merges per-worker
  output batches and sums weights, so the reader sees one weight per row. `Trait Trace::consolidate`
  (`src/trace.rs:252`) merges all unmerged batches of a trace into one.
- **Trace compaction** (`src/trace.rs:225-270`): a `Trace` is the accumulated history an
  incremental operator (join, distinct, aggregate) keeps. `set_frontier(&Time)` declares that
  timestamps below the frontier are indistinguishable, so updates to the same `(key,value)` from
  different steps can be merged or discarded; **"compaction is performed lazily at merge time"**
  (`trace.rs:238-246`). `exert(&mut isize)` spends merge effort even without new input
  (`trace.rs:248-249`). `insert` blocks (async) if too many unmerged batches accumulate
  (`trace.rs:254-258`). A dirty flag tracks whether the trace changed since the last tick, used for
  fixed-point detection (`trace.rs:260-270`). So memory bound comes from frontier advancement
  letting old per-step deltas merge/cancel.

---

## 4. Operator surface relevant to sprefa

All are methods on `Stream` (statically typed wrappers in `src/operator/`; the dynamic dispatch
versions live in `src/operator/dynamic/`).

| Operator | Signature (file:line) | Notes |
|---|---|---|
| `filter` | `fn filter<F>(&self, F) -> Self` where `F: Fn(ItemRef)->bool` (`filter_map.rs:78`) | retains rows |
| `map` | `fn map<F,K>(&self, F) -> Stream<C, OrdWSet<K,..>>` (`filter_map.rs:88`) | per-row transform |
| `map_index` | `fn map_index<F,K,V>(&self, F) -> Stream<C, OrdIndexedWSet<K,V,..>>` (`filter_map.rs:100`) | re-key into indexed Z-set |
| `join` | `fn join<F,V2,V>(&self, &Stream<C,OrdIndexedZSet<K1,V2>>, F) -> Stream<C,OrdZSet<V>>` where `F: Fn(&K1,&V1,&V2)->V` (`join.rs:123`) | both sides indexed by `K1` |
| `join_index` | `... F: Fn(&K1,&V1,&V2)->It, It: IntoIterator<Item=(K,V)>` → `OrdIndexedZSet<K,V>` (`join.rs:185`) | join emitting any # of indexed rows |
| `join_flatmap` | `join.rs:151` | join then flat_map, no intermediate materialization |
| `plus` | `fn plus(&self, &Stream<C,D>) -> Stream<C,D>` (`plus.rs:56`) | Z-set addition ≈ SQL UNION ALL |
| `aggregate` | `fn aggregate<A>(&self, A) -> Stream<C,OrdIndexedZSet<K,A::Output>>` (`aggregate.rs:37`) | `Min`, `Max`, `Fold`, ... |
| `aggregate_linear` | tutorial4 | linear sum aggregation |
| `distinct` / `stream_distinct` | `distinct.rs:45 / 20` | set semantics, drops `w<=0` |
| `topk_desc` | `tutorial9` | top-k per group |
| `weighted_count` | `count.rs:24` | counts via weights |
| `inspect` | tutorial | side-effecting peek per batch |
| `output` | tutorial | terminate a stream into an `OutputHandle` |
| `delta0` | `delta0.rs:22` | import a parent-circuit stream into a child circuit |

### Recursion / fixpoint / transitive closure

`ChildCircuit::recursive` (`src/operator/recursive.rs:229`):

```rust
pub fn recursive<F, S>(&self, f: F) -> Result<S::Output, SchedulerError>
where S: RecursiveStreams<IterativeCircuit<Self>>,
      F: FnOnce(&IterativeCircuit<Self>, S) -> Result<S, SchedulerError>;
```

The closure receives a child (nested) circuit and the *previous iteration's* stream `S`, and
returns the *next* iteration's stream. DBSP runs the body to a fixed point (until the trace's dirty
flag stops changing). Parent streams enter the child via `.delta0(child_circuit)`.

Transitive closure (verbatim, `examples/tutorial/tutorial10.rs:45-70`):

```rust
let closure = root_circuit.recursive(
    |child_circuit, len_n_minus_1: Stream<_, OrdZSet<Tup4<usize,usize,usize,usize>>>| {
        let edges = edges.delta0(child_circuit);
        let len_1 = len_1.delta0(child_circuit);
        let len_n = len_n_minus_1
            .map_index(|Tup4(start,end,cum_weight,hopcnt)| (*end, Tup4(*start,*end,*cum_weight,*hopcnt)))
            .join(
                &edges.map_index(|Tup3(from,to,weight)| (*from, Tup3(*from,*to,*weight))),
                |_end_from, Tup4(start,_end,cw,hc), Tup3(_from,to,w)| Tup4(*start,*to,cw+w,hc+1),
            )
            .plus(&len_1);
        Ok(len_n)
    },
)?;
```

Key behavior matching sprefa's `reaches`/`closure`:
- A **cyclic** graph makes the naive closure **non-terminating** if the carried value keeps growing
  (cumulative weight + hopcount), `examples/tutorial/tutorial10.rs:28-37`. DBSP's fixpoint
  terminates only when the recursive relation stops changing.
- The fix is to aggregate to a canonical value inside the loop: `tutorial11.rs` adds
  `.aggregate(Min)` keyed on `(start,end)` so each node pair keeps only its shortest path, which
  makes the cyclic graph converge (`examples/tutorial/tutorial11.rs:45-74`). This is exactly
  sprefa's "SCC-condense / keep one representative" concern, expressed as an in-loop aggregate.
- Retraction propagates through the closure: tutorial10 step 2 removes one edge and the output is a
  Z-set of `-1` weights for every path that edge enabled (`examples/tutorial/tutorial10.rs:90-98`).
  That is incremental retraction of a transitive closure for free.

---

## 5. Memory: trace storage, spill, footprint, deps

### Spill-to-disk (the relevant differentiator)

DBSP has a real spill story, this is its production "Fallback" batch family
(`src/typed_batch.rs:566-575`): `FallbackZSet`, `FallbackIndexedZSet`, `FallbackKeyBatch`,
`FallbackValBatch` each wrap an enum `Inner::Vec(in-memory) | Inner::File(on-disk)`
(`src/trace/ord/fallback/wset.rs:34-56`).

Spill policy (`src/trace/ord/fallback/utils.rs:30-75`): a batch builds to one of `Memory`,
`Storage`, or `Threshold(bytes)`. The threshold comes from
`Runtime::min_step_storage_bytes()` (`src/circuit/runtime.rs:1193-1205`):

- `None` → no Runtime / storage disabled → stay in memory.
- `Some(0)` → memory pressure is **Critical** → spill *everything* to storage.
- `Some(N)` → spill batches whose size `>= N` to storage; smaller stay in memory.

So storage is **opt-in**: `init_circuit` takes a `CircuitConfig` and you must
`.with_storage(Some(StorageConfig/StorageOptions))` (`src/circuit/runtime.rs:1800, 1871-1986`;
`StorageOptions` from `feldera-types`). `RootCircuit::build` and `init_circuit(N, ...)` with no
storage config keep all traces in RAM. The storage stack is layered (`src/storage.rs:1-10`):
`backend` (block IO) → `buffer_cache` → `file` (data access), file format under
`src/storage/file/`. It raises the process fd limit on init (`src/storage.rs:38-49`).

Memory pressure feeds back into the spill threshold (`runtime.rs:1185-1205`), and `memory-stats` is
a direct dep. There is no single doc on "peak footprint"; the design intent is the incremental
cost bound (`src/lib.rs:33-38`) plus lazy trace compaction (`src/trace.rs:238-246`) plus
spill-when-pressured.

### Dependency weight

`Cargo.toml` (`crates/dbsp/Cargo.toml`):
- **94** workspace-pinned `[dependencies]` lines (grep `= { workspace`), plus several
  `[dev-dependencies]`.
- Heavy hitters: `tokio` (rt-multi-thread), `rkyv` (zero-copy serialization, mandatory on every
  record type), `rayon`-style `core_affinity`, `mimalloc-rust-sys` (bundled allocator),
  `crossbeam`, `petgraph`, `metrics`, `tracing`, `zip`/`snap`/`flate2`/`zstd` (compression),
  `roaring`, `fastbloom`, `clap`, plus five in-tree `feldera-*` crates (`feldera-types`,
  `feldera-storage`, `feldera-buffer-cache`, `feldera-ir`, `feldera-macros`, `feldera-samply`).
- It pulls a tokio runtime even for `RootCircuit::build` single-thread mode
  (`src/circuit/circuit_builder.rs:3088`).

This is a large transitive closure. It is a database engine, not a library you bolt on lightly.

---

## Verdict: adopting dbsp for sprefa v5

**What it would buy.** DBSP is exactly the formal model sprefa hand-rolls: Z-sets with `i64`
weights where `+1`/`-1` are insert/retract, incremental join/map/filter/aggregate, and a
`recursive` fixpoint that does incremental transitive closure *with* retraction (tutorial10 step 2
is sprefa's `--changed` edge removal, for free). The cyclic-non-termination problem sprefa hits in
`reaches`/`scc` is the same one DBSP documents and solves with an in-loop `aggregate(Min)`
(`tutorial11.rs`). The retraction layer sprefa maintains by hand (`retract_paths`, rev-aware
relation variants, per-tick N+1 discipline) is DBSP's native semantics. The trace-compaction +
spill-to-disk machinery (Fallback batches, `min_step_storage_bytes`, memory-pressure feedback) is
more sophisticated than sprefa's SQLite reliance and is genuinely production-grade.

**What it would cost.** Adopting DBSP means you **target a circuit, not SQL**. Today sprefa lowers
recursive rules to a SQLite fixpoint and gets SQLite's on-disk B-tree, mmap, and OS page cache as
the memory story essentially for free, including the 133MB-kernel case the engine already handles.
DBSP's equivalent (Fallback batches + storage backend) is **opt-in and off by default**:
`RootCircuit::build` and an unconfigured `init_circuit` keep *every* trace in RAM. To match SQLite's
spill behavior you must wire `StorageConfig`/`StorageOptions`, accept the fd-limit raise, and trust
the memory-pressure heuristic. So the claim "lose the SQLite-spill memory advantage" is precise:
you do not lose spill capability, but you lose the *free, always-on, well-understood* spill and
trade it for a config surface and a guess-based threshold (`utils.rs:62-66` literally guesses 32
bytes/item). Every record type must derive the `rkyv` + `SizeOf` + `IsNone` stack and use
`Tup0..Tup10` instead of native tuples, which is invasive across sprefa's value model
(`StringId`/`FileId`/`WhereBytesId` would all need the derive stack and ordering). The dep
footprint is ~94 direct workspace deps including a bundled allocator and a tokio runtime, versus
sprefa's current SQLite weld. And the engine surface (async scheduler under a sync API, per-worker
sharding, transaction/step state machine) is a second runtime to understand and debug.

**Fit against the two sprefa targets.**

- *Small-active-repo LSP set* (the live-editing path): **good fit, modulo RAM**. Working set is
  small, traces stay in memory comfortably, and DBSP's incremental retraction is precisely what an
  LSP wants when a file edit retracts and re-derives facts. The recursive operator gives reactive
  reachability/blast-radius queries with correct deletions. If the cost of the `rkyv`/`Tup` rewrite
  and the second runtime is acceptable, this is where DBSP shines and where sprefa's hand-rolled
  incremental layer is most redundant.

- *500-repo path* (bulk, the kernel-scale case): **poor fit without committing to DBSP storage**.
  At that scale the whole point of the SQLite weld is on-disk, OS-managed memory; DBSP only matches
  it if you enable and tune its storage backend, at which point you have replaced a battle-tested
  embedded database with a younger storage engine and inherited its config and failure modes. The
  in-memory default would blow up; the storage path is the unproven part of DBSP relative to SQLite.

**Recommendation shape (data, not decision):** DBSP is the right *semantics* and the wrong
*deployment* for sprefa's stated architecture. The cleanest reuse is conceptual, mine its operator
set and the in-loop-aggregate fixpoint pattern for sprefa's own lowering, rather than swapping the
SQLite execution layer for a circuit. A literal swap is justified only if sprefa decides the LSP
small-set path is primary and the 500-repo bulk path moves off the live engine.
