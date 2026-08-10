# Rust emitter and runtime reconnaissance

## Context

This is read-only reconnaissance. The base is `07b81db7`; no runtime or compiler
code changes are in scope. The standing ruling keeps TypeScript as the core engine
and places Rust emitters in a later arc
(`v6/prolog/conformance/rulings.pl:670-674`). The immediate user goal is a single
binary, with generated TypeScript and Rust types as an incremental first slice.

The current compiler already describes its backend boundary: Prolog owns every
decision, lowering yields SQL text plus plain structure, and another backend is a
second printer over the same lowered term
(`v6/prolog/compile/PIPELINE.md:14-30`,
`v6/prolog/compile/PIPELINE.md:80-84`). The current TS emitter is 2,799 lines,
`lower.pl` is 5,688 lines, `runtime/*.ts` is 3,568 lines, and `serve/*.ts` is
2,388 lines, measured with `wc -l` at `07b81db7`. These are source masses, not
port estimates.

One brief source is absent at this base: `docs/v6-rel-catalog-emitters.md` does
not exist. The live row authority is `catalog_all_rows/10`, which combines
declaration and plane rows (`v6/prolog/lower.pl:830-841`), and the emitted TS
catalog type is `IRelCatalogRow` (`v6/tsv2/runtime/types.ts:376-414`). The first
types slice cannot claim parity with the schema-emit lane until its row-spec doc
or implementation lands.

## Decisions

1. Treat the emitted program object and every helper it calls as the Rust runtime
   contract. Do not infer the contract from `types.ts` alone.
2. Keep SQL construction and plan construction in Prolog for the compiled-Rust
   path. Rust receives generated static plan data and executes it.
3. Spell the Rust tick hot path as a synchronous serialized executor. Use async
   channels and streams at server, watcher, timer, subprocess, and HTTP edges.
4. Use generated `.d.ts` and `.rs` declarations from catalog rows as the first
   Rust-emission slice.
5. Price Bun compilation as the near-term single-binary path, then compiled Rust
   per program, then a generic plan interpreter. This ordering concerns dependency
   and implementation size only.
6. Reuse V5 infrastructure by behavior and dependency, not by importing V5 engine
   semantics.

Rejected alternatives for this recon: porting RxJS operator syntax literally into
the synchronous tick hot path; using V5's evaluator as the V6 evaluator; generating
types separately from OpenAPI and catalog rows; requiring SWI-Prolog in a deployed
Rust runtime after plans have been compiled.

## 1. Emitted contract

### Program object

The stable core is `name`, `internMode`, DDL, relation columns and types, arrival
targets, and `tick(seam, arrivals) -> Observable<ITickDeltas>`
(`v6/tsv2/runtime/types.ts:470-480`). The emitter adds boot statements, final
selects, host/bind/query plans, subscribed relations, the catalog, unsupported
execution modes, and the tick function (`v6/prolog/emit_ts.pl:2641-2661`). The
served runtime declares the same extended object (`v6/tsv2/runtime/types.ts:700-710`).

Rust equivalents:

```rust
pub struct Program {
    pub name: &'static str,
    pub intern_mode: InternMode,
    pub ddl: &'static [&'static str],
    pub rel_columns: &'static [RelColumns],
    pub rel_column_types: &'static [RelColumnTypes],
    pub arrival_targets: &'static [&'static str],
    pub boot: &'static [BootStatement],
    pub final_select: &'static [NamedSql],
    pub host_plans: &'static [HostPlan],
    pub bind_plans: &'static [BindPlan],
    pub query_plans: &'static [QueryPlan],
    pub subscribed_rels: &'static [&'static str],
    pub rel_catalog: &'static [RelCatalogRow],
}

pub fn tick(program: &Program, sql: &mut SqlRunner, arrivals: &[ArrivalRow])
    -> Result<TickDeltas>;
```

The generated Rust may implement `Program::tick` directly or provide static plan
tables to a shared runtime. The result contract remains relation deltas plus
`carry_pending`; carry causes empty drain ticks and is capped at 100
(`v6/tsv2/runtime/types.ts:81-92`, `v6/tsv2/runtime/tickLoop.ts:30-66`).

### SQL seam

Generated TS touches SQLite only through `ISqlSeam { db, runner,
unobserved_rels? }` (`v6/tsv2/runtime/types.ts:55-67`). The actual emitted code
uses these runner operations:

| operation | emitted use | Rust contract |
|---|---|---|
| `execute(db, SqlStatement)` | tick counter, projections, guards, selects | execute one statement and return named rows |
| `batch(db, SqlStatement[])` | arrival/write batches and ordered commits | ordered transactional or runner-defined batch |
| `executeMultiple(db, sql)` | DDL/recompute/promote blocks | execute a multi-statement SQL string |
| `scalar(db, sql)` | recursive/fixpoint counts | return one scalar number |

Receipts are `v6/prolog/emit_ts.pl:1029`, `v6/prolog/emit_ts.pl:1491`,
`v6/prolog/emit_ts.pl:1718`, `v6/prolog/emit_ts.pl:1986`,
`v6/prolog/emit_ts.pl:2076-2079`, and `v6/prolog/emit_ts.pl:2461-2463`.
`SqlStatement` must carry SQL and bound arguments. Row conversion must preserve
the six declared column tags while the wire value is string, number, or boolean
(`v6/tsv2/runtime/types.ts:17-31`).

Candidate Rust storage dependencies:

| candidate | contract fit |
|---|---|
| `rusqlite` with `bundled` | synchronous connection and statement API; V5 already pins this shape at `Cargo.toml:84` |
| `libsql` | useful if remote/libSQL operation becomes a required backend; requires an adapter preserving the four runner operations |
| `sqlx` SQLite | async driver; server-edge candidate, while the ruled sync writer thread still needs serialization |
| `sea-query` | builder for future Rust-owned SQL; the current emitted plan already owns SQL text, so it is outside the first execution slice |

### Plan tables and planes

The generated module imports and calls these shared surfaces
(`v6/prolog/emit_ts.pl:168-243`):

| surface | emitted data/calls | Rust obligation |
|---|---|---|
| `IncrementalRuntime` | relation, edge, level, retention, expand and DRed plans | implement every ordered tick phase below |
| `TextPlane` | `ITextInternPlan`; one batch normalization before storage | collect distinct text values, intern and lookup, rewrite rows |
| `StructPlane` | type plans and ref-column map | validate shape, canonicalize JSON, intern children first, rewrite refs to ids |
| `SubscribeCone` | mode plus relation/edge/level/retention/boot filters | filter static plans once from subscribed relations |
| `select_rows` | boundary/final SQL and column types | execute and convert named SQL rows to ordered row arrays |
| `multiset_diff` | naive referee boundary diff | multiset difference preserving duplicate counts |
| ordered helpers | `intern_then_execute`, `stage_ordered_frontiers` | preserve ordered occurrence semantics and carry |

`IIncrementalRuntime` has 12 required methods: `prepare_tick`, `apply_arrivals`,
`apply_edges`, `apply_levels_before_edges`, `recompute_levels_before_edges`,
`merge_next_into_current`, `apply_levels_after_edges`, `apply_retention`,
`recompute_levels_after_edges`, `read_boundary`, `stage_departures`, and
`promote_frontiers`. The exact signatures and plan inputs are at
`v6/tsv2/runtime/types.ts:267-324`.

The incremental phase order is executable contract:

1. clear/stage tick state;
2. advance `now/1` when used;
3. text interning;
4. struct reference normalization;
5. arrivals;
6. pre-edge level growth and reconciliation;
7. edges;
8. merge next frontier and post-edge level growth;
9. retention;
10. post-edge reconciliation;
11. boundary read and departure staging;
12. frontier promotion and `carry_pending`.

The emitted chain is at `v6/prolog/emit_ts.pl:2503-2558`. Text must precede
struct normalization (`v6/prolog/emit_ts.pl:328-365`). Subscribe pruning keeps
derivation only in the cone, keeps storage for the cone plus arrival targets,
and never prunes DDL (`v6/tsv2/runtime/3_subscribe.ts:12-18`,
`v6/tsv2/runtime/3_subscribe.ts:47-80`).

The catalog is a compile-time constant carrying declaration and plane rows. Its
row includes identity, containment, ordinal, kind, type, arity, module, and three
hashes (`v6/tsv2/runtime/types.ts:376-414`). The emitter constructs the const from
the complete catalog row set (`v6/prolog/emit_ts.pl:764-779`).

## 2. Target-neutral and TS-specific layers

| layer | classification | receipt | Rust treatment |
|---|---|---|---|
| parser, expansion, support checks | target-neutral compiler | compiler plan is built once (`v6/prolog/compile/PIPELINE.md:32-66`) | retain in SWI-Prolog initially |
| stratification and rule order | target-neutral compiler | two explicit orderings (`v6/prolog/compile/PIPELINE.md:68-78`) | serialize result |
| SQL, DDL, boot, plan building | target-neutral for SQLite targets | lowered output has SQL and plain structure (`v6/prolog/compile/PIPELINE.md:80-114`) | reuse the lower terms |
| schedule and arrivals | target-neutral data | ordered duplicates are meaningful (`v6/tsv2/runtime/types.ts:33-53`) | Rust vectors/slices |
| tick phase order | target-neutral semantics | emitted chain (`v6/prolog/emit_ts.pl:2503-2558`) | sync executor |
| relation/edge/level/DRed plan shapes | target-neutral data with SQL dialect content | `v6/tsv2/runtime/types.ts:94-264` | generated Rust structs or serialized IR |
| catalog rows | target-neutral data | `v6/prolog/lower.pl:830-866` | source for schema and types |
| oracle and tick-log diff | target-neutral executable spec | byte diff is the grade (`v6/prolog/compile/PIPELINE.md:125-133`) | run Rust output through same envelope |
| RxJS `Observable` return types and `pipe` composition | TS-bound execution spelling | emitted imports (`v6/prolog/emit_ts.pl:191-241`) | replace tick chain with `Result`; streams at edges |
| `process.env` emitter/subscription mode | TS/Node-bound configuration | `v6/tsv2/runtime/3_subscribe.ts:47-50` | Rust config/env parsing |
| Node filesystem watcher | Node-bound effect source | `node:fs/promises` iterator and abort teardown (`v6/tsv2/serve/2_binds.ts:205-220`) | `notify` plus Tokio channel/stream |
| SWI subprocess compiler and dynamic `.ts` import | Node-bound loader | `v6/tsv2/serve/0_compile.ts:97-113` | compile-time subprocess for paths A/C; plan decode for B |
| transform-types entry | Node-bound launch | `v6/tsv2/serve/main.ts:1-4` | absent from Rust; Bun parses TS itself |
| HTTP, timers, subprocess hosts | current implementation TS-bound; protocol/effect semantics portable | live engine merges submitted batches (`v6/tsv2/serve/3_engine.ts:93-133`) | `axum`, `tokio`, `tokio::process` candidates |

SQL is target-neutral between TS and Rust only while both use SQLite and the same
parameter/result conventions. It is not database-dialect-neutral. The catalog row
construction and schema hashes are compiler facts; the concrete DDL and queries
remain SQLite text.

## 3. Rx in Rust

The tick is serialized. The formal model calls it one state-plus-writer step and
places within-tick level closure in a least fixpoint (`v6/prolog/compile/TICK-MODEL.md:32-46`).
The served TS engine enforces serialization with a `Subject` and `concatMap`
(`v6/tsv2/serve/3_engine.ts:93-107`). The schedule fold calls one tick at a time
and adds empty drain ticks only while carry remains (`v6/tsv2/runtime/tickLoop.ts:39-67`).

Rust mapping:

| location | spelling | candidate crates |
|---|---|---|
| generated `tick` and SQLite fixpoint | synchronous `fn -> Result<TickDeltas>` on one writer thread | `rusqlite`, `thiserror` or `anyhow` at binary boundary |
| queue of external arrivals | bounded `tokio::sync::mpsc`; one owner drains it and calls sync tick | `tokio` |
| output subscribers | `tokio::sync::broadcast` or per-request `oneshot`, selected by delivery semantics | `tokio` |
| HTTP and host subprocesses | async tasks that submit batches; they do not execute ticks concurrently | `axum`, `tokio::process` |
| file watch | watcher callback to bounded channel; cancellation by dropping receiver/watcher | `notify`, `tokio-stream` |
| schedule replay CLI | plain iterator/loop | standard library |

`futures::Stream` and `tokio-stream` fit long-lived external event sources. They
do not need to appear in emitted tick code. A direct port using an RxRust crate
would preserve operator names while adding a dependency and cancellation model
to a phase chain whose database operations are serialized already.

Repository law maps directly: async becomes stream/channel-driven Rx behavior at
HTTP, timer, watch, and subprocess edges; sync stays a synchronous tick/fixpoint
on the writer thread. The V6 sync/async ruling already states server async, tick
hot path sync, and no await inside a fixpoint (`v6/.agents/skills/v6-plan/SKILL.md:25-31`).

## 4. V5 overlap

| V5 asset | receipt | transfer |
|---|---|---|
| bundled SQLite dependency | `Cargo.toml:84` | dependency/features and build/link behavior |
| driver seam and row/value aliases | `src/db.rs:2-25` | connection setup, parameter conversion, row extraction patterns |
| SQL timing/tracing | `src/db.rs:87` | statement spans and slow-SQL fields |
| bulk parameter budgeting | `src/storage.rs:123-133` | chunking and SQLite bind-limit handling |
| structured trace subscriber and Chrome trace | `src/trace.rs:33-62`, `src/trace.rs:95-120` | tracing facade, subscriber/layer setup, flush behavior |
| process scheduling caps | `src/daemon/budget.rs:18-43`, `src/daemon/budget.rs:136-158` | thread, CPU, QoS, and IO cap implementation |
| fixpoint row budget | `src/engine/derive.rs:108-112` | named cap and failure shape |
| tick instrumentation | `src/engine/tick.rs:222-232` | span placement and tick report pattern |

The semantic engine does not transfer as an implementation. V5's typed plan
classifies rules by operator class and components (`src/engine/typed_plan.rs:88-130`),
while V6's emitted contract distinguishes log/set relations, arrival/delta/current/
next/departure planes, edge and level statements, retention, expand, DRed, and
boundary deltas (`v6/tsv2/runtime/types.ts:94-264`). V5 also contains its own
delta-table fixpoint machinery (`src/engine/derive.rs:1655-1671`), but it has no
contractual V6 tick grading or the emitted 12-phase frontier sequence.

Reuse therefore means extracting or copying the storage, tracing, and budget
idioms into a V6 runtime crate, with V6-specific plan structs and tick execution.

## 5. Deleted Rust shootout lab

The 255-line lab proved that Prolog can specialize a Rust source file from rule
facts. It extracted relation and variable names from a fixed two-rule reachability
program (`e0faba55^:v6/prolog/labs/emit_rust_shootout/emit_rust.pl:6-23`), emitted
Rust type aliases and state (`.../emit_rust.pl:51-74`), and generated the
semi-naive delta join from bound variable names (`.../emit_rust.pl:224-255`). It
also emitted deterministic count/checksum receipts and two unit tests
(`.../emit_rust.pl:161-221`).

Its boundary was a specialized benchmark:

- one hard-coded two-rule program;
- `u32` rows only;
- positive recursion only;
- in-memory `FxHashMap`/`FxHashSet` storage;
- file loading and benchmark JSON in generated `main.rs`;
- no SQLite runner seam, catalog, boot, arrivals, edge ticks, keyed replace,
  negative bodies, aggregates, retention, text/struct planes, serving, reload,
  subscriptions, or oracle tick-log parity.

It proved printing and specialization mechanics. It did not prove compatibility
with `lowered/8` or the current emitted contract.

## 6. Types codegen first

### Exists

- Dense primitive rows for `text`, `int`, `float`, `bool`, and `json`
  (`v6/prolog/lower.pl:1309-1315`).
- Nested list rows whose `type_id` points to the element type
  (`v6/prolog/lower.pl:1317-1363`).
- Relation rows with arity, containment, module, schema hash, and rule hash
  (`v6/prolog/lower.pl:1385-1403`).
- Column rows with 1-based ordinal and `type_id`, constructed by
  `catalog_column_rows/9` (`v6/prolog/lower.pl:1397-1400`).
- The emitted catalog const (`v6/prolog/emit_ts.pl:764-779`).
- Runtime column tags and wire values (`v6/tsv2/runtime/types.ts:17-31`).
- A prior OpenAPI lab that emits JSON in memory and can diff without writing;
  it reports 229 lines for the emitter (`plans/2026-07-30-openapi-codegen-lab.md:327-341`).

### Missing

- The brief's row-spec file at this base.
- A ruled mapping for nullability/optional fields. Current `IRowValue` excludes
  null (`v6/tsv2/runtime/types.ts:17-17`).
- Naming and namespace rules for modules, nested relations, Rust keyword escapes,
  and duplicate local names under different parents.
- Ownership mapping: `String` versus borrowed text, `Vec<T>` versus slices, and
  relation references as ids versus expanded wire structs.
- A public/catalog distinction defining which relation rows become exported types.
- Constraint annotations needed for JSON Schema/OpenAPI facets.
- Emitter modules, checked-in outputs, staleness gates, compile gates, and fixtures.

### First slice price

| item | rough source size | gate |
|---|---:|---|
| shared catalog tree decoder and name resolver in Prolog | 80-140 lines | fixtures for primitive, list, nesting, module collision |
| `.d.ts` printer | 80-140 lines | `tsc`/`tsgo` consumes output; inline golden |
| `.rs` printer | 100-180 lines | `rustc` or fixture crate consumes output; `cargo fmt --check` on fixture |
| mapping/refusal fixtures | 100-180 lines | same catalog rows produce both outputs; unknown kind/type refuses by name |
| staleness script and checked-in examples | 40-80 lines plus artifacts | regenerate and byte-diff |

Total: roughly 400-720 source lines plus generated fixtures. First milestone:
one module containing scalars, `list(text)`, one nested relation, and one relation
reference emits compiling `.d.ts` and `.rs` from the identical catalog row list.
SWI-Prolog is compile-time only.

## 7. Single-binary paths

### A. Prolog emits Rust; Cargo builds program plus runtime

```text
.dl6 -> swipl -> program.rs + static plans -> cargo build -> one program binary
```

Steps and rough sizes:

1. Rust equivalents of catalog and plan structs: 500-900 lines.
2. Synchronous SQL runner plus row conversion: 500-900 lines, using `rusqlite`
   initially; `libsql` remains an adapter candidate.
3. Text and struct planes: 500-900 lines.
4. Incremental tick executor, including expand/DRed and naive referee:
   1,800-3,000 lines. The TS behavioral reference is 1,439 lines in
   `runtime/1_incremental.ts`, while generated TS also carries specialized logic.
5. Emitter printer and generated-program glue: 700-1,300 lines.
6. Replay, sweep, and byte-parity harness: 300-600 lines.
7. Optional served runtime: 1,500-2,500 lines across `axum`, Tokio channels,
   `notify`, and subprocess hosts.

Runtime/compiler subtotal: about 4,300-7,600 lines without serving, or
5,800-10,100 with current serve capabilities. Generated `.rs` size is program
dependent and excluded.

Blocks today: no Rust plan types, runner, V6 tick executor, current emitter over
`lowered/8`, or Rust parity lane. The deleted lab does not consume current lower
terms. First milestone: one arrival plus one positive level rule compiles to Rust,
runs DDL/boot/ticks with `rusqlite`, and produces byte-identical tick JSON to the
oracle. SWI-Prolog is required at build/compile time, absent at runtime. Each
program requires a Cargo build and produces its own binary.

### B. Generic Rust runtime interprets program data

```text
.dl6 -> swipl -> versioned plan data -> generic rust-runtime <plan-data>
                                      -> same binary for every program
```

Steps and rough sizes:

1. Versioned, lossless plan schema covering every current plan field:
   600-1,000 lines including codecs and validation.
2. SQL runner and planes: 1,000-1,800 lines, shared with A.
3. Generic tick interpreter: 2,000-3,500 lines.
4. Plan loader, schema-version refusal, hashes, and diagnostics: 400-700 lines.
5. Compiler serializer and parity fixtures: 400-800 lines.
6. Optional serving: 1,500-2,500 lines.

Subtotal: about 4,400-7,800 lines without serving, or 5,900-10,300 with serving.
The principal additional work relative to A is a stable, exhaustive plan wire
format. The payoff is one runtime binary and program data that can change without
relinking.

Blocks today: `fixpoint_ir` is still `unknown` in the TS contract
(`v6/tsv2/runtime/types.ts:194-203`), generated code contains specialized helper
functions beyond static plan arrays, and there is no versioned serialized form.
First milestone: serialize the plan for the same one-arrival/one-level fixture,
load it into a generic binary, and match the oracle. SWI-Prolog remains necessary
to compile `.dl6` into plan data. It is absent from runtime when precompiled data
is shipped. If the binary accepts raw `.dl6`, it must spawn or embed a Prolog
compiler until that compiler is replaced.

### C. Keep TS core; compile it with Bun

```text
served TS entry + runtime + fixed generated program -> bun build --compile
                                                   -> one executable
```

Bun's official executable documentation states that `bun build --compile`
bundles imported files, packages, and the Bun runtime into one executable and
supports macOS, Linux, and Windows targets
([Bun single-file executable](https://bun.sh/docs/bundler/executables)). This is
a packaging path over the current engine rather than a Rust-runtime dependency.

Steps and rough sizes:

1. Add a fixed-program entry that statically imports one emitted program:
   30-80 lines.
2. Add build script/config and asset declaration: 30-80 lines.
3. Probe `@libsql/client`, RxJS, Node fs watch, subprocess, dynamic import, and
   linked `sprefa-store-engine` under the compiled executable: 100-250 lines of
   tests/receipts.
4. Choose compilation mode:
   - per-program executable with generated module statically imported; or
   - executable containing the TS runtime plus a compilation/data-loading door.
5. Package required Prolog compiler files and SWI, or keep compilation outside
   the executable.

First probe: roughly 160-410 source/test lines. Blocks today: the live compiler
writes a content-addressed `.ts` file and dynamically imports its filesystem path
(`v6/tsv2/serve/0_compile.ts:97-113`); the production package links a local store
engine and uses `@libsql/client` and RxJS (`v6/tsv2/package.json:22-27`); no Bun
compatibility or compiled-executable receipt exists; Node's watch source and
subprocess lifecycle need an end-to-end cancellation test.

First milestone: compile one fixed emitted program and the replay runner into a
single executable, execute a conformance schedule, and byte-match the oracle.
Then compile the served entry and test HTTP, host subprocess, interval, watch,
shutdown, and file-backed SQLite.

SWI-Prolog choices:

- per-program Bun executable: compile-time only;
- generic packaged TS server accepting raw `.dl6`: runtime SWI subprocess remains
  because `ProgramCompiler` explicitly spawns it (`v6/tsv2/serve/0_compile.ts:47-113`);
- generic server accepting precompiled generated modules/data: absent at runtime,
  provided dynamic loading is replaced by a bundle-compatible data/module door.

## Sequencing

| milestone | output | dependency |
|---|---|---|
| 0 | Bun fixed-program compile receipt | current TS code only |
| 1 | shared `.d.ts` and `.rs` catalog type emission | schema-emit row contract lands or is recovered |
| 2 | Rust runner seam and one level-rule parity | M1 types plus current lowering |
| 3 | full plan structs and serialized plan experiment | M2 |
| 4A | per-program Rust binary parity sweep | full Rust tick runtime |
| 4B | generic Rust plan-data binary parity sweep | stable plan wire format |
| 5 | served Rust effects and protocols | tick runtime parity first |

## Verification

Every semantic slice uses the existing oracle envelope. The compiler pipeline
defines a byte diff of oracle and emitted tick logs as the grade
(`v6/prolog/compile/PIPELINE.md:125-133`). Required gates:

1. Generated `.d.ts` parses and typechecks.
2. Generated `.rs` parses, compiles, and has deterministic snapshot output.
3. Identical catalog row input feeds both declaration emitters.
4. Rust tick logs byte-match the Prolog oracle fixture by fixture.
5. Naive and incremental Rust modes agree where both are supported.
6. Drain cap, ordered duplicate arrivals, keyed replacement, departures,
   retention, recursion, DRed, text interning, struct refs, and catalog reload
   each have a named fixture.
7. Bun executable runs without Node or Bun installed in the test environment;
   the test records executable size, startup, SQLite persistence, watch teardown,
   subprocess cancellation, and compiler availability.
8. `scripts/verify.sh` remains green before any implementation commit.

## Staffing

Recon author: Codex in worktree `chore/rust-emit-recon`, base `07b81db7`.
Implementation is split by milestone. One agent/worktree per emitter, Rust runtime,
or packaging probe; no shared-source concurrent edits. Suite budget: targeted
fixture during iteration, the full V6 conformance/sweep gate before merge, then
`scripts/verify.sh`. This document contains no implementation authorization.
