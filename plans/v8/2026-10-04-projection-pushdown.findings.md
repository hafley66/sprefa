# Findings: projection pushdown in IR lowering

Brief: `plans/v8/2026-10-04-projection-pushdown.brief.md`. Base sprefa `a52ec373a`, sqlite_ivm
`b791f71` (clone `.boop-worktrees/sqlite_ivm`, branch `perf/projection-pushdown`). Debug binaries
(`[profile.dev] opt-level = 1`, dependencies `opt-level = 3`), `RCARGO_OFF=1`.

Counters added for this measurement (kept, debug/info level only):

- `ivm_dd` `dd install`: `arrangements`, the join arrangements built (`DdRel::keyed`, one per
  `(stream, key columns)`, shared by every join that reads that stream on that key).
- `ivm_dd` `dd settle`: `arranged_cells`, cells sent into those arrangements since install
  (key columns plus row columns, one count per record update). Cumulative per install.
- `ivm_sqlite` `ir schema`: `sqlite_columns`, columns of every table and view the install
  created (`pragma_table_info` over `sqlite_master` and `sqlite_temp_master`).
- `dl8::ir` `ir install`: `nodes`, `joins` (joins that read no constructor relation: a
  constructor join is a decode and is never arranged) and `join_input_cols`, the summed column
  count of those joins' inputs.

Commands: `RUST_LOG=dl8::ir=info,ivm_dd=debug,ivm_sqlite=info dl8 expand
oracle/macrotime/15_standard_plus_1.json` (c15), and `/usr/bin/time -l dl8 build
std/tsi_registry_emit.dl7 --out <tmp>` three times per engine, output `cmp` against
`hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs`. `DL8_ENGINE` per row.

## Before

Load 5.7-7.5 during the runs. Every registry run `cmp`-identical.

### c15 (`dl8 expand`): two installs, the macro library compile and the unit expansion

| install | IR nodes | joins | join input cols | dd operators | dd arrangements | dd arranged cells | sqlite columns |
|---|---|---|---|---|---|---|---|
| macro library | 1072 | 287 | 3369 | 4242 | 337 | 3689 (1 settle) | 9011 |
| c15 unit | 1069 | 287 | 3369 | 4234 | 337 | 6791 wave 0, 8891 after wave 1 | 9012 |

### Registry build (`dl8 build std/tsi_registry_emit.dl7`)

| engine | real s | max RSS |
|---|---|---|
| ir:dd | 0.36, 0.37, 0.38 | 107, 122, 136 MB |
| ir:sqlite | 2.59, 2.57, 2.58 | 275, 286, 296 MB |

Per install (six installs: macro library, four unit expansions, comptime):

| install | IR nodes | joins | join input cols | dd arrangements | dd arranged cells (end) | sqlite columns |
|---|---|---|---|---|---|---|
| macro library | 1072 | 287 | 3369 | 337 | 3689 | 9011 |
| unit 1 | 1076 | 287 | 3369 | 337 | 199954 | 9040 |
| unit 2 | 1072 | 287 | 3369 | 337 | 31522 | 9017 |
| unit 3 | 1069 | 287 | 3369 | 337 | 27197 | 9018 |
| unit 4 | 1065 | 287 | 3369 | 337 | 127076 | 8995 |
| comptime | 3401 | 641 | 17846 | 879 | 375463 | 67719 |

### Suite (`cargo test --test it -- --test-threads=1`, debug)

| engine | wall s (load at start) | passed | failed |
|---|---|---|---|
| ir:dd | 181 (8.7) | 174 | 12, all `_7_book` |
| ir:sqlite | 485 (5.4) | 173 | 13, all `_7_book` (adds `chapter_executors`) |


Both suites fail only `_7_book` tests (book pages expect old evaluator traces and missing
fixtures); the same set before and after.

## After

Load 2.7-4.7 for the c15 and registry runs, 6.4-7.8 at the suite starts. c15 `dl8 expand` JSON
byte-identical to the before run on both engines; every registry run `cmp`-identical.

### c15

| install | IR nodes | join input cols | dd operators | dd arrangements | dd arranged cells | sqlite columns |
|---|---|---|---|---|---|---|
| macro library | 1072 / 1065 | 3369 / 1847 | 4242 / 4052 | 337 / 344 | 3689 / 3688 | 9011 / 5625 |
| c15 unit | 1069 / 1062 | 3369 / 1847 | 4234 / 4044 | 337 / 344 | 6791 / 5315 wave 0, 8891 / 7214 after wave 1 | 9012 / 5626 |

(before / after; joins stay 287.)

### Registry build

| engine | before real s | after real s | before max RSS | after max RSS |
|---|---|---|---|---|
| ir:dd | 0.36, 0.37, 0.38 | 0.38, 0.38, 0.38 | 107-136 MB | 114-131 MB |
| ir:sqlite | 2.59, 2.57, 2.58 | 2.14, 2.15, 2.13 | 275-296 MB | 238-241 MB |

| install | join input cols | dd arrangements | dd arranged cells (end) | sqlite columns | sqlite creates |
|---|---|---|---|---|---|
| unit 1 | 3369 / 1847 | 337 / 344 | 199954 / 183212 | 9040 / 5654 | 1042 / 1026 |
| unit 2 | 3369 / 1847 | 337 / 344 | 31522 / 24292 | 9017 / 5631 | 1038 / 1022 |
| unit 3 | 3369 / 1847 | 337 / 344 | 27197 / 18608 | 9018 / 5632 | 1036 / 1020 |
| unit 4 | 3369 / 1847 | 337 / 344 | 127076 / 118156 | 8995 / 5609 | 1032 / 1016 |
| comptime | 17846 / 5782 | 879 / 906 | 375463 / 252806 | 67719 / 27893 | 2733 / 2729 |

dd arrangements grow by 7 per macrotime install and 27 for comptime: two goals that read one
relation through different column sets now read two streams, so `DdRel::keyed` cannot share
their arrangement. Goals that keep the same columns share one node (`Builder::narrowed_gets`).

### Suite

| engine | before s | after s | passed / failed |
|---|---|---|---|
| ir:dd | 181 (load 8.7) | 171 (load 7.8) | 174 / 12 both runs |
| ir:sqlite | 485 (load 5.4) | 376 (load 6.4) | 173 / 13 both runs |

sqlite_ivm `cargo test --workspace -- --test-threads=1`, fresh `TMPDIR`, extension dylib rebuilt
from the clone: 244 passed, 0 failed.

## How it works

### The problem, in numbers

Every positive goal joined the whole relation onto the chain, and every kernel goal appended
its decode columns (a comparison `int.lt` adds 8); nothing dropped a column until the head
`Mfp`. Across the c15 macro library, the inputs of the 287 arranged joins summed 3369 columns;
comptime's 641 joins read 17846. Each arranged record stores its key plus the whole row.

### Change

1. `later_vars` (`src/_6_eval/_9a_ir_lower.rs:1233`): per goal, the variables of the later
   goals and of the head, one right-to-left pass.
2. Right input (`:1437`): a positive goal reads its relation through `Mfp { project }` keeping
   join-key positions, ground positions, and the first position of each new variable still live;
   a repeated variable's equality filter runs in the same node. Singletons and `?_` columns are
   gone before the arrangement. Identical narrowings share one node (`:1466`).
3. `keep_live` (`:1246`), after every goal (`:1352` kernel, `:1434` antijoin, `:1521` join):
   project the chain onto the live variables' columns (plus the element columns of a live built
   tuple, to a fixpoint for nested tuples), renumbering `bound` and `built_tuples`. Join and
   ground columns already compared, kernel decode scratch, and dead variables drop. Antijoin
   keys, aggregate keys and subjects are head or later-goal variables, so they stay.
4. Engines: `projected_join` (`sqlite_ivm/crates/ivm-engine/src/1_rel.rs:334`) recognizes a
   project-only `Mfp` over a two-input, non-decode `Join`; when `Rel::fuses_join_project`
   (`:127`) is true, `lowered_inputs` (`:349`) reads the join's inputs and `build_node` (`:416`)
   calls `Rel::join_project`. ivm-dd fuses for unmeasured installs
   (`sqlite_ivm/crates/ivm-dd/src/2_dd.rs:448`); `join_rows` (`:252`) emits only the projected
   cells inside `join_core`. Measured installs keep the join node so its taps and counters stay.
   ivm-sqlite keeps the default (join, then `Mfp`) and creates narrower tables and views from the
   narrower IR.

Projection commutes with join, filter, map, mint and union, and antijoin thresholds its right
keys, so each node's Z-set over the kept columns is unchanged; aggregates see the same
weighted rows.

### Step trace: `macrotime/0_standard.dl7:120`

```
(<- (: ?Output item ?Child ?Index)
    (plus_form ?Form ?Output)
    (: ?Form item ?Child ?Index)
    (int.lt 0 ?Index))
```

`later`: goal 0 {Form, Output, Child, Index}; goal 1 {Index, Output, Child}; goal 2 {Output,
Child, Index}.

| step | before (node: op, width) | after |
|---|---|---|
| goal 0 `plus_form` | 66 `Get`, 2 | 67 `Get`, 2 (both columns live) |
| goal 1 `:` join on Form | 68 `Join`, 6 | 69 `Join`, 6 (all four columns needed: key, ground, two live) |
| ground `item` | 69 `Join` +2, 70 `Mfp` back to 6 | 70 `Join`, 8; 71 `Mfp [1,4,5]` (Output, Child, Index), 3 |
| goal 2 ground `0` | 71 cross `Join` from 6 to 8 | 72 cross `Join` from 3 to 5 |
| goal 2 four decodes, filter `Lt` | 72-77, width 16 | 73-78, width 13 |
| goal 2 end | (no projection) | 79 `Mfp [0,1,2]`, 3 |
| head ground `item` | 78 cross `Join` from 16 to 18 | 80 cross `Join` from 3 to 5 |
| head | 79 `Mfp [1,17,4,5]` | 81 `Mfp [0,4,1,2]` |

The two arranged cross joins with the ground rows take a 3-column left input in place of 6 and
16. In ir:dd, node 70 and its `Mfp` 71 are one `join_core` emitting 3 cells per row.

## Shared narrowing (follow-up)

Brief: `plans/v8/2026-10-04-shared-narrowing.brief.md`. Base sprefa `3b1fcd6fc`, sqlite_ivm
`db1e57c` (clone `.boop-worktrees/sqlite_ivm`, branch `perf/shared-narrowing`). Same binaries
profile and commands as above; before = the projection-pushdown build, after = this branch,
run back to back. Load 7.1-7.4 for the registry runs, 9.4 and 6.8 at the suite starts. c15
`dl8 expand` JSON byte-identical on both engines; every registry run `cmp`-identical.

### c15

| install | IR nodes | join input cols | dd operators | dd arrangements | dd arranged cells | sqlite columns |
|---|---|---|---|---|---|---|
| macro library | 1065 / 1075 | 1847 / 1907 | 4052 / 3992 | 344 / 337 | 3688 / 3688 | 5625 / 5500 |
| c15 unit | 1062 / 1072 | 1847 / 1907 | 4044 / 3984 | 344 / 337 | 5315 / 5283 wave 0, 7214 / 7176 after wave 1 | 5626 / 5501 |

(before / after; pre-pushdown: 337 arrangements, 8891 cells.)

### Registry build

| engine | before real s | after real s | before max RSS | after max RSS |
|---|---|---|---|---|
| ir:dd | 0.42, 0.45, 0.50 | 0.35, 0.37, 0.36 | 122-134 MB | 110-122 MB |
| ir:sqlite | 2.11, 2.10, 2.08 | 2.05, 2.06, 2.03 | 220-242 MB | 229-246 MB |

| install | join input cols | dd arrangements | dd arranged cells (end) | sqlite columns | sqlite creates |
|---|---|---|---|---|---|
| macro library | 1847 / 1907 | 344 / 337 | 3688 / 3688 | 5625 / 5500 | 1022 / 995 |
| unit 1 | 1847 / 1907 | 344 / 337 | 183212 / 182096 | 5654 / 5529 | 1026 / 999 |
| unit 2 | 1847 / 1907 | 344 / 337 | 24292 / 24138 | 5631 / 5506 | 1022 / 995 |
| unit 3 | 1847 / 1907 | 344 / 337 | 18608 / 18472 | 5632 / 5507 | 1020 / 993 |
| unit 4 | 1847 / 1907 | 344 / 337 | 118156 / 117626 | 5609 / 5484 | 1016 / 989 |
| comptime | 5782 / 5889 | 906 / 809 | 252806 / 211388 | 27893 / 28040 | 2729 / 2718 |

(pre-pushdown comptime: 879 arrangements, 375463 cells.)

### Suite

| engine | after s (load at start) | passed / failed |
|---|---|---|
| ir:dd | 174 (9.4) | 174 / 12, all `_7_book`, the same set |
| ir:sqlite | 405 (6.8) | 172 / 14, all `_7_book`: the 13 above plus `chapter_effects` (`10_effects outputs: exceeded 10s`); passes rerun by name (6.66 s); `book/check_outputs.sh book/src/10_effects.md` 5.33, 5.43 s before, 5.43, 5.27 s after |

sqlite_ivm `cargo test --workspace -- --test-threads=1`, fresh `TMPDIR`, extension dylib rebuilt
from the clone: 244 passed, 0 failed.

### How it works

The problem, in numbers: each goal narrowed its relation to its own kept columns, and
`DdRel::keyed` shares an arrangement only per `(stream, key columns)`. Two goals that joined
one relation on one key through different column sets read two streams and built two
arrangements: 344 per macrotime install where pre-pushdown had 337, 906 for comptime where it
had 879. Example, c15 macro library, relation `__ir_n27153_a4`: pre-pushdown `Get` arranged on
keys (), 0, 1, 2, 3 (5); after pushdown the readers keyed on position 0 read the narrowings
(0), (0,1) and (0,2), three arrangements of one key (7 total).

1. Groups (`src/_6_eval/_9a_ir_lower.rs:1468`): a non-first positive goal without a repeated
   variable belongs to `(relation, positions holding bound variables)`. The first goal belongs
   to the group of the arrangement its stream feeds: its first ground position, else the
   positions whose variable a positive relation second goal reads, else none (a kernel's cross
   join with ground rows). A goal with a repeated variable keeps its own filtered read.
2. Recording pass (`:2699`): every rule compiles once into a clone of the builder with
   `narrow_record` set; each grouped goal adds its kept positions to `narrow_unions` (`:107`).
   The real pass narrows each grouped goal to its group's union (an error if the union misses
   a kept position); identical unions share one node through `narrowed_gets`.
3. A first goal read through its group (`:1507`) skips `keep_live` (`:1593`): its stream enters
   the second goal's join whole, and the `keep_live` projection after that join drops the dead
   columns. ir:dd fuses that project-only `Mfp` into `join_core` (`projected_join`), so the
   columns a reader does not keep never leave the join.
4. Engine (`sqlite_ivm/crates/ivm-engine/src/1_rel.rs:151`, `:304`;
   `sqlite_ivm/crates/ivm-dd/src/2_dd.rs:106-125`): `Rel::letrec` takes the outer-scope node
   memo the `Let` strata use. ir:dd lowered a fixpoint's hoisted, loop-independent nodes (and
   the bodies of a non-recursive `LetRec`) into a fresh memo per fixpoint, so a narrowing node
   read by several strata became one collection per stratum and one arrangement each; a `Get`
   resolved to the one `defined` collection and was shared. Through the shared memo each node
   is one collection per install. ivm-sqlite ignores the memo. This step takes comptime from
   884 to 809.

After: `__ir_n27153_a4` is read as (0) with no key, (0,1,2) on position 0, and (0,1), (0,2),
(0,3) on positions 1, 2, 3: 5 arrangements, as pre-pushdown. Projection commutes with join, so each reader's rows are the
union narrowing projected onto its own columns; the dead columns of the wider read leave at
the reader's next `keep_live`.
