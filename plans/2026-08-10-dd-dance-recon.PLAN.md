# DD dance reconnaissance: plan term and Rust proof slice

Plain-words twin: `plans/2026-08-10-dd-dance-recon.PLAN.visual.human.unga.md`.

## Context

The goal is a narrow proof: Prolog emits a DD-shaped plan, then emits a Rust
implementation of that plan, preserving the oracle's ordered tick log. This
does not place `differential-dataflow`, `timely`, or `dbsp` in an emitted or
runtime path. The supplied implementation vocabulary is signed delta batches,
indexed state, semi-naive within-tick closure, consolidation, retraction, and
epoch ticks.

The existing compiler has a target-neutral boundary. `program_plan/2` makes the
plan once, `lower_program/2` produces `lowered/8`, and the current backend is a
printer over that result (`v6/prolog/compile/PIPELINE.md:16-30`,
`v6/prolog/compile/PIPELINE.md:80-84`). The lower term currently combines
plain structural terms with SQLite SQL text (`v6/prolog/lower.pl:1-27`).

The oracle specifies an ordered tick: ordered outside arrivals, per-occurrence
edge processing, set/log boundary deltas, next-tick carry, and engine-owned
empty drain ticks capped at 100 (`v6/prolog/conformance/engine.pl:5-33`,
`v6/prolog/conformance/engine.pl:92`). Its tick implementation performs
arrival absorption, level closure before edges, ordered occurrence processing,
retention, a second closure, boundary differencing, then carry construction
(`v6/prolog/conformance/engine.pl:462-495`).

## Decisions

1. Name the target choreography **the signed semi-naive arrangement dance**.
   One epoch accepts a signed batch, indexes the changed and retained rows,
   evaluates only work reached by the batch, consolidates equal keys by signed
   weight, repeats recursive work to a fixed point, then publishes the ordered
   boundary delta.
2. Emit `dd_plan/6` beside `lowered/8`. It contains target-neutral operators
   and ordering data. SQLite text remains only in the existing `lowered/8`
   path.
3. The first Rust proof uses `Vec` batches and `BTreeMap` arrangements, a
   synchronous tick function, and in-memory state. Persistence, SQL execution,
   asynchronous scheduling, and external DD crates are outside this proof.
4. The pilot fixture is
   `retraction_only_tick_retracts_level_view`: one level rule, one add tick,
   one all-retraction tick, and both addition and removal of the derived view
   (`v6/prolog/conformance/fixtures/engine_core.pl:318-329`).
5. The acceptance gate is byte comparison of the existing JSONL tick log. Final
   state is retained as a fixture assertion but is not sufficient evidence.

## 1. The dance and construct map

`TICK-MODEL.md` supplies the semantic planes: B for set/level state, N for
occurrence/refcount state, and Z for the signed boundary stream
(`v6/prolog/compile/TICK-MODEL.md:32-46`). Level closure is explicitly a least
fixed point in B (`v6/prolog/compile/TICK-MODEL.md:43-46`). The current lowerer
already materializes delta, frontier, next-frontier, refcount, expand, DRed,
scope, and average-accumulator planes (`v6/prolog/lower.pl:1025-1093`).

| V6 construct | DD counterpart | planned Rust representation | receipt |
|---|---|---|---|
| `_sign` delta rows | Z-set weight on a timely epoch batch | `Vec<(Row, i64)>`, consolidated by row key | `_sign` is staged as `-1`/`1` (`v6/prolog/lower.pl:3650-3675`); Z is the tick-log ring (`TICK-MODEL.md:39-41`) |
| `frontier` / `next_frontier` planes | current and next semi-naive worksets | two `Vec<Row>` batches plus key-indexed deduplication | names (`v6/prolog/lower.pl:178-185`); both stage sites (`v6/prolog/lower.pl:3670-3675`) |
| DRed ping/pong/cone | retract/rederive worksets | `BTreeMap<Row, i64>` plus alternating work vectors | plane family (`v6/prolog/lower.pl:1054-1066`); DRed plan constructor (`v6/prolog/lower.pl:3810-3885`) |
| `refcount` | arranged support multiplicity | `BTreeMap<Row, i64>` | refcount update and zero collection (`v6/prolog/lower.pl:3647-3655`) |
| `scope` | keyed reduce domain | `BTreeSet<GroupKey>` | scope is a level-plane family (`v6/prolog/lower.pl:1074-1093`) |
| `avg_accumulator` | reduce state `(sum, count)` per group | `BTreeMap<GroupKey, (f64, i64)>` | accumulator plane (`v6/prolog/lower.pl:1075-1080`); signed update (`v6/prolog/lower.pl:3337-3356`) |
| drain cap | epoch-progress safety cap for empty carry ticks | `const DRAIN_CAP: usize = 100` | `drain_cap(100)` and overflow branch (`v6/prolog/conformance/engine.pl:92`, `:611-617`) |
| tick | DD logical timestamp / epoch | monotonic `u64` passed only inside the runtime | grades place level work at `+0` and carry/finalize at `+1` (`v6/prolog/compile/TICK-MODEL.md:83-99`) |

The common core is delta application, indexed lookup, semi-naive work, fixed
point termination, and consolidation. The V6 moves without a direct DD core
operator are ordered occurrence processing, keyed last-write-wins replacement,
the log-occurrence stamp, retention policy, and the formatter's byte spelling.
They remain explicit runtime operators. DD vocabulary absent from the present
V6 lower term includes an explicit operator graph, key projections for every
join, and topologically numbered per-operator schedule entries.

## 2. Proposed `dd_plan` term

```prolog
dd_plan(Name,
        rels([rel(Ref, Columns, Kind)]),
        arrangements([arr(Id, Ref, KeyColumns, ValueColumns, Weight)]),
        operators([op(Id, map(...)), op(Id, filter(...)), op(Id, join(...)),
                   op(Id, reduce(...)), op(Id, iterate(InnerOps))]),
        wires([wire(From, To, delta)]),
        tick_order([phase(absorb), phase(level_before_edges), phase(edges),
                    phase(iterate), phase(consolidate), phase(boundary),
                    phase(carry)])).
```

`rels` names row shapes and B/N storage kind. `arrangements` records the exact
key columns and value columns used by a relation index. `operators` is the
lowered graph. `wires` identifies each delta input/output edge. `tick_order`
serializes the runtime order rather than letting a backend derive one.

The current `plan/9` has declarations, relation plans, arrival targets,
`RuleOrder`, edge rules, subscribed relations, and intern mode
(`v6/prolog/lower.pl:5558-5559`). `lowered/8` adds arrival, edge, level, and
delta statement lists (`v6/prolog/lower.pl:1-20`). The missing DD fields are:

| missing field | existing source that can provide it | transformation into `dd_plan` |
|---|---|---|
| relation columns, types, kind, and keyed columns | `RelPlans` carried in `plan/9` and read by `relplan_*` lowering sites (`v6/prolog/lower.pl:5558-5572`) | emit `rel/3` and arrangement candidates |
| positive/negative body uses and join bindings | `edge_delta_project_sql/11` receives trigger, other, pre, negative, and guards (`v6/prolog/lower.pl:2945-2983`) | emit `map`, `filter`, and keyed `join` nodes before SQL rendering |
| level operator inputs and rule order | `RuleOrder` reaches `level_statement_groups/4` (`v6/prolog/lower.pl:3038-3059`) | emit ordered level subgraph entries |
| recursive seed, hop, and stop tests | `level_expand_plan/5` (`v6/prolog/lower.pl:3724-3783`) and `level_fixpoint_ir/5` (`v6/prolog/lower.pl:4051-4076`) | emit `iterate` body and its arrangement keys |
| departure versus arrival wiring | trigger kind selects the correct frontier (`v6/prolog/lower.pl:2945-2982`) | emit source port and phase constraint |
| schedule order | current emitted runtime has an ordered 12-phase sequence (`plans/2026-08-10-rust-emit-recon.PLAN.md:138-153`) | emit `tick_order/1` directly |

The SQL fragments are insufficient as the source of this graph because they
hide join keys in text. The predicates above still hold the structured input
before the string is built.

## 3. Emitted Rust shape and size

The pilot output needs one shared kernel and one generated program module.

```rust
type Weight = i64;
type Batch<Row> = Vec<(Row, Weight)>;

struct Arrangement<K, V> { rows: BTreeMap<K, BTreeMap<V, Weight>> }
struct Runtime { tick: u64, drain_count: usize, /* per-rel arrangements */ }

fn run_tick(rt: &mut Runtime, arrivals: Batch<Row>) -> TickLogLine {
    // absorb arrivals; update arrangements; derive frontier
    // repeat the plan's iterate nodes until the work batch is empty
    // consolidate; compute ordered boundary add/del; carry next-tick work
}
```

The generated module defines row enums/structs, static `DdPlan` data, operator
functions, arrangement-key projections, and fixture schedule data. `BTreeMap`
is selected for deterministic key traversal; the tick-log printer still performs
the contract's explicit lexical sort.

| material | rough source lines | scope |
|---|---:|---|
| minimal runtime kernel | 260-360 | row weights, BTreeMap arrangements, consolidation, fixed-point loop, boundary formatter, drain cap |
| generated pilot module | 90-140 | `source_row` and `mirror` rows, one map/identity level operator, schedule, static plan |
| generic `dd_plan` printer | 180-260 | Prolog term emission plus deterministic snapshot test |
| Rust emitter beyond pilot | 300-450 | render graph/operator data and program-specific projections |

The emitted tick is synchronous. The existing Rust reconnaissance records a
synchronous serialized executor and places asynchronous channels/streams at
server and host boundaries (`plans/2026-08-10-rust-emit-recon.PLAN.md:29-46`,
`:188-215`). No persistence belongs to this proof slice.

## 4. Pilot slice and milestone ladder

The fixture has one level rule, `mirror(Item) <- source_row(Item)`, then
`+source_row(alpha|beta)` followed by both retractions. Its required sequence is
`+mirror(alpha|beta)` at tick 1 and `-mirror(alpha|beta)` at tick 2
(`v6/prolog/conformance/fixtures/engine_core.pl:321-329`).

| milestone | output | price | gate |
|---|---|---:|---|
| A | compiler prints deterministic `dd_plan` for the pilot | 180-260 Prolog lines and one golden | exact term snapshot, compiler test command |
| B | one hand-written Rust module consumes the plan shape | 350-500 Rust lines including kernel and fixture module | oracle JSONL vs Rust JSONL byte diff |
| C | Rust emitter renders the pilot module from `dd_plan` | 300-450 Prolog lines plus generated-file snapshot | regenerated Rust compiles, then the same byte diff |

Milestone B deliberately freezes the plan shape before general emitter work.
The test protocol runs the normal oracle and compares the Rust program's stdout
to its output. The existing pipeline identifies that exact diff as the grade
(`v6/prolog/compile/PIPELINE.md:125-133`).

## 5. Verification and log contract

Rust prints one JSON object per tick, LF terminated, with exactly this envelope:

```json
{"tick":N,"deltas":{"relName":{"add":[[...]],"del":[[...]]}}}
```

The contract requires ascending relation names, omission of empty relations,
an explicit empty `deltas` object for no-op ticks, relation-argument row order,
lexical sorting inside `add` and `del`, compact JSON, and LF endings
(`v6/prolog/conformance/ticklog.pl:1-19`). `ticklog.pl` increments tick numbers
while printing (`v6/prolog/conformance/ticklog.pl:65-74`) and builds the
canonical relation/row serialization (`v6/prolog/conformance/ticklog.pl:78-176`).

No field in the envelope is SQL-flavored. The implementation-facing names
`DDL`, `BoundarySql`, `__delta_*`, `__frontier_*`, and `__refcount` are lowerer
details and do not enter stdout. The target-neutral respelling is therefore
limited to the Rust formatter using `add`, `del`, row arrays, and canonical JSON
rules, never table names or SQL strings.

## Verification

Run the oracle for the pilot:

```sh
swipl -q -l v6/prolog/conformance/ticklog.pl \
  -g "emit(retraction_only_tick_retracts_level_view)" -g halt
```

The hand-written and emitted Rust paths each write their JSONL to a file, then:

```sh
diff -u oracle.jsonl rust.jsonl
```

The compiler path additionally snapshots the `dd_plan` Prolog term, compiles
the generated Rust, and repeats the same diff. The retraction tick is required
in every gate; final `mirror/1 = []` alone loses ordering and sign evidence.

## Staffing

Implementation owner: one Rust/Prolog agent in a dedicated worktree. Base SHA:
`b3c2b711`. Suggested sequence: A, human review of the term shape, B, C.
Suite budget: one fixture-specific compiler snapshot, one Rust compile, and two
oracle-to-Rust byte diffs per change; broad corpus sweep is deferred until the
single-fixture emitter exists.
