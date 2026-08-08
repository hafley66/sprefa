# REPORT (lane extract-ham, pass 1 of 2, 2026-08-08)

Read-only recon: run the main tree's `extract` binary against the v6 tree, measure v5 liftoff, inventory call facts, catalog gaps. Worktree: `/Users/chrishafley/projects/sprefa-lanes/extractham`, branch `lane/extract-ham`. Base verified: `git rev-parse HEAD` = `cfbe10f360f9a07b65c11e0583cda5a890075b45` (matches the pinned commit).

Binary used (read-only, main tree build): `/Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract`. Verified via `--help` (languages + options) and `--schema` (record shapes: `node/site/resolved_edge`; families `cst,type,call,df`, plus modes `scip`, `diet_scip`).

## 0. Deviations from the brief (reality vs brief)

| # | Deviation | Detail |
|---|-----------|--------|
| D1 | Pinned audit tree is NOT the main tree's HEAD | Lane worktree HEAD `cfbe10f`; main tree `/Users/chrishafley/projects/sprefa` HEAD is `689a6cb` (ahead). Differences observed in `v6/tsv2`: main-only `gen_served/`, `node_modules/`, `golden-flex.ts`, `scale-floor-history.jsonl`; `scripts/intern-ab-classify.ts` differs. Audited the pinned lane tree as the brief pins; the extract **binary** is a build of the ahead main tree, so binary source and audited source are not byte-identical. |
| D2 | `v6/tsv2/scripts/pack` does not exist at the pinned commit | Task 3 listed it as a sweep target. No `pack/` dir in the lane tree. Swept the other scope dirs (runtime, serve, cli, sprefa-store/js/src). The repo-root `scripts/pack/sprefa-run.ts` (a tether, section 2) is the only `pack` reference. |
| D3 | Pre-existing `REPORT.md` overwritten | The worktree's committed `REPORT.md` was Lane C's ("unread-rel skip contract rails", base `e1a9696f`). The brief designates `REPORT.md` as the single writable file; following that instruction overwrote the other lane's artifact. Coordinator should re-point the Lane C lane before its pass 2 if it still needs the file. |
| D4 | Task-6 "prolog blindness" framing is half wrong | `.pl`/`.pl` prolog is fully covered by extract (`cst,type,call,df`). The below-horizon artifact is `.dl6` (silently skipped), not prolog source. Detail in section 6. |

## 1. Recent-touch map (last month)

Command (run at lane worktree root):

```
git log --since='1 month ago' --name-only --format= -- v6/ | grep -v '^$' | sort | uniq -c | sort -rn
```

10661 distinct touched paths. Top 30 by touch count, with line count measured at current HEAD (`wc -l <file>`; three files moved by commit `8a7d2a12` "prolog folder-cycle repair", line counts taken at their new compile-root path and so marked):

| # | touch | lines | path |
|---|-------|-------|------|
| 1 | 324 | 1503 | `v6/INDEX.md` |
| 2 | 147 | 985 | `v6/prolog/ARCH.pl` |
| 3 | 107 | 5076 | `v6/prolog/compile/test/plunit_tests.pl` |
| 4 | 97 | 310 | `v6/prolog/compile/out/manifest.json` |
| 5 | 84 | 1479 | `v6/prolog/compile/out/run-results.json` |
| 6 | 53 | 1739 | `v6/prolog/analyze.pl` (moved from `compile/`) |
| 7 | 51 | 389 | `v6/justfile` |
| 8 | 43 | 993 | `v6/tsv2/runtime/types.ts` |
| 9 | 43 | 614 | `v6/prolog/conformance/rulings.pl` |
| 10 | 43 | 668 | `v6/prolog/conformance/engine.pl` |
| 11 | 43 | 4716 | `v6/prolog/lower.pl` (moved from `compile/`) |
| 12 | 37 | 611 | `v6/prolog/compile/registry.pl` |
| 13 | 37 | 2647 | `v6/prolog/emit_ts.pl` (moved from `compile/`) |
| 14 | 36 | 455 | `v6/tsv2/gen_emitted/switch_as_keyed_replace.ts` |
| 15 | 36 | 363 | `v6/tsv2/gen_emitted/demand_laziness_effect_rows.ts` |
| 16 | 35 | 455 | `v6/prolog/compile/out/switch_as_keyed_replace.ts` |
| 17 | 35 | 363 | `v6/prolog/compile/out/demand_laziness_effect_rows.ts` |
| 18 | 34 | 360 | `v6/tsv2/gen_emitted/terminal_is_terminal.ts` |
| 19 | 34 | 337 | `v6/tsv2/gen_emitted/shuffled_arrival_reorders_log_deltas.ts` |
| 20 | 34 | 337 | `v6/tsv2/gen_emitted/log_deltas_follow_arrival_order.ts` |
| 21 | 34 | 337 | `v6/tsv2/gen_emitted/level_view_reads_set_projection_not_occurrences.ts` |
| 22 | 34 | 433 | `v6/tsv2/gen_emitted/head_move_flips_current_tree_in_one_tick.ts` |
| 23 | 34 | 453 | `v6/tsv2/gen_emitted/fill_as_cache_update_swr.ts` |
| 24 | 33 | 429 | `v6/tsv2/gen_emitted/zombie_scope_negative_case_a2b.ts` |
| 25 | 33 | 363 | `v6/tsv2/gen_emitted/shared_demand_refcount.ts` |
| 26 | 33 | 360 | `v6/prolog/compile/out/terminal_is_terminal.ts` |
| 27 | 33 | 337 | `v6/prolog/compile/out/shuffled_arrival_reorders_log_deltas.ts` |
| 28 | 33 | 337 | `v6/prolog/compile/out/log_deltas_follow_arrival_order.ts` |
| 29 | 33 | 337 | `v6/prolog/compile/out/level_view_reads_set_projection_not_occurrences.ts` |
| 30 | 33 | 433 | `v6/prolog/compile/out/head_move_flips_current_tree_in_one_tick.ts` |

Per-directory rollup of the same touch data, **node_modules excluded** (committed `v6/dl/node_modules` accounts for 7497 of 7595 v6/dl touches; it is dependency noise, not source signal):

| dir | touch-events | distinct files |
|-----|-------------|----------------|
| `v6/prolog` | 7909 | 1905 |
| `v6/tsv2` (other rollup, incl. gen_emitted) | 6066 | 861 |
| `v6/sprefa-store` | 441 | 158 |
| `v6/sprefa-extract` | 401 | 117 |
| `v6/dl` (excl. node_modules) | 370 | 98 |
| `v6/tsv2/runtime` | 120 | 15 |
| `v6/tsv2/serve` | 64 | 8 |

The `v6/tsv2` (other) row breaks down as: `gen_emitted/*.ts` 4408 evts / 219 files, `v6-root/tsv2` 550 / 251, `INDEX.md` 324 / 1, `labs` 255 / 147, `plans` 79 / 31, `sprefa-seed` 78 / 36, `bench-cli` 68 / 22, `labkit` 52 / 38, `justfile` 51 / 1, `hs-prolog` 42 / 41, rest (tools, findings, lab-*, *.md) below 32 each. `v6/sprefa-extract` shows 401 evts over 117 files because its 29 `.rs` + docs churn heavily; the Rust self-extract is section 5. `v6/tsv2/serve` (8 files) and `cli` are small by surface but not by activity depth.

## 2. v5 liftoff audit

### 2a. Mechanical grep sweeps over `v6/` (excl. node_modules/.git)

Command pattern: `grep -rIn --include='*' --exclude-dir=node_modules --exclude-dir=.git '<pat>' v6/`

| sweep | hits | what the hits are | verdict |
|-------|------|-------------------|---------|
| `src/engine` | 245 | v6-internal path `v6/sprefa-store/js/src/engine/*` (engine lib), plus lab fixture path strings `"src/engine/lower/pass_0/..."` (intern_bench keys.rs, exec_shootout). Zero references to the repo-root v5 `src/engine` as a dependency. | lifted off (source) |
| `\.dl"` | 29 | shell scripts naming transient `"$WORK/*.dl"` / `"$label.dl"` files for the bench/parity harness (see 2c). Not source coupling. | lifted off (source) |
| `dl daemon` | 4 | all `v6/plans/*.md` prose documenting a desired v6 feature name. | doc-only |
| `rel_port` | 3 | all `v6/plans/*.md` prose naming a v5 rel for a graph-layer decision. | doc-only |
| `../../src` | 21 | all `v6/sprefa-store/js/tests/../../src/...` v6-internal test imports ("src/engine/counter.ts" etc). | v6-internal |
| `crate::` | 191 | normal intra-crate Rust paths inside v6's own crates (`sprefa-store`, `sprefa-seed`, `sprefa-extract`, labs/exec_shootout). | v6-internal |

Source-level verdict: **lifted off**. No v6 source file imports or path-resolves into the repo-root v5 engine.

### 2b. Dependency edges (Cargo.toml / package.json)

Command: grep `path *=` in `v6/*/Cargo.toml` + labs Cargo.tomls; grep `file:/workspace:|link:` in the four non-node_modules package.jsons.

Result: every `path =` Cargo dep is v6-internal (e.g. `exec-shootout-harness = { path = "../harness" }`); grep for out-of-tree `path = "../.."` returned none. package.json edges: `v6/tsv2` and `v6/dl` both declare `"sprefa-store-engine": "link:../sprefa-store/js"`, an **intra-v6** link. No v6 Cargo.toml or package.json points at a repo-root crate or package.

Verdict: **lifted off**.

### 2c. Shell-outs to the v5 `dl` binary

Yes, v6 does shell out to the v5 binary, but only in benchmark/parity/regression harness receipt scripts (not product code):

| script | binary/var used |
|--------|-----------------|
| `v6/tsv2/labs/effect-chain/3_v5_collect.sh` | `$DL_BIN` (default `$HOME/.cargo/bin/dl`) |
| `v6/tsv2/scripts/crawl-bench.sh` | `$V5_BIN` on a written `v5-crawl.dl` |
| `v6/tsv2/scripts/flagship-flow.sh` | `$DL_V5_BIN` on `$REPO/examples/flow-interproc.dl` |
| `v6/tsv2/scripts/flagship-callgraph.sh` | `$DL_V5_BIN` on `$REPO/examples/callgraph-ast.dl` |
| `v6/tsv2/scripts/v5-parity.sh` | drives `v6/dl/fixtures/v5-parity.dl6` and compares v6 vs v5 |
| `v6/tsv2/goldens/multirepo_crawl/2_gate.sh` | `$DL_V5_BIN` on `$REPO/examples/version-skew.dl` |
| `v6/tsv2/scripts/lsp_diag_driver.py` | takes `DL_BIN` = v5 `dl` path as arg |

Verdict: engine lifted off; the **bench/parity harness is intentionally tethered** to the v5 binary to measure v6 against v5. This is the surviving v5 dependency in v6.

### 2d. Reverse tether: OUTSIDE v6 that references `v6/`

`grep -rIn 'v6/' .` (repo root) = 6533 hits, most under `archive/` (old salvage patches, noise). The active, non-archive tether (outside `v6/`, pointing into it):

| location | reference |
|----------|-----------|
| `dist-workspace.toml:2` | workspace members `["cargo:.", "cargo:v6/sprefa-extract"]` — the v6 extract binary is shipped in the **same cargo-dist workspace as the v5 crate** |
| `scripts/pack/sprefa-run.ts` | `import ... from "../../v6/tsv2/runtime/{1_incremental,3_subscribe,diff,rows,structPlane,2_boot,scratchStore,tickLoop,types}.ts"` |
| `scripts/release-local.sh` | uses `$REPO/v6/tsv2` and `use_module('$REPO/v6/prolog/compile.pl')` |
| `.githooks/pre-commit` | runs `v6/tools/gen-index.sh` and `v6/tsv2/scripts/comment-budget-rail.sh` |
| `justfile` (root, `:55` `:59`) | runs `v6/tsv2/scripts/crawl-bench.sh` and `devlog.sh` |
| `tools/chat-find.sh` | reads `v6/plans`, `v6/findings`, `v6/*.md` as design-doc search roots |
| `labs/teardown-flatten/receipts.sh` | points at `../../v6/tsv2` |

The strong tethers are the pack runner importing v6 runtime TS directly, and the dist workspace shipping v6's extract alongside v5's crate. The rest is tooling/docs.

## 3. Extract-over-v6 inventory

Scope: every `.ts` under `v6/tsv2/runtime`, `v6/tsv2/serve`, `v6/tsv2/cli`, and `v6/sprefa-store/js/src` (44 files; `scripts/pack` absent per D2). Command per file: `extract --family call <file>` to `/tmp/extractham/call/*.jsonl` (4509 lines total, 0 stderr).

Per-file function defs (call-family `node`: function/method/lambda) and call sites (`site`):

| file | fn | method | lambda | DEFS | SITES |
|------|----|--------|--------|------|-------|
| `sprefa-store/js/src/bench/prolog_emit_bench.ts` | 2 | 0 | 2 | 4 | 42 |
| `sprefa-store/js/src/bench/v1_scale_bench.ts` | 9 | 0 | 12 | 21 | 70 |
| `sprefa-store/js/src/engine/algo.ts` | 0 | 5 | 0 | 5 | 4 |
| `sprefa-store/js/src/engine/counter.ts` | 0 | 0 | 0 | 0 | 0 |
| `sprefa-store/js/src/engine/engine.ts` | 9 | 0 | 13 | 22 | 412 |
| `sprefa-store/js/src/engine/ingest.ts` | 17 | 0 | 58 | 75 | 244 |
| `sprefa-store/js/src/engine/lib.ts` | 5 | 18 | 29 | 52 | 149 |
| `sprefa-store/js/src/engine/measure.ts` | **0** | 0 | 0 | **0** | 61 |
| `sprefa-store/js/src/engine/oracle.ts` | 3 | 0 | 0 | 3 | 49 |
| `sprefa-store/js/src/engine/spine.ts` | 6 | 0 | 0 | 6 | 22 |
| `sprefa-store/js/src/engine/sqlRunner.ts` | 2 | 0 | 3 | 5 | 43 |
| `sprefa-store/js/src/engine/tasks.ts` | 0 | 29 | 7 | 36 | 65 |
| `sprefa-store/js/src/engine/types.ts` | 0 | 0 | 0 | 0 | 0 |
| `sprefa-store/js/src/gen/reach.gen.ts` | 0 | 0 | 0 | 0 | 12 |
| `sprefa-store/js/src/index.ts` | 0 | 0 | 0 | 0 | 0 |
| `sprefa-store/js/src/lower/ast.ts` | 14 | 0 | 0 | 14 | 7 |
| `sprefa-store/js/src/lower/lower.ts` | 15 | 1 | 28 | 44 | 151 |
| `sprefa-store/js/src/lower/lowerSql.ts` | 5 | 25 | 43 | 73 | 207 |
| `sprefa-store/js/src/lower/rulegraph.ts` | 3 | 1 | 12 | 16 | 55 |
| `sprefa-store/js/src/lower/types.ts` | 0 | 0 | 0 | 0 | 0 |
| `tsv2/cli/0_inventory.ts` | 0 | 0 | 0 | 0 | 0 |
| `tsv2/cli/bop.ts` | 11 | 0 | 19 | 30 | 144 |
| `tsv2/runtime/0_traceSchema.ts` | 0 | 0 | 0 | 0 | 0 |
| `tsv2/runtime/1_incremental.ts` | 32 | 0 | 81 | 113 | 518 |
| `tsv2/runtime/2_boot.ts` | 1 | 0 | 1 | 2 | 12 |
| `tsv2/runtime/3_subscribe.ts` | 2 | 0 | 0 | 2 | 22 |
| `tsv2/runtime/diff.ts` | 2 | 0 | 0 | 2 | 13 |
| `tsv2/runtime/rows.ts` | 3 | 0 | 0 | 3 | 20 |
| `tsv2/runtime/scratchStore.ts` | 0 | 0 | 0 | 0 | 3 |
| `tsv2/runtime/serveStats.ts` | 2 | 0 | 3 | 5 | 18 |
| `tsv2/runtime/structPlane.ts` | 10 | 0 | 12 | 22 | 95 |
| `tsv2/runtime/textPlane.ts` | 3 | 0 | 5 | 8 | 27 |
| `tsv2/runtime/tickLoop.ts` | 2 | 0 | 0 | 2 | 11 |
| `tsv2/runtime/ticklog.ts` | 6 | 0 | 4 | 10 | 34 |
| `tsv2/runtime/trace.ts` | 0 | 0 | 0 | 0 | 2 |
| `tsv2/runtime/types.ts` | 0 | 0 | 0 | 0 | 0 |
| `tsv2/serve/0_compile.ts` | 4 | 0 | 7 | 11 | 45 |
| `tsv2/serve/0_trace.ts` | 1 | 0 | 5 | 6 | 24 |
| `tsv2/serve/1_hosts.ts` | 16 | 9 | 59 | 84 | 219 |
| `tsv2/serve/2_binds.ts` | 8 | 6 | 27 | 41 | 122 |
| `tsv2/serve/3_engine.ts` | 6 | 5 | 13 | 24 | 67 |
| `tsv2/serve/4_http.ts` | 20 | 0 | 35 | 55 | 168 |
| `tsv2/serve/main.ts` | 0 | 0 | 2 | 2 | 12 |
| `tsv2/serve/reloadPlan.ts` | 2 | 0 | 0 | 2 | 17 |
| **TOTAL** | **221** | **99** | **480** | **800** | **3186** |

### 3a. Zero-call-site candidate dead exports

Commands, per package group (the two logical packages: `v6/sprefa-store/js/src` = "store", `v6/tsv2/{runtime,serve,cli}` = "tsv2"):

```
extract --resolve --family call $(cat pkg-store.txt) > resolve-store.jsonl   # 232 resolved_edge
extract --resolve --family call $(cat pkg-tsv2.txt) > resolve-tsv2.jsonl     # 209 resolved_edge
```

A def is "called" if it appears as `callee_path+callee_name` in the package's resolved-edge set. Defs not reached are labeled **CANDIDATE**.

| package | defs | resolved callee targets | candidates |
|---------|------|------------------------|------------|
| store (`sprefa-store/js/src`) | 176 | 123 | 54 |
| tsv2 (`runtime`+`serve`+`cli`) | 165 | 116 | 49 |

**Limitation stated (per brief): the label is CANDIDATE, not DEAD.** The resolve universe is per-package only, so every use that extract cannot see registers as a false-positive candidate: (1) imports from *outside* the sweep set (store functions called by `tsv2` via the `link:../sprefa-store/js` package, and by `js/tests/*`, neither in the resolve set); (2) re-exported names (`index.ts` barrel, e.g. `lowerProgram`, every `ast.ts` constructor `relRef/notRel/v/lit/wild/...`); (3) dynamic/string dispatch (CLI command handlers in `bop.ts` `command_summary/check/serve/load/query/stats/ticks`, and `main`/`run` bench entrypoints). Representative false-positive examples are bolded below.

Notable CANDIDATE names (store): `engine.ts` `fixpoint_rounds/uncounted_query/uncounted_multi/exec/query_ids/query_bigints`; `ingest.ts` `parse_line/ingestJsonl` (exported API, used by tests); `lib.ts` `open_db/open` (exported, used by tests); `sqlRunner.ts` `bracket/splitStatements`; `spine.ts` `family_as_i32/family_from_i32/revkind_as_i32/table_names`; **`lower/ast.ts` `relRef/notRel/v/lit/wild/compare/headVar/headAgg/edbRel/derivedRel`** (re-exported constructors); **`lower.ts` `lowerProgram`** (exported, exercised by `js/tests/lower/*`); `tasks.ts` `upsert_node/upsert_edges/...`.

Notable CANDIDATE names (tsv2): `bop.ts` CLI handler names (dynamic dispatch); `1_incremental.ts` `keyed_arrival_rows_statement/stage_ordered_frontiers/storage_row/apply_edge_statement/apply_level_statement/recursive_heads/sequence_work/reconcile_ref_count_statement/apply_retention_statement/boundary_delta` (SQL-statement builders, exported and used by the pack runner outside the package); `structPlane.ts` `intern_one_type`; `3_subscribe.ts` `stored_names`; `serve/1_hosts.ts` `runSprefaExtract` (exports the extract host).

Many of these are clearly alive across the package boundary; the list is the eyeball starting set, not a demolition list.

### 3b. 15 largest functions by span (cleanup targets)

`span end - start` bytes, from the per-file call extraction. Files shown as lane-relative paths (globs were `v6_tsv2_runtime_*.jsonl` etc). `None` = anonymous lambda (naming them is part of the cleanup).

| span B | file | name | byte span |
|--------|------|------|-----------|
| 6006 | `v6/tsv2/runtime/1_incremental.ts` | `maintain_head_in_place` | [24351,30357) |
| 4123 | `v6/tsv2/runtime/1_incremental.ts` | `reconcile_ref_count_statement` | [20014,24137) |
| 3781 | `v6/sprefa-store/js/src/engine/ingest.ts` | `resolve_nodes` | [13998,17779) |
| 3425 | `v6/sprefa-store/js/src/engine/ingest.ts` | (lambda) | [14345,17770) |
| 3258 | `v6/sprefa-store/js/src/lower/rulegraph.ts` | `stratify` | [8472,11730) |
| 3200 | `v6/sprefa-store/js/src/lower/lowerSql.ts` | `compileRuleJoin` | [9733,12933) |
| 3142 | `v6/sprefa-store/js/src/lower/lowerSql.ts` | `refCountPlan` | [6409,9551) |
| 2990 | `v6/sprefa-store/js/src/engine/ingest.ts` | (lambda) | [14660,17650) |
| 2962 | `v6/sprefa-store/js/src/engine/ingest.ts` | (lambda) | [14687,17649) |
| 2876 | `v6/tsv2/serve/2_binds.ts` | `WatchBindRunner` | [17688,20564) |
| 2747 | `v6/tsv2/runtime/1_incremental.ts` | (lambda) | [27282,30029) |
| 2526 | `v6/tsv2/serve/2_binds.ts` | (lambda) | [17964,20490) |
| 2435 | `v6/tsv2/serve/3_engine.ts` | `run_batch` | [4945,7380) |
| 2430 | `v6/tsv2/runtime/structPlane.ts` | `intern_one_type` | [8026,10456) |
| 2388 | `v6/sprefa-store/js/src/engine/ingest.ts` | `resolve_rels` | [21056,23444) |

### 3c. Call-family empty output

7 files produced 0 defs AND 0 sites (all legitimate type/const/barrel modules): `engine/counter.ts`, `engine/types.ts`, `index.ts`, `lower/types.ts`, `runtime/types.ts`, `runtime/0_traceSchema.ts`, `cli/0_inventory.ts`. A call-family sweep returns nothing for a type-only module with no way to say "this file is type-only", which is a horizon gap (section 4).

## 4. Gap catalog (dogfood findings)

Command receipts per row. All findings surfaced while running task 3.

| # | gap | example file / spot | what a family would need |
|---|-----|----------------------|---------------------------|
| G1 | SQL text inside template literals is invisible to call facts | `v6/tsv2/runtime/1_incremental.ts` line 120/137/1367 `... FROM json_each(?)` (3 occurrences). `extract --family call` emits nothing for the SQL. | A `sql` fact family that parses the string content of SQL statement templates and emits the SQL function calls (`json_each`, `json_extract`, ...) as callable sites. |
| G1a | ...reachability of that same SQL from cst | Same file, `extract --family cst`: `template_string` (43) and `template_substitution` (72) nodes exist; all 3 `json_each` byte offsets fall inside a `template_string` span. So the content is *reachable* at the cst layer only as a byte span you must re-read; there is no decoded structure. `extract --family type` const-template records: 3 captured, **none** carry the `json_each ...` INSERT text (only `DELETE FROM ${table}`, a CASE concat, and a VALUES concat). | A cst-adjacent string-content fact (already spans exist; needs the source slice or dedicated SQL parse). |
| G2 | `export namespace { }` emits zero call defs | `v6/sprefa-store/js/src/engine/measure.ts`: ~20 `export function` inside `export namespace benchgraph`/`memcap`; `extract --family call` reports 0 function/0 method/0 lambda but 61 `site` facts. Minimized repro (`export namespace benchgraph { export function gen(...){...} }`) confirms empty call output; cst still shows `function_declaration` nested under `internal_module`. | The call front-end must register function/method/lambda defs nested in TS `internal_module` (namespace), instead of only top-level/module-scope ones. |
| G3 | Cross-package calls are invisible to a single-package `--resolve` | `tsv2` runtime/serve import the engine via `sprefa-store-engine = "link:../sprefa-store/js"`. A tsv2-only `--resolve --family call` yields 209 edges; calls into the linked store package never appear, and `site.callee_path` stays `null` in single-file mode | `--resolve` needs to either cross the `link:` package boundary or take a multi-root corpus so cross-package edges resolve. Without it, section 3a's candidate list over-labels (false-positive dead exports). |
| G4 | `site.callee_path` null in default (phase-1) mode for qualified calls | throughout task-3 output, e.g. `dispatch.rs` `source_for` callee with `callee_path:null` | Default mode gives the trailing callee name only; qualified-path/alias resolution only appears under `--resolve`/SCIP. |
| G5 | `.sh` scripts get cst only, no flow | `v6/labs/exec_shootout/dl6/bench.sh`: `--family call` = 0, `--family cst` = 957 nodes. These harnesses shell out to `dl` (section 2c) | A call/flow family for shell (or at minimum a `file_edge`-style "this .sh invokes binary dl/other scripts" fact). |
| G6 | `.dl6` silently skipped (the engine's own defining language) | `v6/labs/exec_shootout/dl6/reachability.dl6`, `v6/prolog/labs/.../6-ordinal.dl6`: exit 0, 0 stdout, 0 stderr. Extract cannot dogfood on v6's own DL source. | A DL/Datalog grammar front-end for the v6 `.dl6` dialect (cst+type+call at minimum). |
| G7 | `.jsonl` receipts below horizon | `v6/tsv2/p1-receipts.jsonl` etc (544 jsonl files touched in the month, section 6); and `compile/out/{manifest,run-results}.json` only via cst. | A JSON/JSONL fact family or a `file`-fact only. (JSON is cst-only today.) |
| G8 | type/const-only module gives a blank call sweep with no signal | 7 files in section 3c | A per-file coverage marker (e.g. `file` fact noting "0 callables, N types") so a blank result is distinguishable from a parse failure. |
| G9 | generated vs handwritten indistinguishable in facts | `sprefa-store/js/src/gen/reach.gen.ts` (generated) and all `gen_emitted/*.ts` are extracted identically to source; no "generated" bit | A `file`-fact generated marker (it is the source of much of the month's churn, section 1). |

## 5. Rust side: does extract self-extract?

Command: `extract --family call` over each of 29 `.rs` in `v6/sprefa-extract/src`.

**Yes, it self-extracts.** Per-file defs (function+method+lambda) and sites:

| file | fn | method | lambda | DEFS | SITES |
|------|----|--------|--------|------|-------|
| `src/0_query.rs` | 13 | 0 | 19 | 32 | 151 |
| `src/bin/extract.rs` | 13 | 0 | 8 | 21 | 151 |
| `src/bin/extract/help.rs` | 0 | 0 | 0 | 0 | 0 |
| `src/deps.rs` | 11 | 4 | 15 | 30 | 150 |
| `src/dispatch.rs` | 1 | 0 | 1 | 2 | 3 |
| `src/family.rs` | 0 | 0 | 0 | 0 | 0 |
| `src/lang/astgrep.rs` | 1 | 8 | 7 | 16 | 73 |
| `src/lang/go.rs` | 33 | 7 | 51 | 91 | 599 |
| `src/lang/kotlin.rs` | 25 | 5 | 32 | 62 | 406 |
| `src/lang/mod.rs` | 2 | 0 | 1 | 3 | 5 |
| `src/lang/prolog/_0_source.rs` | 23 | 5 | 13 | 41 | 266 |
| `src/lang/prolog/mod.rs` | 0 | 0 | 0 | 0 | 0 |
| `src/lang/rust.rs` | 39 | 11 | 29 | 79 | 566 |
| `src/lang/ts.rs` | 59 | 30 | 24 | 113 | 671 |
| `src/lib.rs` | 0 | 0 | 0 | 0 | 0 |
| `src/project.rs` | 20 | 4 | 19 | 43 | 210 |
| `src/rows.rs` | 0 | 0 | 0 | 0 | 0 |
| `src/schema.rs` | 0 | 0 | 0 | 0 | 0 |
| `src/scip.rs` | 7 | 9 | 19 | 35 | 166 |
| `src/scip/scip_proto.rs` | 0 | 16 | 0 | 16 | 268 |
| `src/scip_decode.rs` | 7 | 0 | 17 | 24 | 104 |
| `src/scip_ensure.rs` | 16 | 4 | 25 | 45 | 203 |
| `src/scip_rows.rs` | 6 | 4 | 10 | 20 | 147 |
| `src/scip_v5_rels.rs` | 10 | 2 | 24 | 36 | 179 |
| `src/seams.rs` | 0 | 0 | 0 | 0 | 0 |
| `src/shape.rs` | 0 | 0 | 0 | 0 | 0 |
| `src/source.rs` | 0 | 0 | 0 | 0 | 0 |
| `src/types.rs` | 6 | 46 | 13 | 65 | 97 |
| `src/wire.rs` | 8 | 0 | 10 | 18 | 163 |
| **TOTAL** | **300** | **155** | **337** | **792** | **4578** |

Zero-output files are declaration/const/doc modules: `help.rs` (all `LONG_ABOUT`/doc string consts), `family.rs`/`rows.rs`/`schema.rs`/`shape.rs`/`source.rs`/`lib.rs` (type, const and mod declarations), `lang/prolog/mod.rs` (re-export module). `ts.rs`/`go.rs`/`kts.rs`/`rust.rs` are the front-end heavies and correctly yield the most defs. `scip_proto.rs` is a protobuf-generated file with 16 methods and 268 sites.

## 6. Prolog blindness statement

Confirming what extract accepts (from `--help` coverage + feeding it files): extract infers language from extension and **fully** covers `pl/pro/prolog/datalog/horn` (kinds cst/type/call/df), `ts/tsx/...`, `rs`, `go`, `kt/kts`. `html/yaml/json/css` and `python/java/c/cpp/cs/rb/php/sh/lua/scala/swift/ex/hs` are cst-only. **Not covered (silent, exit 0, zero lines): `md`, `toml`, `xml`, and any unknown extension including `.dl6` and `.jsonl`.** Receipts: `extract v6/prolog/lower.pl` = 57,751 fact lines (prolog is *not* below the horizon); `extract v6/labs/exec_shootout/dl6/reachability.dl6` = 0 lines stdout, 0 stderr, exit 0 (below horizon, silently); `extract v6/index.md` = 0 lines; shell is cst-only (`bench.sh`: call 0, cst 957).

Fraction of recently-touched files below horizon (extensions not producing any extract family):

| view | below-horizon touch-events | below-horizon files | % events | % files |
|------|---------------------------|---------------------|----------|---------|
| all touch data | 6277 / 30370 | 3231 / 10661 | 20.7% | 30.3% |
| excluding `node_modules` | 2658 / 15371 | 1422 / 3162 | 17.3% | **45.0%** |

The message factor: excluding committed `node_modules`, roughly **45% of recently-touched files are below extract's horizon**, driven by `map` source maps (node_modules), `md` (852 evts / 221 files), `dl6` (822 / 455), `jsonl` (601 / 544), `dl` (184 / 124, though no committed `.dl` exists in v6 today), `toml`/`tsv`/`lock`/`yml`. The sub-horizon set is dominated by documentation, DL-format sources (`.dl6`), and JSONL receipts, not by extract-covered code.

The brief's "prolog, .dl6, .sh, .md" framing needs one correction: **prolog (`.pl`) is covered**; the actually-invisible prolog-world artifact is `.dl6` (the current DL source form) and `.dl` (v5's form, none committed in v6). So extract covers the engine's Rust/TS/pl implementation across the board, but sits below the horizon for its own DL-defined language and for the JSONL receipts it would want to consume.

---

### Done-signal summary (for the coordinator hail)

v5-liftoff verdict: engine source lifted off (zero v5 imports/deps); surviving v5 tether is only the bench/parity harness + the reverse ship-time tethers (`dist-workspace.toml` member, `scripts/pack/sprefa-run.ts`, `.githooks/pre-commit`). Gaps cataloged: 9 (G1-G9), headlined by SQL-in-template invisibility, `export namespace` def loss, cross-package `--resolve` blindness, and `.dl6` being below the extract horizon.
