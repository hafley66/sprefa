# Reactive style port — design doc

Snapshot of a planning session 2026-07-19, after a filter pass on
2026-07-20. Body = what v6 adopts. Appendix = comparison tables that
informed the filter, preserved for the rationale trail.

## 1. Your reactive style (from `hafley-rxjs/packages/signals/src/`)

| trait | source locus | consequence |
|---|---|---|
| `$` is callable + BehaviorSubject + Observable + meta-stream + React hook | `1_SignalCreator.ts:163-243` | one object, many faces, two stacked Proxies |
| Lazy child-signal creation | `1_SignalCreator.ts:262-267` | `target[p] ??= createProxy([...path, p])`; subtree materializes on access |
| Lazy BS-method binding | `1_SignalCreator.ts:182-217` | BehaviorSubject surface arrives when first method is touched |
| `shareReplay({refCount:true, bufferSize:1})` default | `1_SignalCreator.ts:202-206` | cold until subscribed, warm while subscribed, cold on last unsub |
| Computed signal with dynamic dep graph | `1_SignalCreator.ts:287-396` | Solid-style; deps re-discovered each run |
| Stack-scoped dep collector | `1_SignalCreator.ts:299-309` | `dependencyCollectors.push/pop`; active memo owns reads |
| Memo survives transient throw | `1_SignalCreator.ts:319-335` | last good value kept; throw is not an Observable error |
| Last unsub tears down | `1_SignalCreator.ts:386-393` | refcount 0 → clear deps, mark dirty |
| Read override | `1_SignalCreator.ts:105` | `read?: () => T` injects lazy reads |
| Numeric-prefix file ordering | every file | `0_types → 1_SignalCreator → 2_Signal → 3_Endpoint → 4_Query → 5_Route → 6_Storage` |

## 2. Mapping table (RxJS concept → v6 Rust target, field citations)

| RxJS / JS concept | Rust target in v6 | field precedent |
|---|---|---|
| `BehaviorSubject<T>` | port = subject; `@next` carry table (`engine/mod.rs:112`) | RxJS, every UI framework |
| `combineLatest` | rule join (SQL fixpoint) | every SQL planner |
| cold `Observable<T>` | rel as cold observable; subscription activates cone | Materialize SUBSCRIBE, RxJS cold |
| `share({refCount:true})` | refcount on `(root, query rel)` handle | RxJS refcount, Materialize |
| `take(1)` | CLI one-shot query | Materialize one-shot |
| `.$` meta events | `effect_log`, `query_log`, `events.jsonl` | Kafka, OpenTelemetry |
| static dep cone | from lowered Program | every SQL planner, differential dataflow |
| fixpoint | semi-naive over SQLite | Soufflé, Flix, Datomic |
| endpoints (file watch, HTTP, MCP, LSP, timer, CLI, hook) | one trait, N impls | Materialize SOURCES, Flink connectors |

## 3. What v6 does NOT adopt (with rationale)

| idea | rationale |
|---|---|
| Proxy `$` callable accessor | Rust has no Proxy; derive-macro cost outweighs ergonomic gain |
| Solid-style runtime dynamic dep tracking | static cones are cheaper; standard at corpus scale |
| Circuit-as-universal-IR framing | no system unifies files/queries/typechecks; petgraph as graph rep is enough |
| `@virtual` annotation on rels | materialized SQL views are field-proven; add annotation only on profiling evidence |
| Per-row origin column | OpenTelemetry spans at tick + event level cover the debug need; row-level is research-grade |
| leptos_reactive, DBSP, differential-dataflow, datafusion | RAM or scope mismatch with the corpus vision |

The v6 crate-map and the demand plan stand. No new reactive runtime dep.
StreamExt is the operator vocabulary at the type level; SQL via
sea-query does the work.

## 4. What changes about the demand plan

Nothing. The filter pass confirms:

- cold rels + refcount subscriptions = v6 demand.md as written
- static cones = demand.md line 35
- SQL pushdown via sea-query = crate-map.md as written
- petgraph for graph storage = crate-map.md as written
- event log via `events.jsonl` + `tracing` = already in v5

Open todos carried from demand.md unchanged:

- unsubscribe cache policy (warm cache vs TTL vs immediate)
- cross-repo demand propagation (deferred to phase 2)
- watcher as standing sub to source rels vs fully cold extraction

---

# Appendix

Comparison tables generated during the planning session. Preserved for
the rationale trail, not as a to-do list.

## Appendix A: Reactive runtime candidates

| crate | sync/async | dep tracking | maps to |
|---|---|---|---|
| `tokio::sync::watch` | async | none | BehaviorSubject only |
| `futures::stream` | async | none | cold Observable only |
| `leptos_reactive` | sync | dynamic, Solid-style | `createComputedSignal` exact match |
| `sycamore_reactive` | sync | dynamic, Solid-style | same |
| `reactive_signals` | sync | dynamic | same |
| hand-roll on `parking_lot::RwLock` + `SmallVec` | sync | you own it | the original Fork B proposal |

## Appendix B: SQL as IR — prior art

| stance | example systems | shape |
|---|---|---|
| SQL is the IR | Spark, Trino, dbt | everything lowers to SQL; backends are SQL engines |
| Relational algebra is the IR; SQL is one backend | DataFusion, Materialize, DuckDB | physical plan in IR; SQL or columnar execution |
| SQL is the only backend, IR is for storage choice | Soufflé, Flix, Datomic | datalog IR; SQL or in-memory as a per-rel annotation |

v6 picks stance 1 (sea-query builder, dialect as config).

## Appendix C: Materialization annotation — prior art

| system | syntax | what it marks |
|---|---|---|
| Postgres | `CREATE MATERIALIZED VIEW v AS ...` | explicit, separate keyword from view |
| Snowflake | `CREATE MATERIALIZED VIEW ...` + auto-refresh policy | same |
| Soufflé | `relation` (materialized) vs `inline` (computed per use) | annotation on the relation |
| Flix | no annotation; everything is re-derived per fixpoint | none |
| Datomic | `:db/alters` attribute on attribute | per-attribute |
| Differential Dataflow / Materialize.io | every rel is a stream; materialization is a side-channel ("arranged") | implicit per operator |
| Prolog tabled LP | `:- table foo/2.` directive | per predicate |

## Appendix D: Virtual vs materialized (if ever adopted)

| aspect | today (v6 demand plan) | if most rels are virtual |
|---|---|---|
| source rels | materialized | materialized (facts must persist) |
| derived rels | materialized, cold until subscribed | virtual unless `@materialize` |
| reads | SELECT from a table | re-compute via in-memory IR, or lower to a SQL query on demand |
| cone activation | tick writes rows for active rels | tick may not write at all; iterator produces rows |
| storage cost | one table per rel | one table per source rel + explicit `@materialize` rels |

## Appendix E: Events-first model — what would change

| aspect | current v6 | events-first |
|---|---|---|
| what triggers a tick | watcher fires, poll timer | an Event enters the system |
| what a subscription owns | `(root, query rel)` refcount | `(root, query rel, origin_event)` |
| every row knows | nothing | its originating EventId chain |
| diagnostic output | static | answers "why did this fire?" via origin trace |
| replay | impossible | event log can replay from any point |

Filtered out: event log stays at the `events.jsonl` + `tracing` span
level; row-level origin is research-grade and dropped.

## Appendix F: Origin tracking — prior art

| system | mechanism | granularity |
|---|---|---|
| Materialize.io | every SUBSCRIBE emit carries the timestamp/trigger | per row emit |
| Datomic | transaction log is the source of truth; `tx` entity on every assertion | per transaction |
| OpenTelemetry | trace context propagates across service boundaries | per span |
| Buneman why-provenance | polynomial-time witness for a derived tuple | per tuple |
| Apache Atlas / OpenLineage | dataset + job lineage | per job run |
| v5 today | `eventlog::emit` + `tracing` spans, not queryable from dl | per phase |

## Appendix G: Endpoint ingestion — prior art

| endpoint | mode (pull/push) | existing in v6 as |
|---|---|---|
| file watcher | pull | `notify` integration, daemon poll |
| timer / cron | pull | `every(N)`, `clock(N)` |
| LSP open / close | push | `textDocument/didOpen` (becomes a demand sub) |
| MCP tool call | push | `--mcp` serving |
| HTTP route | push | `@serve` in language-interfaces plan |
| CLI gesture | push | one-shot `dl run` |
| agent hook | push | `dl --hook` ingest (writes `hook_event`) |
| external broadcast | push | a `dl setup`-registered webhook |

## Appendix H: Strict lazy vs log-first lazy

| model | shape | cost |
|---|---|---|
| strict lazy | event with no active subscriber drops on the floor | minimal storage; no replay; can't debug "what happened" |
| log-first lazy | every event appended to `_event` first, subscribers see new events from their subscribe time, replay optional | event log grows; needs retention; full debuggability |

## Appendix I: Circuit framing — prior art

| system | what is the circuit | lang |
|---|---|---|
| Differential dataflow (McSherry) | operators over `(data, time, diff)` triples | Rust |
| Materialize.io | SQL queries as differential circuits | Rust |
| DBSP | streaming SQL circuit engine | Rust, MIT |
| Noria | pre-Materialize, partial stateful circuits | Rust |
| DataFusion | SQL plan as DAG of physical operators | Rust |
| Polars | lazy frames as DAG | Rust |
| timely-dataflow | differential's substrate | Rust |
| Apache Flink | stream processing DAG | JVM |
| Apache Calcite | relational algebra plan | JVM |
| Verilog / VHDL | hardware circuits (the source of the metaphor) | HDL |
| Arrows (Hughes) | circuit of arrow combinators | Haskell |
| Reflex FRP | time-varying values as a circuit | Haskell |
| Solid / leptos signals | fine-grained dep graph as circuit | JS / Rust |

## Appendix J: Circuit concept mapping (HDL analog)

| your term | circuit term | HDL analog |
|---|---|---|
| source rel | primary input | input pin |
| derived rel | combinational logic gate | AND/OR/XOR |
| `@next` carry | clocked state element | D flip-flop |
| `every(N)` | clock signal | oscillator |
| subscription | enable pin | chip select |
| unsubscribe | disable pin | tri-state |
| tick | one clock edge evaluation | propagation |
| fixpoint | circuit settling | quiescent state |
| materialized rel | stateful node | register |
| virtual rel | stateless node | wire |
| event | signal transition | edge |
| origin | signal source | driver |
| cone(Q) | backward slice to inputs | fan-in cone |
| closure(edge) | feedback path | combinational loop (SCC-resolved) |
| stratification | topological order to break cycles | scheduling |
| demand | observability gate | output pad |
| cross-repo cut | subcircuit boundary | hierarchical boundary |

## Appendix K: StreamExt translation table

| proposed `NodeKind` | StreamExt / existing primitive | what's missing |
|---|---|---|
| `Source { rel }` | `stream::iter(rows)` / `tokio::sync::watch::Receiver` | nothing |
| `Project { expr }` | `StreamExt::map` | nothing |
| `Filter { pred }` | `StreamExt::filter` | nothing |
| `Extract { op }` | `StreamExt::flat_map` | nothing |
| `Aggregate { fn, group }` | `StreamExt::scan` + `TryFold`, or `chunks` + custom | windowing policy is yours |
| `State { initial }` | `tokio::sync::watch::Sender` (BehaviorSubject) | nothing |
| `Clock { interval }` | `tokio_stream::wrappers::IntervalStream` | nothing |
| `Sink { kind }` | `SinkExt::send` / `StreamExt::for_each` | nothing |
| `Join { keys }` | `StreamExt::zip` (1:1 only), `StreamMap` (fan-in, no join key) | N-way equi-join on key |
| `Closure { edge_rel }` | `stream::unfold` over a worklist | delta-tracking, termination |
| `Fixpoint { body }` | none in std | knaster-tarski loop with delta frontier |
| `Combine` (combineLatest) | `StreamExt::zip` is strict; needs `select_with_strategy` + state | last-value-buffer per input |

StreamExt covers ~70%. The gap is the database-shaped parts: joins with
keys, fixpoint with delta, last-value buffers. Those are what SQL gives
you natively.

## Appendix L: RAM constraint math (per-repo)

| approach | per-repo resident | fits corpus |
|---|---|---|
| full circuit per repo, all rows in memory | rows(repo) ~ MB to GB | no |
| full circuit per repo, rows on disk, only active cone resident | cone(Q) ~ KB to MB | yes |
| circuit definition only (no rows), all rows in SQLite | ~10KB per circuit | yes |
| no per-repo state, just SQL tables | 0 + SQLite cache | yes |
| differential-dataflow / timely per repo | worker thread + arrangement traces, MB-min | no |
| dbsp per repo | operator state, MB-min | no |
| datafusion context per repo | ~10MB baseline | no |
| leptos reactive runtime per repo | signal graph, KB-MB | yes (small) |

The RAM constraint is what kills the heavy reactive runtimes and what
favors SQL pushdown.

## Appendix M: Library lab (RAM-conscious)

| crate | what | per-repo RAM | async? | adopt shape |
|---|---|---|---|---|
| `futures::stream` / `StreamExt` | the operator vocabulary | as big as the consumer pulls | yes | type-level reference for `NodeKind` |
| `tokio_stream::StreamMap` | fan-in keyed stream | bounded by buffer | yes | join input stage |
| `tokio::sync::watch` | BehaviorSubject (1 latest) | tiny | yes | `@next` carry, ports |
| `tokio::sync::broadcast` | replay-N subject | bounded buffer | yes | root `$` accessor analog |
| `parking_lot::RwLock` + `SmallVec` | sync reactive primitive | tiny | no | Solid-style `Computed` |
| `petgraph::stable::StableGraph` | the IR graph | nodes + edges (KB) | no | crate-map already adopts |
| `sea-query` | SQL builder, no driver | none (builds strings) | no | crate-map already adopts |
| `rusqlite` | SQLite driver | file-backed, cache size config | no | current driver |
| `sqlx` | async driver alternative | file-backed | yes | bench todo in crate-map |
| `lru` / `moka` / `quick-cache` | bounded cache | hard cap configurable | both | cone cache, dep cache |
| `roaring` | compressed bitmap | tiny for sparse sets | no | row-id sets, set intersection |
| `rayon` | parallel iterator | zero per-repo | no | already in sprefa |
| `leptos_reactive` | Solid-port signals | per-signal, KB-MB | no | drop-in reactive core |
| `sycamore_reactive` | same | same | no | same |
| `differential-dataflow` | full circuit runtime | traces per operator, MB-min | no | heavy, conflicts with RAM |
| `timely-dataflow` | differential substrate | worker process | no | heavy |
| `dbsp` | circuit-model SQL | operator state | both | closest fit, heavy |
| `datafusion` | SQL query engine | context ~10MB baseline | both | too big per repo |
| `polars` | lazy frame DAG | bounded | both | single-shot, not reactive |

## Appendix N: The actual gap (what no off-the-shelf Rust library covers)

| missing thing | where it lives today | what to do |
|---|---|---|
| N-way equi-join on key | SQL only, or hand-rolled hash join | push to SQL (crate-map already does) |
| combineLatest (last-value buffer per input) | hand-rolled on `watch` receivers | ~50 lines |
| Fixpoint with delta frontier | semi-naive literature, no Rust crate | hand-rolled; v5 already has `rebuild_derived` |
| Demand propagation (cancel backward) | actor model; no std primitive | `Arc<Subscription>` + RAII |
| Materialization as a node property | database literature only | `@virtual` / `@materialize` annotation |
| Origin tracking per row | provenance literature; no Rust crate | `_row_origin` table |
| Event ingestion endpoints | each endpoint is one crate (axum, notify, rmcp) | one trait, N impls |

Nothing on this list is a single library you can import. Everything is
either "use StreamExt + hand-roll the gap" or "adopt a heavy runtime
that breaks the RAM budget."

## Appendix O: Filter pass (PROVEN / RATIONAL / SPECULATIVE)

| idea | tier | seen in | keep? |
|---|---|---|---|
| Cold observable + refcount subscription | PROVEN | RxJS, Reactive Streams, Materialize SUBSCRIBE | yes |
| BehaviorSubject (single value, replay-1) | PROVEN | RxJS, every UI framework | yes |
| combineLatest | PROVEN | RxJS, Svelte stores, MobX | yes |
| Static dependency cone for demand | PROVEN | every SQL query planner, differential dataflow | yes |
| Fixpoint eval with semi-naive frontier | PROVEN | Soufflé, Flix, Datomic, every datalog | yes |
| Materialized views (rel persists in SQL) | PROVEN | Postgres, Snowflake, BigQuery, Oracle | yes |
| SQL as one backend, dialect as config | PROVEN | Calcite, Flink, Beam, Trino | yes |
| Event log + trace IDs (origin) | PROVEN | Kafka, OpenTelemetry, Materialize | yes |
| Pushdown to SQL, keep RAM bounded | PROVEN | every OLAP engine | yes |
| Endpoints as ingestion trait | PROVEN | Materialize SOURCES, Datomic tx log, Flink connectors | yes |
| StreamExt / RxJS operators as IR vocabulary | PROVEN | Calcite, Flink, DataFusion, Polars | yes |
| petgraph for graph storage | PROVEN | adopted in v6 crate-map | yes |
| sea-query builder for SQL gen | PROVEN | crate-map adoption | yes |
| Proxy `$` callable accessor | SPECULATIVE | only hafley-rxjs; Rust has no Proxy | drop for Rust |
| Solid-style runtime dynamic dep tracking | RATIONAL | SolidJS (JS only); no Rust impl at corpus scale | drop unless needed |
| Circuit as universal IR | SPECULATIVE | no system unifies these; differential is closest | drop the unification pitch |
| `@virtual` annotation | RATIONAL | Soufflé `inline` closest; no adoption in your space | drop unless profiling demands |
| Per-row origin column | RATIONAL | provenance literature; production uses spans | drop row-level, keep span-level |
| leptos_reactive as the runtime | RATIONAL | leptos ships, but for UI not corpus scale | drop unless v6 needs UI |
| DBSP / differential as the runtime | RATIONAL | proven at Materialize, but RAM-heavy per repo | drop, RAM kills it |
| Adopting datafusion | RATIONAL | big, brings its own planner | drop, conflicts with sea-query |
| Files-as-circuits framing | SPECULATIVE | novel | drop the framing, keep petgraph |
| Cross-repo demand propagation | RATIONAL | Materialize does cross-source; not common | keep, but defer to phase 2 |

## Appendix P: Three forks (original analysis, pre-filter)

The fork question is moot after the filter pass: v6 plan stands, no new
reactive runtime dep. Preserved for the rationale trail.

### Fork A (inside `sprefa-engine`): static cone, no new types

Honor crate-map.md line 50. Demand plan as written. Type sigs:

```rust
// sprefa-engine/src/demand.rs

pub struct Cone {
    pub root: RelId,
    pub rels: FxHashSet<RelId>,
}

pub struct ConeUnion {
    pub root: Root,
    pub rels: FxHashSet<RelId>,
    pub cross_repo_cut: FxHashSet<RepoId>,
}

pub struct Subscription {
    id: SubId,
    root: RelId,
    registry: Arc<Mutex<SubscriptionRegistry>>,
}

pub struct SubscriptionRegistry {
    refcounts: FxHashMap<(Root, RelId), usize>,
    cones: FxHashMap<(ProgramDigest, RelId), Arc<Cone>>,
    unions: FxHashMap<Root, ConeUnion>,
}

impl Engine {
    pub fn tick(&mut self, at: &RepoRev, demand: &ConeUnion) -> Result<TickReport>;
}
```

Read / write sequence:

```
subscribe(Q):
  registry.lock()
  refcounts[(root,Q)] += 1
  if first subscriber for Q:
    cone = cones.entry((prog_digest, Q)).or_insert(Cone::of(prog, Q))
    unions[root].add_cone(&cone)
  writer_thread.notify()
  return Subscription { ... }

unsubscribe(Q) [via Drop]:
  registry.lock()
  refcounts[(root,Q)] -= 1
  if refcount == 0:
    unions[root].remove_cone(&cones[(prog_digest, Q)])
    writer_thread.notify()

writer thread loop:
  loop {
    wait_on_notify()
    for root in dirty_roots(): engine.tick(at, &registry.demand(root))?
  }
```

Uniqueness: `(Root, RelId)` refcount unique; `(ProgramDigest, RelId)`
cone immutable per digest; `(Root)` ConeUnion derived.

### Fork B (new crate alongside v6): dynamic dep, Solid-style

Bypass the no-runtime ruling by being the runtime. Type sigs:

```rust
// sprefa-reactive/src/lib.rs

pub struct Cold<T> {
    produce: Arc<dyn Fn(&mut Observer<T>) + Send + Sync>,
}

pub struct Behavioral<T> {
    cur: RwLock<T>,
    observers: RwLock<SmallVec<[Arc<Observer<T>>; 4]>>,
}

pub struct Computed<T> {
    inner: Mutex<ComputedInner<T>>,
}

struct ComputedInner<T> {
    value: Option<T>,
    dirty: bool,
    running: bool,
    last_run_failed: bool,
    subscribers: SmallVec<[Arc<Observer<T>>; 4]>,
    dep_subs: SmallVec<[Subscription; 8]>,
    compute: Box<dyn Fn() -> T + Send + Sync>,
}

thread_local! {
    static DEP_COLLECTORS: RefCell<Vec<CollectorHandle>> = RefCell::new(Vec::new());
}
```

Filtered out: dynamic dep tracking not adopted at corpus scale.

### Fork C (standalone port of `hafley-rxjs`)

Direct port with derive-macro for nested signal access. Pick a Proxy-gap
solution: derive macro, path lens, or type-erased `Value`.

Filtered out: no Rust Proxy; derive-macro cost exceeds ergonomic gain
for a Rust library.

## Appendix Q: Cone pick per fork (the static-vs-dynamic question)

| fork | cone pick | cost | precision | what breaks |
|---|---|---|---|---|
| A | static | free (declare-time graph) | low (branched queries keep dead rels warm) | nothing in v6 plan |
| B | dynamic | per-compute collector + re-arm | high (matches the JS style) | crate-map "no reactive runtime" ruling |
| C | dynamic | same as B + derive macro | same as B | standalone, no v6 integration |

Static cone (Fork A) wins the filter pass: cheaper, standard, what
every field-proven system uses at corpus scale.
