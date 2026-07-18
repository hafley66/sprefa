# Repo-wide decomposition + normalization (2026-07-18)

Planning-only arc. Branch `decomp-plan` off next @ e06b14f7. Executes ONLY after
the in-flight `callable-lambda-ctor` typegraph work merges (hard dependency,
section 8) and coordinates with the in-flight scheduler plan for daemon steps.

## 0. Scope and goals

First-party src is 80,248 lines. Thirteen files sit at or above the 1,500-line
budget the repo's own rail (`examples/coupling-metrics.dl`) enforces — but the
rail's glob only covers `src/engine/*.rs` + `src/rels/*.rs`, so the five worst
offenders have never tripped it:

| file | lines | % of src | rail sees it |
|---|---|---|---|
| src/graph/typegraph.rs | 8,653 | 10.8% | no |
| src/graph/modgraph.rs | 2,781 | 3.5% | no |
| src/daemon.rs | 2,714 | 3.4% | no |
| src/engine/derive.rs | 2,401 | 3.0% | yes |
| src/storage/call.rs | 1,938 | 2.4% | no |
| src/engine/mod.rs | 1,908 | 2.4% | yes |
| src/engine/extract/mod.rs | 1,727 | 2.2% | yes |
| src/parse.rs | 1,612 | 2.0% | no |
| src/propose.rs | 1,611 | 2.0% | no |
| src/engine/meta.rs | 1,576 | 2.0% | yes |
| src/engine/tick.rs | 1,499 | 1.9% | yes (at edge) |
| src/typecheck.rs | 1,420 | — | no (under) |
| src/lsp.rs | 1,400 | — | no (under) |

Goals: (1) no first-party file over ~1,500 lines on the split targets, (2) the
rail sees all of src so regressions scream, (3) friction items (f) and (h) from
plans/2026-07-10-change-cost-friction-inventory.md closed, (4) normalization
pass (banned-word identifiers, dl variable names, per-family file conventions),
(5) zero behavior change — every step proves byte-identical extraction via the
existing determinism rails.

Non-goals: the engine-trait epic (friction item 4's RelKind registry work),
derive.rs/meta.rs/tick.rs internal redesign (they are engine-family files the
trait epic owns), any schema or rel change.

## 1. Prior art

- **plans/2026-07-10-change-cost-friction-inventory.md** — item 4 ranks the
  engine monolith as the standing epic (the only L); items (f) and (h) are the
  named normalization debts. Since it was written, engine/mod.rs went from
  ~7,800 to 1,908 lines (derive/meta/tick/extract/etc. already split out) and
  **item (h) is RESOLVED on next**: `AST_LANG_TABLE` lives in
  `src/engine/lang_tables.rs` (69 lines, `pub use lang_tables::ast_langs` at
  engine/mod.rs:57). This plan records that and handles only the residue.
- **Abandoned branch `refactor/file-splits`** (worktree ~/projects/sprefa-refactor,
  tip 615d5b1d, 7 commits, all 2026-07-12, merge-base 12ded209 off
  release/0.10.0). It already executed per-language splits of typegraph
  (mod 1879 / ts 2178 / python 1224 / rust 1181 / go 1025 / kotlin 848 /
  doc 104) and modgraph (mod 1196 / rust 495 / python 341 / ts 289 / kotlin 249
  / go 233), plus daemon/{paths,loops,health}, parse/{ops,desugar},
  propose/{sequence,shapes}, and widened the rail glob (efed0e4f). Every split
  was a pure relocation (insertions ≈ deletions, +~25 lines module glue). It
  died of divergence only: 146 commits behind next, and next independently
  split the daemon a different way (`src/daemon_shell/`), producing
  modify/delete conflicts. Verdict: the per-language axis was already validated
  once; the branch is a harvestable reference for module shapes and glue, not a
  mergeable base. The daemon/{loops,paths,health} shape is superseded by next's
  daemon_shell and must NOT be harvested.
- **next's own daemon evolution**: `src/daemon_shell/` (tokio transport shell:
  uds/http/jobs/timers/watch, 1,001 lines), `src/daemon_read.rs` (lock-free
  read path, 443), `src/daemon_http.rs` (discovery helpers, 43) already peeled
  three concerns off daemon.rs. The daemon step below continues that direction.
- **plans/2026-05-31-auto-refactor-use-path-rewrite.md** — Route A (`--move`)
  rewrites Rust `use` paths via byte-span edits; residual gaps are brace-head
  `use crate::{a::X, ..}` leaves, physical file moves, and the moved file's own
  imports. Relevant below (section 7) for which steps it can mechanize.

## 2. Measured evidence — structure receipts (grep/wc, this worktree)

### 2.1 typegraph.rs region map

Top-level item inventory (grep `^pub fn |^fn |^struct |^impl ...`) partitions
the file into contiguous regions; the sum reproduces 8,653 exactly:

| region | lines | content |
|---|---|---|
| 1–452 | 452 | shared model: TypeEdge, EntityKind, TypeRef/TypeExpr/TypeEntity, TypeFacts, ConstValueFact, DocFact/DocTag + shared doc helpers (doc_summary, clean_block_comment, parse_jsdoc_tags, parse_rust_sections), CallFacts/CallDef/CallSite/CallKind, DataflowFacts/LoopFact/NestFact/DfNode/DfEdge, mint_sym, `trait TypeLang`, AnalysisMask/AnalysisBundle, `type_langs()` registry + LANG-JUNCTION(typelang-registry) comment (line 439), the five unit structs |
| 453–483 | 31 | impl TypeLang for GoTypes |
| 484–548 | 65 | impl TypeLang for RustTypes |
| 549–605 | 57 | impl TypeLang for KotlinTypes |
| 606–967 | 362 | Kotlin dataflow (kt_*) |
| 968–1788 | 821 | TS dataflow + JSX (ts_flow_*) |
| 1789–1856 | 68 | impl TypeLang for TsTypes |
| 1857–1895 | 39 | shared helpers: source_type_for, line_index, line_at, line_col |
| 1896–2161 | 266 | Rust type edges (syn) |
| 2162–2422 | 261 | Kotlin edges + call defs/sites |
| 2423–3726 | 1,304 | TS edges/comments/templates/unresolved/entities/consts/docs/call defs+sites |
| 3727–4729 | 1,003 | Rust entities/consts/docs/call defs+sites/dataflow |
| 4730–4898 | 169 | Kotlin entities/docs/fn_type |
| 4899–5908 | 1,010 | Go (everything) |
| 5909–6127 | 219 | shared analysis: type_shape_hashes, type_lgg_pairs, hex_string |
| 6128–7349 | 1,222 | Python (everything) |
| 7350–8653 | 1,304 | `#[cfg(test)] mod tests` — 81 test fns, mixed languages (helper-prefix counts: ts 27, kotlin 9, go 8, rust 6, python 5) |

Languages are interleaved (Kotlin in 3 regions, Rust in 3, TS in 2): the split
is cut-paste into per-language modules, NOT contiguous range slicing, and git
will not detect it as a rename (confirmed on the old branch: `-M -C` reports
delete+create for every fragmentation).

Per-language totals: TS ~2,193 · Rust ~1,334 · Python ~1,222 · Go ~1,041 ·
Kotlin ~849 · shared core ~710 · tests ~1,304.

### 2.2 External consumers of typegraph (the true pub surface)

`grep -rn "typegraph::" src` excluding the file itself — every consumer and
what it reaches for:

| consumer | symbols used |
|---|---|
| src/engine/mod.rs | TypeFacts, CallFacts, DataflowFacts, TemplatePart, UnresolvedRef (AnalysisBundle cache maps) |
| src/engine/extract/mod.rs | AnalysisMask, AnalysisBundle, type_langs() |
| src/engine/extract/call.rs | type_langs(), CallFacts, CallDef |
| src/engine/extract/type_rels.rs | type_langs(), TypeFacts |
| src/engine/extract/dataflow.rs | type_langs(), DataflowFacts |
| src/engine/extract/text.rs | ts_comments, ts_template_parts, ts_unresolved_refs, TemplatePart, UnresolvedRef |
| src/cst.rs | clean_block_comment (+ ts_comments in docs) |
| src/rels/analysis.rs | type_shape_hashes, type_lgg_pairs |
| src/ingest/mod.rs | TypeLang (doc reference only) |

**No consumer reaches per-language internals.** The external surface is the
shared model + registry + trait, three TS text helpers, and two analysis fns.
A per-language split keeps 100% of the pub surface in the new `mod.rs`
(re-exported), so no consumer changes a use path. A per-family split would
instead force every extract/* consumer through three family files each of which
still needs all five languages — it severs nothing.

### 2.3 Library affinity (the second axis, per user direction)

Crate-mention counts per typegraph region (grep `syn::` / `oxc` /
`tree_sitter`, bucketed by the region map above):

| crate | total | in "its" language's regions | leakage |
|---|---|---|---|
| syn:: | 103 | 96 in Rust regions + 4 impl | 3 in core (comments) |
| oxc* | 75 | 61 in TS regions + 6 in shared `source_type_for` helper | 8 (comments in core/kt) |
| tree_sitter | 117 | kt 39 + go 36 + py 42 | 0 outside kt/go/py regions |

The affinity partition is near-perfect along the LANGUAGE axis: each
per-language module ends up owning exactly one parser crate (rust→syn, ts→oxc,
kotlin/go/python→tree-sitter + their grammar crates). The family axis would put
all three parser stacks into each of types/calls/dataflow. This is the
"functions that only matter while this dependency is alive" seam the user
described — realized as per-language modules whose only owned dep is their
parser, no DI shapes.

Whole-repo affinity (grep proxy, counts > 5 per file):

| file | crate clusters |
|---|---|
| src/graph/typegraph.rs | syn 121, tree_sitter 117, oxc 27 (three parser stacks in one file) |
| src/storage/call.rs | rusqlite 37 (single-crate, cohesive — argues AGAINST splitting it) |
| src/lsp.rs | serde_json 78 (single-crate, cohesive, under budget — leave) |
| src/daemon.rs | serde_json 8, tokio 9, anyhow 8 (no dominant library; its seams are behavioral, not library) |
| src/engine/derive.rs | rusqlite 8, serde_json 9 (engine-family; trait epic owns it) |

### 2.4 dl dogfood receipts (hermetic one-shot runs)

Environment for every run (dl 0.10.0 at ~/.cargo/bin, corpus = this worktree,
157 files under src/):

```sh
cd .claude/worktrees/decomp-plan
touch /tmp/decomp-empty-config.toml
export SPREFA_CONFIG=/tmp/decomp-empty-config.toml
export SPREFA_SCIP_INDEX=~/projects/sprefa/index.scip   # same content as base
dl <prog> --db /tmp/decomp-dogfood.db --no-daemon
```

Wall-clocks: module family 0.22s; measures 6.9s (call extraction 4.5s +
reach_from 1.6s); arch-flow 12.6s cold. The engine's own self-lint fired
during the cold dataflow run (`[n+1] 'INSERT _strings' ran 128x this tick`).

**(a) scip_edge file fan-in/fan-out** (compiler-backed; module_edge is
re-export-routed and reads leaf files as 1/1, so scip is the hub measure):

| file | fan-in | fan-out |
|---|---|---|
| src/engine/mod.rs | 108 | 47 |
| src/engine/tick.rs | 52 | 24 |
| src/parse.rs | 36 | 4 |
| src/daemon.rs | 23 | 42 |
| src/engine/meta.rs | 18 | 10 |
| src/engine/extract/mod.rs | 11 | 20 |
| src/graph/typegraph.rs | 11 | 2 |
| src/typecheck.rs | 7 | 9 |
| src/propose.rs | 5 | 3 |
| src/engine/derive.rs | 4 | 19 |
| src/graph/modgraph.rs | 4 | 1 |
| src/storage/call.rs | 3 | 11 |
| src/lsp.rs | 1 | 15 |

engine/mod.rs is the single dominant hub both directions (repo-wide fan-in
top: build.rs 177, engine/mod.rs 108, db.rs 92, ast.rs 91, tick.rs 52).
typegraph.rs, despite being 10.8% of src, is a NEAR-LEAF: fan-in 11, fan-out 2
(its only source dependency is cst.rs). Splitting it severs no consumer.

**(b) module cycles**: the module_edge graph excluding lib.rs still contains an
89-file mutually-reachable cluster (1,879 pairs) whose fuse is engine/mod.rs
(48 engine/ files). Direct mutual pairs with lib.rs and mod.rs files excluded —
only 5, one of which this arc touches:

```
src/setup.rs            src/setup/wire.rs
src/setup.rs            src/setup/vscode.rs
src/setup/manifest.rs   src/setup/manifest/write.rs
src/setup/manifest.rs   src/setup/manifest/actions.rs
src/storage.rs          src/storage/call.rs
```

daemon.rs cycles only with daemon_shell/* (6 mutual pairs); no direct
daemon↔engine mutual edge — the section-4 cut lines cross no cycle.

**(c) dag-layers** (examples/dag-layers.dl): cyclic_n 105, edge_n 1,524; tiers
0→158, 1→11, 2→11, 3→3, 4→1, cap(64)→171. Caveat, recorded honestly: the
2-cycle collapse misses 3+-cycles on this corpus, so top tiers are degenerate;
usable facts are modgraph.rs at tier 1, parse.rs tier 2, and typegraph/daemon/
engine/mod inside the capped mass.

**(d) typegraph consumers and symbol surface** (scip_ref × scip_occurrence):

```
tg_consumer => ref_file                      syms      tg_dependency => def_file
src/engine/extract/type_rels.rs              38        src/cst.rs   7
src/engine/extract/dataflow.rs               32        build.rs     1
examples/extract_ab.rs                       31
src/engine/extract/call.rs                   18
src/engine/extract/text.rs                   14
src/engine/extract/mod.rs                    13
src/engine/mod.rs                             6
src/rels/analysis.rs                          3
src/cst.rs                                    2
```

Externally-referenced symbols: **159**; internal-only: **368** (70% of the
symbol population is private implementation). The external set concentrates in
the shared facade: AnalysisBundle (7 files), type_langs, AnalysisMask,
RustTypes, TypeFacts/DataflowFacts/CallFacts + field accessors. Note the
consumers partition per-FAMILY (type_rels/dataflow/call/text) — but each pulls
only facade types that live in the shared core under either split axis, so the
family-shaped consumer pattern is satisfied by mod.rs re-exports regardless;
the 70% private mass is what partitions, and it partitions by language (2.1,
2.3).

**(e) daemon.rs / engine/mod.rs symbol surface** (same run):

- daemon.rs: 106 external / 105 internal-only. External set: root_label 25,
  Daemon 14, socket_path_for 12, ServedRoot 9, daemon_home 8, rpc_call 6,
  enabled_for 6, ensure_singleton 5, connect 5 — i.e. the client half +
  lifecycle, matching the section-4 cut lines.
- engine/mod.rs: **817 external / 26 internal-only** — the opposite shape;
  essentially everything is surface (db field 427 refs, Engine 371,
  refresh_rel 82). Consequence: section-5 trims must be pure re-exports, and
  the real seams (db, refresh_rel) belong to the engine-trait epic, not this
  arc.

**(f) feature-envy** (examples/feature-envy.dl, ran clean; types_known 1,056,
envied_pairs 1,448): the strongest envy row in the corpus is
`daemon.rs dispatch_root() → Engine, 13 distinct methods` (next:
run_settle→Engine 11, desugar/classify→Rule 10, tick_report→Rule 9). Receipt
for carving dispatch.rs out of daemon.rs as its own module.

**(g) measures top-K** (std/measures.dl via /tmp wrapper — the installed
binary's embedded std predates it): blast_top10 is headed by
engine/type_arena.rs (TypeArena.iter 1,068 dependents, .get 1,056), verbs.rs,
rpc.rs. fan_in_top10 includes `typegraph.rs::push` 103. **cycle_member_top10:
all rows are typegraph TS-flow fns** (ts_lift_fn 8, ts_flow_body_stmt 8,
ts_flow_call 7, ts_for_in_of 7, …) — the only fn-level recursion knots in the
corpus live in the TS dataflow walkers, the exact code ts/flow.rs isolates.

**(h) arch-flow** (examples/arch-flow.dl): 13 numeric-prefixed ARCH markers
confirmed (00-main … engine/60-gen); 31 resolved flow edges, heaviest
40-tick→55-meta 16; ONE backwards edge `50-derive→40-tick 1` (fixpoint calling
back into the orchestrator — pre-existing, out of scope, recorded); one
`arch-df-golden` warn attributable to the index.scip being hours older than
the worktree (staleness, not a violation).

**(i) library affinity via module_import + counted occurrences** (module_import
does record external specifiers, at one-row-per-use-statement granularity;
occurrence counts below are `grep -oE '\bX::'` per file vs src-wide total):

| file | crate | occurrences | src-wide | locality |
|---|---|---|---|---|
| graph/typegraph.rs | ts_ast (alias of oxc_ast, typegraph.rs:2420) | 221 | 221 | 100% |
| graph/typegraph.rs | syn | 108 | 108 | 100% |
| graph/typegraph.rs | tree_sitter | 120 | 198 | 61% |
| graph/typegraph.rs | oxc_span/oxc_allocator/oxc_ast_visit/oxc_parser | 17/13/11/8 | same | 100% each |
| lsp.rs | lsp_types / lsp_server | 8 / 4 | 8 / 4 | 100% |
| lsp.rs | serde_json | 81 | 394 | 21% |
| storage/call.rs | rusqlite | 37 | 242 | 15% |
| engine/extract/mod.rs | blake3 | 15 | 57 | 26% |
| daemon.rs | serde_json 8 / anyhow 8 / tokio 3 | — | — | spread |
| parse.rs, typecheck.rs, engine/tick.rs | ~zero external | — | — | pure |

The whole oxc surface and ALL of syn live in typegraph.rs; tree_sitter is 61%
there. Three parser stacks in one file, each 100%-partitionable to one
per-language module.

**(j) defect found by dogfooding**: `dl examples/coupling-metrics.dl` FAILS on
the current binary — `error[reserved-name]: relation 'file_lines' is a
reserved built-in engine relation` (the example predates the file_lines
builtin). The receipts above came from a `file_lines→eng_file_lines` patched
copy, which reported: files_over_budget 4 (derive.rs 2401, engine/mod.rs 1908,
extract/mod.rs 1727, meta.rs 1576) — confirming the rail is blind to the five
worst offenders. Fixing the example is folded into step 0.

## 3. typegraph split design

Axis decision: **per-language, shared core in mod.rs** — chosen on four
measurements: (a) the external pub surface is 100% core (2.2, confirmed
scip-level in 2.4d: 159 external vs 368 internal-only symbols, facade types
only); (b) library affinity partitions perfectly by language (2.3, 2.4i:
ts_ast 221/221 and syn 108/108 both 100%-local, whole oxc stack local); (c)
the family axis severs no dependency and multiplies parser-crate spread —
the per-family CONSUMERS (2.4d) touch only facade types that stay in mod.rs
under either axis; (d) the corpus's only fn-level recursion knots are the TS
dataflow walkers (2.4g), which the language axis isolates into one file. The
old branch reached the same shape independently.

Target tree (estimates from the region math; in-flight callable work adds ~+139
net to Rust/TS/Kotlin arms — numbers below include a rough +50/+50/+40 spread):

```
src/graph/typegraph/
  mod.rs        ~780   shared model structs, mint_sym, trait TypeLang,
                       AnalysisMask/Bundle, type_langs() registry
                       (LANG-JUNCTION comment moves here with it),
                       line_index/line_at/line_col, source_type_for,
                       shared doc helpers, pub use re-exports
  rust.rs     ~1,390   RustTypes + syn edges/entities/consts/docs/calls/dataflow
  ts/mod.rs     ~950   TsTypes + oxc edges/entities/consts/docs/call defs+sites
  ts/flow.rs    ~870   TS dataflow + JSX walkers
  ts/text.rs    ~380   ts_comments, TemplatePart/ts_template_parts,
                       UnresolvedRef/ts_unresolved_refs (+ their walkers)
  kotlin.rs     ~890   KotlinTypes + tree-sitter-kotlin everything
  go.rs       ~1,050   GoTypes + tree-sitter-go everything
  python.rs   ~1,270   PyTypes + tree-sitter-python everything
  analysis.rs   ~220   type_shape_hashes, type_lgg_pairs, hex_string
```

Every file lands under the 1,500 budget. TS as a single ts.rs would be ~2,240
(over budget — the old branch shipped it that way at 2,178 and it would trip
the widened rail immediately), hence the ts/ sub-tree (open decision 1).

Planning-protocol layers:

1. **Type signatures.** Nothing changes. The pub surface stays exactly:
   `pub trait TypeLang`, `pub fn type_langs() -> &'static [&'static dyn TypeLang]`,
   the model structs of region 1–452, `pub fn mint_sym(..) -> String`,
   `pub fn edges/kotlin_edges/ts_edges(..) -> Vec<TypeEdge>`,
   `pub fn ts_comments/ts_template_parts/ts_unresolved_refs`,
   `pub fn type_shape_hashes/type_lgg_pairs`. mod.rs carries
   `pub use {rust::edges, kotlin::*, ts::*, analysis::*}` so every existing
   `crate::typegraph::X` path resolves unchanged (same pattern graph/mod.rs
   already uses for the crate root).
2. **Pseudo-code.** n/a — pure code motion, zero logic edits. The one
   permitted edit class: `fn` → `pub(crate) fn` where a moved test or sibling
   module needs visibility (the old branch needed exactly one such bump,
   expand_use, 615d5b1d).
3. **Instance lifetimes.** The five lang unit structs are stateless statics in
   the `type_langs()` registry; AnalysisBundle instances live in engine-side
   per-tick cache maps (engine/mod.rs). No lifetime changes.
4. **Storage.** No schema, rel, or digest change. Uniqueness condition:
   extraction output must be byte-identical pre/post split — pinned by the
   determinism it-test (a45c34d9) and scripts/rails-oracle.sh prev-rev oracle.

Details the split must carry:

- **Tests**: the 1,304-line tests mod splits per language into each lang
  module's own `#[cfg(test)] mod tests`; shared fixtures (the `Engine{db:Db}`
  fixture struct at 7560, `struct S`/`impl E` shapes) stay in mod.rs tests.
- **@callable markers** (landing on callable-lambda-ctor): each marker is a
  comment adjacent to its emitter arm — they move WITH their arms, and
  examples/callable-coverage.dl joins on marker text + call_def rows, not line
  numbers, so it stays green through the move. Verify its rail as part of the
  step's gate.
- **LANG-JUNCTION(typelang-registry)** comment (line 439) stays glued to
  `type_langs()` in mod.rs.
- **Doc citations**: docs/df-coverage.md and docs/callable-coverage.md carry
  ~40 raw `src/graph/typegraph.rs:NNNN` citations. Step 3 of the migration
  rewrites them to `file + fn-name` form (`src/graph/typegraph/rust.rs
  rust_call_defs_from`) — function-name anchors survive future refactors, raw
  line numbers do not. Where a @callable marker exists, cite the marker.

## 4. daemon.rs split + friction (f)

daemon.rs (2,714) has no dominant library affinity (2.3, 2.4i); its seams are
behavioral, and the receipts name them: dispatch_root is the corpus's single
strongest feature-envy row (13 distinct Engine methods, 2.4f) — it is engine-
driving code hosted in daemon.rs — and the external symbol surface (2.4e)
clusters on the client/lifecycle half (root_label, socket_path_for, rpc_call,
connect, ensure_singleton). daemon.rs participates in no cycle outside
daemon_shell/* (2.4b), so every cut below is cycle-free. next already peeled
transport (daemon_shell/), reads (daemon_read.rs), and discovery
(daemon_http.rs). Remaining regions:

| region | lines | concern |
|---|---|---|
| 80–230 | ~150 | home/socket/pid/roots.json paths |
| 231–861 | ~630 | Shared + ServedRoot (state, engine locking, per-root db) |
| 862–895 | ~35 | RootRecord json |
| 896–1074 | ~180 | Daemon struct + impl |
| 1075–1207 | ~130 | budget: daemon_thread_count, apply_daemon_budget (standing-law code) |
| 1188–1612 | ~420 | run_daemon + DaemonJobRunner + exe-stamp/idle/mem env fns |
| 1613–2187 | ~575 | RPC dispatch: req_root, daemon_summary, dispatch_root, run_eval, run_q_eval, json renderers |
| 2188–2714 | ~525 | CLIENT half: enabled/connect/rpc_call/read_frame_watched/ensure_daemon/ensure_singleton/restart/spawn_detached/wait_ready/stop/drop_root/await_quiescent/load |

Target: promote to `src/daemon/`:

```
src/daemon/
  mod.rs       ~640   Daemon, Shared, run_daemon, DaemonJobRunner, tracing init
  root.rs      ~660   ServedRoot + RootRecord + roots.json
  home.rs      ~150   daemon_home/socket_path/pid_path helpers
  budget.rs    ~130   apply_daemon_budget + thread/mem/idle env fns
  dispatch.rs  ~575   dispatch_root + run_eval/run_q_eval + json renderers
  client.rs    ~525   the client-side connection/lifecycle fns
```

mod.rs `pub use`s keep every `crate::daemon::X` path working. This is the
continuation of next's own decomposition direction, not the old branch's
(superseded) paths/loops/health cut.

**Friction (f) concrete rename**: `git mv src/cli/daemon.rs
src/cli/daemon_cmd.rs`, `mod daemon;` → `mod daemon_cmd;` in src/cli/mod.rs,
fix the handful of `daemon::run_cmd`-style references inside cli/. After it,
`daemon` names exactly one module path (`crate::daemon`); the cli module reads
as what it is (the `dl daemon <verb>` command family). Pure git mv + use-path
fix — Route A-shaped, though at O(5) references a hand edit is cheaper than
driving `--move`.

Sibling absorption (`daemon_read.rs` → `daemon/read.rs`, `daemon_http.rs` →
`daemon/http_discovery.rs`, `daemon_shell/` → `daemon/shell/`, with lib.rs
re-exports for back-compat) is open decision 2 — recommended, second wave,
because it completes the single-namespace story but touches lib.rs pub paths.

Timing: the daemon steps hold until the in-flight scheduler plan (jobq/daemon
design) lands or explicitly releases the files — same merge-order rule as
typegraph (section 8).

## 5. engine/mod.rs + extract/mod.rs residue (friction h)

(h) itself is resolved: AST_LANG_TABLE lives in src/engine/lang_tables.rs.
The receipts bound what this arc may do here: engine/mod.rs has 817 external
symbols vs 26 internal-only and fan-in 108 (2.4a/e) — essentially everything
in it is surface, so any move MUST be a re-exported relocation, and the real
seams (`db` 427 external refs, `refresh_rel` 82) are the engine-trait epic's
to cut, not this arc's. Residual placement work to get both files under
budget:

- engine/mod.rs (1,908): move the nine pub query-result structs
  (DiagRow, QueryResult, RefHit, RefLens, LocateHit, SymbolRow, HierarchyItem,
  HierarchyCallEdge, SpineDelta — lines ~694–850) to `engine/results.rs`
  (re-exported); move the family rel-list constants (MODULE_RELS … SPINE_RELS,
  lines ~315–501) into `engine/family/mod.rs` where the family registry already
  lives. Net: mod.rs ≈ 1,540 → with the misc time/env helper block (~900–955)
  moved to an existing util home, under 1,500.
- engine/extract/mod.rs (1,727): move ScipOccIndex + narrow_ambiguous (~300–422)
  to `extract/scip_narrow.rs`; move `mod verdict_reason_tests` (1,457–end,
  ~270 lines) to `extract/verdict_tests.rs`. Net: mod.rs ≈ 1,300.

Both are engine-family files; these moves are deliberately minimal so they do
not collide with the parked engine-trait epic (which owns the real
derive/meta/tick redesign).

## 6. Normalization pass

- **Banned-word identifiers** (repo law): full grep of src for
  provenance/substrate/load_bearing/regime finds exactly one identifier
  cluster: `src/engine/pipeline/full_sources_tests.rs` —
  `fn prepared_rows_apply_with_provenance_then_cleanup()` + `let provenance`
  (3 hits) and one comment in `src/engine/pipeline/full_sources.rs`. Rename to
  `..._with_source_marking_then_cleanup` / `let source_count` (or match the
  column the test actually reads). No other identifier violations exist in src.
- **Rail glob widening** (harvest efed0e4f's intent): coupling-metrics.dl's
  `file_lines`/`file_over_budget` rules scan only `src/engine/*.rs` +
  `src/rels/*.rs`. Widen to recursive `src/**/*.rs`. This is step 0 — it makes
  the whole arc self-measuring and defines done (rail quiet).
- **Single-letter dl variables**: the instruments this plan's receipts cite
  carry single-letter binders (probe counts: coupling-metrics.dl 21,
  dag-layers.dl 22, measures.dl 16, feature-envy.dl 6, arch-flow.dl 1). Per
  the standing style law, rename opportunistically in the same commit that
  touches each file (step 0 touches coupling-metrics.dl; the others only when
  edited).
- **Per-family file conventions**: src/engine/extract/ already follows
  one-family-one-file (call/dataflow/doc/node/text/type_rels); src/rels/ is
  already per-domain (analysis/git/scip/perf/...). Convention to write down in
  the arch doc: a family's extractor file, its rels registration, and its
  storage module share the family stem (call.rs ↔ storage/call.rs ↔
  CALL_RELS). storage/call.rs itself stays whole (single rusqlite concern,
  2.3).

## 7. Migration sequencing

Every step compiles, passes the full suite, and is one PR-sized commit. "Pure
git mv" = git tracks the rename; everything else is fragmentation (delete +
create, per the old-branch measurement) and needs the determinism gate.

| # | step | kind | gate |
|---|---|---|---|
| 0 | fix coupling-metrics.dl's reserved-name break (its `file_lines` rel collided with the new builtin, 2.4j — rename the rel, or better: rewrite the rules on the `file_lines` BUILTIN and drop the awk cmds); widen the budget glob to `src/**/*.rs`; rename its single-letter dl vars; record the baseline violation list in the commit message | .dl edit | program runs on the current binary; rail emits exactly the table in section 0 |
| 1 | typegraph → `src/graph/typegraph/` per-language (mod/rust/kotlin/go/python/analysis + ts single file for now), tests split per-language, markers move with arms | fragmentation | build + suite + determinism oracle (scripts/rails-oracle.sh) byte-identical; callable-coverage.dl rail green |
| 2 | ts sub-split → ts/{mod,flow,text}.rs | fragmentation | same gate |
| 3 | rewrite docs/df-coverage.md + docs/callable-coverage.md citations to file+fn-name (marker-relative where @callable exists) | docs | every cited fn greps in its cited file |
| 4 | modgraph → `src/graph/modgraph/` per-language (harvest old-branch tree shape: mod/rust/ts/kotlin/go/python) | fragmentation | suite + determinism oracle |
| 5 | `git mv src/cli/daemon.rs src/cli/daemon_cmd.rs` (friction f) | pure git mv | build + `dl daemon status` smoke |
| 6 | daemon.rs → `src/daemon/` {mod,root,home,budget,dispatch,client} | fragmentation | suite + daemon it-tests; holds for scheduler-plan landing |
| 7 | engine/mod.rs residue: results.rs + rel-consts → family/ | fragmentation | suite |
| 8 | extract/mod.rs residue: scip_narrow.rs + verdict tests out | fragmentation | suite |
| 9 | banned-word rename in full_sources_tests.rs | rename | suite |
| 10 | (optional, decision 2) absorb daemon_read/daemon_http/daemon_shell under src/daemon/ | pure git mv + lib.rs re-exports | suite |
| 11 | (optional, decision 4) parse.rs → parse/{mod,ops}; propose.rs → propose/{mod,sequence,shapes} per old-branch shapes | fragmentation | suite + determinism oracle |
| 12 | receipts re-run (section 9) + arch-doc note recording conventions | docs | — |

Route A (`--move`) applicability: it rewrites `use` paths when a module PATH
changes. Steps 1/2/4/6/7/8 deliberately keep every existing path via mod.rs
re-exports, so there is nothing for `--move` to rewrite; step 5 changes a path
with O(5) internal references (hand edit cheaper); step 10 changes lib.rs-level
paths but keeps re-exports. Conclusion: the splits do not depend on the
auto-refactor residual work, and none of it blocks this arc.

Ordering constraints: 0 and 9 anytime; 1→2→3 strictly ordered and gated on the
callable merge; 4 independent after 0; 5 anytime; 6 (and 10) gated on the
scheduler plan; 7/8 independent. Suggested landing order: 0, 9, 5, 1, 2, 3, 4,
7, 8, 6, 10?, 11?, 12.

## 8. Merge-order dependency (hard)

- **callable-lambda-ctor**: the active agent has ~185+/46− uncommitted lines in
  typegraph.rs (EntityKind::Lambda, ctor emission, @callable markers) — its
  committed tips (4e7b297f) are already ancestors of this plan's base. Steps
  1–3 DO NOT START until that branch merges to next; the split then rebases as
  pure re-cutting (the region map shifts by known deltas; the item-level cut
  list is regenerated by the same grep). Starting earlier guarantees the exact
  modify/delete conflict that killed refactor/file-splits.
- **scheduler plan** (jobq/daemon design, in flight): steps 6 and 10 hold until
  it lands or its owner confirms daemon.rs is untouched by it.
- This plan itself lands as a document now; execution is a follow-up arc.

## 9. Receipts that prove the arc

- **file-size**: the widened rail's `file_over_budget` row set goes from the
  section-0 table (13 files) to ∅ among split targets; max first-party file
  size drops 8,653 → ~1,500.
- **extraction identity**: scripts/rails-oracle.sh prev-rev oracle byte-identical
  across each fragmentation step (the strongest zero-behavior-change proof this
  repo has).
- **coupling**: re-run the section-2.4 receipts post-arc; module-level
  fan_in/fan_out for `typegraph` becomes fan-in to `typegraph::mod` only;
  no new cycles in dag-layers strata.
- **library affinity**: per-file parser-crate spread goes from {syn, oxc,
  tree_sitter} × 1 file to exactly one parser stack per language module
  (re-run the 2.3 counts).
- **compile time** (best-effort): `cargo build --timings` before/after —
  fragmenting typegraph.rs is expected to improve incremental rebuilds touched
  by one language; record, don't promise.
- **docs**: callable-coverage.dl rail green; every rewritten citation greps.

## Open user decisions

1. **TS sub-tree** — ship typegraph/ts as one 2,240-line file (old-branch
   shape, trips the rail) or ts/{mod,flow,text}? Recommendation: ts/ sub-tree
   (steps 1+2 can land as one commit if preferred).
2. **Daemon namespace absorption** (step 10) — move daemon_read/daemon_http/
   daemon_shell under src/daemon/ with lib.rs re-exports? Recommendation: yes,
   after step 6 settles; it completes friction (f)'s "one daemon namespace"
   story.
3. **cli rename target** — `cli/daemon_cmd.rs` vs `cli/daemon_verbs.rs`?
   Recommendation: daemon_cmd (matches the file's own `run_cmd` entry point).
4. **parse.rs / propose.rs** (1,612/1,611, marginally over) — split now per
   old-branch shapes or waive at the rail (budget bump to 1,650)?
   Recommendation: split, as step 11, low urgency; a waiver line invites drift.
5. **Budget number** — keep 1,500 after widening the glob, accepting
   engine/derive.rs (2,401), meta.rs (1,576), storage/call.rs (1,938) as
   standing warnings owned by the engine-trait epic? Recommendation: keep
   1,500; the warnings are the epic's public backlog, which is what the rail
   is for.
6. **Citation style** — adopt fn-name anchors only, or also add numeric ARCH-
   style markers inside the new typegraph modules? Recommendation: fn-name
   anchors only; the @callable markers already cover the emitter arms, and a
   second marker system is upkeep without a reader.
