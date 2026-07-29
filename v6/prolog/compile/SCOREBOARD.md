# TSV2 Phase C scoreboard

Sweep of `v6/prolog/compile/` over every fixture in
`v6/prolog/conformance/fixtures/*.pl`, per
`plans/2026-07-27-tsv2-compile-target-header.md`'s PHASE C CONTRACT. Driver:
`v6/tsv2/scripts/sweep.sh` (compile every fixture -> `v6/prolog/compile/out/`,
run the oracle over the same fixtures, run every compiled program on the
phase-A runtime, diff tick logs byte-for-byte).

Regenerate: `cd v6/tsv2 && bash scripts/sweep.sh`. Raw data:
`v6/prolog/compile/out/manifest.json` (compile bucket + refusal reason per
fixture) and `v6/prolog/compile/out/run-results.json` (run bucket + diff
excerpt per compiled fixture).

## Totals (current)

Refreshed by the STRUCT-AS-ROWS arc (2026-07-29, ruling `compound_storage =
struct_as_rows`), which added the declared value plane: `type` declarations,
per-type storage-plane dictionaries, intern-at-arrival, boundary render joins
and decode/2 as a dictionary join. The counts before it were 139 / 87 / 85.
The arc before that was FLAGSHIP CALLGRAPH; before that TICK PHASE ALIGNMENT.
The prose sections below this one are historical and were written against the
110-fixture corpus; the numbers here and in the two tables that follow come
from `out/manifest.json` + `out/run-results.json`.

| bucket | count |
|---|---|
| fixtures swept | 155 |
| UNSUPPORTED (compiler refuses, named construct) | 61 |
| compiled (lowering + emission succeeded) | 94 |
| — of which IDENTICAL (tick log byte-identical to oracle) | 92 |
| — of which WRONG (diff vs oracle) | 0 |
| — of which run_error / no_oracle_log (rejection-path fixtures) | 2 |

IDENTICAL + run_error/no_oracle + UNSUPPORTED = 92 + 2 + 61 = 155.

Both emitter modes agree row for row: the incremental default and
`SPREFA_TSV2_EMITTER_MODE=naive` produce the same 92/0/2.

### What the struct-as-rows arc moved

+16 fixtures (`conformance/fixtures/4_struct_values.pl`), of which 7 compile
IDENTICAL in both modes and 9 are named refusals on the new value plane. Zero
movement in the prior 139: every previously-compiled emitted module is
byte-identical after the arc (`git diff` over `v6/tsv2/gen_emitted/`).

Two json-family fixtures moved to a SHARPER refusal rather than compiling:
`decode_open_pattern_binds_nested` and `decode_missing_key_fails_quietly` were
the generic `level_body_goal(..., decode(...))` and are now
`decode_source_not_struct` -- decode/2 lowers over a column with a DECLARED
struct type, and those two read an untyped json column. That is the whole
difference the acceptance fixture
(`struct_ghcacher_stars_normalization`) demonstrates against
`ghcacher_json_normalization`.

The nine `edge_body_needs_json_destructure` fixtures did NOT move and are not
this arc's to move: they destructure a prolog COMPOUND term
(`fresh(tag_w1, body1)`), and a compound term renders as canonical prolog text
where a struct renders as canonical JSON. Accepting the functor form as a
struct spelling would silently change the graded bytes of a value that already
has a meaning. SLOT-TERM-STRUCT names that question.

### Named gap found by the flagship arc, unowned

`callgraph_unused_inverts_with_the_call_set` ends on a tick whose level delta
is NEGATIVE, and its header says why. Ending it one tick earlier — on the
retraction tick that RE-ASSERTS `unused(main)` through the refCount reseed —
makes the emitted run mint one extra `{"tick":5,"deltas":{}}` drain tick that
the oracle does not have: the reseed's re-INSERT stages a next-frontier row,
`promoteFrontiers` reports `carryPending`, and `TickFold` drains once. It takes
a non-monotone level rule to observe, which is why the corpus had not hit it;
the sibling fixture on the same schedule (last tick deletes only) is identical,
and the three-tick prefix of this one is identical. Repro: truncate this
fixture's schedule to four ticks. Owner: the arc that owns
`v6/tsv2/runtime/1_incremental.ts`.

### The UNSUPPORTED bucket, by named reason (61)

| reason | count |
|---|---:|
| `edge_body_needs_pre` | 13 |
| `edge_body_needs_json_destructure` | 9 |
| `type_arrival_shape_mismatch` (the value plane's boundary refusal) | 5 |
| `level_body_goal` (json_each in a level body) | 4 |
| `aggregate_head` (json_array / json_object) | 4 |
| `type_cycle` | 2 |
| `json_value_expression` | 2 |
| `decode_source_not_struct` | 2 |
| 20 more, one each (see `out/manifest.json`) | 20 |

`edge_body_needs_pre` and the json family are what remain of the phase-3
edge-body list. `edge_body_with_latest`, `edge_body_needs_negation`,
`edge_body_needs_bind`, `edge_body_needs_comparison`, `edge_body_needs_now`
and `edge_head_column_type_mismatch` went in the edge-body arc;
`edge_body_needs_finalize` (2) and `edge_body_joins_arrival_fed_level` (1)
went in this one.

### The three fixtures this arc moved

| fixture | was | now |
|---|---|---|
| `clock_rel_join_storms` | `edge_body_joins_arrival_fed_level(diagnostic/4)` | IDENTICAL both modes |
| `keyed_replace_departs_the_old_row` | `edge_body_needs_finalize` | IDENTICAL both modes |
| `departed_fires_next_tick_on_retraction` | `edge_body_needs_finalize` | IDENTICAL both modes |

Zero movement anywhere else: re-running the whole sweep across this arc left
every one of the previously-compiled emitted modules byte-identical except for
the three lines the phase fix adds to a program that has edge rules, and the
departure feature changed the text of exactly ZERO programs that carry no
`finalize` (`git diff` receipt over `v6/tsv2/gen_emitted/`).

### the mid-tick level freeze (`edge_body_joins_arrival_fed_level`, removed)

engine.pl:tick/7 computes `MidLevel = level_closure(store AFTER arrivals)` and
hands it to `process_occurrences/7` frozen, so a level row an arrival retracted
this tick is already gone from what an edge body reads. The emitted mid-tick
level plane only GREW (`applyLevelsBeforeEdges` runs `INSERT OR IGNORE ...
RETURNING`); the retracting half ran in `recomputeLevelsAfterEdges`, after the
edges had joined the stale rows.

MEASURED on `clock_rel_join_storms`, tick 3, BOTH emitter modes, with the
refusal switched off:

    actual  "diag_seen":{"add":[["a_rs",3,..],["a_rs",5,..],["a_rs",7,..]]}
    oracle  "diag_seen":{"add":[["a_rs",5,..]]}

`IncrementalRuntime.recomputeLevelsBeforeEdges` now runs the refCount
reconcile between the two, staging its new rows into THIS tick's frontier
(phase 2) instead of next tick's; the naive referee calls `recomputeLevels`
once before the edge batch and once after, engine.pl's two level closures. It
costs nothing on a drain tick (empty arrival batch returns immediately) and one
guard SELECT on an arrival tick that retracted nothing;
`v6/tsv2/tests/levelFreeze.test.ts` is the count receipt.

### the departure frontier (`edge_body_needs_finalize`, removed)

`finalize/1` in an edge body is the DEPARTURE trigger, and it fires the tick
AFTER the row left (engine.pl `DepartureCarry`; update-arm verdict). Each rel
some rule listens to that way (`listened_departure_refs/2`, mirrored in
analyze.pl and pinned equal to the oracle's by a plunit unit) gets its own
`__departure_frontier_<rel>` TEMP table with the SAME column shape as the
arrival frontier, so the arm's SQL is the arrival arm's text with one table
name swapped and no new statement shape enters the emitter. A separate table
rather than a `_sign` column on the shared frontier is what keeps every
unlistened relation's DDL, promote list and merge list unchanged.

The source is the tick's NET boundary delta, never the raw staged events: a row
removed and re-added inside one tick nets to zero and is not a departure. Log
rels never reach it (`boundaryDelta` fills `del` for set rels only, mirroring
`delta_ref_is_set/2`), so `finalize` over a Log rel stays silently dead in both
implementations (SLOT-LOG-FINALIZE-REFUSAL, unruled).

DURABILITY: the departure frontier is a `CREATE TEMP TABLE` beside
`__frontier_*` and `__next_frontier_*`, so it INHERITS match-frontier lab C7
(the carry set is not durable; a crash loses pending firings) rather than
closing it. `v6/tsv2/tests/departureFrontier.test.ts` measures the inheritance
against a file-backed db: a new connection sees neither carry table.

### `edge_body_needs_pre`: why it is not a widening

The other edge-body buckets were arm-local: one more join, one more WHERE
term, one more bound expression. `pre` is not. engine.pl processes
occurrences ONE AT A TIME (`process_occurrences/7`) and `pre(Atom)` reads the
store as the writes SO FAR THIS TICK left it (step 4: "First occurrence
therefore reads T-1; later occurrences chain"). Every one of the 13 fixtures
reads `pre` over an EDGE-HEADED rel, so the chaining is the point, not an
incidental.

MEASURED, against `merge_family.pl:batched_increments_both_count` (the arc's
own central fixture: `counter(Name,Next) <+ increment(Name,_),
pre(counter(Name,Total)), Next := Total + 1`, seeded `counter(clicks,0)`,
tick 1 = `+increment(clicks,ev1), +increment(clicks,ev2)`). Lowering `pre` the
way `latest` was lowered -- a sampled join against the base table -- and
applying the projected rows the way `IncrementalRuntime.applyKeyedEdge` does:

    pre-as-sampled arm projects: [["clicks",1],["clicks",1]]
    counter after the tick:      [["clicks",1]]
    oracle pins:                 [["clicks",2]]      (deltas -0 then +2)

Two arrivals, one increment. The fold is also CROSS-ARM
(`increment_decrement_same_tick_nets_zero` interleaves an increment rule and a
decrement rule in arrival order, and each rule is a separate emitted
statement), so no single recursive CTE per arm expresses it either. The
faithful lowering is an ordered occurrence loop with writes applied between
occurrences -- a new execution shape in the runtime, not a wider arm. The
refusal stays until that shape exists.

### The final-state leg (new, and it changes how to read this table)

The sweep used to grade **tick logs only**, which says NOTHING about a fixture
whose `Schedule` is empty: both sides print zero lines and the diff calls it
IDENTICAL. Findings 1 and 2 below flagged that as the "vacuous pass" class.
It was much bigger than two fixtures — 25 of the 30 fixtures the expression
and aggregate buckets contain have an empty schedule.

`oracle_dump.pl:dump_final_state/5` now also writes
`out/<name>.oracle.final.jsonl` (the same envelope over `run_program/5`'s
`FinalAll`), `emit_ts.pl` exports a per-rel `finalSelect`, and `sweep.ts`
reports a second `FINAL` line. It is **additive**: the tick-log bucket stays
the gate, and the final-state bucket is reported beside it.

| final bucket | count |
|---|---|
| final_identical | 83 |
| final_wrong | 2 |
| no_oracle_final | 0 |

Both remaining `final_wrong` rows are the rejection-path fixtures that throw
before a final state exists (`log_retraction_rejected`,
`fork_join_error_arm_is_a_value`). The third, `retention_count_prunes_oldest`,
was closed when keep(count) was lowered.

The leg earned its keep a second time in the edge-body arc: with edge heads
inheriting body column types, `xref_rev_is_pin_data_not_live_head` compiled
and graded IDENTICAL on the tick log while dropping `known_repo(2)` from its
final state — a ref only an `Initial` row mentions, which the compiler's ref
inventory did not carry (fixed as `analyze.pl:seeded_refs/2`).

The leg earned its keep on its first run by catching
`braces_in_head_position`, which this scoreboard had listed as
"IDENTICAL (vacuous)" since phase C and which was in fact storing
`{}({"fn":":","args":["repo","cli"]})` where the oracle holds
`obj([|](-(repo,cli),[]))`. It is now a named refusal
(`json_value_expression`), not a silent pass.

**F8 fixture added (2026-07-28 cleanup audit, not a ruling)**:
`log_stacks_within_tick_and_across_ticks` (`occurrence_identity.pl`) — the
tsv2 Phase A hand-carved oracle corpus (tickLoop.test.ts) never put a
byte-identical row on a Log rel twice, so a multiset-diff regression
(`runtime/diff.ts`'s multiplicity loop collapsing to a single push) was
invisible to that corpus end to end; only `tsv2/tests/diff.test.ts`'s unit
case caught it. This fixture compiles clean and lands IDENTICAL, exercising
the same regression through the sweep's byte-for-byte grade on TWO Log rels
(a direct one and an edge-triggered derived one) at once: `fixtures swept`
109 -> 110, `compiled` 30 -> 31, `IDENTICAL` 27 -> 28.

**PHASE C2 RULING 1 (typed columns) landed** (commit `tsv2 C2a`): the 5
int-vs-string WRONGs Finding 3 documented are now IDENTICAL — see the
widening history's entry 4 and Finding 3's resolution note below.

**PHASE C2 RULING 2 (unmarked edge triggers) landed** (commit `tsv2 C2b`):
13 more fixtures went UNSUPPORTED -> IDENTICAL (compiled 17 -> 30, identical
14 -> 27, WRONG stays 3 — all three pre-existing and out of this ruling's
scope, unchanged from Ruling 1's landing). unsupported 92 -> 79. See the
widening history's entry 5 for the mechanism, the two stop-and-report sites
this widening surfaced (both left refused, not hacked around), and the
`edge_marked_with_extra_goal`/`level_body_goal` bucket count changes (more
fixtures now REACH those checks since their earlier-in-program unmarked
edge rule no longer blocks them first — not new gaps, existing ones now
visible under a precise name instead of a blanket `edge_body_shape`).

## Per-fixture table: compiled (85)

Regenerated from `out/manifest.json` + `out/run-results.json`. `run bucket` is
the tick-log grade, `final bucket` the final-state grade (see above).

| fixture | file | run bucket | final bucket |
|---|---|---|---|
| enum_decl_variant_rows_round_trip_through_tag_view | 0_enum_variants.pl | identical | final_identical |
| enum_decl_two_variants_union_in_tag_view | 0_enum_variants.pl | identical | final_identical |
| match_classify_response | 1_match_block.pl | identical | final_identical |
| match_classify_response_desugared | 1_match_block.pl | identical | final_identical |
| match_edge_arm_keeps_edge_semantics | 1_match_block.pl | identical | final_identical |
| keyed_edge_head_still_replaces | 1_match_block.pl | identical | final_identical |
| extraction_fork_callgraph | 2_hosts_wiring.pl | identical | final_identical |
| extraction_fork_span_line | 2_hosts_wiring.pl | identical | final_identical |
| native_ts_query_term | 2_hosts_wiring.pl | identical | final_identical |
| diag_scenario_seven_ticks_end_to_end | check_eventing.pl | identical | final_identical |
| clock_rel_join_storms | check_eventing.pl | identical | final_identical |
| retention_count_prunes_oldest | engine_core.pl | identical | final_identical |
| log_retraction_rejected | engine_core.pl | run_error | final_wrong |
| world_fed_keyed_arrival_replaces | engine_core.pl | identical | final_identical |
| now_reads_the_tick | engine_core.pl | identical | final_identical |
| edge_chain_hops_tick_per_stage | engine_core.pl | identical | final_identical |
| marker_stops_backlog_replay | engine_core.pl | identical | final_identical |
| unmarked_edge_replays_backlog | engine_core.pl | identical | final_identical |
| retraction_only_tick_retracts_level_view | engine_core.pl | identical | final_identical |
| departed_fires_next_tick_on_retraction | engine_core.pl | identical | final_identical |
| keyed_replace_departs_the_old_row | engine_core.pl | identical | final_identical |
| set_dedups_log_stacks | engine_core.pl | identical | final_identical |
| head_expression_evaluates_derived_column | expressions.pl | identical | final_identical |
| comparison_filters_rows | expressions.pl | identical | final_identical |
| range_join_over_arithmetic | expressions.pl | identical | final_identical |
| bind_computes_derived_value_then_comparison_filters | expressions.pl | identical | final_identical |
| interpolation_desugars_to_concat | expressions.pl | identical | final_identical |
| division_truncates_toward_zero_mod_follows_divisor_sign | expressions.pl | identical | final_identical |
| typed_int_without_literal_witness | expressions.pl | identical | final_identical |
| count_is_bag_of_derivations | json_arm.pl | identical | final_identical |
| sum_min_max_group_by_plain_columns | json_arm.pl | identical | final_identical |
| aggregate_count_min_max_track_arrivals_and_retraction | json_arm.pl | identical | final_identical |
| aggregate_min_recomputes_when_the_minimum_is_retracted | json_arm.pl | identical | final_identical |
| aggregate_sum_tracks_a_growing_and_shrinking_group | json_arm.pl | identical | final_identical |
| merge_batches_per_tick | merge_family.pl | identical | final_identical |
| merge_never_retracts | merge_family.pl | identical | final_identical |
| key_last_write_wins | merge_family.pl | identical | final_identical |
| key_identical_write_is_silent | merge_family.pl | identical | final_identical |
| key_same_tick_ordered_not_conflict | merge_family.pl | identical | final_identical |
| log_deltas_follow_arrival_order | occurrence_identity.pl | identical | final_identical |
| shuffled_arrival_reorders_log_deltas | occurrence_identity.pl | identical | final_identical |
| level_view_reads_set_projection_not_occurrences | occurrence_identity.pl | identical | final_identical |
| demand_view_fires_its_consumer_once | occurrence_identity.pl | identical | final_identical |
| log_stacks_within_tick_and_across_ticks | occurrence_identity.pl | identical | final_identical |
| filter_map_is_a_level_rule | operators.pl | identical | final_identical |
| repeat_is_a_self_carry_chain | operators.pl | identical | final_identical |
| fork_join_is_a_conjunctive_body | operators.pl | identical | final_identical |
| fork_join_error_arm_is_a_value | operators.pl | run_error | final_wrong |
| switch_as_keyed_replace | scopes.pl | identical | final_identical |
| merge_policy | scopes.pl | identical | final_identical |
| exhaust_policy | scopes.pl | identical | final_identical |
| completion_propagation_lattice_tick | scopes.pl | identical | final_identical |
| fill_as_cache_update_swr | scopes.pl | identical | final_identical |
| demand_laziness_effect_rows | scopes.pl | identical | final_identical |
| shared_demand_refcount | scopes.pl | identical | final_identical |
| zombie_scope_negative_case_a2b | scopes.pl | identical | final_identical |
| identical_demand_dedups | shell_stream.pl | identical | final_identical |
| new_salt_refires_fresh_stream | shell_stream.pl | identical | final_identical |
| terminal_is_terminal | shell_stream.pl | identical | final_identical |
| live_nonzero_exit_keeps_rows | shell_stream.pl | identical | final_identical |
| worktree_edit_replaces_digest_and_flips_kind_view | spine_semantics.pl | identical | final_identical |
| worktree_edit_identical_resave_is_silent | spine_semantics.pl | identical | final_identical |
| dirty_derives_from_digest_mismatch | spine_semantics.pl | identical | final_identical |
| dirty_retracts_on_matching_commit | spine_semantics.pl | identical | final_identical |
| head_move_replaces_key | spine_semantics.pl | identical | final_identical |
| head_move_flips_current_tree_in_one_tick | spine_semantics.pl | identical | final_identical |
| pin_to_unknown_repo_derives_repo_candidate | spine_semantics.pl | identical | final_identical |
| xref_rev_is_pin_data_not_live_head | spine_semantics.pl | identical | final_identical |
| changed_since_spans_two_turns | spine_semantics.pl | identical | final_identical |
| changed_since_ignores_events_before_turn | spine_semantics.pl | identical | final_identical |
| two_pins_dedup_to_one_demand_row | spine_semantics.pl | identical | final_identical |
| rev_fill_not_behind_keeps_stale_pin_empty | spine_semantics.pl | identical | final_identical |
| clean_state_no_diags | timeless_rail.pl | identical | final_identical |
| clean_state_gate_and_exit_zero | timeless_rail.pl | identical | final_identical |
| waiver_range_join_exact_rows | timeless_rail.pl | identical | final_identical |
| over_baseline_diag_exact_rows | timeless_rail.pl | identical | final_identical |
| over_baseline_count_row | timeless_rail.pl | identical | final_identical |
| over_baseline_gate_blocks_commit_only | timeless_rail.pl | identical | final_identical |
| fix_by_waiver_returns_to_clean | timeless_rail.pl | identical | final_identical |
| new_file_diag_at_hit_line_exact_rows | timeless_rail.pl | identical | final_identical |
| new_file_no_exceeded_diag | timeless_rail.pl | identical | final_identical |
| unwrap_aggregate_and_interpolation | timeless_rail.pl | identical | final_identical |
| unwrap_unchanged_file_silent | timeless_rail.pl | identical | final_identical |
| unwrap_below_budget_silent | timeless_rail.pl | identical | final_identical |
| tightened_baseline_catches_regrowth | timeless_rail.pl | identical | final_identical |

## Per-construct blocked tally (UNSUPPORTED, ranked)

Regenerated from `out/manifest.json` by the TICK PHASE ALIGNMENT arc. The
comparison / arithmetic-bind / head-arithmetic buckets that dominated this
table through phase C are lowered; the `latest` / `negation` / `now` /
`edge_trigger_is_derived` / `edge_head_column_type_mismatch` rows went in the
edge-body arc; `edge_body_needs_finalize` and
`edge_body_joins_arrival_fed_level` went in this one. What is left is the
`pre` occurrence-loop family, the json/decode family, and one-off refusals
each pinned by its own fixture.

| construct | fixtures blocked |
|---|---:|
| `edge_body_needs_pre` | 13 |
| `edge_body_needs_json_destructure` | 9 |
| `level_body_goal` | 6 |
| `aggregate_head` | 4 |
| `json_value_expression` | 2 |
| `aggregate_in_edge_head` | 1 |
| `arith_operand_not_int` | 1 |
| `comparison_type_mismatch` | 1 |
| `decl_type_conflicts_witness` | 1 |
| `edge_body_with_negation` | 1 |
| `edge_head_conflict_risk` | 1 |
| `edge_into_unkeyed_set` | 1 |
| `enum_variant_name_collision` | 1 |
| `join_column_type_mismatch` | 1 |
| `keep_on_non_log_rel` | 1 |
| `keyed_level_head` | 1 |
| `keyed_log_rel` | 1 |
| `latest_in_level_rule` | 1 |
| `log_on_level_headed_rel` | 1 |
| `match_nonexhaustive` | 1 |
| `missing_retention` | 1 |
| `pre_in_level_rule` | 1 |
| `trigger_arg_not_var` | 1 |
