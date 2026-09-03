# PLAN: extract and soopy bindings in dl7 comptime, shared with the dbsp emitter

Auditor copy: every claim cites file:line on origin/main `03e441cc6` or the
branch named. Companion for Chris: `plans/2026-09-03-comptime-bindings.PLAN.visual.human.unga.md`.
Inventory this plan reads: `docs/comptime-bindings-inventory.md`.

1. [The ask](#the-ask)
2. [What exists](#what-exists)
3. [Fork 1: binding mechanism](#fork-1-binding-mechanism)
4. [Fork 2: the surface in dl7](#fork-2-the-surface-in-dl7)
5. [Fork 3: comptime tier](#fork-3-comptime-tier)
6. [Fork 4: cache home](#fork-4-cache-home)
7. [Types, bodies, lifetimes, storage](#types-bodies-lifetimes-storage)
8. [Same table, dbsp emitter](#same-table-dbsp-emitter)
9. [Sequencing](#sequencing)
10. [Receipts](#receipts)

## The ask

Chris 2026-09-03: "can we get sprefa extract and soopy bindings in the swi
prolog compiler comptime for dl7 and then that is comptime (for importing
types yada). i want to get the prolog comptime setup and then we are able to
use same api when outputting a dbsp program (for code relations ivm
maintenance)".

## What exists

| piece | state | cite |
|---|---|---|
| executor roster, the ONE naming table | 44 `arrival_executor/2` rows: `soopy__files` = `/soopy/files`, `soopy__watch` = `/soopy/watch`, `extract` family, `clock__tick` | `v6/prolog/compile/registry.pl:311-326` (count: `grep -c '^arrival_executor' registry.pl`) |
| the same roster, linked in Rust | `LINKED_EXECUTORS` pinned equal to the registry by `executor_roster_matches_registry` | `v6/sprefa-engine-rs/src/hosts.rs:57-66` |
| executor contract | `IHostExecutor::run(host, command_line, env) -> Vec<HostRow>`, cadence Once or Continuing | `hosts.rs:38-50` |
| soopy executors | checkout, watch, repo_at, refs, history; soopy linked in-process, no git spawn | `src/executors/{checkout,watch,repo_at,git_refs,git_history}.rs`; `executors/mod.rs:76` |
| dl6 programs bind by module name | `use extract.` `use soopy.` | `v6/dl/dataflow/report_extract.dl6:94-95` |
| decided surface for a host | an arrival rel `rel n(ins) -> (outs) key(..)`; `sh`/`bind` words dead; dotted executor names; `use` imports a module then bare leaf names | rulings `arrival_arrow_spelling` (:802), `sh_bind_surface_removed` (:823), `executor_namespacing` (:840), `executor_modules_use_import` (:860) |
| dl7 side of arrivals | NOT ported: reader-owned audit items L42, L43 | `v7/audit/results/11_ORACLES.md:73-74,87` |
| dl7 comptime | phases read, expand, lower, check, comptime; the evaluator makes no out-of-process call | `v7/src/2_comptime/2_compiler.pl:60-351`; `1_libtime/0_evaluator.pl` zero hits for process_create/foreign |
| dl7 reaches extract today | emit time only: `process_create` in the region mainer, `--witness --family type`, then `region <target> <region> --apply` | `v7/src/3_emit/3_rust_type_region_mainer.pl:49-75` (branch `feature/dl7-source-intelligence`) |
| dl7 loader for the TSI wire | `load_tsi_stream/3`, `install_tsi_graph/6` | `v7/src/2_comptime/0c_extract_loader.pl:1-6` |
| compile cache | `with_compilation_cache(Key, Producer, Compiled, Diagnostics, Hit)`, `with_prelude_cache` | `v7/src/2_comptime/1c_compiler_cacher.pl:16,39` |
| content identity | `content_id_of` (blake3 / git blob id) | `v6/sprefa-extract/src/shape.rs`; soopy `ContentId::GitBlob` (`executors/watch.rs`) |
| dd emit | `dd_plan(Name, rels, arrangements, operators, wires, tick_order)` from `lowered/8`, JSON twin, read by `v6/dd-runner` | `v6/prolog/compile/6_isolated_compiler_dd.pl:73-80` |
| dbsp crate map | core types, embedder program, retraction, operator surface, memory | `docs/ext-dbsp-incremental.md:16-306` |

## Fork 1: binding mechanism

Measured 2026-09-03, DEBUG binary (`.boop-worktrees/fix/extract-one-hash/.../target/debug/extract`), syntax tier, `tests/fixtures/tsi/rust_probe/src/lib.rs`, three runs each:

| path | wall | bytes |
|---|---|---|
| `extract --witness --family type lib.rs` direct | 0.05, 0.00, 0.00 s | 17029 |
| same through swipl `process_create` + `read_string` | 0.03, 0.02, 0.02 s | 17029 |

| candidate | what | cost | serves dbsp at runtime | verdict |
|---|---|---|---|---|
| (a) `process_create` + JSONL, one child per binding call, result cached by content id | the shape `3_rust_type_region_mainer.pl:67` already runs; `library(process)` loads (swipl 10.0.2) | 0.02 s per call measured; zero new build; the JSONL is the decoded wire the loader already reads | the emitted program does not spawn: it links the executor by roster name; the mechanism is comptime-only, the TABLE is shared | RECOMMENDED |
| (b) swipl `library(ffi)` over a Rust cdylib exposing extract and soopy | `library(ffi)` is absent on this machine (`use_module(library(ffi))`: source_sink does not exist); needs the pack plus a cdylib build of extract and soopy | new build artifact, ABI surface to maintain, no measured win over 0.02 s | no | dismissed on the absent library and the unmeasured need |
| (c) engine-rs embeds swipl (`swipl` crate) so comptime runs inside the Rust process | inverts ownership: the compiler becomes a library of the runtime; every `just build` of v7 needs the Rust toolchain | large; couples two release trains | yes, one process | deferred: nothing in the ask needs one process; revisit if (a)'s per-call cost ever shows on the 10-second law |
| (d) resident extract daemon over a socket (v5 `docs/daemon.md` shape) | daemon lifecycle, spawn-if-missing, socket protocol | infra to own; the law says infra is bought | partially | dismissed: (a) at 0.02 s leaves nothing for a daemon to save |

Recommendation: (a). The one change to the mainer's shape: the call moves
from emit time to a comptime phase, keyed by content id, and the spawn
lives behind the roster name, never a literal path.

## Fork 2: the surface in dl7

Options, Chris's call (language design):

| option | spelling in a `.dl7` | pro | con |
|---|---|---|---|
| (i) port the decided dl6 arrival form as is | `(<- (extract.tsi ?file) ...)` bound by `(use extract)`; the roster row names the executor | one spelling across dl6 and dl7, rulings already decided it | dl7 has no arrival rel yet (L42/L43 unported): the port IS the work |
| (ii) a comptime-only import form | `(import rust "src/trail.rs")` expanding to the loader's rows | smallest first arc | a second spelling for the same executor; forbidden by `executor_namespacing` unless it desugars to (i) |
| (iii) both: (ii) as sugar over (i) | | matches how `sh` desugared to `sh_decl/4` | two things to test |

Recommendation: (i), with the first arc landing only the comptime answer
for `extract.tsi` and `soopy.files` (no runtime tick), so the reader port
and the binding land as one row each.

## Fork 3: comptime tier

| option | cost | fidelity |
|---|---|---|
| syntax tier only (`--witness --family type <file>`) | 0.02 s per file, no project load | best guess, ids per file |
| checker tier when `--project-root` is known (`--rust-checker`, `--ts-checker`) | rust-analyzer release walk 5.0 s on `src/trace.rs` (failure-modes 108), tsc load per project | resolved ids, `coverage complete` per relation |

Recommendation: syntax by default; a program opts into the checker tier
per binding call with a `tier` input column, and the 10-second law bounds
it (a checker call over the cap is a diagnostic row, never a wait).

## Fork 4: cache home

| option | pro | con |
|---|---|---|
| `1c_compiler_cacher.pl` (`with_compilation_cache/5`), key = (binding, content id, tier) | one cache, one law, already in the compile path | prolog-side store |
| the one db `~/.agent/dl6.db` (decision "one server, one db") | shared with the runtime, survives across compiles | a compile writing into the runtime db |

Recommendation: the cacher, keyed on content id; the runtime db stays the
runtime's.

## Types, bodies, lifetimes, storage

Type signatures first, pseudo-code under each, then lifetimes, storage,
reads and writes, uniqueness.

```prolog
%! binding_answer(+Binding:atom, +Inputs:list(Col=Value), -Rows:list(row), -Diagnostics:list) is det.
%  Binding is a roster name ('/extract/tsi', '/soopy/files'), Inputs the key columns.
%  cache_key(Binding, Inputs, Key),
%  with_compilation_cache(Key, run_binding(Binding, Inputs), Rows, Diagnostics, _Hit).

%! run_binding(+Binding, +Inputs, -Rows, -Diagnostics) is det.
%  binding_argv(Binding, Inputs, Executable, Argv),
%  run_process(Executable, Argv, "", Exit, Out, Err),      % the mainer's run_process/6
%  ( Exit =:= 0 -> binding_decode(Binding, Out, Rows), Diagnostics = []
%  ; Rows = [], Diagnostics = [diagnostic(Binding, process_exit(Exit, Err))] ).

%! binding_argv(+Binding, +Inputs, -Executable, -Argv) is det.
%  '/extract/tsi': extract, ['--witness','--family',type | Files]  (+ '--rust-checker','--project-root',Root when tier=checker)
%  '/soopy/files': soopy, ['--repo',Root,files,'--revision',Rev,'--glob',Glob,'--format',jsonl]
%  '/soopy/read' : soopy, ['--repo',Root,read,'--glob',Path,'--format',jsonl]

%! binding_decode(+Binding, +Jsonl:string, -Rows) is det.
%  '/extract/tsi': load_tsi_text/3 (0c_extract_loader.pl) then accepted_rows/2
%  '/soopy/*'   : one row per jsonl line, columns as the CLI prints them

%! cache_key(+Binding, +Inputs, -Key) is det.
%  content_id of every file input (read the bytes once, blake3), the tier atom, the binding name.

%! binding_columns(+Binding, -Inputs:list(col(Name,Type,key)), -Outputs:list(col(Name,Type))) is det.
%  the roster contract the checker reads; ONE table shared with the emitter (section 8).
```

Lifetimes: `binding_columns/3` is static (module facts). The cache lives
for the process and is durable through the cacher's store. A `run_binding`
child lives for one call.

Storage: the cacher's existing store, one row per Key holding the decoded
rows. No new table in the runtime db.

Reads and writes, in order, per comptime phase: read program, collect
binding demands from the lowered rels whose executor is in the roster,
hash inputs, cache lookup, spawn on miss, decode, store, install rows
into the type graph (`install_tsi_graph/6`).

Uniqueness: Key is unique per (binding, content ids, tier). Two demands
with one Key run once (the cacher's Hit).

## Same table, dbsp emitter

`binding_columns/3` (section 7) IS the roster with column contracts. The
emitter reads it the way `6_isolated_compiler_dd.pl` reads `lowered/8`:
one dbsp input handle per binding the program demands (`docs/ext-dbsp-incremental.md`
section 2, `add_input_zset`), typed by `binding_columns`. `/soopy/watch`
deltas (cadence Continuing, `hosts.rs:32-35`) are the change stream: a
delta row becomes a ZSet insert or retraction on the file rows, and the
extract binding re-answers for the changed content ids only (the cache
key is the content id, so an unchanged file costs one hash).

The `dd_plan` term grows one argument, `bindings(Bindings)`, beside
`rels/arrangements/operators/wires/tick_order`; the JSON twin gains a
`bindings` array. Nothing else in the term changes.

## Sequencing

| arc | deps | receipt |
|---|---|---|
| `dl7_binding_table` | none | `binding_columns/3` facts equal to the registry roster rows for `/extract/tsi`, `/soopy/files`, `/soopy/read`, `/soopy/watch`; a plunit case pins equality with `registry.pl` `arrival_executor/2` |
| `dl7_comptime_extract_tsi` | `dl7_binding_table` | a `.dl7` fixture demanding `extract.tsi` over `rust_probe/src/lib.rs` compiles with the type graph installed; second compile hits the cache (Hit = true); `just build` green x3 |
| `dl7_comptime_soopy_files_read` | `dl7_binding_table` | same over `soopy files` and `soopy read` JSONL |
| `dl7_arrival_rel_reader` | `dl7_binding_table` | audit L42/L43 ported: `(use extract)` binds the module, dotted leaf names resolve to roster rows; reader tests |
| `dd_plan_bindings` | `dl7_binding_table` | `dd_plan/6` grows `bindings/1`; goldens updated by hand and cited; dd-runner reads the array |
| `dbsp_emitter_first_program` | `dd_plan_bindings` | one program with `soopy.watch` + `extract.tsi` emitted as a dbsp circuit, fed one file change, output delta observed |

## Receipts

- Latency table above: three runs each, debug profile, syntax tier.
- `swipl -g "use_module(library(ffi))" -t halt`: `ERROR: source_sink library(ffi) does not exist` (SWI-Prolog 10.0.2 arm64-darwin).
- `swipl -g "use_module(library(process))" -t halt`: loads.
- Roster count: 44 `arrival_executor/2` rows.
