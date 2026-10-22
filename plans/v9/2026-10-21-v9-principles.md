# v9 principles: DD as the core, the compiler as dataflow

Context (Chris, 2026-10-21): v9 keeps the traits, drops eager `for`/`loop` passes, starts from
principles; DD is the IVM core; generics and specifics both; storage automated; comptime is the
server. Facts below are from `differential-dataflow 0.25.1` (the version `ivm-dd` pins,
`sqlite_ivm/crates/ivm-dd/Cargo.toml:10`) as vendored in `~/.cargo/registry`.

## What DD 0.25.1 already has

| capability | where in DD 0.25.1 | used by ivm-dd today |
| --- | --- | --- |
| columnar storage (trie of updates, columnar `BatchContainer`) | `src/columnar/` (`layout`, `updates`, `trie_merger`), header says "Experimental" | no (0 hits for `columnar` in `ivm-dd/src`) |
| arrangement chunks resident or paged out to a byte store | `src/columnar/trace` `ColChunk`, `trace::chunk::col::spill`; `examples/columnar_spill.rs` supplies an lz4 tempfile `BytesStore` | no |
| capture and replay a stream of updates as bytes | `src/capture.rs` | no |
| graph algorithms over collections | `examples/scc.rs`, `bfs.rs`, `pagerank.rs`, `graspan.rs` (a points-to static analysis) | no |
| iteration scopes with product time | `iterate`, `dynamic` (`src/dynamic`, `examples/dynamic.rs`) | ivm-dd has its own fixpoint driver |

## Principles

1. **Everything is a collection of `(data, time, diff)`.** No phase loops over a whole program.
   A phase is a dataflow built once; input is deltas. `for`/`loop` stays only inside operators.
2. **Time is the structure.** One product timestamp:
   `(event, comptime_round, macro_wave)`. An outside event (file save, http, git) advances
   `event`; comptime rounds and macrotime waves are nested iteration scopes. No hand-written
   fixpoint loop (`TERM_BUDGET` in `_9b_ir_eval.rs`, waves in `_8_waves.rs`).
3. **Diffs are the multiplicity.** Two producers of one row give weight 2; a reader that wants a
   set applies `distinct` (or `threshold`) where it reads. The `Held` refcount (learnings 3) and
   the `PresentInsert` refusal (learnings 1) both exist because ivm-dd forces set semantics at
   its input; DD's own model needs neither.
4. **Arrangements are the only shared state.** An index is built once and shared by every
   operator that reads it (and imported across dataflows). No Rust side maps holding rows
   (`answered`, `held`, `mapped`, `decoded` in `_9b_ir_eval.rs`).
5. **Rows are ids, bytes in columns.** Text interned once to an id; rows are fixed-width id
   tuples in columnar containers; text lives in one dictionary collection.
6. **Effects are only at the edges.** Executors are input sessions (sources) and inspect/capture
   (sinks). Nothing inside the dataflow calls out.
7. **Storage is the update log plus paged arrangements.** Save = capture the input update log;
   load = replay it. Large arrangements page through the `Chunk` spill hook into a byte store
   (sqlite blob table as that store). Schema needs no per-relation DDL: every relation is the
   same columnar row shape keyed by relation id.
8. **Generics are data.** A program's relations and rules are rows; the dataflow is built from
   rows by one interpreter of the rule IR (one operator shape per IR node), so a generic rule and
   a specific rule run through the same code.
9. **Measure before claiming.** Each step lands with a committed probe and a budget (see
   `2026-10-21-v9-scope.plan.md`, "Performance discipline").

## The compiler as graph algorithms

| compiler step today | as a dataflow over rows | DD shape |
| --- | --- | --- |
| read (characters to terms) | the one parse; a source of `syntax_*` rows (or rows from an editor, no parse) | input |
| macrotime (`macrotime/*.dl7`) | rewrite rules over `syntax_*` rows | iterate scope |
| stratify | SCC of the rule dependency graph, negation edges checked against components | `examples/scc.rs` |
| demand / reachability | which relations a query reaches | `examples/bfs.rs` |
| name binding, imports | joins over declaration and reference rows | join |
| type equality classes | connected components over "same type" edges | label propagation (`iterate`) |
| comptime rounds | iteration to the fixpoint | iterate scope |
| runtime | the same dataflow, `event` time advancing | outer timestamp |

## 10. Hosts and libraries plug in turnkey (Chris, 2026-10-21)

Goal: adding a Rust library or a host runtime in any language (TS, Python, Go, ...) is one
contract plus generated glue, over the most stable transport for that host.

Contract: a TypeSpec model (or this kit's own schema rows) declares each served relation:
request product, answer products, cadence (Once, Continuing). Codegen emits from it:

| output | for |
| --- | --- |
| `@std/<name>.dl7` products | the dl7 side (today hand-frozen, e.g. `std/ryi.dl7` from `5_facts.json`) |
| Rust `IService` stub | in-process Rust libraries |
| host adapter (TS, Python, Go) | a library in another runtime: decode requests, encode answer rows |
| a contract test | request rows in, answer rows out, same fixture for every transport |

Transport tiers, one contract over all of them (candidates; buy, not build):

| tier | transport | candidates | fits |
| --- | --- | --- | --- |
| 0 | in-process Rust | `IService` trait (exists) | Rust crates |
| 1 | shared memory, zero copy | Arrow C Data Interface (columnar buffers across languages); `iceoryx2` (zero-copy IPC) | high-volume answers (ryi rows) between processes |
| 2 | sandboxed plugin | WASM component model (`wasmtime`, WIT interfaces) | untrusted or portable hosts |
| 3 | child process, framed bytes | stdio JSONL (exists: ryi), Arrow IPC stream | anything with a CLI |

Columnar rows (principle 5) make tier 1 natural: a batch of answer rows is a set of id columns
plus a dictionary, which is the Arrow layout. Each tier gets one candidate-by-candidate library
analysis before code (`CLAUDE.md`, build vs buy).

## 11. Locks, effects, arenas, caches: declared in one place each (Chris, 2026-10-21)

Projection of each rule: mechanism (bought), what it costs, where it overreaches.

| rule | mechanism | cost | overreach to avoid |
| --- | --- | --- | --- |
| every lock declared in one file (`locks.rs`) with a rank; acquire only through it | clippy `disallowed-types` / `disallowed-methods` (`clippy.toml`) bans `Mutex`, `RwLock`, `parking_lot::*` constructors outside `locks.rs`; ranks give one acquisition order (no deadlock by construction); each acquire is a hafley-observe span | one module; a CI clippy gate | inside DD there is nothing to lock: timely workers own their state and talk by channels. Locks live only at the edges (executor tasks, caches, the daemon). If `locks.rs` grows, the core is sharing state it should not |
| every effect (process, fs, net, env, clock, thread spawn) in one colocation (`effects/`) | same clippy gate over `std::process::Command`, `std::fs::*`, `std::net::*`, `std::env::var`, `Instant::now`, `thread::spawn`; executors are the only callers | one crate or module tree; a gate | dependencies (`tokio`, `rusqlite`, `reqwest`) do their own syscalls; enumerate them as dependencies with the effects they may perform, not line by line |
| arenas used correctly | the interner is the one long-lived arena (ids for text and terms); DD's columnar containers are the row arena; a bump arena (`bumpalo`) only for read-phase scratch dropped per unit | no new structure | a second arena for rows duplicates DD's containers |
| caches used correctly | arrangements are the cache inside the dataflow; outside it, only at an effect edge (blob cache, checker hosts), bounded with eviction from a library (`quick_cache`, `moka`) | each edge cache declared next to its effect | a Rust-side cache of rows the dataflow already arranges is a second source of truth (the `answered`/`held`/`mapped` maps today) |
| constant RAM throughput | budgets per probe (principle 9); arrangement paging (principle 7) caps resident size | probes in CI | none: this is the measurement rule |

Existing precedent: hafley-rs gate allowlists for `static` items (the `NAMES_TEMPLATES` gate
violation, 2026-10-20); sprefa "zero shell in the engine" and the `eprintln!` ban in
`CLAUDE.md`, enforced by scanner tests.

## 12. Ids, not strings; batches, not nested loops (Chris, 2026-10-21)

| rule | mechanism |
| --- | --- |
| no string keys or indexes | text enters once, at the edge, into `dict(id, text)`; every row, arrangement key and join key is an id |
| hash liberally | 64-bit content hashes for anything that crosses a process or survives a restart (files, blobs, terms: the 10-03 ryi boundary, `ContentId`) |
| surrogate ints for what moves through time | dense ids minted by the interner for rows inside a running engine; smaller than hashes, ordered, arrangement-friendly |
| never hold a full object | operators carry ids; a payload is fetched by id only where it is printed or sent out |
| batch per frontier / wake | an executor receives every request of a frontier at once and answers in one batch (`answer_requests` already batches by handout); no per-row round trip (n+1) |
| no nested loops over collections | a join or a `reduce`, never a loop inside a loop over rows; `for` stays inside an operator over one batch |

Hash vs surrogate, which where: hashes are stable across processes and runs and need no lookup to
mint, but are 8 bytes and unordered; dense ints are small and ordered but meaningless outside
their interner. So: hash at every boundary and in storage, dense int inside one engine, the
dictionary maps between them. 64-bit hashes collide with probability about n^2 / 2^65 (about 3%
at a billion distinct items), so the dictionary insert checks text equality on an id hit and
fails loudly on a collision.

Finding nested loops with ryi (dogfood): `ryii --file-fact --kinds df <dir>` emits `node`
records with `kind: "loop"` (span, enclosing function) and `df_nest` records (a call inside a loop,
with its `depth`). A loop span inside another loop span in one file is a nested loop; `depth >= 2`
is a call two loops deep. Baseline, sprefa `src` at 4d75f231a:

| measure | count |
| --- | --- |
| loops | 966 |
| loop-in-loop pairs | 324 |
| calls at depth 2 or more | 1683 |

| file | loop-in-loop pairs |
| --- | --- |
| `src/_6_eval/_9a_ir_lower.rs` | 82 |
| `src/_6_eval/_9b_ir_eval.rs` | 48 (39 in `run_session`, the answer loop) |
| `src/_5_reify/_7_sqlite.rs` | 35 |
| `src/_6_eval/_9a1_rule_interp.rs` | 16 |
| `src/_3_check/_9_keep.rs` | 12 |
| `src/_2_lower/_7_goals.rs` | 10 |

Not every pair is a defect (a loop over a fixed small set, a loop inside an operator over one
batch); the count is the starting list, read through ryi before any change.

## 13. The thesis: one reactive database core, three programs on it (Chris, 2026-10-21)

The compiler, its macro expander and the runtime are dataflows on one DD worker, sharing
arrangements (interner, syntax rows, rules). The core is a reactive, incremental database.

| need | DD / timely mechanism |
| --- | --- |
| sync and async control | timely is step-driven (`worker.step()`): sync steps until the probe passes the frontier; async steps on a wake from a tokio task. One engine, two drivers |
| batched backpressure | not built in; advance the input to the next epoch only when `probe.less_than(input.time())` is false, bounding epochs in flight |
| constant memory | logical compaction of traces (`set_logical_compaction`) plus chunk paging (columnar spill, principle 7) |
| graph algorithms | `iterate` scopes (reach, connected components, SCC) |
| swap storage by trait and env | the byte store under paging and capture (memory, file, sqlite) is the swap point; the engine is not swapped (scope decision 5) |
