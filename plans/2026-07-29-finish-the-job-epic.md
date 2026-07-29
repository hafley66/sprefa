# FINISH THE JOB: the v6 alpha driven to done

Written 2026-07-29 (last session of this model load), base sha
`b535ca62`. This document SUPERSEDES `plans/2026-07-29-v6-alpha-golden-plan.md`
as the driving plan. That plan's phases P0-P4 are DONE; its phase 5 is epic
E2 here. Nothing in this file needs re-derivation: every count, path, symbol
and refusal name below was read off the tree at the base sha or off a commit
message cited inline.

Standing user words that govern every epic here:

| word | effect |
|---|---|
| "problems are turbo mid, find the SMALLEST CORRECT solution" (2026-07-29) | no epic below proposes a rewrite where a fixture, a column or a refusal closes it |
| "no tagging a non working thing" | E11 pushes; the tag waits for the user |
| labs run on OPUS ONLY (2026-07-29 night) | every `opus lab` owner-shape below is deliberate, not a default |
| build-vs-buy law | E6 and E7 both open with a research lane and a written candidate table before any bespoke line |
| zero new constructs unless a program PROVES a gap | every construct request below names the program that proves it |

---

## 0. RECON: observed facts at base `b535ca62`

These are measurements, not tasks. A later session that disagrees with one
should re-measure, not re-argue.

### 0.1 Battery

| gate | command | value at base |
|---|---|---|
| conformance | `cd v6 && just conformance` | 163 PASS / 0 findings |
| sweep (both modes) | `cd v6 && just sweep` | 163 swept, 102 compiled, 100 identical, 0 wrong, 2 run_error, 61 unsupported |
| plunit | `cd v6 && just plunit` | 140/140 |
| text door | `cd v6 && just text-door` | 102/102/0 |
| roundtrip | `cd v6 && just roundtrip` | ALL PASS |
| everything | `cd v6 && just green-all` | exit 0; END GOAL / ENDURANCE / MEMORY SOAK / EXTRACTION LIVE / ENUMERATE / FLAGSHIP / LSP DIAGS all HOLD |

`green` = conformance roundtrip text-door plunit prolog-lint tsv2-test
import-gate one-subscribe dl-test store-test.
`green-all` = green + endurance leak-soak serve-endurance serve-leak-soak
memory-soak extraction-live enumerate flagship lsp-diags sweep staleness-gate.
`crawl-bench` is deliberately NOT in green-all.

### 0.2 The 61 unsupported fixtures, by refusal head

Read from `v6/prolog/compile/out/manifest.json` at base.

| count | refusal | epic that owns it |
|---:|---|---|
| 13 | `edge_body_needs_pre` | E9 (ARCH `pre_occurrence_loop`, own execution shape) |
| 9 | `edge_body_needs_json_destructure` | E2 (SLOT-TERM-STRUCT ruling gates it) |
| 4 | `level_body_goal` | E2 |
| 4 | `type_arrival_shape_mismatch` | intentional refusal fixtures |
| 4 | `aggregate_head` | E2 (json agg heads: emission refusal outlives the ticklog ruling) |
| 2 each | `type_cycle`, `column_type_unknown`, `json_value_expression`, `decode_source_not_struct` | intentional |
| 1 each x 18 | `enum_variant_name_collision`, `match_nonexhaustive`, `keyed_level_head`, `decode_field_unknown`, `missing_retention`, `aggregate_in_edge_head`, `keep_on_non_log_rel`, `keyed_log_rel`, `edge_into_unkeyed_set`, `log_on_level_headed_rel`, `latest_in_level_rule`, `pre_in_level_rule`, `arith_operand_not_int`, `join_column_type_mismatch`, `comparison_type_mismatch`, `decl_type_conflicts_witness`, `edge_body_with_negation`, `edge_head_conflict_risk`, `trigger_arg_not_var` | all intentional named refusals with fail-first fixtures |

Reading: 41 of 61 are refusals the language MEANS. The real construct debt is
`pre` 13 + json destructure 9 + the 4 aggregate heads = 26.

### 0.3 The 2 run_error fixtures (pre-existing, in every sweep line)

`log_retraction_rejected` and `fork_join_error_arm_is_a_value`, from
`v6/prolog/compile/out/run-results.json`. Both are rejection-semantics
fixtures with no comparable oracle log. Owned by E9.

### 0.4 Flow parity, four-query table (commit `ed81cdc6`, verbatim)

| rel | v5 | v6 | match | note |
|---|---:|---:|---:|---|
| flow_edge | 2462 | 2184 | 2184 | v6 is a strict subset; 278 v5-only rows unexplained |
| flow_reach | - | - | 9112 | plus 177 v6-only reflexive rows |
| flow_param_type | - | - | 0 | referee key gap: v5 `root::` sym prefix + qualified type names vs v6 bare |
| flow_node_type | - | 0 | 0 | v6 EMPTY, real rail gap, open |

Rig: `v6/tsv2/scripts/flagship-flow.sh` + `flagship-flow-classify.py`
(coordinate translation lives in the referee; calibration receipt in the
classifier header: byte 3981 of `src/bin/extract.rs` = line 94 col 8).
Program: `v6/dl/fixtures/flagship-flow.dl6`. Held-then-merged branch:
`codex/flow-parity` (`ccfe53ec`), merged at `837fe7f2`.

### 0.5 Comment-node lab receipts (`plans/2026-07-29-comment-node-verdict.md`)

| receipt | value |
|---|---|
| v5 `comment_node` parity | 745/745 rows byte-identical on pinned corpus, 0 only-v5, 0 only-v6 |
| `std/arch.dl` `arch_node` parity | 4/4 |
| v5 text-operation call sites across rails | 57 (`=~`, `replace_re`, `trim`, `split`, `json`, `match_line`, `match_ast`) |
| v6 writable expression surface | 11 rows in `registry.pl expression/5`, all `both_int`/`same_type`, ZERO text ops |
| route boundary amplification, route b1 vs route a | 38.7x (230,096 rows vs 5,939) |
| selected route | b2, host template pre-filters (5,939 rows, 597,770 bytes, 494ms) joined to route c's scanner |
| scanner false positives | 9 across 8,193 flagged lines = 0.11% |
| statement growth | 63 -> 92 stmts/tick while corpus grew 57x (1.46x, bounded not flat) |
| 7 techniques status | 1,2,7 graded live; 3,4,6 not ported (WORK, not gaps); 5 blocked on markdown grammar |
| 4 fixture candidates | `comment_witness_gates_a_scanner_hit`, `disable_next_line_shifts_the_effect_by_one`, `unused_suppression_antijoins_the_finding`, `arch_hierarchy_from_decomposed_marker_rows` (4/4 PASS, not promoted) |

Lab last copy: `git show 9b5ba958:v6/prolog/labs/comment_node/<file>`. The
byte-span flattener is `cn.py`.

### 0.6 Crawl bench (`v6/tsv2/CRAWL-BENCH.md`)

| engine | files | repos | wall | files/s | RSS | db |
|---|---:|---:|---:|---:|---:|---:|
| v5 org-fan, 389 repos | 42,739 | 389 | 12.07s | 3,540.93 | 367,230,976 B | 52,371,456 B |
| v6 served + extraction, 8 repos | 779 | 8 | 19.15s | 40.68 | 174,014,464 B | 1,069,056 B |
| v5 memory doc, historical | 42,739 | 389 | 5.9s | 7,244 | - | not recorded |

v6 `stmts/tick` 54.03 both runs. Named gap in the doc: v6 has NO org fan-out
spelling; the shell loop supplies it. There is NO data-amplification column in
this table, in `SCALE.md`, or in `v6/sprefa-store/PERF-REPORT.md`. PERF-REPORT
already carries `host_peak_mb,sqlite_hw_mb,db_mb` as the common CSV extension,
so the sensor has a home.

### 0.7 Perf, ingest

`commit_ms` ~10.8ms/file is the named next cost (74.2 files/s after the
`rowsForPath` fix; v5 yardstick 7,244 files/s). `v6/dl/src/4_ingest.ts:93`
still defaults `DL_EXTRACT_BIN` to a DEBUG build path;
`extraction-live.sh` already exercises the correct resolution order
(`DL_EXTRACT_BIN` -> in-tree release -> build) and is the shape 4_ingest
should adopt.

### 0.8 Rulings in force that shape these epics

From `v6/prolog/conformance/rulings.pl` plus two recorded after this base
(coordinator relay, cite them, do not re-derive):

| ruling | choice |
|---|---|
| `compound_storage` | `struct_as_rows` |
| `json_ticklog_encoding` | `canonical_json_text` |
| `struct_arrival_key_order` | `decl_induced_canonicalize` |
| `keyed_level_head` | `named_refusal` |
| `retention_count_lowering` | `retracting_rule_over_log` |
| `udf_residency` | `libsql_fuse_and_delta_deopt` |
| `expression_residency` | `fuse_to_sql_deltas_ts_deopt_last` |
| `host_residency` | `rows_stay_in_sqlite_host_sees_deltas` |
| `watcher_dep` | `fs_watch_until_bench_regression` |
| `edb_definition` | `never_headed_rel_is_pure_subject` |
| `bool_column_type` (NEW, post-base) | `two_valued_column_type` (user overruled the presence/enum shape as un-ergonomic; strict 2VL, no null; storage spelling rides phase 5) |
| `numeric_precision` (NEW, post-base) | `approved_phase5_design` (float/REAL + avg yes; precision spelling designed in-arc) |

### 0.9 Plan boundary and lowering boundary

Unchanged from the tsv2 reorientation, restated because three epics below
cross it:

| layer | owner | what may live here |
|---|---|---|
| authoring surface | `.dl6` text | `parse_dl.pl` DCG is CANONICAL; `registry.pl` is the construct budget; SYNTAX.md generated half comes from registry |
| canonical term form | prolog terms | shared expansions in phase order: enum -> decl spread -> row spread -> match -> host |
| reference semantics | `conformance/engine.pl` + `ticklog.pl` | THE ORACLE. Every semantics change lands here AND in the compiler in the SAME arc (the A4 law) |
| checked IR | `compile/analyze.pl` + `lower.pl` `lowered/8` | target-neutral plan; rust backend plugs the same plan |
| target | `compile/emit_ts.pl` -> `v6/tsv2/gen_emitted/*.ts` | literal readable TS, real SQL, real rxjs |
| runtime | `v6/tsv2/runtime/` + `v6/tsv2/serve/` | hand-written, import-gated; `v6/dl` is the OTHER runtime and stays untouched |
| extraction | `v6/sprefa-extract` (rust) | policy-free; markers/key significance/suppression stay in-language |

---

## 1. EPIC ORDER AND ONE-LINE CONTRACTS

| # | epic | contract (what done means) | blocks / blocked by |
|---|---|---|---|
| E1 | Simplify wave | the 19 deduped review findings applied, 3 P0s closed, battery unmoved except the named growth | blocks E2 (shared compiler files) |
| E2 | Phase 5: types, checker gate, ingest perf | bool + float column types live with literals and `avg`; the clock checker has a PROVEN bug-class table before one line of it is built; commit_ms halved | blocked by E1 |
| E3 | Span flattener + comment-rail wiring | every extractor family reports line numbers; 4 comment fixtures promoted; techniques 3/4/6 ported | blocked by nothing (E1 for file peace) |
| E4 | Flow-parity residue | four-query table has zero unexplained rows; flow_node_type non-empty; flow_param_type matched | blocked by E3's flattener (coordinate units) |
| E5 | Amplification sensors, then diet | one sensor column in the shared bench CSV, measured on 3 corpora; diet arc only if the number says so | blocked by nothing |
| E6 | Doc-format extraction | html/xml/md/json/yaml/toml enter `sprefa-extract` behind a research table | blocked by nothing |
| E7 | Schema/spec import (TypeSpec, JSON-Schema, OpenAPI v3) | a real OpenAPI doc becomes rel/type/enum decls, round-trips back out | blocked by E6 (json/yaml) and E2 (bool/float type plane) |
| E8 | Analysis-oracle exam | a 15-task graded analysis exam replaces v5 as the standing oracle | blocked by E4 (capability) and E6 (breadth) |
| E9 | Standing cracks with no owner | every unowned defect in ARCH is closed or has a dated owner | parallel |
| E10 | Decision tail | the ~20 open rulings are presented as cards and closed | continuous, user-gated |
| E11 | Release path | main pushed; v5 pile executed; tag on user word | user-gated |

Critical path: E1 -> E2 -> E7. Longest independent chain: E3 -> E4 -> E8.
E5, E6, E9 run beside anything.

---

## 1.5 HANDOFF: HOW TO DRIVE THIS FROM CODEX

The driving agent after this session is codex, with sol / luna / terra lanes.
This document plus the briefs it names is the whole program: nothing below
depends on a coordinator remembering anything. A session that opens only this
file can dispatch every epic.

### Lane map

| lane | proven for | epics it owns here |
|---|---|---|
| **sol** | compiler and emitter trade-offs: `analyze.pl`, `lower.pl`, `emit_ts.pl`, `registry.pl`, `engine.pl`, refusal design | E1 P0s, E2a, E2b.4-5, E7.3-7.5, E9.4, E9.5, E9.12 |
| **luna** | mechanical sweeps, renames, benches, scripts, research tables, TS runtime cleanup | E1 P1-P3, E3 (all), E4 (all but 4.2 rust), E5, E6.1, E7.1, E8.1-8.3, E9.1-9.3, E9.6, E9.10, E9.11, E11.10 |
| **terra** | the rust extractor `v6/sprefa-extract`, and any brief that leaves a real decision open | E1 P4 rust half, E4.2 if extractor, E6.2-6.6 |
| **opus lab** | only where the user has said labs, and only from a planner-seeded header | E2a.1, E2b.1-2b.3, E5.6 if taken, E7.2, E8.4, E9.7, E9.8 |
| **coordinator-inline** | justfile wiring, merges, ARCH rows, greps, the push | E4.7, E8.5, E9.3a, E9.6, E9.15, E11.1 |

Model routing per `claude-research/commands/codex-delegate.md`: luna is the
DEFAULT and the brief's quality picks the model, not the diff size. Terra costs
more and is justified only when a brief genuinely leaves decisions open. There
is no escalation past terra; if terra stalls, ask the user.

### Launch protocol, verbatim, for every lane below

```bash
git worktree add ../sprefa-codex-<slug> -b codex/<slug> <base-sha>
# commit the brief to the base branch FIRST so the worktree contains it
cd ../sprefa-codex-<slug> && codex exec --sandbox workspace-write \
  -m gpt-5.6-luna -c model_reasoning_effort=high - <<'EOF'
<brief>
EOF
```

Never `--full-auto`. Always `--sandbox workspace-write`. Verify the header echo
(`model:` and `reasoning effort:`) in codex's first ten lines before walking
away.

### The coordinator-cut worktree constraint (measured 2026-07-29, do not relearn)

A worktree cut by the coordinator keeps its real git dir at the main repo's
`.git/worktrees/<name>/`, which is OUTSIDE the codex sandbox's writable roots.
Consequence:

| what | in a coordinator-cut worktree |
|---|---|
| `git merge --ff-only <sha>` | DIES on `ORIG_HEAD.lock: Operation not permitted` |
| `git commit` | fails the same way |
| base verification | must be READ-ONLY: `git rev-parse HEAD` compared against the sha stated in the brief |
| commit flow | NO-COMMIT: the lane leaves the tree dirty, the coordinator reviews file by file and commits |

Telling a codex agent in such a worktree to merge or commit wastes the launch;
it will correctly stop and report.

### Brief template every epic below must be expanded into

A dispatching session copies this, fills the bracketed parts from the epic's
tables, and commits it to `plans/` before launching.

```
1. WORKTREE + BASE. You are in ../sprefa-codex-<slug> on branch codex/<slug>.
   FIRST ACTION, read-only: `git rev-parse HEAD` and confirm it equals <sha>.
   If it does not, STOP AND REPORT. Do not merge. Do not commit anything;
   this is the no-commit flow and the coordinator commits your work.
2. THE PLAN. Read plans/2026-07-29-finish-the-job-epic.md section <E#> and
   <the epic's named brief, if any>. Follow them exactly.
3. LINE NUMBERS IN PLANS ARE STALE. Re-find every site by SYMBOL NAME.
4. HARD LAWS.
   - Files you own: <exact list>. Every other file is another lane's.
     A change you need outside that list is a NAMED STOP, not a patch.
   - The A4 law: an oracle semantics change lands with its emitter fixture
     in the same arc.
   - Fail-first receipts: every fix carries a sabotage that was red before
     and green after, pasted into the test header.
   - Count tests: a formerly-quadratic path gets a statement-count or
     EXPLAIN SEARCH-not-SCAN assertion, never end-state equality alone.
   - Hermetic tool runs: SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1
     plus a scratch --db. Never touch ~/.local/state/sprefa or a running
     daemon. ~/orgs is READ-ONLY.
   - No em dashes. Banned words in prose and identifiers: provenance,
     substrate, load-bearing, regime. Descriptive dl variable names, never
     single letters.
   - Vocabulary law: construct names use only rxjs, prolog, or SQL words.
5. TEST BUDGET. <the epic's exit-receipt commands>. Maximum <N> full sweep
   runs (the sweep takes minutes).
6. ESCAPE HATCH. If a step cannot be done within these laws, STOP that step
   and say why in the final summary. Improvising around a blocked command is
   a defect, never a fix.
7. FINAL SUMMARY SHAPE. Base sha you verified; per-task outcome; every exit
   receipt's actual output; every named stop with its refusal text; the exact
   sweep and conformance movement (which fixtures moved and to what bucket);
   anything you skipped and why.
```

### Existing briefs and headers each epic builds on

A dispatching session reads THESE plus the epic section. Nothing else is
required.

| epic | existing document | state |
|---|---|---|
| E1 | `plans/2026-07-29-simplify-wave-brief.md` | complete brief, 19 items + 3 P0s; item 1 already landed at `6522f848`; line numbers pre-date the `265da55f` merge and must be re-found by symbol |
| E2a | `v6/prolog/conformance/rulings.pl` (`bool_column_type`, `numeric_precision`) | rulings only; the arc brief must be written from E2a's table |
| E2b | `v6/prolog/compile/TICK-MODEL.md` sections 5 and 6 | the checker spec exists; section 7 is where the gate's bug-class table goes |
| E3 | `plans/2026-07-29-comment-node-verdict.md` | complete verdict with 4 graded fixtures and 5 answered slots; the flattener is `git show 9b5ba958:v6/prolog/labs/comment_node/cn.py` |
| E4 | `plans/2026-07-29-flow-parity-upgrade-brief.md`, `plans/2026-07-29-flow-interproc-scout.md`, commit `ed81cdc6` message | the scout carries the v5 producer table with rust file:line receipts; the commit message carries the four-query numbers |
| E5 | `v6/sprefa-store/PERF-REPORT.md` (CSV column doc), `v6/tsv2/CRAWL-BENCH.md` | the CSV extension precedent exists; the columns do not |
| E6 | `plans/2026-07-29-extract-doc-formats-header.md` | complete header with 4 named slots; step 0 research not started |
| E7 | none yet; `~/projects/hafley-tsp` is the prior art to read first | write the research brief from E7.1 |
| E8 | none yet | write the research brief from E8.1 |
| E9 | `v6/prolog/ARCH.pl` task rows carry each crack with its measurement | rows are the brief |
| E10 | `v6/prolog/conformance/rulings.pl` is the record; cards here are the presentation | user-gated |
| E11 | `archive/worktree-salvage-2026-07-27/README` carries the exact worktree commands | user-gated |

### Standing gate the coordinator runs on every returning lane

```
cd v6 && just green        # exit 0
cd v6 && just sweep        # movement stated exactly; 0 wrong, always
cd v6 && just green-all    # exit 0 before the merge is called landed
```

Re-run these on the MERGED tree as well as in the worktree. Three separate
landings this month were green in a worktree and red on main.

---

## E1. SIMPLIFY WAVE

**Goal.** Land the four-opus-reviewer dedup
(`plans/2026-07-29-simplify-wave-brief.md`) now that the sol host-seam lane
merged (`265da55f`) and freed `emit_ts.pl`, `parse_dl.pl`, `structPlane.ts`,
`serve/1_hosts.ts`, `plunit_tests.pl`.

**Contract.**

```
input : the brief's 19 items, each with file:line at write time
output: same battery numbers, minus the three P0 defects
invariant: every file:line RE-VERIFIED against the post-merge tree before edit
```

The brief was written against a pre-merge tree. An item whose line no longer
matches is a re-read, never a blind apply.

### Tasks

| id | task | files / symbols | owner-shape |
|---|---|---|---|
| 1.1 | P0-1 `__dict_` prefix sniff is a banned magic-name pattern | `lower.pl:1767` `dictionary_use/1`, `:678`, `:690`, `dictionary_relplans/2:800`; mint plan kind `dictionary`, test on relplan kind, derive DDL index name from `dictionary_table_name/2` | sol |
| 1.2 | P0-2 refusal umbrella misses 15 bare throws | `0_refusal_messages.pl` renders only `unsupported_construct/1`; `1_host_expand.pl` throws `refused_host_decl/1:120`, `column_mismatch/2:126,147`, `template_mismatch/1:153-157`, `bind_mismatch/2:212`, `bind_and_rule_head/1:217`, `query_mismatch/1:234`, `unmapped_feature/2:242,244,296`, `probe_mismatch/1:331,360,366`. Add multifile `refusal_term/1` inventory; derive module list instead of 10 hardcoded `refusal_source_module/1`; collapse per-signature rescan to one findall+keysort (`:48-63`) | sol |
| 1.3 | P0-3 watch vs enumerate digest incompatibility | `2_binds.ts:231` sha256 vs enumerate host `git hash-object`: two values under one `digest` column. Smallest fix = `digestOf` switches to git-hash-object semantics + test pinning watch-row digest == enumerate-row digest for identical bytes | luna |
| 1.4 | P1 cross-file dedup, 5 items | `call_name_match` x5 (`ts.rs:2655`, `rust.rs:741`, `go.rs:1541`, `kotlin.rs:1049`, `prolog/_0_source.rs:455`) -> one fn in `seams.rs` + HashMap def lookup; `canonicalizeJson` (`structPlane.ts:71` vs `ticklog.ts:18`) -> one export; `check_world_shapes/3` (`conformance/engine.pl:460` vs `compile/compile.pl:155`) -> one gatherer in `0_type_plane.pl`; `ScriptedWatchSource` x3 -> `tests/serveHelpers.ts`; `FlatFact::Sig` owner span emitted 3x (`types.rs:1290-1296`, `wire.rs:100-108`) -> one flat-pair spelling | luna (TS/prolog) + terra (rust) |
| 1.5 | P2 structPlane write path, one edit serves three findings | `collect()` records per-(arrival,column) semantic key while walking, `rewriteRow()` becomes a lookup: kills double canonicalization `:163/:183`, O(depth) re-render, impossible-error throw `:186`; drop `{childSemantic}` wrapper; memoize per-tick map `:202`. Plus `2_binds.ts` boot dedup memo | luna |
| 1.6 | P3 prolog mechanical, 6 clusters | `lower.pl` (dup `incremental_json_select_exprs_from/3` `:915`/`:966`, `goals_conjunction/2` `:832`, `braces_pattern_pairs/2`, `dictionary_storage_kind/3` `:695`, dead `decode_slot/6` Acc `:884`, dead `Types==[]` `:723,:2092`, `boot_column_slots` `:1999-2002`/`:2026-2029`, `boot_statements/5` signature `:2109` + 4 sites); `emit_ts.pl` (3 dead shims `:163,:1287,:1342`, dead `js_string` `:261`, `struct_ref_entry`/`column_type_ref_entry` collapse); `0_type_plane.pl`; `parse_dl.pl:453-457` `type_decl_columns/3` reuses `decl_a_columns/3`; `analyze.pl:383,:385-390` + `engine.pl:444`; `level_eval.pl:127-141` finish registry adoption (9 hardcoded functor clauses) | luna |
| 1.7 | P4 rust + scripts + tests | df edge+aux pairing helpers at 20 sites, 4 langs (`go.rs:707`/`:1080`, `kotlin.rs:573`/`:808` ALREADY DRIFTED on `param_pos` increment: verify against a fixture FIRST); `extraction-live.sh reload_program` helper (5 copies); `staleness-gate.sh` drop bash-4 assoc array (macOS `/bin/bash` is 3.2); shared `ensure_binary` for 6 sites; `structPlane span(start,end)` test helper (8 casts); 7th hand-rolled ScratchStore harness -> tests helper | terra + luna |
| 1.8 | Explicitly OUT of this wave | `2_binds trackedPaths` unification beyond 1.3 (A12 crossing is sanctioned in the header, re-architecting is a design call); `golden.test.ts storesOpened` probe (live diagnostics for E9's flake hunt); prolog canonical-JSON triplication (disclosed, pinned, structural) | - |

### Exit receipts

```
cd v6 && just green                       # exit 0
cd v6 && just sweep                       # 163 swept / 102 compiled / 100 identical / 0 wrong
cd v6 && just conformance                 # 163 PASS
cd v6 && just plunit                      # 140/140 or higher
cd v6 && just text-door                   # 102/102/0
cd v6/sprefa-extract && cargo test        # all pass, fixture snaps regenerated and reviewed
cd v6 && just green-all                   # exit 0
```

Plus one fail-first receipt per P0, pasted into the test header:

| P0 | sabotage that must go red first |
|---|---|
| 1.1 | a rel literally named `__dict_x` keeps its delta arm (today the prefix sniff steals it) |
| 1.2 | a `probe_mismatch` throw prints with a human message, not a swipl `Unknown message` |
| 1.3 | watch row digest and enumerate row digest for the same bytes assert equal |

### Golden test

`v6/prolog/compile/test/plunit_tests.pl` gains `dictionary_plan_kind_not_name_sniff`,
and the refusal-message coverage test already in `0_refusal_messages.pl` is
extended from `unsupported_construct/1` to the full `refusal_term/1`
inventory (77 signatures today; the test asserts every inventoried term
renders).

### Done condition

Battery table in section 0.1 reproduced exactly, with plunit and conformance
allowed to GROW only. Any movement in the 100-identical bucket is a stop.

### User decisions gating E1

None. This epic is pure cleanup on already-ruled ground.

---

## E2. PHASE 5: TYPE PASS, CLOCK-CHECKER VALUE PROOF, INGEST PERF

**Goal.** Close the last golden-plan phase. Three independent legs; the
checker leg is DELIBERATELY GATED on a proof, not on a design.

### E2a. Column type plane: bool and float

Rulings in force: `bool_column_type = two_valued_column_type` (strict 2VL, no
null, storage spelling rides this arc) and `numeric_precision =
approved_phase5_design` (float/REAL + avg yes; precision spelling designed
in-arc). The earlier golden-plan recommendation (bool = row presence / two
variant enum, never a column type) is OVERRULED by the user and must not be
re-proposed.

**Contract.**

```prolog
% decl surface, per ruling decl_column_spelling = colon_typed_ordered_columns
rel finding(path: text, line: int, suppressed: bool, score: float).

% type plane obligations, each an entry in 0_type_plane.pl:
%   bool  : storage spelling (SLOT below), literals true/false, strict 2VL,
%           comparison and negation semantics, NO null, NO third value
%   float : REAL storage, literal syntax, avg() head form, precision spelling
%   both  : oracle engine.pl accepts identically, compiler infers identically,
%           tick log renders identically (canonical JSON contract)
```

| id | task | files / symbols | owner-shape |
|---|---|---|---|
| 2a.1 | bool storage spelling decision, worked three ways with receipts (INTEGER 0/1, TEXT 'true'/'false', or sqlite's own truthiness) then one chosen | `0_type_plane.pl`, `lower.pl canonical_column_expr/3` int/text split | opus lab (small, 1 file of probes) |
| 2a.2 | bool literals in the parser and printer, registry row, grammar regen | `parse_dl.pl`, `print_dl.pl`, `registry.pl`, `dl6.tmLanguage.json` via `emit_dl6_grammar/0` | sol |
| 2a.3 | bool in the oracle: `engine.pl` type check, `ticklog.pl` render | `conformance/engine.pl`, `conformance/ticklog.pl`, `0_type_plane.pl` | sol, SAME ARC as 2a.4 (A4 law) |
| 2a.4 | bool in the compiler: inference, comparison typing, negation, DDL | `analyze.pl`, `lower.pl`, `emit_ts.pl` | sol |
| 2a.5 | float: REAL storage, literal, `avg()` head form as a per-group accumulator beside count/sum | same set; `avg` joins the `expression_lift` aggregate family | sol |
| 2a.6 | precision spelling designed in-arc: what `float` means at the boundary (REAL round-trip, tick-log rendering of `0.1+0.2`, comparison tolerance or none) | design note inside the arc + fixture; the ruling authorized the design, not a specific spelling | opus, same lane |
| 2a.7 | fixtures: at least 6 (bool literal round trip, bool in comparison, bool in negation, float arithmetic, avg over a group, float tick-log render) | `conformance/fixtures/` | same lane |
| 2a.8 | `@libsql` REAL bind corruption regression guard extended to float columns (the `bootBind.test.ts` int->bigint lesson) | `v6/tsv2/tests/bootBind.test.ts` | same lane |

**Golden test.** `bool_and_float_columns_round_trip`: a program declaring
both, fed a schedule with both, tick log byte-identical oracle vs both
emitter modes, final-state leg included, plus an EXPLAIN receipt showing the
bool column participates in an indexed SEARCH rather than forcing a scan.

### E2b. Clock checker VALUE-PROOF GATE

The user asked, of the checker: "does it handle any form of bug class". That
question is the gate. `TICK-MODEL.md` section 5 already records five
cross-plane refusals as HAND-PROVEN theorems of the ring model, and section 6
specifies the checker. What does not exist is evidence that the checker
catches anything the five refusals do not already catch.

**Contract.**

```
GATE (must complete and be shown to the user BEFORE any checker code):
  produce a bug-class table with these columns:
    bug class | a real program exhibiting it | caught today by? | would the
    ring/grade checker catch it? | what the checker needs to catch it
  populated by REPLAYING the project's own defect history against a paper
  ring analysis, not by speculation.
  Minimum corpus: every silent-wrong class this repo has logged.
IF the table shows classes the five theorems miss -> build the checker.
IF it does not -> the checker is a formalization with no new catch, and the
  correct outcome is a documented no-build with the table as the receipt.
```

| id | task | source material | owner-shape |
|---|---|---|---|
| 2b.1 | assemble the defect corpus | A2 edge join cardinality = f(batching); A4 keyed arrival divergence; A5 typo/arity compiles clean + Name/Arity collision emits invalid TS; A6 mid-tick level rows fire edges while visible nowhere; A7 invalidation-as-log permanent poison; A8 keep(count) per-rel never per-key; A9 collapse logging implemented nowhere; A11 count never 0; the 5 shipped theorems; `pre`-as-sampled measured wrong (`fork pre_lowering_premise`); `edge_body_joins_arrival_fed_level`; the F8/retention-inert class; `keyed()` silently inert on level heads | opus lab, read-only |
| 2b.2 | paper ring analysis per class | for each: name the ring (B/N/Z), the junction, the grade; state whether a ring signature per registry row + a junction rule would have refused it | opus lab |
| 2b.3 | the table, presented to the user as the gate | one markdown table in this doc's successor or in `TICK-MODEL.md` section 7 | coordinator relays |
| 2b.4 | CONDITIONAL: registry gains ring signature + tick grade columns; per-body junction check; per-path grade sum; derivable tick-offset table | `registry.pl` surface/5 grows two columns; `analyze.pl` supported-subset gate; `engine.pl check_program/1` | sol, ONLY on a positive gate |
| 2b.5 | CONDITIONAL: the tick-offset table cross-checks the oracle's observed placement in every fixture (this is the leg that makes the checker earn its keep: a derived answer to "what tick is this row on" that is graded, not asserted) | new sweep leg | same lane |

**Golden test (conditional).** A fixture whose ring junction is unstated and
which today compiles and grades identical, but whose join cardinality depends
on arrival batching (the A2 shape). The checker must refuse it BY NAME. If no
such fixture can be written, the gate has answered negative.

### E2c. Ingest perf: commit_ms

**Contract.** `commit_ms` per file drops from ~10.8ms with a COUNT test, not
an end-state timing alone (the formerly-quadratic law).

| id | task | files | owner-shape |
|---|---|---|---|
| 2c.1 | decompose commit_ms with the existing `DL_PERF_LOG` sub-spans before touching anything | `v6/dl/src/0_trace.ts`, `4_ingest.ts` | luna |
| 2c.2 | `4_ingest.ts:93` adopts `extraction-live.sh`'s resolution order (`DL_EXTRACT_BIN` -> in-tree release -> build); the DEBUG default dies | `v6/dl/src/4_ingest.ts:93` | luna, trivial |
| 2c.3 | the actual commit-path fix, whatever 2c.1 names | `v6/dl/src/` store commit path | luna |
| 2c.4 | COUNT test: statements per committed file flat across a 5-file and a 20-file corpus, EXPLAIN SEARCH-not-SCAN on the commit statement | `v6/dl/tests/` | same lane |

**Golden test.** `commitStatementCount.test.ts`: statements-per-file exactly
equal at 5 and 20 files, with a sabotage receipt in the header showing the
count growing when the batching is removed.

### Exit receipts (whole E2)

```
cd v6 && just green-all                  # exit 0
cd v6 && just conformance                # 163 + 6 new bool/float fixtures = 169 PASS minimum
cd v6 && just sweep                      # compiled and identical grow by the new fixtures; 0 wrong
cd v6 && just plunit                     # grows
node --test v6/dl/tests/commitStatementCount.test.ts   # pass, count assertions
```

Plus, for 2b, the bug-class table itself is the receipt whether the answer is
build or no-build.

### User decisions gating E2

Cards in section E10: `SLOT-BOOL-STORAGE` (E2a.1 will present three priced
options), `SLOT-FLOAT-PRECISION` (E2a.6), and the clock-checker BUILD/NO-BUILD
call after 2b.3.

---

## E3. SPAN FLATTENER AND COMMENT-RAIL WIRING

**Goal.** Two shipped honesty gaps close on one small change, then the
comment-node lab's durable output lands.

### E3a. The byte-span flattener (do this first, it is the smallest correct
win in the whole document)

Today `diag-rail.dl6` reports line 0 for every diagnostic and
`flagship-callgraph.dl6` drops its `line` column, both because extractor
spans are half-open bytes and nothing converts them. The comment lab's `cn.py`
already contains a generic flattener that reached 745/745 parity against v5's
convention.

**Contract.**

```
input : (path, byte_start, byte_end) from any extractor family
output: (line, col, end_line, end_col) in v5's exact convention
        line 1-based, col 0-based, end = position after last byte
residency: the HOST TEMPLATE (SLOT-SPAN-UNITS answer: byte spans stay
        transport, line/col computed at the seam). NOT the extractor,
        NOT the language.
```

| id | task | files | owner-shape |
|---|---|---|---|
| 3a.1 | recover the flattener from the lab, promote it to a real tool | `git show 9b5ba958:v6/prolog/labs/comment_node/cn.py` -> a permanent home beside the extraction hosts | luna |
| 3a.2 | wire it into `diag-rail.dl6`'s host template; line numbers stop being zero | `v6/dl/fixtures/diag-rail.dl6` | same |
| 3a.3 | wire it into `flagship-callgraph.dl6`; the dropped `line` column returns | `v6/dl/fixtures/flagship-callgraph.dl6` | same |
| 3a.4 | calibration test pinning the convention against a known byte offset (the classifier header's receipt: byte 3981 of `src/bin/extract.rs` = line 94 col 8) | new test | same |

**Golden test.** `just lsp-diags` still HOLDS and the published diagnostic now
carries a NONZERO line that a sabotage (shifting col 0-based to 1-based) turns
red at the real v5 LSP client leg, which is the discriminating check the LSP
arc already proved.

### E3b. Comment fixtures promoted

| id | task | detail | owner-shape |
|---|---|---|---|
| 3b.1 | promote the 4 graded candidates into `conformance/fixtures/` | `comment_witness_gates_a_scanner_hit`, `disable_next_line_shifts_the_effect_by_one`, `unused_suppression_antijoins_the_finding`, `arch_hierarchy_from_decomposed_marker_rows`; each already has its sabotage recorded in the verdict | luna |
| 3b.2 | each must ALSO compile or refuse by name in the sweep, not just pass the oracle | `just sweep` movement stated exactly | same |

### E3c. Techniques 3, 4, 6 ported

All three are WORK, not language gaps, per the verdict.

| id | technique | what it needs | owner-shape |
|---|---|---|---|
| 3c.1 | technique 2's missing half: BLOCK pairing (`dl-disable`..`dl-enable`, nearest-enable argmax via `span_beaten`) | ordinary datalog: `<=`, `<`, `not`. This is the prerequisite shape for 3c.3 | luna |
| 3c.2 | technique 4 LANG-JUNCTION(slug) registry | `match_line` with named captures ported as one host per side + the grammar-witness join; drift rails as antijoins | luna |
| 3c.3 | technique 6 BEGIN: gen zone ownership | reuses 3c.1's start/end range pairing | luna |
| 3c.4 | technique 3 README(anchor) doc prose, per-file case | one host + a join; the cross-file assembled-name case stays UNBUILT and named as the proving program for any future text construct | luna |
| 3c.5 | ARCH markers actually authored in `v6/prolog/**` (today: zero), so `arch-rail.dl6` has input | mechanical | luna |

### E3d. Technique 5 (markdown) is deferred to E6

The one real extractor hole. `SLOT-EXTRACTOR-WAIVER` answer stands: no waiver
needed for six of seven techniques; markdown is a scoped USER CALL, and E6
subsumes it by adding a markdown grammar for its own reasons.

### Exit receipts

```
cd v6 && just lsp-diags        # HOLDS, and the diagnostic line is nonzero
cd v6 && just flagship         # exit 0, same table, line column present
cd v6 && just conformance      # +4 fixtures
cd v6 && just sweep            # movement stated exactly; 0 wrong
```

### User decisions gating E3

`SLOT-EXTRACTOR-WAIVER` scoped to markdown only (rolls into E6's card). Nothing
else; the other four comment slots were answered by the lab with receipts.

---

## E4. FLOW-PARITY RESIDUE

**Goal.** The flagship's second program stops having unexplained rows. The
alpha's headline receipt is `callgraph` (done, 0 unclassified); this makes
`flow-interproc` match it.

**Contract.**

```
flagship-flow.sh exits 0 with:
  flow_edge        : every one of the 278 v5-only rows BUCKETED
                     (extraction-input / expression-gap / defect), 0 unclassified
  flow_reach       : the 177 v6-only reflexive rows explained by a stated
                     rule-fidelity difference or eliminated
  flow_param_type  : match > 0 (today 0, and the cause is a REFEREE key gap,
                     not an engine gap)
  flow_node_type   : rows > 0 (today EMPTY, and this one IS a rail gap)
```

| id | task | detail | owner-shape |
|---|---|---|---|
| 4.1 | flow_param_type referee key: v5 emits `root::`-prefixed syms and QUALIFIED type names; v6 emits bare. The translation belongs in `flagship-flow-classify.py`, same as the coordinate translation already there (neither engine bends) | `v6/tsv2/scripts/flagship-flow-classify.py` `v5_sym_key` and the `flow_param_type` arm | luna |
| 4.2 | flow_node_type EMPTY: the rail joins df param nodes + `df_param` pos + `sig`. Determine which of the three inputs is absent at run time, then either fix the rail or name the extractor gap | `v6/dl/fixtures/flagship-flow.dl6`; check `df_param` rows actually arrive (the scout measured 6,131 in v5) | terra (if the gap is extractor) or luna (if the gap is the rail) |
| 4.3 | the 278 v5-only flow_edges: classify. Prior scout receipt says v5's total was 147,664 edges on a 200-file corpus vs this rig's 13-file pin, so the 278 are shape-specific, not scale | classifier gains a bucket + spot-checked pairs per bucket | luna |
| 4.4 | the 177 v6-only reflexive `flow_reach` rows: v5's closure may exclude self-pairs. State which, assert it | classifier | same |
| 4.5 | glob semantics divergence already pinned by assertion in the callgraph rig (v5 globset vs git pathspec); confirm the same assertion exists here | `flagship-flow.sh` | same |
| 4.6 | promote at least 2 more oracle-graded fixtures from the shapes this work exercises | `conformance/fixtures/` | same |
| 4.7 | `flagship-flow` gets a justfile recipe and joins `green-all` (today only `flagship` = callgraph is in it) | `v6/justfile` | coordinator-inline |

### Exit receipts

```
cd v6 && just flagship-flow    # NEW recipe; exit 0, four-query table, 0 unclassified
cd v6 && just green-all        # exit 0 with the new recipe inside
```

### Golden test

The rig itself is the golden test. Its sabotage receipt: flipping the referee's
coordinate convention (0-based col to 1-based) must flip the match column of
`flow_edge` to zero and be named by the classifier as a
position-convention bucket, exactly as the comment lab's parity rig did.

### User decisions gating E4

`closure()` spelling stays option (a), direct two-rule recursion, per the
scout. No card needed unless 4.2 lands on an extractor gap, which becomes a
scoped extractor-waiver card.

---

## E5. AMPLIFICATION SENSORS, THEN DIET

**Goal.** Answer the user's question ("do we have sensors on that yet in a
common bench at all?") with a number, then decide the diet on the number.
Today the answer is NO: no amplification column exists in `CRAWL-BENCH.md`,
`SCALE.md`, or `PERF-REPORT.md`.

**The measured motivation** (coordinator receipt, 2026-07-29): a 3.4MB comment
db was roughly two thirds duplicated join-key text; 64-char digests are stored
per-row inside WITHOUT ROWID primary keys; the host response cache doubles
every host row. Struct dictionaries are the existing opt-in interning and are
NOT applied to these paths.

**Contract.**

```
SENSOR (do this first, it is one column and a divide):
  amplification = db_bytes / corpus_bytes
  boundary_ratio = boundary rows emitted / input rows ingested
  both land in the SHARED bench CSV beside host_peak_mb, sqlite_hw_mb, db_mb
  (v6/sprefa-store/PERF-REPORT.md already documents that CSV extension)
  measured on THREE corpora so the number has a shape, not a point.

DIET (conditional on the sensor):
  only if amplification exceeds a threshold the user sets after seeing the
  three numbers. v5's standing defect is a 39x db/corpus ratio; that is the
  comparison line, not a target.
```

| id | task | detail | owner-shape |
|---|---|---|---|
| 5.1 | add the two columns to the shared CSV and to `bench/run.sh`'s field mapping (legacy runners map to `N/A`, the precedent already in PERF-REPORT) | `v6/sprefa-store/bench/run.sh`, `PERF-REPORT.md` column doc | luna |
| 5.2 | emit them from `crawl-bench.sh` (corpus bytes are already walked) | `v6/tsv2/scripts/crawl-bench.sh`, `CRAWL-BENCH.md` | same |
| 5.3 | emit them from `memory-soak.sh` (page_count x page_size is already read via `/stats`) | `v6/tsv2/scripts/memory-soak.sh` | same |
| 5.4 | measure on three corpora: the 13-file flagship pin, the 58-file prolog tree, one grafana repo. Report the three ratios | new table in `CRAWL-BENCH.md` | same |
| 5.5 | itemize where the bytes go, using the ONE grouped `dbstat` statement that `/stats` already runs (`ServeStats`) | analysis only | same |
| 5.6 | CONDITIONAL diet items, each priced, none started without the user's threshold: (a) digest columns become dictionary refs (struct-as-rows machinery already exists); (b) join-key text interned rather than repeated; (c) host response cache stops doubling every row; (d) v5-side: WITHOUT ROWID junctions, dense dictionary ids | - | opus lab if taken |

### Exit receipts

```
cd v6 && just crawl-bench      # CSV now carries amplification and boundary_ratio
cd v6 && just memory-soak      # MEMORY SOAK HOLDS, and prints the ratio
```

The three-corpus table IS the deliverable of the sensor half.

### Golden test

`memory-soak` gains an amplification ceiling assertion with a sabotage
receipt: removing the digest-dedup (or, before any diet, simply doubling a
stored text column) pushes the ratio past the ceiling and the soak goes red,
the same shape as the existing `keep_all` page-count sabotage that already
works (page count 17 -> 33 vs ceiling 19).

### User decisions gating E5

`SLOT-AMPLIFICATION-THRESHOLD`: after the three numbers land, what ratio is
acceptable. Diet does not start before that card is answered.

---

## E6. DOC-FORMAT EXTRACTION

**Goal.** Execute `plans/2026-07-29-extract-doc-formats-header.md`: html, xml,
md, json, yaml, toml enter `sprefa-extract`.

**Contract.** Unchanged from the header, restated with owner-shapes.

```
ALL SIX  -> cst family (nodes + spans), which unlocks ts_query and sg_pattern
            hosts over these files for free
json/yaml/toml -> doc family, one row per leaf:
            (key_path, value_text, value_kind, span)
html/xml       -> doc family: (element_path, attr_name, attr_value, text, span)
md             -> cst + comment rows (closes the markdown comment hole) +
            doc rows for headings/sections/fences/links
ONE record shape per family across formats: a yaml doc row and a toml doc row
            differ only in path spelling, never in columns.
```

| id | task | detail | owner-shape |
|---|---|---|---|
| 6.1 | STEP 0, build-vs-buy research lane. Written candidate table BEFORE any code: which of `tree-sitter-html`, `tree-sitter-xml`, `tree-sitter-markdown` (block/inline SPLIT grammars), `-json`, `-yaml`, `-toml` ship inside the existing ast-grep dependency's grammar registry vs need a direct dep. Version pins, maintenance state, parse-failure behavior on dirty real-world html | new plan doc | luna, research only |
| 6.2 | cst family for all six | `v6/sprefa-extract/src/lang/` | terra |
| 6.3 | doc family for json/yaml/toml; yaml anchors and aliases resolved at emit, cycles = named refusal; toml tables flattened into the same key_path spelling | same | terra |
| 6.4 | doc family for html/xml; entities and DTD out of scope with a named refusal | same | terra |
| 6.5 | md: cst + comment rows + heading/section/fence/link doc rows | same | terra |
| 6.6 | per format: fixture corpus files, snapshot tests, AND a CLI-level golden test (the bin-vs-lib parity lesson from the `--resolve` arc: nothing asserted bin-vs-lib capability parity, and that asymmetry is the lesson) | `v6/sprefa-extract/tests/` | same lane |
| 6.7 | one dogfood program per format over our own tree (`v6/justfile` as toml? `package.json`? `INDEX.md` headings?) graded like the comment receipts | `v6/dl/fixtures/` | luna |
| 6.8 | technique 5 from E3d lands here: markdown comments, closing the last comment-parity hole | - | rides 6.5 |

### Exit receipts

```
cd v6/sprefa-extract && cargo test              # all pass incl. 6 new CLI golden tests
cd v6 && just extraction-live                   # EXTRACTION LIVE HOLDS
cd v6 && just staleness-gate                    # OK (the extract binary is in its scope)
```

### Golden test

Per format, one CLI-level golden: a fixture document in, exact JSONL out,
snapshot-pinned. The one that matters most is html, because it is the only
format where real-world input is routinely malformed, and the snapshot must
pin the ERROR-node policy chosen in `SLOT-HTML-DIRT`.

### User decisions gating E6

| slot | question |
|---|---|
| `SLOT-KEYPATH-SPELLING` | json-pointer `/a/0/b` vs dotted `a.0.b` vs jq `$.a[0].b`, ONE spelling for json/yaml/toml/md-heading paths |
| `SLOT-MD-GRAMMAR` | block-only vs block+inline tree-sitter-markdown (inline doubles cost; most rails need block only) |
| `SLOT-HTML-DIRT` | on parse errors, do ERROR nodes emit rows or a named finding |
| `SLOT-DOC-VALUE-TYPES` | value_kind vocabulary. Note the doc family REPORTS the SOURCE document's kind as text; it does not import the source's type system into the language's own. E2's bool/float rulings are untouched by this |
| `SLOT-EXTRACTOR-WAIVER` (markdown scope) | the comment lab's one remaining waiver question, now subsumed |

---

## E7. CROSS-COMPAT IMPORT: TYPESPEC, JSON-SCHEMA, OPENAPI v3

**Goal.** A schema or API description document (json or yaml) becomes
declarations in the language: rels, types, enums. This is the "rel decls are
types" claim made executable, and the user's recorded position is that it is
BIDIRECTIONAL (decls export TO json-schema as well as import FROM it, one
mapping table both directions).

**Prior art the user already built:** `~/projects/hafley-tsp` (TypeSpec
app-gen). ARCH.pl's `prior_art(hafley_tsp, ..., bind_vocabulary, 'TypeSpec
app-gen; config/env/CLI sources + @secret redaction for binds')` names it as
the bind-vocabulary feed. Read it before designing anything: the mapping it
already performs is the starting table, not a blank page.

**Contract.**

```
input : an OpenAPI v3 / JSON-Schema / TypeSpec document (json or yaml on disk)
        parsed through E6's doc family (this epic adds NO parser)
output: .dl6 declaration text -- `type` decls for objects, `rel` decls for
        collections, enum variant decls for enums and for optionality
mapping: ONE table, consulted in both directions
inverse: given the emitted decls, re-emit a json-schema document; the
        round trip is the grading leg
```

The mapping is constrained by rulings already in force, which is what makes
this tractable rather than open-ended:

| schema concept | language concept | ruling that forces it |
|---|---|---|
| object with named fields | `type name(field: t, ...)` | `compound_storage = struct_as_rows`, `decl_column_spelling` |
| array of objects | a rel over the item type | `rel_default_policy = value_unkeyed` |
| enum / oneOf / anyOf | semicolon variant decls | `enum_decl_in_rel`, `enum_variant_separator` |
| optional field / nullable | a variant, or field absence | null NEVER exists (golden plan, SYNTAX term table); Option = variants or absence |
| boolean | `bool` column | `bool_column_type = two_valued_column_type` (E2a) |
| number / integer | `float` / `int` | `numeric_precision = approved_phase5_design` (E2a) |
| required vs optional | arity is TOTAL in heads; body kwargs may omit | kwargs partial application, landed hosts-wiring p1 |
| `$ref` | a ref column to the referenced type | struct-as-rows ref columns |
| recursive `$ref` | REFUSED on the value plane | types lab: content ids cannot express cycles; `type_cycle` refusal already exists (2 fixtures) |

| id | task | detail | owner-shape |
|---|---|---|---|
| 7.1 | STEP 0, build-vs-buy research. Candidate table for parsing/validating each of the three input languages, per candidate, with a rejection reason where rejected. Include at minimum: the TypeSpec compiler itself (`@typespec/compiler`), `@apidevtools/swagger-parser`, `@redocly/openapi-core`, `ajv` for JSON-Schema draft resolution, `openapi-types`, and the option of NOT parsing at all (E6's doc family already yields key_path/value rows, so a `$ref`-resolving pass over rels may be the smallest correct answer). Read `~/projects/hafley-tsp` first and report what it already does | new plan doc | luna, research only, NO code |
| 7.2 | the mapping table, written and reviewed BEFORE code, one row per schema construct with both directions stated and the refusal named where a direction is impossible | new plan doc | opus lab |
| 7.3 | import: a program (in the language, per `spine_residency`) that reads doc-family rows and emits decl text. This is a dl6 program plus a host, NOT a compiler feature, unless the mapping table proves otherwise | `v6/dl/fixtures/schema-import.dl6` | sol |
| 7.4 | export: decls back out to json-schema. `print_dl.pl` already renders decls; this adds a second renderer over the same term form | `v6/prolog/compile/` | sol |
| 7.5 | `$ref` resolution and the cycle refusal wired to the EXISTING `type_cycle` refusal, not a new one | shared with `0_type_plane.pl` | same |
| 7.6 | fixtures: at least one real-world document per input language, round-tripped | `conformance/fixtures/` + a corpus dir | same |

### Exit receipts

```
cd v6 && just conformance                       # + round-trip fixtures
node <schema round-trip receipt script>         # a REAL OpenAPI v3 document in,
                                                # decls out, schema back out,
                                                # semantically equal (not byte-equal:
                                                # state the equality relation used)
```

### Golden test

The widest deterministic slice:

```
petstore.yaml (or an equally real document)
  -> E6 doc-family rows
  -> mapping table
  -> .dl6 decl text
  -> parse_dl.pl term form
  -> emitted TS + oracle both accept the program
  -> print_dl.pl decl text (round trip 1, byte-identical)
  -> json-schema export (round trip 2, semantically equal)
  -> diagnostics snapshot for every construct the mapping REFUSES
```

Round trip 1 must be byte-identical (it is the existing roundtrip.sh
contract). Round trip 2's equality relation is a decision, see the card.

### User decisions gating E7

| slot | question |
|---|---|
| `SLOT-SCHEMA-ROUNDTRIP-EQUALITY` | is export graded byte-identical, structurally equal, or validator-equivalent (a document that validates the same instance set) |
| `SLOT-SCHEMA-IMPORT-RESIDENCY` | does import run as a language program over doc rows (per `spine_residency`) or as a compiler pass. The ruling says hosted; 7.2 must prove or disprove that it is expressible |
| `SLOT-OPTIONALITY-SPELLING` | JSON-Schema `required` absent + `nullable: true` are TWO different absences; which maps to a variant and which to row absence |
| `SLOT-TYPESPEC-SCOPE` | TypeSpec is a full language with decorators; which subset enters. The `hafley-tsp` prior art bounds this |

---

## E8. ANALYSIS-ORACLE EXAM

**Goal.** Replace v5 as the standing oracle. User directive: "we need like
glean or joern and test ourselves on how to achieve analysis things commonly".
Today every parity claim is graded against v5's own output, which means the
ceiling is v5.

**Contract.**

```
PHASE A (research, no code):
  a written capability table over Glean, Joern, CodeQL, Kythe, and
  stack-graphs. Per system, per analysis question: does it answer it, in
  what query language, at what indexing cost. Reject none in one line.
PHASE B (the exam):
  ~15 analysis tasks, each a QUESTION a working engineer asks, each with:
    - a pinned corpus
    - an expected answer produced by at least one external oracle from
      phase A (not by us)
    - a v6 program answering it
    - a graded diff, bucketed like the flagship rigs
  The exam becomes a justfile recipe and joins green-all.
```

Candidate exam questions, to be finalized in phase A (this list is a seed,
not the answer):

| # | question | v6 capability today |
|---|---|---|
| 1 | who calls X, transitively | landed (callgraph flagship, recursive strata) |
| 2 | what breaks if I change this signature | needs the flow port (E4) |
| 3 | which values reach this sink | flow_reach, E4 |
| 4 | dead code: definitions never referenced | landed (`unused` in the callgraph rail) |
| 5 | cross-language references (kotlin interface -> ts client -> rust impl) | extractor has 5 languages; the join is a program |
| 6 | which config keys are read but never set | needs E6 (doc family) |
| 7 | which API endpoints have no test | needs E6 + E7 |
| 8 | import cycles | recursive strata, expressible today |
| 9 | which files changed in this commit affect which tests | needs `changed(path)` (golden plan phase 4 residue, unbuilt) |
| 10 | taint from an untrusted source to a dangerous sink | flow + E4 |
| 11 | every place a given type is constructed | type family, expressible |
| 12 | ownership: which module owns this symbol | ARCH markers + E3c |
| 13 | duplicated logic across files | NOT expressible; a real gap to name, not paper over |
| 14 | which comments claim something the code no longer does | E3 comment rails + a drift antijoin |
| 15 | blast radius of deleting a file | closure over the import graph |

| id | task | owner-shape |
|---|---|---|
| 8.1 | phase A research table, five systems, no one-line dismissals | luna, research only |
| 8.2 | pick the external oracle(s) to install; state the install cost honestly (CodeQL and Glean are both heavy) | coordinator relays as a card |
| 8.3 | pin an exam corpus that all oracles and v6 can index | luna |
| 8.4 | per question: external answer captured, v6 program written, diff classified | opus lab (this is judgment-heavy; a wrong classifier passed sabotage once already in the callgraph arc) |
| 8.5 | `just exam` recipe, joins green-all, exits nonzero on unclassified rows | coordinator-inline |

### Exit receipts

```
cd v6 && just exam       # exit 0, 15-row table, 0 unclassified,
                         # each row naming its external oracle
```

### Golden test

The exam IS the golden test, and it grades US. Its own sabotage receipt: break
one v6 rail deliberately and confirm the exam turns that row red rather than
reclassifying it into a gap bucket. The callgraph arc already learned this
lesson the hard way (a wrong classifier draft that sabotage caught).

### User decisions gating E8

`SLOT-EXAM-ORACLE`: which external system(s) to install, given install cost.
`SLOT-EXAM-SIZE`: 15 is a guess; the phase-A table may argue for 8 or 25.

---

## E9. STANDING CRACKS WITH NO OWNER

Every row here exists in ARCH.pl or in a sweep artifact today. None is
blocking; all of them together are the difference between "green" and
"trustworthy".

| id | crack | evidence | shape | owner-shape |
|---|---|---|---|---|
| 9.1 | `golden_flake_hunt` | 1/18 sub-runs under 3x concurrent load; all 11 subtests print pass yet the FILE fails with bare `test failed`, zero error payload (native/process signature). Ruled out: tmp paths, ports, memcap singleton, RSS budget. Leading unproven candidate: ~40 unclosed `:memory:` @libsql clients per run. Diagnostics already landed | close the clients, re-run at 3x N times, or prove the candidate wrong | luna |
| 9.2 | `reactor_buffertime_flake` | 6/18 under 3x load, `tests/labs/reactor.test.ts` "reactor A file+folder coalesce", actual `[1]` vs expected `[1,2,3]`. Wall-clock `bufferTime` assertion, the SAME class F3 killed in v6/dl with TestScheduler | rewrite on TestScheduler asserting `scheduler.actions.length`, exactly as F3 did; sabotage receipt in the header | luna, small |
| 9.3 | `v5_lsp_exit_hang` | `dl --lsp --diag-db` answers shutdown then hangs on exit + stdin EOF. `lsp-diags.sh` is in green-all, so EVERY battery run can leak one; 3 hung processes ~4h old were found and killed manually | TWO fixes: (a) interim, `lsp-diags.sh` pkills its own spawned pid on exit; (b) real, `src/lsp.rs` v5 side | (a) coordinator-inline, (b) luna on the v5 side |
| 9.4 | `log_retraction_rejected` run_error | sweep run-results, pre-existing | either give it a comparable oracle log or reclassify it as a named refusal fixture; a permanent run_error is a hole in the grading contract | sol |
| 9.5 | `fork_join_error_arm_is_a_value` run_error | same; the verdict notes it produces malformed JSON | same treatment | sol |
| 9.6 | swipl 10.0.2 GC compaction abort | worked around by `set_prolog_flag(gc,false)` for the one-shot batch in `v6/tsv2/scripts/sweep.sh:26-31` | file it upstream or pin the swipl version with a receipt; a disabled GC in the grading path is a standing risk, not a fix | coordinator-inline (file), luna (pin) |
| 9.7 | `c7_durable_carry` | frontier and departure TEMP tables die with the connection; `kill -9` loses staged carries in BOTH implementations including the oracle-side Ti carry. Endurance-law violation, measured non-vacuously by `departureFrontier.test.ts` | own arc: durable carry set. This is the largest crack in the list | opus lab |
| 9.8 | `pre_occurrence_loop` | 13 fixtures. `pre` in an edge body needs an ORDERED OCCURRENCE LOOP with writes applied between occurrences (`engine.pl process_occurrences` chaining); pre-as-sampled projects `[1,1]` where the oracle pins `2` (receipt in SCOREBOARD.md). A new execution shape in the emitted runtime | own arc | opus lab |
| 9.9 | `struct_dictionary_gc` | SLOT-GC-TIMING: dictionaries are MONOTONE and unobservable in the tick log (collected and uncollected print identical bytes). Zero cost for churning programs; real growth only for ever-new distinct values | rides E5's amplification number: if the sensor says dictionaries grow, this becomes urgent; if not, it stays a debt row | gated on E5 |
| 9.10 | `org_banked_findings` | 4 test-pinned drift sites: `trigger_items`/`body_atoms` misclassify `next`/`combine`/comparisons/lifecycle wrappers as relation atoms; `goal_rel_refs` reports `next/1` + `combine/2` as positive refs; `finalize_in_level_rule` diagnostic drift plus both doors accepting `not(finalize(...))`; 3 private cross-module calls in `sprefa-store/bench/v1-scale-gen.pl` outside the lint gate's load set | one small lane; all four are pinned by tests so drift is loud but not fixed | luna |
| 9.11 | `gen_staleness_gate` residue | the gate landed and has already caught 3 real staleness incidents (door-handwritten regen x3, a stale extract binary, a Jul-20 `target/release/dl`). Residue: it fails on binaries only when source is NEWER, so a binary built from a DIFFERENT branch passes | consider hashing source into the binary check | luna, small |
| 9.12 | `probe_output_guard` locations | closed by the cq bundle, but refusal locations still say "rule-index unavailable" because `parse_dl` keeps no source positions | source positions in the DCG. This is review-B4's remaining half and the single biggest cold-author complaint | sol, own lane |
| 9.13 | store `golden.test` flake under parallel load | pre-existing, 74/74 clean isolated. Overlaps 9.1 | fold into 9.1 | - |
| 9.14 | one node segfault seen in a sweep run | clean on rerun; experimental transform-types under load suspected | record only; do not chase without a second occurrence | - |
| 9.15 | `emitter_groupby_literal` workaround removal | the `(N+0)` wrap landed in the cq bundle and the `0+0` workaround was removed from the rail | verify no other rail carries the workaround | coordinator-inline, grep |

### Exit receipts

```
cd v6 && just green-all              # exit 0, 3x concurrently, 20 consecutive runs, 0 flakes
cd v6 && just sweep                  # 0 run_error (was 2)
ps aux | grep 'dl --lsp'             # empty after a full green-all
```

The 20-consecutive-runs-at-3x receipt is the only honest way to close 9.1 and
9.2, given 1/18 and 6/18 base rates.

### User decisions gating E9

None for 9.1-9.6 and 9.10-9.15. 9.7 and 9.8 are each their own arc and want a
go-ahead before an opus lab is spent on them.

---

## E10. DECISION TAIL: THE OPEN RULINGS

Presented as cards. This document does not decide any of them. Each card
states the question, the options with their consequence, and what is blocked.

### Index: 12 cards, 26 slots

| card | subject | blocks |
|---|---|---|
| D1 | `SLOT-BIND-SPELLING` (`:=` is nobody's word) plus `=` dying as `unbound_head_var` | nothing; the debt compounds per fixture |
| D2 | the five vocabulary squatters: `pre`, `keep`, `combine`, `finalize`, `now` | nothing; rename cost grows |
| D3 | `SLOT-TERM-STRUCT` (prolog compound is not a struct spelling) | 9 of 61 unsupported fixtures |
| D4 | `keep(count)` per-rel vs per-key | the channel-with-N-readers pattern |
| D5 | the 12 lab slots (consumption+arms 7, update-arm 5, spreading 6) | the arms arc and the spreading wiring, both out of alpha |
| D6 | A5 declared-only strict mode | nothing; second-most-cited cold-author complaint |
| D7 | diagnostics source locations | cold-author experience; E9.12 is the arc |
| D8 | `SLOT-ARM-SIBLING-WILDCARD` head-loud body-silent asymmetry | nothing |
| D9 | Q8 residual (left-of-arrow is the demand key) | nothing, long-open |
| D10 | extraction ambiguities A4 (fence escape), A14 (comment_span bind) | nothing |
| D11 | DECL LEGIBILITY, three faces: `SLOT-TYPE-DECL-DISTINGUISHABILITY`, entity-extras spelling, plane visibility | nothing today, and that is why it keeps deferring |
| D12 | the 14 new slots this document raises, listed at the end of E10 | each named epic |

### D1. `SLOT-BIND-SPELLING`

`:=` is the bind-goal spelling (`registry.pl:72`, 16 fixture files) and is not
an rxjs, prolog, or SQL word, which the vocabulary law forbids (review-B8).
Prolog candidates: `is` (arithmetic evaluation) or `=` (unification). Separately:
`Var = expr` today is UNREGISTERED and dies as `unbound_head_var`, the wrong
name with no mention of `=`. Whatever the ruling, `=` must refuse or bind BY
NAME. Rename cost: registry + parse/print + engine op + 16 fixtures, mechanical.
Blocked: nothing; but every day it waits, more fixtures spell `:=`.
An assign-composition lab is running in parallel and may reframe this card.

### D2. The five vocabulary squatters (review-B8)

| word | violation |
|---|---|
| `pre` | not an rxjs, prolog, or SQL word |
| `keep` | same |
| `combine` | IS an rx word with NON-rx semantics: it cross-joins where `combineLatest` samples. Receipt in the design review |
| `finalize` | in rx this is stream teardown; here it is per-row retraction |
| `now` | binds the TICK, not wall time, and will collide with clock binds |

Options: rename each (mechanical, luna-shaped, N fixtures each), amend the
vocabulary law to permit these five with stated semantics, or accept the debt
explicitly. Blocked: nothing. Cost of waiting: every new fixture and doc
compounds the rename.

### D3. `SLOT-TERM-STRUCT`

A prolog COMPOUND term is currently NOT a struct spelling. 9 `temporal_pipe`
fixtures destructure compounds (`fresh(tag_w1, body1)`) and sit in the
`edge_body_needs_json_destructure` bucket. A compound renders as canonical
prolog text where a struct renders as canonical JSON, so accepting the functor
form would silently change graded bytes of a value that already has a meaning.
Coordinator analysis: a DECLARED-compound arrival could canonicalize
POSITIONALLY via the decl, the same induction that `struct_arrival_key_order`
already ruled for named fields, and those 9 fixtures would migrate by adding
decls. Blocked: 9 of the 61 unsupported fixtures.

### D4. `keep(count)` per-rel vs per-key (review-B6, A8)

`keep(count)` is per-rel, never per-key. A channel with N readers wants
per-key. Options: per-rel only (status quo, documented), per-key as an
additional spelling, or per-key as the semantics with per-rel becoming the
special case. Blocked: the channel-with-N-readers pattern.

### D5. The 12 lab slots still open

| lab | slots |
|---|---|
| consumption + arms | `SLOT-QUEUE-PACING`, `SLOT-ARM-ARGUMENT`, `SLOT-ERROR-VARIANT-NAME`, `SLOT-ERROR-TERMINALITY`, `SLOT-RETENTION-SPELLING`, `SLOT-COLLAPSE-CHANNEL`, `SLOT-BOOT-OCCURRENCE` |
| update arm | `UPDATE-ARM-LEVEL-SPELLING`, `DELETE-ARM-DISCRIMINATION`, `LOG-FINALIZE-REFUSAL`, `ARM-SIBLING-WILDCARD`, `UPDATE-ARM-COMPILED` |
| spreading | 6 slots incl. `slot_spread_marker_position`, `slot_spread_and_kwargs_overlap` (NOT WIRED, design record only) |

Each has a priced verdict already. Blocked: the arms implementation arc and
the spreading wiring arc, both of which are OUT of alpha and correctly parked.

### D6. A5 declared-only strict mode

Today: typos and arity mismatches compile clean because an undeclared rel is a
legal EDB by the `edb_definition` ruling (un-headed = pure Subject). A
Name/Arity collision emits INVALID TypeScript (duplicate `CREATE TABLE` +
TS1117). Options: (a) a strict mode where every rel must be declared, opt-in
per program; (b) collision detection only (fixes the invalid-TS half without
touching the ruling); (c) status quo with the cost documented. Blocked:
nothing, but this is the second-most-cited cold-author complaint after
diagnostics.

### D7. Diagnostics: source locations

`parse_dl.pl` keeps no source positions, so every refusal says "rule-index
unavailable". The `prolog:message//1` umbrella landed (77 signatures,
coverage test), so the messages are HUMAN now; they are just unlocated.
Blocked: cold-author experience, and E9.12 is the arc.

### D8. `SLOT-ARM-SIBLING-WILDCARD` asymmetry

Arm scope is trigger bindings plus own body, never siblings. Unbound in HEAD
is loud (`unbound_in_expression`); unbound in BODY is a SILENT fresh wildcard.
Options: make body-side loud too, or document the asymmetry.

### D9. Q8 residual

Confirm left-of-arrow is the demand key on effect rels and that `Key()` never
appears there (the shipped TS reading, and the extraction lab's preference).
Low stakes, long-open.

### D10. Extraction ambiguities A4, A14

A4 fence escape, A14 comment_span bind
(`plans/2026-07-27-extraction-spellings.md`). A12 and A1 are RESOLVED.

### D11. DECL LEGIBILITY (one card, three faces)

The user hit this today as "types and rels are indistinguishable to a human".
It is one coherent card because all three faces are the same question asked at
three depths, and because face (b) sits in direct tension with the user's own
anti-magic ruling. Present the tension, do not resolve it here.

**Face (a): `SLOT-TYPE-DECL-DISTINGUISHABILITY`** (filed in the tail of
`plans/2026-07-29-simplify-wave-brief.md`).

```
type span(start: int, end: int).
rel  file(path: text, digest: text).
```

Visually identical, opposite semantics. `type` declares a value shape whose
rows live in a storage-plane dictionary, are content-addressed, are
boundary-INVISIBLE, and are referenced by id. `rel` declares a table that is a
replay subject, appears in every boundary delta, and is the thing programs
join. A reader distinguishes them only by the leading keyword, which sits at
the same indentation as everything else on the line.

Candidates priced in that brief: braces for value shapes; uppercase type names
plus a lint; editor semantic tokens (the grammar is already generated from
`registry.pl` via `emit_dl6_grammar/0`, so a token class is cheap).

**Face (b): entity-extras spelling debt.** `rulings.pl:288` reads, verbatim:

> `no_policy_suffix_words, bare_rel_is_set_log_is_the_only_kind_word` /
> "i dont want magic suffix words to trip anything up. no set. ... log all and
> a rel without any form of specificity are just tables no?"

That same ruling's own comment body then says: "Entity-plane EXTRAS (immutable
history, explicit checked retirement) still need a future spelling; whatever it
is, it will not be a bare suffix word on the decl." So the ruling removed the
suffix vocabulary at the user's direction AND left a hole that some
non-suffix spelling must eventually fill. The tension is real and is the
user's own: more legibility wants more marks on the decl, and the anti-magic
ruling wants fewer.

**Face (c): should the value-vs-entity plane be VISIBLE at all.** Today the
plane is carried invisibly by the `key(...)` choice plus id binds. The
types round-2 verdict proved that is SUFFICIENT ("the value policy word is
optional sugar ... fully expressible with key(...) plus the id bind",
verdict line 260). Sufficient is not the same as legible. If face (a) gets a
visible mark, face (c) is the natural place to put the plane on it, and face
(b)'s hole gets filled by the same mechanism.

| option | what it does to all three faces | cost |
|---|---|---|
| leave as is | (a) unaddressed, (b) still open, (c) invisible | zero code, standing complaint |
| editor-only (semantic tokens from registry) | (a) solved in an editor, unsolved in `cat` and in review diffs; (b) and (c) untouched | small, grammar is already generated |
| a lint (uppercase type names, or any naming law) | (a) solved everywhere including diffs; (b) and (c) untouched | small, one rail, a rename of existing types |
| a visible non-suffix mark on the decl (braces, a sigil, a position) | all three at once, and (b)'s hole gets its mechanism | parser + printer + registry + grammar + every fixture; and it must not read as a magic suffix word, which is exactly the constraint the ruling set |

Blocked by this card: nothing today, which is precisely why it will keep being
deferred until a cold author reads a decl wrong in a way that costs an hour.
The comment lab and the flow rig both produced programs whose decls a second
reader had to check twice.

### D12. New cards raised by this document

`SLOT-BOOL-STORAGE` (E2a.1), `SLOT-FLOAT-PRECISION` (E2a.6), clock-checker
BUILD/NO-BUILD (E2b.3), `SLOT-AMPLIFICATION-THRESHOLD` (E5),
`SLOT-KEYPATH-SPELLING` / `SLOT-MD-GRAMMAR` / `SLOT-HTML-DIRT` /
`SLOT-DOC-VALUE-TYPES` (E6), `SLOT-SCHEMA-ROUNDTRIP-EQUALITY` /
`SLOT-SCHEMA-IMPORT-RESIDENCY` / `SLOT-OPTIONALITY-SPELLING` /
`SLOT-TYPESPEC-SCOPE` (E7), `SLOT-EXAM-ORACLE` / `SLOT-EXAM-SIZE` (E8).

---

## E11. RELEASE PATH

**Goal.** The work leaves this machine.

| id | item | state | owner-shape |
|---|---|---|---|
| 11.1 | push main | the bop gate that user-set for 6.2.0 IS satisfied (`cli_bop` done, exit contract verified). User said "push to save HEAD" | coordinator-inline on user word |
| 11.2 | v6.2.0 tag | USER WORD REQUIRED. Recorded verbatim: "no tagging a non working thing". The tag waits until the user calls the thing working | user |
| 11.3 | daemon restart decision | stopped this session for machine reclaim. Also open: whether it ever watches the orgs root (a daemon-level `SPREFA_CONFIG` puts 800 repos under EVERY wildcard rail, which the safe-default comment warns against; alternatives are a per-root config feature or a cron one-shot) | user |
| 11.4 | v5 pile: 3 orphan roots, ~1.86GB | `cd ~/.local/state/sprefa/roots && rm -rf 5658fb5a59d0f252 c22f2b330d2dd1f7 ea3041acfc1af14c`. `dl daemon health` prints this exact line | user runs, or grants |
| 11.5 | v5 pile: `rel_port_of_reach` | daemon stopped, then `DROP VIEW IF EXISTS rel_port_of_reach_txt; DROP TABLE IF EXISTS rel_port_of_reach; VACUUM;` against `~/.local/state/sprefa/roots/fbabddda40d22347/db.sqlite`. 15.5MB reclaimed | same |
| 11.6 | v5 pile: lazy-rel tier | `plans/2026-07-19-lazy-rel-tier.md`. Three questions: syntax (`rel lazy foo(...)` vs `@lazy`), opt-in vs health-suggested, and whether demand-materialize-with-eviction is wanted at all or VIEW-only suffices (VIEW-only = zero new deps, zero policy code). Context: 39x db/corpus ratio. E5's sensor gives this decision its number | user, informed by E5 |
| 11.7 | v5 pile: filesize rail | `verify.sh` exits 2; 29 src files over 500 lines are not in `scripts/filesize-allow.txt`, all already over budget at pushed main. Grandfather (allowlist + shrink-only law) or schedule splits | user |
| 11.8 | v5 pile: `instant` dom-match.dl rewrite | user-side repo | user |
| 11.9 | worktree removal | 42 worktrees found; 34 fully merged; uncommitted work banked as 13 patches in `archive/worktree-salvage-2026-07-27/` with a per-patch README. The exact removal and branch-deletion commands are in that README. `git worktree remove` was permission-blocked for agents | user runs |
| 11.10 | `.dl/rails.dl:62-64` still uses `p`/`l` | violates the descriptive-names law; owes the rename | luna, trivial |

### Exit receipts

```
git -C ~/projects/sprefa push origin main
cd v6 && just green-all                       # exit 0 on the pushed sha
dl daemon health                              # no orphan-root line
```

### User decisions gating E11

11.2 through 11.9. This entire epic is a card.

---

## 2. FRONTIER: what is deliberately NOT decided here

| item | why deferred | evidence needed to resolve |
|---|---|---|
| the rust backend | user "calm down"; the `lowered/8` plan is target-neutral and `emit_rust.pl` plugs the same plan, so nothing rots | a perf wall that TS cannot clear, measured |
| spreading wiring | lab verdict banked, design record only | an alpha priority that needs it |
| channel checker | gated on the arms-lab SLOT rulings (D5) | those rulings |
| daemon verb parity, multi-repo config surface | out of alpha | the org fan-out gap named in CRAWL-BENCH.md is the first real pull |
| org fan-out spelling | crawl-bench supplies it with a shell loop and SAYS SO. The language has no spelling for "for each repo in this org" | whether the exam (E8) or a real user program needs it |
| whether `v6/dl` (the second runtime) is retired | it is byte-untouched and the bridge WRAPPED rather than adopted, with evidence. Retiring it is a separate decision from finishing the alpha | ingest perf (E2c) landing in tsv2 rather than v6/dl would force it |
| duplicated-logic detection (exam question 13) | not expressible today, and NOT worth a construct until the exam proves demand | E8 phase A |
| entity-plane extras (immutable history, explicit checked retirement) | ruled to need a future spelling that is NOT a bare decl suffix | a program that needs them |

---

## 3. STANDING PROCESS RULES FOR EVERY EPIC ABOVE

These are not new. They are collected here so a dispatching session does not
have to re-read the ledger.

| rule | text |
|---|---|
| worktree dispatch law | every worktree agent's FIRST action is `git merge --ff-only <sha>` with the coordinator's stated sha. Failure or a missing tree = STOP AND REPORT. Working around a blocked command through another mechanism is a defect, never a fix |
| coordinator verification | the coordinator verifies the base sha in the first report and refuses work on any other base (cherry-pick at most, never a history merge) |
| lab protocol | planner seeds the header; labs run in worktrees; labs DIE on landing with the last-copy commit hash recorded |
| the A4 law | every oracle semantics change lands with its emitter fixture in the SAME arc. This is alpha exit criterion 5 |
| fail-first receipts | every fix carries a sabotage that was red before and green after, pasted into the test header |
| count tests | any formerly-quadratic path gets a statement-count or EXPLAIN assertion, never end-state equality alone |
| disjoint file ownership | concurrent agents get disjoint files and are told so in the prompt |
| codex no-commit flow | codex lanes leave the tree uncommitted; the coordinator reviews file by file, runs every receipt itself, and commits |
| gen staleness | `just staleness-gate` is in green-all and covers gen modules AND release binaries. It has caught 3 real incidents; trust it over a clean-looking tree |
| model routing | see section 1.5. luna is the default workhorse; terra only when a brief leaves real decisions open; sol for compiler and emitter trade-offs; opus labs only where the user has said labs, and only on a planner-seeded header |

---

## 4. ALPHA EXIT, RESTATED AGAINST THIS DOCUMENT

The golden plan's six criteria, with their state at base and the epic that
closes each.

| # | criterion | state at base | closed by |
|---|---|---|---|
| 1 | one real dataflow rail graded byte-vs-v5 on a pinned corpus | callgraph DONE, 0 unclassified. flow-interproc has 4 unexplained columns | E4 |
| 2 | LSP diags served from v6 with live re-tick on file edit | HOLDS, but line numbers are honestly zero | E3a |
| 3 | ghcacher runs on the GRADED runtime | schedule parity held; both ghcacher fixtures stop at named refusals | E2 (json/decode buckets) |
| 4 | CLI with exit codes, `dl6 check` usable in a hook | DONE | - |
| 5 | zero open oracle/emitter divergence class | zero open; the standing fixture rule is in force | maintained by every epic |
| 6 | endurance + leak-soak gates in green-all | DONE | - |

Two criteria are fully closed, three are one epic away, and one (5) is a
standing discipline rather than a task. The remaining alpha distance is
E2 + E3 + E4. Everything else in this document is what comes AFTER the alpha
and is enumerated here so that finishing the alpha does not require
re-deriving the next thing.
