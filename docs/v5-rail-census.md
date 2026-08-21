# v5 rail census

Every `.dl` file outside `v6/`, bucketed. Measured 2026-08-21 at `6967750a7`.

## Contents

- [Counts](#counts)
- [How a file lands in a bucket](#how-a-file-lands-in-a-bucket)
- [1. Ported: a dl6 twin exists](#1-ported-a-dl6-twin-exists)
- [2. Portable as-is](#2-portable-as-is)
- [3. Blocked](#3-blocked)
- [4. Dead](#4-dead)
- [5. What still runs a v5 rail](#5-what-still-runs-a-v5-rail)
- [6. The two live rails named in CLAUDE.md](#6-the-two-live-rails-named-in-claudemd)

## Counts

The corpus is 195 files, not the 163 the brief estimated.

```bash
find . -name '*.dl' -not -path './v6/*' -not -path './.git/*' -type f | wc -l
# 195
```

| directory | files |
|---|---|
| `examples/` | 132 |
| `.dl/` | 28 |
| `bench/` | 16 |
| `std/` | 9 |
| `deck/snippets/` | 8 |
| `assets/`, `tree-sitter-dl/test/` | 2 |

| bucket | files | share |
|---|---|---|
| ported (a dl6 twin exists) | 4 | 2% |
| portable as-is | 35 | 18% |
| blocked | 144 | 74% |
| dead | 12 | 6% |

## How a file lands in a bucket

Each file's tokens are matched against v5's op set and v5's 112 built-in
relation names, then against the DOOR column of
[docs/v5-extraction-parity.md](v5-extraction-parity.md).

| bucket | rule |
|---|---|
| ported | a `.dl6` file names it, or a receipt script runs both sides |
| portable | referenced somewhere, and every construct it uses has a DOOR=yes dl6 spelling |
| blocked | referenced somewhere, and at least one construct is DOOR=no |
| dead | no justfile, CI workflow, shell script, rust test or markdown file names it |

`comment_node` and `template_parts` count as blockers: their SPANS are reachable
(cst nodes) but their `text` column is not, and every program that uses them
reads the text.

## 1. Ported: a dl6 twin exists

| v5 rail | dl6 twin | receipt |
|---|---|---|
| `examples/callgraph-ast.dl` | `v6/dl/fixtures/flagship-callgraph.dl6` | `just flagship` -> `v6/tsv2/scripts/flagship-callgraph.sh` (tsv2 door, paused) |
| `examples/gh-cache.dl` | `v6/dl/fixtures/ghcacher.dl6`, `ghcacher_live.dl6` | `v6/dl/fixtures/ghcacher.dl6:1` names the v5 line ranges it re-expresses |
| `examples/version-skew.dl` | `v6/tsv2/goldens/multirepo_crawl` | `just multirepo-golden` (tsv2 door, paused) |
| `examples/recompute-guard.dl` | `v6/dl/rails/recompute-guard-rail.dl6` | `just recompute-guard` (Rust door, THIS LANE) |

Three of the four twins grade through the paused TypeScript door, so their
receipts cannot be re-run and their green is historical. The fourth is this
lane's and runs on the Rust door.

Classes with a dl6 program that answers the same QUESTION without naming a
single v5 file, so they are not counted as ports:

| class | v5 files | v6 program |
|---|---|---|
| comment-marker rails | `.dl/rails.dl`, `examples/checked-notes.dl`, `examples/doc-marks.dl`, `examples/chat-marks.dl`, `examples/md-fences.dl`, `std/suppress.dl` | `v6/dl/fixtures/comment-{lint,zone,arch,readme,prod,suppress,lang-junction}-rail.dl6` (7 files) |
| git-fact diagnostics | `.dl/git-graph.dl`, `.dl/graph-diff.dl` | `v6/dl/fixtures/v5-git-diags.dl6` |
| structural-pattern diag rails | `examples/lint-unwrap.dl`, `examples/lints/rust.dl`, `examples/lints/ts.dl`, `examples/ban.dl` | `v6/dl/fixtures/sg-rail.dl6`, `extraction-live.dl6` |
| dataflow reports | `examples/flow-*.dl`, `examples/loop-nests.dl`, `examples/taint.dl` | `v6/dl/dataflow/report_extract.dl6`, `v6/dl/fixtures/flagship-flow.dl6` |
| openapi / rtkq | `examples/openapi.dl`, `std/parsers/openapi.dl`, `examples/rtkq-op-recovery.dl` | `v6/dl/fixtures/1_openapi-userland.dl6`, `openapi-data-family.dl6`, `1_rtkq-extraction-golden.dl6` |
| hover notes | `examples/lsp-def-target.dl` | `v6/dl/fixtures/import-hover-rail.dl6` (lays the sink rel; no LSP behind it) |

## 2. Portable as-is

35 files whose every construct has a DOOR=yes dl6 spelling. Port cost is a
rewrite, never a language change.

| file | lines | ops | v5 rels | referenced by |
|---|---|---|---|---|
| `.dl/file-size.dl` | 118 | diag, scan | file_lines | gate |
| `.dl/marks.dl` | 5 | - | - | docs |
| `bench/seams/shared-names.dl` | 23 | scan | type_entity | docs |
| `deck/snippets/argmax.dl` | 18 | - | - | docs |
| `deck/snippets/chatmarks.dl` | 24 | - | - | docs |
| `deck/snippets/ports.dl` | 15 | - | - | docs |
| `deck/snippets/recursion.dl` | 7 | closure | - | docs |
| `examples/doc-coverage.dl` | 22 | diag, scan | doc_comment, type_entity | docs |
| `examples/dup-collapse.dl` | 44 | scan | type_entity | docs |
| `examples/flow-ctor.dl` | 59 | scan | df_arg, df_edge, df_field, df_node | gate |
| `examples/flow-interproc.dl` | 75 | closure, scan | df_node, df_param, type_sig | gate |
| `examples/flow-jsx.dl` | 56 | scan | call_name, df_field, df_node | gate |
| `examples/flow-services.dl` | 83 | closure, jsonp, scan | call_name, df_arg, df_node, df_param | gate |
| `examples/flow-slice.dl` | 80 | scan | df_field, df_node | docs |
| `examples/gh-cache-batch.dl` | 72 | json | - | docs |
| `examples/gh-cache-config.dl` | 88 | json, jsonp, scan | clock | docs |
| `examples/gh-cache-full.dl` | 139 | json, jsonp | clock | gate |
| `examples/gh-checkout.dl` | 58 | - | checkout, checkout_done, repo | gate |
| `examples/mcp-echo.dl` | 39 | - | - | gate |
| `examples/mcp-server.dl` | 46 | jsonp | - | gate |
| `examples/missing-repo.dl` | 40 | - | repo | docs |
| `examples/net-atlas.dl` | 245 | - | - | docs |
| `examples/openapi-lsp.dl` | 54 | diag, jsonp, scan | call_def, call_name, call_site | docs |
| `examples/repo-nearest.dl` | 9 | scan | - | docs |
| `examples/rtkq-op-recovery.dl` | 60 | diag, jsonp, scan | call_site | docs |
| `examples/stale-doc.dl` | 36 | diag, scan | changed_line, doc_comment, type_entity | docs |
| `examples/string-fns.dl` | 59 | scan | call_def, call_name | docs |
| `examples/string-values.dl` | 34 | scan | const_value, type_entity | docs |
| `examples/taint.dl` | 71 | diag, scan | - | docs |
| `examples/type_coincidence.dl` | 79 | scan | type_sig | docs |
| `examples/vendored-drift.dl` | 55 | scan | file | docs |
| `std/entry.dl` | 129 | jsonp, scan | call_name, df_node, type_entity | gate |
| `std/flow-collections.dl` | 55 | - | - | gate |
| `std/parsers/openapi.dl` | 14 | jsonp, scan | - | docs |
| `std/strings.dl` | 56 | - | const_value, df_edge, df_lit, type_entity | gate |

The dl6 spellings these need, all live:

| v5 construct | dl6 spelling | site |
|---|---|---|
| `scan(rev, glob, path, rev_out)` | `sh files(glob) -> (path, digest)` | `v6/sprefa-engine-rs/src/hosts.rs:92` |
| `closure(edge)` | a recursive rule head | `v6/dl/deadcode/dead-module-rail.dl6:353-357` |
| `json` / `jsonp` | `--family data` + `decode/2` | `v6/sprefa-extract/src/schema.rs:43`, `registry.pl:85` |
| `diag(...)` sink | an ordinary rel a rail heads | `v6/dl/fixtures/diag-rail.dl6` |
| `type_entity` / `type_sig` / `const_value` / `doc_comment` | `sh type_node_at` / `sh sig_at` | `registry.pl:392,395` |
| `df_*` | `sh df_node_at` / `df_edge_at` / `df_param_at` / `df_arg_at` | `registry.pl:371-380` |
| `call_def` / `call_site` / `call_name` | `sh call_node_at` / `sh extract` | `registry.pl:342,383` |
| `checkout` / `repo` | `sh repo_checkout` / `sh repos` | `registry.pl:414,359` |
| `clock` | the `bucket` freshness input | `registry.pl:359` |

## 3. Blocked

148 files carry at least one blocker; 4 of them are the ported twins in section
1, so 144 are blocked and unported. The table below counts over all 148. A file
with two blockers is counted under each, so the column sums past 148.

| blocker | files | why the door stops | issue |
|---|---|---|---|
| `match_line` (line regex) | 32 | no v6 text plane at all; the cst plane's `name` is null | `@dl6-no-text-extraction-door` |
| `call_edge` (resolved calls) | 30 | needs `--resolve`; only `sh scip.call` / `sh scip.diet.call` reach a resolved edge, and they answer `caller_path`, not v5's `caller` symbol | `@dl6-scip-facts-door` |
| `comment` op | 26 | the comment SPAN is a cst node, the TEXT is not on the wire | `@dl6-no-text-extraction-door` |
| `gen` (codegen sink) | 25 | dl6 cannot write files | `@fs-effects-door` (open) |
| `match_ast` | 20 | `hosts.rs:907` rejects `--ast-pattern` | `@dl6-no-text-extraction-door` |
| `type_edge` (+`_rev` 2) | 16 | needs `--resolve` | `@dl6-scip-facts-door` |
| `ast` (tree-sitter query) | 12 | `ts_query/1` compiles to a `tree_sitter` host demand; `executor_for` has no arm for it (`hosts.rs:41-59`) | `@dl6-no-text-extraction-door` |
| `module_edge` (+`_rev` 3) | 10 | needs `--deps` / `--scip-deps` | `@dl6-deps-package-door` |
| `comment_node` | 9 | its `text` column is not on the wire | `@dl6-no-text-extraction-door` |
| `scc` | 9 | no dl6 spelling for strongly-connected-component condensation | `@dl6-no-text-extraction-door` |
| `scip_def` / `scip_name` / `scip_edge` | 8 / 8 / 8 | `--family scip` not linked in-process (`hosts.rs:947`) | `@dl6-scip-facts-door` |
| `type_link` (+`_rev` 4) | 7 | needs `--resolve --scip-index` | `@dl6-scip-facts-door` |
| `scip_fn_edge` | 7 | same as the other scip rows | `@dl6-scip-facts-door` |
| `sg` (deprecated `match_ast`) | 7 | same as `match_ast` | `@dl6-no-text-extraction-door` |
| `rel_catalog` / `rel_col` / `fn_catalog` / `op_catalog` / `verb_catalog` | 6 / 2 / 3 / 3 / 1 | v5 describing v5; v6's equivalent is compile-time | delete |
| `ast_yaml` | 5 | `sg_pattern/3` is `refuse(slot_sg_metavariable_semantics)`, `registry.pl:199` | `@dl6-no-text-extraction-door` (needs a LANG DESIGN call) |
| `diag_stage` / `diag_mute` | 5 / 1 | v5 daemon staging; no v6 twin, none planned | delete |
| `agent_touch` / `agent_edit` / `skill_loaded` | 5 / 2 / 2 | boop owns the harness trail now | delete |
| `scip_occurrence` / `scip_local` / `scip_ref` / `scip_impl` / `scip_callee_type` / `scip_binding` / `scip_want` | 5 / 4 / 3 / 2 / 2 / 1 / 1 | `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `hook_event` | 4 | v5 harness hooks | delete |
| `module_unresolved` (+`_rev` 1) | 4 | needs `--deps` | `@dl6-deps-package-door` |
| `match` (deprecated `match_line`) | 3 | same as `match_line` | `@dl6-no-text-extraction-door` |
| `crate_edge` | 3 | needs `--package-deps` | `@dl6-deps-package-door` |
| `rel_count` / `stmt_ms` / `query_log` / `dl_diag` | 3 / 3 / 1 / 3 | v6 emits `tracing` spans and compile-time diagnostics | delete |
| `hover_note` / `def_target` | 3 / 2 | no v6 LSP | `plans/2026-08-12-v6-native-lsp.PLAN.md` |
| `type_shape` / `type_lgg` | 3 / 2 | no anti-unification plane in v6 | delete |
| `effect_cmd` / `effect_log` | 3 / 2 | the host demand/response pair IS the effect log | delete |
| `graph_node` / `graph_edge` | 2 / 2 | no v6 flow panel | delete |
| `created` | 2 | no v6 first-author host | delete (zero live consumers) |
| `doc_ref` | 2 | needs `--resolve` | `@dl6-scip-facts-door` |
| `module_binding_resolved` (+`_rev` 1) | 2 | needs `--deps` | `@dl6-deps-package-door` |
| `propose_clone` / `propose_extract` | 2 / 1 | no refactor-proposal plane in v6 | delete |
| `node2vec` / `similar` | 2 / 1 | no embedding plane in v6 | delete |
| `template_parts` | 1 | its `text` column is not on the wire | `@dl6-no-text-extraction-door` |
| `cmd` | 1 | zero shell in the engine, by decision 2026-08-21 | delete |

### Which gap buys the most

"Touches" is how many blocked files name the construct at all; "is the only
blocker" is how many go green the moment that one gap closes.

| gap | touches | is the only blocker |
|---|---|---|
| the text door (`match_line`, `match`, `match_ast`, `sg`, `ast`, `comment`, `comment_node`) | 78 | 40 |
| the same plus `ast_yaml` and `scc` | 87 | 40 |
| the resolve and scip doors (`call_edge`, `type_edge`, `type_link`, `module_edge`, `doc_ref`, every `scip_*`) | 66 | — |
| the codegen sink (`gen`) | 25 | 4 |
| all three doors shipped together | — | 96 |

The text door is the single highest-value row: it alone frees 40 rails, it is
the largest overlap group, and two of its three arms are wiring rather than
design.

## 4. Dead

12 files nothing names. Not a justfile recipe, not a CI workflow, not a shell
script, not a rust test, not a markdown page.

| file | last touched | what it was |
|---|---|---|
| `.dl/enum-matches.dl` | 2026-07-02 | enum-variant match audit |
| `.dl/rev-alias-mistyped.dl` | 2026-07-20 | rev-alias typo rail |
| `.dl/triage.dl` | 2026-07-20 | finding triage |
| `.dl/type-seed.dl` | 2026-07-06 | type-shape seeding |
| `bench/agent-eval/gen-tasks.dl` | 2026-07-10 | agent-eval task generator |
| `bench/agent-eval/mcp-server.dl` | 2026-07-10 | agent-eval MCP server |
| `bench/c.dl` | 2026-07-01 | C corpus bench |
| `bench/flow/flow.dl` | 2026-07-01 | dataflow bench |
| `bench/seams/authorship.dl` | 2026-07-01 | `created`-based authorship seam |
| `bench/seams/doc-drift.dl` | 2026-07-01 | doc-drift seam |
| `bench/seams/impact.dl` | 2026-07-01 | blast-radius seam |
| `bench/seams/xlang-siblings.dl` | 2026-07-01 | cross-language sibling seam |

Nothing here has moved in seven weeks. Delete with the tree; no port owed.

## 5. What still runs a v5 rail

Thirteen justfile recipes, and CI runs none of them.

```bash
grep -rn 'v5\|bin dl' .github/workflows/*.yml
# .github/workflows/ci.yml:3:# v5 gates removed 2026-08-11 per user decree; v6 gate wired in 2026-08-11.
```

| recipe | file | rail |
|---|---|---|
| root `justfile:20` | `justfile` | `examples/{{name}}.dl`, any by name |
| root `justfile:24` | `justfile` | `examples/callgraph-ast.dl` |
| root `justfile:28` | `justfile` | `examples/callgraph-sg.dl` |
| root `justfile:32` | `justfile` | `examples/openapi.dl` |
| root `justfile:36` | `justfile` | `examples/time.dl` |
| root `justfile:40` | `justfile` | `examples/{{name}}.dl --watch` |
| root `justfile:95` | `justfile` | `examples/oracle-check.dl` |
| root `justfile:103` | `justfile` | `examples/symbol-profile.dl` |
| root `justfile:111` | `justfile` | `examples/dag-layers.dl` |
| root `justfile:117` | `justfile` | `examples/fn-graph.dl` |
| root `justfile:124` | `justfile` | `examples/feature-envy.dl` |
| `v6/justfile:202` `flagship` | `v6/tsv2/scripts/flagship-callgraph.sh` | `examples/callgraph-ast.dl` |
| `v6/justfile:327` `multirepo-golden` | `v6/tsv2/goldens/multirepo_crawl/2_gate.sh` | `examples/version-skew.dl` |

Plus `.dl/watch-ext.dl`, `bench/printk.dl` and `bench/rust.dl`, named by the
root justfile's bench recipes.

Every recipe in `v6/` that reaches a v5 rail runs through the paused
TypeScript door. The nine files under `v6/` that name `target/release/dl`:

| file | what it does | live? |
|---|---|---|
| `v6/justfile` | 11 lines, the recipes below | the recipes are |
| `v6/tsv2/scripts/flagship-callgraph.sh` | `just flagship` | tsv2, paused |
| `v6/tsv2/scripts/v5-parity.sh` | `just v5-parity`, writes `plans/2026-07-30-v5-parity-table.tsv` | tsv2, paused |
| `v6/tsv2/scripts/comment-parity.sh` | `just comment-rails` second half | tsv2, paused |
| `v6/tsv2/scripts/crawl-bench.sh` | the crawl bench | tsv2, paused |
| `v6/tsv2/goldens/multirepo_crawl/2_gate.sh` | `just multirepo-golden` | tsv2, paused |
| `v6/tsv2/CRAWL-BENCH.md` | prose | n/a |
| `v6/tools/staleness-gate.sh` | checks binaries for staleness | live, but only names the binary |
| `v6/tools/lsp-v5-bridge.sh` | the diagnostics bridge | **already broken**, see below |

### The diagnostics bridge is already dead

CLAUDE.md records "diagnostics is the only v6 editor feature that reaches an
editor through v5". `v6/tools/lsp-v5-bridge.sh` is that bridge. It points at
files that no longer exist:

```bash
ls v6/dl/src
# ls: cannot access 'v6/dl/src': No such file or directory
```

The script's own header names `v6/dl/src/5_diag.ts` (line 6) and
`v6/dl/src/main.ts` (line 25) as the v6 side of the bridge. The TypeScript v6
server under `v6/dl/src/` is gone; `v6/dl/` now holds only `.dl6` rails
(`dataflow/`, `deadcode/`, `fixtures/`, `hotpath/`, `typegen/`).

So the cost of retiring v5 is not "lose diagnostics in the editor". Diagnostics
through v5 stopped working when that server was deleted, and nobody noticed.
The retirement plan carries the bridge on the DELETE list, not the PORT list.

## 6. The two live rails named in CLAUDE.md

### `examples/recompute-guard.dl` — PORTED

`v6/dl/rails/recompute-guard-rail.dl6`, run by
`v6/dl/rails/recompute-guard-rail.sh`, gated by `just recompute-guard`.

The v6 version is stricter than v5's, in one named way. v5 attributes a marker
to "the nearest fn decl at or above it" — a flat line comparison with no nesting,
which the v5 file's own header (`examples/recompute-guard.dl:36-46`) records as
having misfired on `Engine::eval_node2vec_rule` when a closure sat between the
guard and the recompute. v6 has real spans, so the port uses span CONTAINMENT
and the closure problem cannot happen:

```
v5:  fn_decl(f, fl), fl <= cl, !(another fn_decl strictly between)
v6:  ProcStart <= SiteStart, SiteEnd <= ProcEnd
```

One contract change, stated: the waiver moves from a `//` line comment to a
`///` doc comment on the function.

```rust
// v5:  // @recompute unguarded: <reason>   anywhere in the body
/// v6:  /// @recompute unguarded: <reason>  on the fn's own doc comment
```

The reason is the text gap: `//` comments reach the wire as a cst node with a
span and a null `name`, while `///` comments reach it as
`record=doc family=type` with the text intact (`schema.rs:39`). Probed
2026-08-21:

```
extract --family type probe.rs
  {"record":"doc","family":"type","owner":{"start":54,"end":60},
   "parent":null,"text":"@recompute unguarded: node2vec is bounded here"}
```

Attribution improves with the change: a doc comment names its owner by span, so
the waiver cannot drift to a neighbouring function the way a line comment can.

### `.dl/no-new-eprintln.dl` — BLOCKED

Not portable today. The rail asks "which lines call `eprintln!`", and no v6
record carries that fact.

Probed 2026-08-21 on a rust file holding `eprintln!("x")`:

| family | what came back |
|---|---|
| `call` | two `node` records for the enclosing fns. **No `site` record**: the rust front-end does not project macro invocations as call sites |
| `cst` | `{"kind":"macro_invocation","name":null}` and `{"kind":"identifier","name":null}` — the span is there, the identifier TEXT is not |
| `df` | zero rows |
| `data` | zero rows |
| `cfg` | entry/exit/stmt nodes, no names |

Both of v5's mechanisms are door-blocked:

| v5 line | construct | v6 stop |
|---|---|---|
| `.dl/no-new-eprintln.dl:25` | `match_line(f, rev, /eprintln!/, line)` | no text plane |
| `.dl/no-new-eprintln.dl:32` | `comment(f, rev, /@eprintln-ok:/, line)` | comment text not on the wire |
| `.dl/no-new-eprintln.dl:42` | `match_line(f, rev, /\/\/.*@eprintln-ok:/, line)` | no text plane |

Issue `@v5-rail-eprintln-blocked`, blocked by `@dl6-no-text-extraction-door`.

The number the rail exists to keep at zero, measured 2026-08-21 by applying v5's
own waiver rule (`@eprintln-ok` on the hit line or the line above) with a script
instead of the rail:

| | count |
|---|---|
| `eprintln!` sites in `v6/*/src/**/*.rs` | 17 |
| waived by `@eprintln-ok` | 12 |
| **unwaived** | **5** |

| site | what it prints |
|---|---|
| `v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:89` | harness usage / arg error |
| `v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:306` | harness run failure |
| `v6/sprefa-extract/src/bin/extract.rs:281` | top-level CLI error before exit |
| `v6/sprefa-extract/src/bin/extract.rs:592` | top-level CLI error before exit |
| `v6/sprefa-store/src/engine.rs:75` | `[cascade]` timing line, **not** a CLI-UX contract |

Four of the five are CLI top-level error reports in `src/bin/**`, which is
exactly what the waiver word exists for; they want an `@eprintln-ok` comment,
not a rewrite. The fifth, `sprefa-store/src/engine.rs:75`, is machinery
narration in a library and belongs in `tracing`. Filed as
`@v6-eprintln-ratchet-five`.
