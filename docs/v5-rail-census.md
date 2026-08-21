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

Measured twice, because the door moved mid-census. `ast_rule` landed on main at
`3da1100f2` / `cd58a6917` and opened v5's whole pattern surface to a `.dl6`
program; the second column is the tree as it stands.

| bucket | before `ast_rule` | **now** |
|---|---|---|
| ported (a dl6 twin exists) | 4 | **5** |
| portable as-is | 35 | **68** |
| blocked | 144 | **110** |
| dead | 12 | **12** |

33 rails moved from blocked to portable on one commit. The five ported now
include both rails CLAUDE.md names as live.

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

`comment_node` and `template_parts` counted as blockers in the first
measurement, because their SPANS were on the cst wire and their `text` was not.
`ast_rule` answers the text, so they are no longer blockers; the "before"
column keeps the old reading so the 33-rail move is legible.

## 1. Ported: a dl6 twin exists

| v5 rail | dl6 twin | receipt |
|---|---|---|
| `examples/callgraph-ast.dl` | `v6/dl/fixtures/flagship-callgraph.dl6` | `just flagship` -> `v6/tsv2/scripts/flagship-callgraph.sh` (tsv2 door, paused) |
| `examples/gh-cache.dl` | `v6/dl/fixtures/ghcacher.dl6`, `ghcacher_live.dl6` | `v6/dl/fixtures/ghcacher.dl6:1` names the v5 line ranges it re-expresses |
| `examples/version-skew.dl` | `v6/tsv2/goldens/multirepo_crawl` | `just multirepo-golden` (tsv2 door, paused) |
| `examples/recompute-guard.dl` | `v6/dl/rails/recompute-guard-rail.dl6` | `just recompute-guard` (Rust door, THIS LANE) |
| `.dl/no-new-eprintln.dl` | `v6/dl/rails/no-new-eprintln-rail.dl6` | `just no-new-eprintln` (Rust door, THIS LANE) |

Three of the five twins grade through the paused TypeScript door, so their
receipts cannot be re-run and their green is historical. The two this lane
added run on the Rust door and are gated by `just v5-rails`.

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

68 files whose every construct has a DOOR=yes dl6 spelling. Port cost is a
rewrite, never a language change. 33 of them arrived here when `ast_rule`
landed and are marked by an ops column naming `match_line`, `match_ast`, `sg`,
`ast_yaml` or `comment`.

| file | lines | ops | v5 rels | referenced by |
|---|---|---|---|---|
| `.dl/dishonest-flag.dl` | 121 | comment, diag, match_ast, match_line, scan | - | gate |
| `.dl/file-size.dl` | 118 | diag, scan | file_lines | gate |
| `.dl/lossy-dedup.dl` | 49 | comment, diag, match_ast, scan | - | gate |
| `.dl/marks.dl` | 5 | - | - | docs |
| `.dl/static-n1.dl` | 85 | comment, diag, match_line, scan | - | docs |
| `.dl/unordered-select.dl` | 86 | comment, diag, match_line, scan | - | gate |
| `.dl/vsix-version-drift.dl` | 35 | diag, match_line, scan | - | gate |
| `bench/printk.dl` | 17 | scan, sg | - | gate |
| `bench/rust.dl` | 22 | scan, sg | - | gate |
| `bench/seams/shared-names.dl` | 23 | scan | type_entity | docs |
| `bench/stress_c.dl` | 31 | scan, sg | - | gate |
| `deck/snippets/argmax.dl` | 18 | - | - | docs |
| `deck/snippets/chatmarks.dl` | 24 | - | - | docs |
| `deck/snippets/diag.dl` | 11 | diag, match, scan | - | docs |
| `deck/snippets/facts.dl` | 9 | scan, sg | call_site | docs |
| `deck/snippets/flowpanel.dl` | 14 | closure, scan, sg | - | docs |
| `deck/snippets/join.dl` | 14 | scan, sg | call_site | docs |
| `deck/snippets/ports.dl` | 15 | - | - | docs |
| `deck/snippets/recursion.dl` | 7 | closure | - | docs |
| `examples/ban.dl` | 23 | diag, match_ast, scan | - | docs |
| `examples/banned-word-guard.dl` | 42 | diag, match_line, scan | - | docs |
| `examples/call-seams.dl` | 65 | match_ast, scan | type_entity | docs |
| `examples/callgraph-sg.dl` | 47 | match_ast, scan | - | gate |
| `examples/doc-coverage.dl` | 22 | diag, scan | doc_comment, type_entity | docs |
| `examples/dup-collapse.dl` | 44 | scan | type_entity | docs |
| `examples/flow-ctor.dl` | 59 | scan | df_arg, df_edge, df_field, df_node | gate |
| `examples/flow-interproc.dl` | 75 | closure, scan | df_node, df_param, type_sig | gate |
| `examples/flow-jsx.dl` | 56 | scan | call_name, df_field, df_node | gate |
| `examples/flow-services.dl` | 83 | closure, jsonp, scan | call_name, df_arg, df_node, df_param | gate |
| `examples/flow-slice.dl` | 80 | scan | df_field, df_node | docs |
| `examples/gen-zone-info.dl` | 42 | comment, diag, scan | comment_node | docs |
| `examples/gh-cache-batch.dl` | 72 | json | - | docs |
| `examples/gh-cache-config.dl` | 88 | json, jsonp, scan | clock | docs |
| `examples/gh-cache-full.dl` | 139 | json, jsonp | clock | gate |
| `examples/gh-checkout.dl` | 58 | - | checkout, checkout_done, repo | gate |
| `examples/latest-turn-guardrail.dl` | 83 | diag, match_line, scan | changed | docs |
| `examples/lint-unwrap.dl` | 61 | ast_yaml, diag, scan | - | gate |
| `examples/lints/rust.dl` | 37 | diag, match_ast, scan | - | docs |
| `examples/lints/ts.dl` | 26 | diag, match_ast, scan | - | docs |
| `examples/mcp-echo.dl` | 39 | - | - | gate |
| `examples/mcp-server.dl` | 46 | jsonp | - | gate |
| `examples/md-fences.dl` | 65 | match_ast, match_line, scan | - | docs |
| `examples/missing-repo.dl` | 40 | - | repo | docs |
| `examples/net-atlas.dl` | 245 | - | - | docs |
| `examples/openapi-lsp.dl` | 54 | diag, jsonp, scan | call_def, call_name, call_site | docs |
| `examples/openapi.dl` | 23 | jsonp, match_ast, scan | - | gate |
| `examples/phantom-deps.dl` | 66 | match_line, scan | repo | docs |
| `examples/pin-skew.dl` | 48 | match_line, scan | repo, rev_behind, rev_cmp_want | gate |
| `examples/rails-call-kind.dl` | 54 | diag, match_ast, scan | call_kind, call_site, changed_line | docs |
| `examples/rails.dl` | 64 | diag, match_ast, match_line, scan | changed | docs |
| `examples/repo-nearest.dl` | 9 | scan | - | docs |
| `examples/route-norm.dl` | 76 | match_ast, scan | - | docs |
| `examples/rtkq-op-recovery.dl` | 60 | diag, jsonp, scan | call_site | docs |
| `examples/stale-doc.dl` | 36 | diag, scan | changed_line, doc_comment, type_entity | docs |
| `examples/string-fns.dl` | 59 | scan | call_def, call_name | docs |
| `examples/string-values.dl` | 34 | scan | const_value, type_entity | docs |
| `examples/styled-components.dl` | 49 | match_ast, scan | - | docs |
| `examples/taint.dl` | 71 | diag, scan | - | docs |
| `examples/time.dl` | 20 | match_line, scan | - | gate |
| `examples/type_coincidence.dl` | 79 | scan | type_sig | docs |
| `examples/vendored-drift.dl` | 55 | scan | file | docs |
| `std/arch.dl` | 97 | json | comment_node | gate |
| `std/entry.dl` | 129 | jsonp, scan | call_name, df_node, type_entity | gate |
| `std/flow-collections.dl` | 55 | - | - | gate |
| `std/parsers/openapi.dl` | 14 | jsonp, scan | - | docs |
| `std/strings.dl` | 56 | - | const_value, df_edge, df_lit, type_entity | gate |
| `std/suppress.dl` | 299 | diag | comment_node, file | gate |
| `tree-sitter-dl/test/ban.dl` | 27 | diag, scan, sg | - | docs |

The dl6 spellings these need, all live:

| v5 construct | dl6 spelling | site |
|---|---|---|
| `scan(rev, glob, path, rev_out)` | `sh files(glob) -> (path, digest)` | `v6/sprefa-engine-rs/src/hosts.rs:251` |
| `closure(edge)` | a recursive rule head | `v6/dl/deadcode/dead-module-rail.dl6:353-357` |
| `json` / `jsonp` | `--family data` + `decode/2` | `v6/sprefa-extract/src/schema.rs:43`, `registry.pl:85` |
| `diag(...)` sink | an ordinary rel a rail heads | `v6/dl/fixtures/diag-rail.dl6` |
| `type_entity` / `type_sig` / `const_value` / `doc_comment` | `sh type_node_at` / `sh sig_at` | `registry.pl:392,395` |
| `df_*` | `sh df_node_at` / `df_edge_at` / `df_param_at` / `df_arg_at` | `registry.pl:371-380` |
| `call_def` / `call_site` / `call_name` | `sh call_node_at` / `sh extract` | `registry.pl:342,383` |
| `checkout` / `repo` | `sh repo_checkout` / `sh repos` | `registry.pl:414,359` |
| `clock` | the `bucket` freshness input | `registry.pl:359` |

## 3. Blocked

110 files. Grouped by the construct that stops each one; a file with two
blockers is counted under each, so the column sums past 110.

| blocker | files | why the door stops | issue |
|---|---|---|---|
| `call_edge` | 30 | needs `--resolve`; `sh /scip/call` answers `caller_path`, not v5's symbol | `@dl6-scip-facts-door` |
| `gen` | 25 | dl6 cannot write files | `@fs-effects-door` (open) |
| `type_edge` | 16 | needs `--resolve` | `@dl6-scip-facts-door` |
| `ast` | 11 | `ts_query/1` compiles to a `tree_sitter` host demand; `executor_for` has no arm | `@dl6-ts-query-executor` |
| `module_edge` | 10 | needs `--deps` / `--scip-deps` | `@dl6-deps-package-door` |
| `scc` | 9 | no dl6 spelling for strongly-connected-component condensation | `@dl6-ts-query-executor` |
| `scip_def` | 8 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `scip_name` | 8 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `scip_edge` | 8 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `type_link` | 7 | needs `--resolve --scip-index` | `@dl6-scip-facts-door` |
| `scip_fn_edge` | 7 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `rel_catalog` | 6 | a v5-only plane with no v6 twin and none planned | delete |
| `agent_touch` | 5 | a v5-only plane with no v6 twin and none planned | delete |
| `scip_occurrence` | 5 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `diag_stage` | 4 | a v5-only plane with no v6 twin and none planned | delete |
| `call_edge_rev` | 4 | same | `@dl6-scip-facts-door` |
| `type_link_rev` | 4 | same | `@dl6-scip-facts-door` |
| `module_unresolved` | 4 | needs `--deps` | `@dl6-deps-package-door` |
| `scip_local` | 4 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `hook_event` | 4 | a v5-only plane with no v6 twin and none planned | delete |
| `stmt_ms` | 3 | a v5-only plane with no v6 twin and none planned | delete |
| `rel_count` | 3 | a v5-only plane with no v6 twin and none planned | delete |
| `crate_edge` | 3 | needs `--package-deps` | `@dl6-deps-package-door` |
| `module_edge_rev` | 3 | same | `@dl6-deps-package-door` |
| `effect_cmd` | 3 | a v5-only plane with no v6 twin and none planned | delete |
| `dl_diag` | 3 | a v5-only plane with no v6 twin and none planned | delete |
| `scip_ref` | 3 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `type_shape` | 3 | a v5-only plane with no v6 twin and none planned | delete |
| `op_catalog` | 3 | a v5-only plane with no v6 twin and none planned | delete |
| `fn_catalog` | 3 | a v5-only plane with no v6 twin and none planned | delete |
| `hover_note` | 3 | no v6 LSP | `plans/2026-08-12-v6-native-lsp.PLAN.md` |
| `graph_node` | 2 | a v5-only plane with no v6 twin and none planned | delete |
| `graph_edge` | 2 | a v5-only plane with no v6 twin and none planned | delete |
| `scip_impl` | 2 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `agent_edit` | 2 | a v5-only plane with no v6 twin and none planned | delete |
| `module_binding_resolved` | 2 | needs `--deps` | `@dl6-deps-package-door` |
| `propose_clone` | 2 | a v5-only plane with no v6 twin and none planned | delete |
| `created` | 2 | a v5-only plane with no v6 twin and none planned | delete |
| `type_edge_rev` | 2 | same | `@dl6-scip-facts-door` |
| `doc_ref` | 2 | needs `--resolve` | `@dl6-scip-facts-door` |
| `skill_loaded` | 2 | a v5-only plane with no v6 twin and none planned | delete |
| `rel_col` | 2 | a v5-only plane with no v6 twin and none planned | delete |
| `scip_callee_type` | 2 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `type_lgg` | 2 | a v5-only plane with no v6 twin and none planned | delete |
| `def_target` | 2 | no v6 LSP | `plans/2026-08-12-v6-native-lsp.PLAN.md` |
| `node2vec` | 2 | a v5-only plane with no v6 twin and none planned | delete |
| `scip_want` | 1 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `verb_catalog` | 1 | a v5-only plane with no v6 twin and none planned | delete |
| `diag_mute` | 1 | a v5-only plane with no v6 twin and none planned | delete |
| `propose_extract` | 1 | a v5-only plane with no v6 twin and none planned | delete |
| `query_log` | 1 | a v5-only plane with no v6 twin and none planned | delete |
| `scip_binding` | 1 | `--family scip` / `--scip-facts` not linked in-process | `@dl6-scip-facts-door` |
| `cmd` | 1 | zero shell in the engine, by decision 2026-08-21 | delete |
| `similar` | 1 | a v5-only plane with no v6 twin and none planned | delete |
| `effect_log` | 1 | a v5-only plane with no v6 twin and none planned | delete |
| `module_binding_resolved_rev` | 1 | same | `@dl6-deps-package-door` |
| `module_unresolved_rev` | 1 | same | `@dl6-deps-package-door` |

### Which gap buys the most, re-measured

The text door has been bought. What is left, over the 108 still blocked:

| gap | touches | is the only blocker |
|---|---|---|
| the resolve and scip doors (`call_edge` 30, `type_edge` 16, `module_edge` 10, `type_link` 7, `doc_ref`, every `scip_*`) | 66 | — |
| the codegen sink (`gen`) | 25 | 11 |
| `ast`, the tree-sitter s-expression form | 11 | 4 |
| `scc`, strongly-connected-component condensation | 9 | — |
| the v5-only planes on the delete list | the remainder | — |
| resolve + scip + `gen` + `ast` + `scc` together | — | 81 |

`gen` is now the cheapest single row: 11 rails need nothing else, and
`@fs-effects-door` is already open. The resolve and scip doors are the biggest
group but free nothing alone, because every rail that wants a resolved edge
wants two of them.

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

### `.dl/no-new-eprintln.dl` — PORTED

`v6/dl/rails/no-new-eprintln-rail.dl6`, run by
`v6/dl/rails/no-new-eprintln-rail.sh`, gated by `just no-new-eprintln`.

This section first recorded the rail as BLOCKED, with a probe table showing that
no v6 record carried "this line calls `eprintln!`". That was true and is not any
more: `ast_rule` landed on main at `3da1100f2` / `cd58a6917` while this census
was being written, and it answers both of the rail's halves.

Both v5 mechanisms map onto one host:

| v5 line | v5 construct | v6 spelling |
|---|---|---|
| `.dl/no-new-eprintln.dl:25` | `match_line(f, rev, /eprintln!/, line)` | `all: [kind: expression_statement, has: {pattern: eprintln!($$$ARGS)}]` |
| `.dl/no-new-eprintln.dl:32` | `comment(f, rev, /@eprintln-ok:/, line)` | `follows: {all: [kind: line_comment, regex: '@eprintln-ok']}` |
| `.dl/no-new-eprintln.dl:42` | `match_line(f, rev, /\/\/.*@eprintln-ok:/, line)` | `precedes: {...}`, the same rule the other way |

v5 needed two waiver rules because its `comment` op saw only whole-line
comments; one `any: [follows, precedes]` covers both here, with no line
arithmetic. Measured on the fixture set 2026-08-21:

```
hits=7 waived=3 new=4 exceeded=0
ok     bare.rs                  no baseline row and no waiver, one row per site
ok     waived_above.rs          the comment-above form, v5's waiver_line == line - 1
ok     waived_trailing.rs       the trailing form v5 needed a second rule to see
ok     near_miss.rs             a marker neighbouring another statement waives nothing
ok     clean.rs                 tracing only, no print to find
ok     multiline_waiver.rs      a marker on a multi-line call's closing line, which v5 missed
NO-NEW-EPRINTLN OK  findings=4
```

#### Where the structural rule beats v5's line rule

A MULTI-LINE `eprintln!(` whose `@eprintln-ok` marker sits on the closing `);`
line. v5's window is `[line-1, line]` against the `eprintln!` TOKEN line, so it
never sees a marker four lines down and reports a waived print as a finding.
The statement's next sibling is that comment either way, so `precedes` waives
it correctly.

Two of v6's own 17 sites are exactly this shape
(`v6/sprefa-engine-rs/src/bin/emit_rust_harness.rs:89` and `:306`, the
`--arrive` and `--live-hosts` argument errors). `multiline_waiver.rs` pins the
case.

#### Named limit

The unit is `expression_statement`, so an `eprintln!` in a non-statement
position (a match arm expression, a tail expression) is not seen; v5's line
regex saw those. Zero such sites exist in v6 today. Widen the `kind` list when
one appears rather than going back to a line scan.

#### The live ratchet

Measured 2026-08-21 by the rail itself over `v6/sprefa-*/src/*.rs` (105 files;
git's `*` crosses `/`, so this is every crate-root and nested source file):

```
hits=17 waived=14 new=0 exceeded=0
== rail_eprintln_counted (unwaived sites, against the baseline) ==
  v6/sprefa-extract/src/bin/extract.rs  @10175
  v6/sprefa-extract/src/bin/extract.rs  @20088
  v6/sprefa-store/src/engine.rs  @3861
```

| | count |
|---|---|
| `eprintln!` statements in `v6/sprefa-*/src/*.rs` | 17 |
| waived by an `@eprintln-ok` neighbour | 14 |
| **surviving, and baselined** | **3** |

The three are two top-level CLI error reports in `extract.rs`, which want an
`@eprintln-ok` comment rather than a rewrite, and `sprefa-store/src/engine.rs`,
which is machinery narration in a library and belongs in `tracing`. Carded at
`@v6-eprintln-ratchet-three`.

A plain grep with v5's line rule reports FIVE survivors, not three. The two
extra are the multi-line case above: the grep is wrong and the rail is right.
