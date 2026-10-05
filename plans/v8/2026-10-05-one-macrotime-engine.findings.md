# Findings: one engine for all macrotime units (page 2)

Brief: `plans/v8/2026-10-05-one-macrotime-engine.brief.md`. Base sprefa `f2f879c3b`, sqlite_ivm
`e5afc8f` (clone inside the worktree, branch `perf/one-macrotime-engine`). Debug binaries
(`[profile.dev] opt-level = 1`, dependencies `opt-level = 3`), `RCARGO_OFF=1`, extension dylib
rebuilt from the clone. Commands:

```
RUST_LOG=dl8::macrotime=info,dl8::ir=info,ivm_sqlite=info DL8_ENGINE=<e> \
  dl8 build std/tsi_registry_emit.dl7 --out <tmp>          # counts (traced)
DL8_ENGINE=<e> dl8 build std/tsi_registry_emit.dl7 --out <tmp>   # wall/RSS (untraced)
cmp <tmp>/target/tsi/registry.rs ~/projects/hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs
DL8_ENGINE=<e> dl8 expand oracle/macrotime/15_standard_plus_1.json   # c15
```

`installs` counts `ir install` lines, `CREATEs` sums `sqlite_create_count` of the `ir schema`
lines (objects in the in-memory database after each install), `macrotime wall` sums the
`unit_us` of the `macrotime unit` lines. Wall and RSS from `/usr/bin/time -l`.

## Before

Load 4.6-6.5 during the runs. Registry output `cmp`-identical every run.

### Installs per registry build (ir:sqlite, traced run 1)

| # | what | nodes | install ms | CREATEs | first settle sent / changed |
|---|---|---|---|---|---|
| 1 | comptime of the macro library (`standard_macro_program`) | 1075 | 63 | 995 | 1197 / 570 |
| 2 | macrotime unit 1 | 1079 | 55 | 999 | 10893 / 4094 |
| 3 | macrotime unit 2 | 1075 | 56 | 995 | 2109 / 559 |
| 4 | macrotime unit 3 | 1072 | 53 | 993 | 1520 / 403 |
| 5 | macrotime unit 4 | 1068 | 53 | 989 | 12109 / 3519 |
| 6 | comptime of the registry program | 3657 | 309 | 2718 | 5095 / 4519 |

The fifth source unit dispatches `Absent` (no claim writer names an atom it holds) and installs
nothing. The four macrotime installs differ in node count (1079, 1075, 1072, 1068): the
constructor relations and seed sources follow the unit's rows.

### Totals

| measure | ir:sqlite | ir:dd |
|---|---|---|
| installs per build (all / macrotime) | 6 / 4 | 6 / 4 |
| CREATEs per build (all / macrotime) | 7689 / 3976 | n/a |
| macrotime wall, sum of `unit_us` (3 traced runs) | 777, 778, 772 ms | 128, 138, 125 ms |
| registry build real s (3 untraced runs) | 2.01, 1.99, 1.97 | 0.36, 0.36, 0.35 |
| registry build max RSS MB (untraced) | 258, 242, 255 | 119, 117, 105 |
| c15 `dl8 expand` real s (installs: 1 comptime + 1 macrotime) | 0.26, 0.26, 0.25 | 0.06, 0.06, 0.07 |

Per unit (ir:sqlite): wave 0 `evaluate_us` 256, 131, 119, 245 ms; the install inside it 53-56 ms.
ir:dd unit wall 56, 18, 17, 36 ms; install 5.5-5.9 ms each.

### Suites (before)

| suite | result | wall |
|---|---|---|
| `cargo test --workspace --no-fail-fast -- --skip _7_book --test-threads=1` (ir:sqlite) | 164 passed, 0 failed | 4m50s |
| same with `DL8_ENGINE=ir:dd` | 164 passed, 0 failed | 2m09s |

## After

Load 3.6-4.5 during the runs. Registry output `cmp`-identical every run on both engines; c15
`dl8 expand` JSON byte-identical to the before run on both engines.

### Installs per registry build (ir:sqlite, traced run 1)

| # | what | nodes | install ms | CREATEs |
|---|---|---|---|---|
| 1 | comptime of the macro library | 1075 | 64 | 995 |
| 2 | macrotime, all four units | 1087 | 57 | 1007 |
| 3 | comptime of the registry program | 3657 | 309 | 2718 |

One `macrotime prepare` line per build; every `macrotime unit` line carries the same IR
fingerprint (`8002036408111583593` in this universe), `prepares=1 reprepares=0`.

### Totals, before / after

| measure | ir:sqlite before | ir:sqlite after | ir:dd before | ir:dd after |
|---|---|---|---|---|
| installs per build (all / macrotime) | 6 / 4 | 3 / 1 | 6 / 4 | 3 / 1 |
| CREATEs per build (all / macrotime) | 7689 / 3976 | 4720 / 1007 | n/a | n/a |
| macrotime wall, sum of `unit_us`, 3 traced runs, ms | 777, 778, 772 | 621, 617, 620 | 128, 138, 125 | 131, 134, 131 |
| registry build real s, 3 untraced runs | 2.01, 1.99, 1.97 | 1.86, 1.86, 1.86 | 0.36, 0.36, 0.35 | 0.35, 0.35, 0.36 |
| registry build max RSS MB, untraced | 258, 242, 255 | 250, 237, 246 | 119, 117, 105 | 119, 97, 109 |
| c15 `dl8 expand` real s | 0.26, 0.26, 0.25 | 0.27, 0.27, 0.27 | 0.06, 0.06, 0.07 | 0.06, 0.06, 0.06 |

### Per unit, `unit_us` ms (run 1)

| unit | graph rows | sqlite before | sqlite after | dd before | dd after | first settle sent before / after |
|---|---|---|---|---|---|---|
| 1 | 3928 | 282 | 306 | 56 | 63 | 10893 / 10713 |
| 2 | 552 | 131 | 131 | 18 | 22 | 2109 / 5243 (3928 retracted + 1315) |
| 3 | 396 | 119 | 35 | 17 | 12 | 1520 / 1429 (552 + 877) |
| 4 | 3512 | 245 | 149 | 36 | 33 | 12109 / 10230 (396 + 9834) |

Unit 1 still pays the one install (57 ms sqlite, 6.6 ms dd) plus a separate settle of the macro
program's own seeds (`base`, 180 rows sent, 6 changed). Units 2-4 pay no install but retract the
previous unit in their first settle: retracting unit 1 (3928 seed rows) is most of unit 2's
131 ms on sqlite. The last unit is never retracted. On dd the install was 6 ms per unit, about
what the retraction costs, so dd wall does not move.

### A first cut that retracted eagerly

The first implementation retracted each unit's rows in a settle of their own when the unit
finished, and missed `expansion_node/4` in the reader shapes (one reprepare in unit 1). sqlite
macrotime was 631 ms of units plus 255 ms of retraction settles (112, 20, 15, 108 ms), registry
2.15-2.26 s; dd 134 ms plus 51 ms, 0.45 s. Moving the retraction into the next unit's first
settle drops the last unit's retraction and one per-settle fixed cost per unit.

### Suites (after)

| suite | before | after | wall after |
|---|---|---|---|
| dl8, ir:sqlite (`--skip _7_book --test-threads=1`) | 164 passed | 166 passed, 0 failed | 4m47s |
| dl8, `DL8_ENGINE=ir:dd` | 164 passed | 166 passed, 0 failed | 2m17s |
| `ivm-sqlite` (`-p ivm-sqlite --test-threads=1`, fresh `TMPDIR`, extension rebuilt from the clone) | not run | 111 passed, 9 ignored after one expectation update | 45s |

The two new dl8 tests: `macrotime_cases_share_one_engine` (all 24 oracle cases in one universe on
one session: every case equals v7, `prepares=1 reprepares=0`, one fingerprint) and
`macrotime_reprepares_for_an_undeclared_shape`. `ivm-sqlite`
`11_settle_spans` counts `stmt` spans across install and settle: 109 on `e5afc8f`, 107 after the
source DDL change (checked by running it with the change stashed); the expected literal moved
to 107.

## How it works

### The problem, in numbers

Before, every unit lowered the macro rule cone with the unit's rows as seeds. The lowering
derives constructor relations from the term shapes it finds in the seeds and a source per seeded
relation, so each unit's IR differed (1079, 1075, 1072, 1068 nodes) and each unit installed its
own engine: four sqlite installs of about 55 ms and about 995 CREATEs each, on top of the first
settle. After, the IR depends on the program alone, one engine serves the four units, and the
build creates 2969 fewer schema objects.

### What the IR is built from

- `ir_lower::lower_declared(u, program, declared)` (`src/_6_eval/_9a_ir_lower.rs:2340`):
  `Declared` (`:2328`) lists the seed relations (`sources`, added to the relation keys at
  `:2670`) and the constructor shapes (`shapes`, added at `:2528`) the caller will send later.
  `lower` is `lower_declared` with nothing declared.
- The macrotime session declares the eight protocol input relations (`seed_relations`,
  `src/_1_macrotime/_2_protocol.rs:387`) and `READER_SHAPES` (`:407`): `reader_node/2`,
  `expansion_node/4`, `variable/2`, `tree_sitter_query/1`, `symbol/1`. Every other term shape a
  graph row can hold is either built into every lowering (int, float, bool, str, atom, `const`,
  `primitive`, `ref`, `[|]`, `application`, `edge`) or minted by a macro rule, so it is in the
  program.
- Seed terms enter as engine terms through `IrSession::apply` (`src/_6_eval/_9b_ir_eval.rs`,
  `install_fresh` `:928`, `apply_session` `:959`), with their term-map and plain-list rows, as
  page 1 already did for waves 1+.

### Step trace of a build (`src/_8_driver/_2_macro.rs:45`)

1. `expand_units_with_macros` creates one `MacroSession` (`src/_1_macrotime/_4_expand.rs:105`)
   and passes it to every unit. `dl8 expand` creates one per case (`_6_cli.rs:221`).
2. A unit whose dispatch is `Absent` returns before touching the engine, as before.
3. `enter` (`_4_expand.rs:201`): if the session holds no program, or a different rule cone or
   protocol, `prepare` (`:150`) lowers with the declared sources and shapes, installs, settles the
   macro program's own seeds and keeps the decoded rows as `base`. Then one settle sends the
   previous unit's seed rows (`resident`) at -1 and this unit's seed rows at +1.
4. `settle` (`:176`) folds the protocol rows of every delta into `net`: the protocol rows the
   engine holds above `base`. After the combined settle `net` is exactly this unit's closure
   minus `base` (previous `net` plus the delta), so `enter` returns `net` as the wave-0 rows and
   folds `base` into a fresh `Available` (`:258`). The expander then reads the same closure a
   fresh install would have produced.
5. Waves run as in page 1 (`expand_graph`, `:366`), each apply through `settle` (`:471`).
6. `leave` (`:320`, called at `:361` on every exit) records the unit's seed rows as `resident`;
   nothing settles until the next unit enters. The session is dropped after the last unit.
7. `reprepare` (`:267`): an apply that meets a shape the IR has no constructor for returns
   `ApplyError::Unprepared` before settling; the session logs `macrotime reprepare`, adds every
   compound shape of the graph's seeds to `declared.shapes` (logged as `macrotime declared
   shapes`), prepares again and enters the whole graph. A second `Unprepared` is a
   `unprepared_after_reprepare` diagnostic. Zero reprepares on the registry build and on every
   oracle case; `macrotime_reprepares_for_an_undeclared_shape` renames c15's node ids to
   `reader_nodf/2` and checks the reprepared expansion equals the declared one and v7's.

### ivm-sqlite source tables

`Sqlite::install_on` (`sqlite_ivm/crates/ivm-sqlite/src/4_engine.rs:240`) reads the table names
once from `sqlite_master` and builds the source DDL (`CREATE TABLE ... WITHOUT ROWID`, or the
unique index on a host table already present) into `catalog::CreatedSources` (`catalog.rs:264`).
`install_program` runs that DDL inside the `frontier_sp_install` savepoint ahead of the program
DDL (`:344`); `compile_ir` takes the created tables' columns from it instead of
`PRAGMA table_info` (`:618`), and `scan_derived_sources` skips them (`:217`). Before, each source
cost an `EXISTS` query, a separate `execute_batch` outside the savepoint, a `PRAGMA table_info`,
a catalog lookup and a `sqlite_master` lookup.
