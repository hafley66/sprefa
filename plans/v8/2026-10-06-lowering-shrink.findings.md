# Findings: lowering shrink (census changes 1-9)

Brief: `plans/v8/2026-10-06-lowering-shrink.brief.md`. Census: `plans/v8/2026-10-05-lowering-census.findings.md`.
sprefa `perf/lowering-shrink` from `f63e7a1ea` (main `08e53559d` plus the brief); sqlite_ivm clone
`sqlite_ivm/`, branch `perf/lowering-shrink` from main `80d416d`. Counts only, no clock.

Installs: hello = `dl8 compile plans/v8/probes/2026-10-05-one-dd-engine/0_hello.dl7` (#1 macro library
comptime, #2 macrotime cone, #3 comptime); registry = `dl8 compile std/tsi_registry_emit.dl7`, its
comptime install. Nodes and dd operators from the `dd install` event (`RUST_LOG=ivm_dd=debug`), dd
default mode.

## 1. Per change

IR nodes / dd operators after each commit (row 0 is main).

| # | change | hello #1 | hello #2 | hello #3 | registry #3 | sprefa commit | sqlite_ivm commit |
|---|---|---|---|---|---|---|---|
| 0 | main | 1057 / 3897 | 1069 / 3936 | 2137 / 6960 | 3640 / 13770 | `f63e7a1ea` | `80d416d` |
| 1 | hash-cons equal nodes | 704 / 3477 | 713 / 3516 | 1531 / 6468 | 2873 / 13136 | `abc9a3ed3` | |
| 2 | ground terms as IR literals | 502 / 2187 | 511 / 2226 | 1300 / 5118 | 2412 / 9436 | `15f72bdb9` | `c7b288f` |
| 3 | drop the all-sources sentinel | 453 / 2047 | 459 / 2077 | 1175 / 4750 | 2265 / 9002 | `b2649b2df` | `faab80a` |
| 4 | project fused into Join | 434 / 2047 | 440 / 2077 | 1111 / 4750 | 2195 / 9002 | `0e7ae0f51` | `d5e1836` |
| 5 | no unit read before a positive first goal | 433 / 2047 | 439 / 2077 | 1111 / 4750 | 2195 / 9002 | `fa2710ff7` | |
| 6 | no seed source for relations no row reaches | 405 / 1943 | 439 / 2077 | 989 / 4456 | 2195 / 9002 | `f18a244ea` | |
| 7 | semantic decode as one join | 383 / 1957 | 417 / 2091 | 923 / 4470 | 2035 / 9044 | `8e496cd22` | |
| 8 | terms interned through `intern_terms` only | 351 / 1845 | 379 / 1958 | 887 / 4344 | 1997 / 8911 | `f93503e8b` | `668946b` |
| 9 | threshold only where duplicates can arise | 351 / 1845 | 379 / 1958 | 876 / 4256 | 1997 / 8911 | `6e1f9c829` | |

Nodes removed per change against the census prediction (net, hello #1 / #2 / #3 / registry):

| # | measured | census | difference |
|---|---|---|---|
| 1 | 353 / 356 / 606 / 767 | 353 / 356 / 606 / 767 | none |
| 2 | 202 / 202 / 231 / 461 | 222 / 222 / 198 / 448 | measured after 1, which already shared the term-map reads; the Mfp-over-Mfp fusion also folds chains that hold no literal |
| 3 | 49 / 52 / 125 / 147 | 91 / 97 / 243 / 287 | after 1 the sentinel's `Get` per source is the node every reader shares |
| 4 | 19 / 19 / 64 / 70 | 132 / 132 / 139 / 174 | after 2 most project-only Mfps over a Join absorbed a literal Mfp and carry a filter or map |
| 5 | 1 / 1 / 0 / 0 | 66 / 66 / 129 / 160 | after 1 one `Get(__ir_unit)` per install; #3 and the registry keep it for kernel-first rules and unit-row literals |
| 6 | 28 / 0 / 122 / 0 | 28 / 26 / 139 / 157 | #2 (`IrSession::prepare`) and the registry (serves `fs.file`) lower unsealed |
| 7 | 22 / 22 / 66 / 160 | 24 / 24 / 72 / 166 | |
| 8 | 32 / 38 / 36 / 38 | 32 / 38 / 36 / 38 | none |
| 9 | 0 / 0 / 11 / 0 | 0 / 0 / 14 / 14 | registry unsealed (change 6) |
| all | 706 / 690 / 1261 / 1643 | 757 / 767 / 1250 / 1783 | |

dd operators: hello 3897 / 3936 / 6960 -> 1845 / 1958 / 4256; registry 13770 -> 8911. Change 7 adds
14 / 14 / 14 / 42 operators (a join on the derived `__ir_semantic` arranges both inputs where two
dictionary decodes cost 2 operators each). `DL8_ONE_ENGINE=1` hello, the one composed install: 4676
nodes / 15302 operators / 1034 arrangements -> 1737 / 8328 / 390.

No change skipped. Changes 6 and 9 apply only where the condition holds statically (section 2).

## 2. How it works

| # | where | rule |
|---|---|---|
| 1 | `Builder::node` | The op's `Debug` text (a `Union` with its inputs sorted) keys a map to its node; an equal op returns the existing node. Every op is a function of its operator and its inputs' collections (`Mint` of functor and columns, `Get` of a relation with one binder). `substitute` and the narrowing pass key their memos by node id; a shared node stays a valid key. |
| 2 | `ivm_ir::Program::terms`, `Expr::Term`, `join_ground`, `ground_row`, `fuse_projections` | A ground term in a rule is a term-table entry `TermLit { functor, args }` (args: an earlier entry, a text, a raw cell); `Expr::Term(i)` is its cell, typed `Id`. ivm-dd mints the table at install (the rows enter the constructor inputs with the first settle); ivm-sqlite interns it in `terms::install` and renders `Term(i)` as a scalar read of its constructor table. A keyed ground argument is `Mfp { filter: col = Term(i), map: [TermId, Term(i)] }`, a cross one the same map without the filter: the appended columns match the old term-map join, so no column bookkeeping changed. `ir.texts` holds only texts the term table names, so a program's IR stays equal across comptime rounds whose seeds carry new strings (session reuse by fingerprint). Terms that arrive after install still enter through `__ir_term_map` rows. `fuse_projections` then folds an Mfp into its reader Mfp when nothing else reads it (filters run before maps, so an inner map an outer filter reads is inlined). The IR bundle replaces each `Term(i)` with the cell it reserves; ivm-rxjs rejects a program with a term table. |
| 3 | lowering, `ivm_ir::compose`, ivm-sqlite bundle | No `__ir_all_sources`. SQLite creates every source table from `Program::rels` and names a host row's columns `c0..` when no plan scans the table; the bundled SQLite program is named `__ivm_bundle` (a composed first output's name holds a dot). The rung 10 pin moved 2 -> 3 (interpret 3 -> 4): the sentinel's row weight changed on the request settle and counted as one written row there. |
| 4 | `Op::Join { project }`, `fuse_projections`, `ivm_engine::lower_node` | A project-only Mfp over a Join it alone reads becomes the Join's `project` (empty = all columns). `projected_join` builds a two-input non-decode Join with a project as `join_project` where the engine fuses; otherwise the join or decode, then an Mfp of the projection. ivm-rxjs picks the projected columns per joined row. |
| 5 | `compile_rule` | `Get(__ir_unit)` only when the first goal is a kernel, negative, served, or the body is empty. A positive first read sets `current` before anything reads it. |
| 6 | `Declared::sealed`, `evaluate_compiled` | The comptime round path lowers sealed: rows enter only as the round's seeds (the next round's frozen edges, intern snapshots and answers are that round's seeds) and served answers. With no served relation, a relation an ordinary rule heads gets no seed source unless a seed or declared source names it, it is a wave or cone relation, or it is `edge_snapshot` / `intern_snapshot`; its Let is the Threshold of its bodies, its LetRec body their Union. Unsealed: runtime runs (`--frontiers`, live) and `IrSession::prepare` (frontiers and `apply` may name any relation), and any program with a served relation (an executor's answers may write any relation: `fs.json` writes `:` rows; `_5_store_host::ir_retracting_a_goal_retracts_its_request_and_answers` caught the unsealed answer case). |
| 7 | `join_constructor` | A decode through `__ir_semantic` is one Join with `Get(__ir_semantic)`, the Union of the `9:1:ref` and `9:1:const` constructors, whose ids are disjoint. The Let sits at `semantic_stratum`, before every reader. |
| 8 | lowering, `ir_bundle::with_prewarm` | No prewarm source, Get and Mint per constructor shape: sessions intern every term through `Engine::intern_terms` (`bootstrap_changes`, `install_terms`); `TermSpec` loses `source`/`output`, `Lowered` loses `installers`. The bundle's generated runtime has no `intern_terms`, so the bundle adds the prewarm relations and Lets ahead of every stratum, in `Lowered::constructors` order, and reserves cells in that order as before. A program with no relations now has no output; ivm-sqlite installs it (source tables, no plan) where it refused one before. |
| 9 | `lower_declared`, ready Let strata | A relation with no seed source and one rule body skips the Threshold when the rule has no aggregate, a head of variables only, every variable of its positive relation goals in the head, and no `intern` goal (its listed and vector branches are a Union). Inputs are sets (sources, Thresholded relations, LetRec variables, constructors); the other kernels are functions of their inputs. |

## 3. Parity (after change 9)

| run | result |
|---|---|
| `cargo test -j 4 --no-fail-fast -- --skip _7_book --test-threads=1` (dd default) | 182 passed (2 + 180), 0 failed, 5 ignored |
| same, `DL8_ENGINE=ir:sqlite` | 182 passed, 0 failed |
| same, `DL8_ONE_ENGINE=1` | 182 passed, 0 failed |
| registry `dl8 build std/tsi_registry_emit.dl7`, `cmp` with `hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs` | identical: dd and sqlite, `DL8_ONE_ENGINE` 0 and 1 |
| macrotime oracle `every_macrotime_case_matches_v7` | passes in all three suite runs |
| census `dl8 compile`, 132 programs (`fixtures examples book/src/probes std`), stdout + exit vs a `f63e7a1ea` build, both run from this worktree | dd 132 equal; sqlite 132 equal; dd `DL8_ONE_ENGINE=1` 132 equal |
| sqlite_ivm `cargo test -j 4 --workspace --no-fail-fast -- --test-threads=1` (extension dylib rebuilt) | 265 passed, 0 failed, 14 ignored |

Every commit ran the dd suite; changes 2-4 and 6-9 also the sqlite suite. Pins re-recorded in the
commit that moved them: `tsi_registry_sqlite_work_counts` (changes 1-8), rung 10 edit writes
(change 3). The sqlite_ivm `extension` test once failed on a stale shared temp target
(`ivm-sqlite-extension-target`, a `libsqlite3-sys` build dir without `bindgen.rs`); removing that
build dir made it pass.

## 4. Engine-neutral IR capabilities added (sqlite_ivm)

| capability | crates |
|---|---|
| `Program::terms` + `Expr::Term`: ground terms interned at install | ivm-ir, ivm-engine (`eval_with_text`, `eval_typed_with_text` take a term resolver), ivm-dd, ivm-sqlite, compose, ivm-rxjs (rejects) |
| `Op::Join { project }` | ivm-ir, ivm-engine `lower_node` / `projected_join`, compose, ivm-rxjs |
| compose without the all-sources sentinel | ivm-ir |
| a program with no output | ivm-sqlite |
