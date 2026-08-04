# Lane L-A report: the 304 cache fix and its golden

## TOC

- [1. Ownership check](#1-ownership-check)
- [2. What changed](#2-what-changed)
- [3. The tick golden after the fix](#3-the-tick-golden-after-the-fix)
- [4. The new 304 golden](#4-the-new-304-golden)
- [5. Fail-first receipt](#5-fail-first-receipt)
- [6. Verbatim validation outputs](#6-verbatim-validation-outputs)

## 1. Ownership check

First action `git log --oneline -1` shows `1e7b6843`, the required HEAD. No
deviation written.

Files touched, all inside the lanes I own:

```
M v6/tsv2/goldens/ghcacher_tick_golden/0_ghcacher_clock_golden.dl6
M v6/tsv2/goldens/ghcacher_tick_golden/2_expected.tick.jsonl
M v6/tsv2/goldens/ghcacher_tick_golden/3_expected.final.jsonl
M v6/tsv2/goldens/ghcacher_tick_golden/README.md
A v6/tsv2/goldens/ghcacher_304_golden/*   (new directory)
A REPORT.md
```

`v6/justfile` untouched. `pnpm` used, `npm` never. `v6/tsv2/node_modules` was
absent; both it and the linked `v6/sprefa-store/js/node_modules` were restored
with `pnpm install --frozen-lockfile`.

## 2. What changed

Applied the section-1.1 fix verbatim to
`v6/tsv2/goldens/ghcacher_tick_golden/0_ghcacher_clock_golden.dl6`. Before:

```dl6
rel cache_view(ep: text, tag: text, stars: int, full_name: text).

cache_view(Ep, Tag, Stars, FullName) <-
  resp(Ep, _, 200, Tag, Stars, FullName).
```

After:

```dl6
rel fresh_hit(ep: text, tag: text, stars: int, full_name: text).
rel cache_view(ep: text, tag: text, stars: int, full_name: text) key(1).

fresh_hit(Ep, Tag, Stars, FullName) <-
  resp(Ep, _, Status, Tag, Stars, FullName),
  Status == 200.

cache_view(Ep, Tag, Stars, FullName) <+ fresh_hit(Ep, Tag, Stars, FullName).
```

The literal-200-in-trigger spelling was not used, per the brief: the compiler
refuses it by name (`trigger_arg_not_var`) and another lane owns that fixture.

Regenerated `2_expected.tick.jsonl` and `3_expected.final.jsonl` from the oracle
output; the emitted runtime is byte-identical to it. Both doors now agree.

## 3. The tick golden after the fix

The observable change is the clock-move tick. Before the fix, moving `current_clock`
to bucket 2 emitted `cache_view del (repo, tag-v1, 17, cli/cli)`. After the fix
the keyed latch survives the clock move (zero delta on `cache_view`), because
the latch has no input edge from the clock. `fresh_hit` still retracts its row;
only the latch holds.

Updated the tick README table rows 3 and 4 to record the `fresh_hit`/`cache_view`
split and the latch replacement, and added a pointer to the 304 golden.

## 4. The new 304 golden

`v6/tsv2/goldens/ghcacher_304_golden/` mirrors the tick golden's layout:

```
0_ghcacher_304_golden.dl6
1_schedule.json
2_expected.tick.jsonl
3_expected.final.jsonl
4_oracle.pl
6_gate.sh
README.md
```

The program is the fixed tick program. The schedule: bootstrap, a 200 at tick 2,
then the clock moves and two consecutive polls answer 304 (ticks 4 and 6).

Graded expectations, all met:

- (a) `cache_view` keeps its last 200 row `(repo, tag-v1, 17, cli/cli)` through
  both 304 ticks. The final has
  `"cache_view":[["repo","tag-v1",17,"cli/cli"]]`.
- (b) the etag latch advances: `current_etag` moves `""` to `"tag-v1"` on tick 3
  and `poll` rides buckets 1, 2, 3.
- (c) the tick log is byte-identical between the Prolog oracle and the emitted
  SQLite runtime (`diff` empty).

## 5. Fail-first receipt

Reverted the fix in a scratch copy of the 304 golden (back to the plain level
rule over `resp`), ran its `6_gate.sh`, captured the red output, then restored.
The scratch gate exits 1. The drift, trimmed to the two lines that prove the
defect plus the final:

```
@@ -1,7 +1,7 @@
-{"tick":2,...,"cache_view":{"add":[["repo","tag-v1",17,"cli/cli"]],"del":[]},"fresh_hit":{...},"resp":{...}}}
+{"tick":2,...,"cache_view":{"add":[["repo","tag-v1",17,"cli/cli"]],"del":[]},"resp":{...}}}
-{"tick":3,...,"fresh_hit":{"add":[],"del":[["repo","tag-v1",17,"cli/cli"]]},"interval":...}}
+{"tick":3,...,"cache_view":{"add":[],"del":[["repo","tag-v1",17,"cli/cli"]]},"interval":...}}
-{"final":{...,"cache_view":[["repo","tag-v1",17,"cli/cli"]],...}}
+{"final":{...,"current_clock":[[300,3]],...}}     (no "cache_view" key at all)
```

The broken oracle's tick 3 emits `"cache_view":{"add":[],"del":[["repo","tag-v1",17,"cli/cli"]]}`,
the cache row destroyed when the clock bucket moves, before any 304. Its final
has no `cache_view` key at all: the empty cache. Restoring the `key(1)` latch
plus the status-filtered `fresh_hit` edge restores the row and the gate goes
green.

## 6. Verbatim validation outputs

### `bash v6/tsv2/goldens/ghcacher_tick_golden/6_gate.sh`

```text
COMPILE-TRACE program=0_ghcacher_clock_golden parse=8/78411 plan=5/65237 lower=3/14613 boot=0/424 emit=8/66739 write=1/92 total=25/225516
GHCACHER_CLOCK_GOLDEN_HOLDS ticks=5 final=1
```

### `bash v6/tsv2/goldens/ghcacher_304_golden/6_gate.sh`

```text
COMPILE-TRACE program=0_ghcacher_304_golden parse=9/78411 plan=4/65237 lower=3/14613 boot=0/424 emit=8/66739 write=1/92 total=25/225516
GHCACHER_304_GOLDEN_HOLDS ticks=6 final=1
```

### `cd v6 && just conformance`

Exit 0, 293 tests pass. Full output, verbatim:

```text
cd /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/conformance && /Users/chrishafley/projects/sprefa-lab-gh304/v6/tools/run-capped.sh "${CONFORMANCE_BUDGET_S:-300}" swipl -q -l go.pl -g go -g halt
PASS  enum_decl_variant_rows_round_trip_through_tag_view
PASS  enum_decl_two_variants_union_in_tag_view
PASS  enum_decl_variant_name_collision_is_refused
PASS  match_classify_response
PASS  match_classify_response_desugared
PASS  match_edge_arm_keeps_edge_semantics
PASS  match_enum_nonexhaustive_is_refused
PASS  keyed_level_head_is_refused
PASS  keyed_edge_head_still_replaces
PASS  ghcacher_json_normalization
PASS  ghcacher_host_program_term
PASS  extraction_fork_callgraph
PASS  extraction_fork_span_line
PASS  native_ts_query_term
PASS  host_output_column_shadows_runtime_ordinal
PASS  host_input_column_shadows_runtime_witness
PASS  duplicate_host_name_is_refused
PASS  repo_on_bind_watch_is_refused
PASS  callgraph_derivation_over_extraction
PASS  callgraph_unused_inverts_with_the_call_set
PASS  flagship_flow_reach_over_resolved_edges
PASS  struct_type_cycle_rejected
PASS  struct_type_mutual_cycle_rejected
PASS  struct_column_type_unknown_rejected
PASS  struct_host_output_type_unknown_rejected
PASS  struct_arrival_missing_key_rejected
PASS  struct_arrival_field_type_rejected
PASS  struct_arrival_unknown_key_rejected
PASS  struct_arrival_key_order_canonicalized
PASS  struct_arrival_functor_term_rejected
PASS  struct_column_renders_canonical_json
PASS  struct_intern_order_a
PASS  struct_intern_order_b
PASS  struct_nested_value_renders_whole_tree
PASS  struct_ghcacher_stars_normalization
PASS  struct_decode_field_unknown_rejected
PASS  struct_span_columns_are_int_after_decode
PASS  struct_host_output_schedule_answer_interned
PASS  struct_shared_child_survives_one_release
PASS  relation_reference_target_and_parent_share_tick
PASS  groupby_two_bare_integer_literals
PASS  groupby_aggregate_two_bare_integer_literals
PASS  probe_output_comparison_guard
PASS  higher_order_call_goal_rejected
PASS  higher_order_call_over_atom_rejected
PASS  backslash_in_string_literal_survives_both_doors
PASS  host_free_query_leaves_a_derived_rel_unsubscribed
PASS  flow_arg_param_hop_is_positional_and_site_pinned
PASS  flow_sig_owner_join_types_the_resolved_callee
PASS  bool_literals_round_trip
PASS  bool_identity_comparison_filters
PASS  bool_relation_negation_is_two_valued
PASS  float_arithmetic_is_binary64
PASS  int_float_arithmetic_keeps_real_result
PASS  float_avg_is_grouped
PASS  float_exact_comparison_has_no_epsilon
PASS  float_exact_join_has_no_epsilon
PASS  float_negative_zero_canonical_boundary
PASS  float_integral_value_keeps_real_storage
PASS  float_shortest_round_trip_wire
PASS  float_avg_retracts_to_empty_group
PASS  int_out_of_range_is_named_refusal
PASS  bool_rejects_text_ingress
PASS  float_rejects_non_float_ingress
PASS  text_rejects_number_ingress
PASS  int_rejects_fractional_ingress
PASS  int_accepts_integral_float
PASS  float_widens_integer_ingress
PASS  wide_int_refused_at_undeclared_column
PASS  wide_int_refused_inside_json_document
PASS  float_widens_wide_integer_ingress
PASS  head_column_type_conflict_is_refused
PASS  head_column_int_widens_into_float
PASS  head_column_list_and_json_share_storage
PASS  relation_depth2_construct_and_read
PASS  relation_depth2_literal_leaf_selects_zero_and_one
PASS  relation_depth2_many_rows_share_one_leaf
PASS  relation_depth2_chained_decode
PASS  relation_depth2_nested_decode_pattern
PASS  relation_depth2_dot_read
PASS  relation_depth2_member_dot_pattern
PASS  relation_depth3_construct_and_read
PASS  relation_depth3_chained_decode
PASS  relation_depth3_many_rows
PASS  relation_pattern_text_literal_in_ref_column_rejected
PASS  relation_pattern_wrong_target_rejected
PASS  relation_pattern_target_arity_rejected
PASS  relation_ref_column_fed_by_text_variable_rejected
PASS  relation_ref_column_fed_by_ref_variable_accepted
PASS  relation_value_under_negation_rejected
PASS  relation_value_in_edge_rule_rejected
PASS  coalesce_defaults_the_absent_row
PASS  coalesce_default_returns_when_source_retracts
PASS  coalesce_over_derived_source
PASS  coalesce_in_edge_body_samples
PASS  coalesce_without_an_output_is_refused
PASS  coalesce_with_two_outputs_is_refused
PASS  coalesce_with_a_variable_default_is_refused
PASS  coalesce_under_negation_is_refused
PASS  json_string_control_escapes_are_valid_json
PASS  json_control_escapes_inside_a_document
PASS  json_non_ascii_keys_sort_by_code_point
PASS  json_nfc_and_nfd_keys_stay_distinct
PASS  json_empty_string_key_round_trips
PASS  json_marker_shaped_keys_are_ordinary_data
PASS  json_safe_integer_boundary_survives_both_doors
PASS  json_empty_containers_nest
PASS  json_deep_exact_key_chain_binds
PASS  json_top_level_scalar_document_is_a_value
PASS  json_absent_key_yields_no_row_under_arrivals
PASS  json_spread_and_capture_and_descent_multiply
PASS  json_typed_capture_folds_into_a_keyed_int_total
PASS  json_typed_capture_filters_a_wrong_typed_value
PASS  json_untyped_capture_binds_without_a_type
PASS  json_capture_type_bool_is_refused
PASS  json_capture_type_typo_is_refused
PASS  ordered_json_group_array_value
PASS  ordered_json_group_array_integer_values
PASS  ordered_json_group_array_ordinal
PASS  ordered_group_concat_value
PASS  ordered_group_concat_ordinal
PASS  ordered_aggregate_retraction_rebuild
PASS  ordered_json_group_array_nested_json
PASS  ordered_mermaid_line_assembly
PASS  ordered_fragment_line_assembly
PASS  ordered_group_rels_v5_collect
PASS  ordered_group_rels_json_head
PASS  arrival_affinity_rewrite_keeps_delta
PASS  arrival_dup_batch_partial_ignore
PASS  combine_level_is_the_conjunction_spelling
PASS  conjunction_level_control_for_combine
PASS  combine_edge_is_the_conjunction_spelling
PASS  conjunction_edge_control_for_combine
PASS  next_level_is_the_bare_atom_spelling
PASS  next_edge_is_the_bare_atom_spelling
PASS  zip_is_a_named_refusal
PASS  subscribe_is_a_named_refusal
PASS  unsubscribe_is_a_named_refusal
PASS  complete_is_a_named_refusal
PASS  error_is_a_named_refusal
PASS  scan_is_a_named_refusal
PASS  scan_is_a_named_refusal_at_five_arguments
PASS  diag_scenario_seven_ticks_end_to_end
PASS  clock_rel_join_storms
PASS  retention_count_prunes_oldest
PASS  retention_prune_is_a_visible_minus
PASS  finalize_over_log_fires_on_retention_prune
PASS  created_at_pinned_updated_at_advances
PASS  log_without_retention_rejected
PASS  aggregate_in_edge_head_rejected
PASS  unimplemented_aggregate_head_rejected
PASS  keep_on_non_log_rel_rejected
PASS  keyed_log_rejected
PASS  edge_into_unkeyed_set_rejected
PASS  log_retraction_rejected
PASS  world_fed_keyed_arrival_replaces
PASS  log_on_level_headed_rel_rejected
PASS  retention_head_conflict_risk_rejected
PASS  retention_single_arm_still_prunes
PASS  latest_in_level_rule_rejected
PASS  pre_in_level_rule_rejected
PASS  now_reads_the_tick
PASS  edge_chain_hops_tick_per_stage
PASS  marker_stops_backlog_replay
PASS  unmarked_edge_replays_backlog
PASS  retraction_only_tick_retracts_level_view
PASS  departed_fires_next_tick_on_retraction
PASS  keyed_replace_departs_the_old_row
PASS  pairwise_reads_state_at_the_departure_tick
PASS  pairwise_pairs_adjacent_values_when_the_source_idles
PASS  set_dedups_log_stacks
PASS  head_expression_evaluates_derived_column
PASS  comparison_filters_rows
PASS  range_join_over_arithmetic
PASS  bind_computes_derived_value_then_comparison_filters
PASS  interpolation_desugars_to_concat
PASS  division_truncates_toward_zero_mod_follows_divisor_sign
PASS  arithmetic_rejects_non_int_operand_at_runtime
PASS  text_one_and_numeric_one_never_join
PASS  text_one_and_numeric_one_are_not_equal
PASS  typed_int_without_literal_witness
PASS  typed_int_contradicts_text_witness
PASS  braces_literal_canonicalizes
PASS  braces_in_head_position
PASS  decode_open_pattern_binds_nested
PASS  decode_missing_key_fails_quietly
PASS  json_each_fans_out
PASS  json_array_spread_fans_out_correlated_siblings
PASS  json_array_spread_skips_non_matching_elements
PASS  json_key_capture_binds_key_and_value
PASS  json_key_capture_nests_and_fans_out
PASS  json_descent_matches_at_any_depth
PASS  json_descent_into_scalars_is_silent
PASS  json_empty_object_pattern_matches_any_object
PASS  list_column_fans_out_through_spread
PASS  count_is_bag_of_derivations
PASS  sum_min_max_group_by_plain_columns
PASS  json_array_keeps_bag_duplicates
PASS  json_array_groups_and_nests
PASS  json_object_builds_document
PASS  json_object_dup_key_rejected
PASS  aggregate_count_min_max_track_arrivals_and_retraction
PASS  aggregate_min_recomputes_when_the_minimum_is_retracted
PASS  aggregate_sum_tracks_a_growing_and_shrinking_group
PASS  json_round_trip_decode_to_document
PASS  merge_batches_per_tick
PASS  merge_never_retracts
PASS  key_last_write_wins
PASS  key_identical_write_is_silent
PASS  key_same_tick_ordered_not_conflict
PASS  counter_fold_matches_hand_computation
PASS  seed_and_transition_are_disjoint
PASS  batched_increments_both_count
PASS  increment_decrement_same_tick_nets_zero
PASS  one_occurrence_two_rows_still_conflicts
PASS  log_driver_fold_needs_no_id_column
PASS  identical_increments_stack_as_log_deltas
PASS  lww_fold_follows_arrival_order
PASS  concat_fold_follows_arrival_order
PASS  concat_fold_reversed_arrival_reverses_result
PASS  log_deltas_follow_arrival_order
PASS  shuffled_arrival_reorders_log_deltas
PASS  level_view_reads_set_projection_not_occurrences
PASS  demand_view_fires_its_consumer_once
PASS  log_stacks_within_tick_and_across_ticks
PASS  set_rel_identical_arrival_is_one_occurrence
PASS  log_rel_identical_arrival_is_two_occurrences
PASS  any_two_tagged_arms_land_on_one_tick
PASS  one_attempt_keyed_head_loses_the_first_arm_silently
PASS  one_attempt_bounded_log_two_arms_refused
PASS  one_attempt_guard_by_negation_lands_one_unnamed_winner
PASS  one_attempt_guard_by_negation_arrival_order_beats_arm_order
PASS  filter_map_is_a_level_rule
PASS  repeat_is_a_self_carry_chain
PASS  fork_join_is_a_conjunctive_body
PASS  fork_join_error_arm_is_a_value
PASS  ordered_program_level_fold_reaches_three_links
PASS  unordered_program_level_fold_reaches_three_links
PASS  switch_as_keyed_replace
PASS  stale_keyed_retraction_keeps_replacement
PASS  merge_policy
PASS  exhaust_policy
PASS  concat_program_queue
PASS  scope_done_three_spellings
PASS  completion_propagation_lattice_tick
PASS  take_until_keyed_replace_negated_done
PASS  state_flap_nets_to_zero_scope_churn
PASS  fill_as_cache_update_swr
PASS  demand_laziness_effect_rows
PASS  shared_demand_refcount
PASS  zombie_scope_negative_case_a2b
PASS  seq_wire_surface
PASS  seq_wire_hand
PASS  identical_demand_dedups
PASS  new_salt_refires_fresh_stream
PASS  terminal_is_terminal
PASS  live_nonzero_exit_keeps_rows
PASS  worktree_edit_replaces_digest_and_flips_kind_view
PASS  worktree_edit_identical_resave_is_silent
PASS  dirty_derives_from_digest_mismatch
PASS  dirty_retracts_on_matching_commit
PASS  head_move_replaces_key
PASS  head_move_flips_current_tree_in_one_tick
PASS  pin_to_unknown_repo_derives_repo_candidate
PASS  xref_rev_is_pin_data_not_live_head
PASS  changed_since_spans_two_turns
PASS  changed_since_ignores_events_before_turn
PASS  two_pins_dedup_to_one_demand_row
PASS  rev_fill_not_behind_keeps_stale_pin_empty
PASS  async_state_machine_with_pattern_scan
PASS  same_tick_error_then_fresh_chains_arms
PASS  desugared_trace_equals_hand_written
PASS  trigger_marker_is_what_stops_backlog_replay
PASS  unmarked_chain_replays_to_late_subscriber
PASS  unmarked_first_stage_refires_on_late_watch
PASS  pipe_stage_costs_one_tick
PASS  chain_into_keyed_head_replaces
PASS  guard_stage_fires_on_negation_and_comparison
PASS  guard_stage_silent_when_muted
PASS  guard_stage_silent_below_threshold
PASS  clean_state_no_diags
PASS  clean_state_gate_and_exit_zero
PASS  waiver_range_join_exact_rows
PASS  over_baseline_diag_exact_rows
PASS  over_baseline_count_row
PASS  over_baseline_gate_blocks_commit_only
PASS  fix_by_waiver_returns_to_clean
PASS  new_file_diag_at_hit_line_exact_rows
PASS  new_file_no_exceeded_diag
PASS  unwrap_aggregate_and_interpolation
PASS  unwrap_unchanged_file_silent
PASS  unwrap_below_budget_silent
PASS  tightened_baseline_catches_regrowth
```

### `cd v6 && just plunit`

Exit 0, 324 tests passed. Full output, verbatim:

```text
cd /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile && /Users/chrishafley/projects/sprefa-lab-gh304/v6/tools/run-capped.sh "${PLUNIT_BUDGET_S:-600}" swipl -q -l test/plunit_tests.pl -g run_tests -g halt
Warning: /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/2_subscribe.plt:81:
Warning:    Singleton variables: [Id]
% [1/324] clock_checker:reg.._roles_are_complete .... passed (0.004 sec)
% [2/324] clock_checker:edg.._labels_and_offsets .... passed (0.000 sec)
% [3/324] clock_checker:sin.._fact_are_queryable .... passed (0.000 sec)
% [4/324] clock_checker:pip..atch_observed_ticks .... passed (0.001 sec)
% [5/324] clock_checker:equ..rade_diamond_passes .... passed (0.000 sec)
% [6/324] clock_checker:une..ade_diamond_refuses .... passed (0.000 sec)
% [7/324] clock_checker:pos..scc_is_constructive .... passed (0.000 sec)
% [8/324] clock_checker:pos..y_scc_is_productive .... passed (0.000 sec)
% [9/324] clock_checker:clo..ear_in_chain_length .... passed (0.004 sec)
% [10/324] clock_checker:clo.._in_parallel_routes ... passed (0.003 sec)
% [11/324] clock_checker:zer..egative_scc_refuses ... passed (0.000 sec)
% [12/324] clock_checker:com.._runs_clock_checker ... passed (0.000 sec)
% [13/324] clock_checker:ora.._runs_clock_checker ... passed (0.000 sec)
% [14/324] clock_checker:fiv..classes_are_derived ... passed (0.000 sec)
% [15/324] clock_checker:his..ed_ids_and_programs ... passed (0.000 sec)
% [16/324] clock_checker:his.._partition_is_exact ... passed (0.000 sec)
% [17/324] clock_checker:his.._partition_is_exact ... passed (0.000 sec)
% [18/324] clock_checker:a2_..t_provable_boundary ... passed (0.000 sec)
% [19/324] clock_checker:sin..gger_batch_boundary ... passed (0.000 sec)
% [20/324] clock_checker:a4_..no_rule_clock_claim ... passed (0.000 sec)
% [21/324] clock_checker:a5_..arity_refs_distinct ... passed (0.000 sec)
% [22/324] clock_checker:a6_..untime_crosschecked ... passed (0.001 sec)
% [23/324] clock_checker:a7_..no_rule_clock_claim ... passed (0.000 sec)
% [24/324] clock_checker:a8_..ing_partition_clock ... passed (0.000 sec)
% [25/324] clock_checker:a9_..no_rule_clock_claim ... passed (0.000 sec)
% [26/324] clock_checker:a11..ggregate_dependency ... passed (0.000 sec)
% [27/324] clock_checker:a4_..g_b_and_stops_there ... passed (0.000 sec)
% [28/324] clock_checker:a12..eplay_from_sampling ... passed (0.001 sec)
% [29/324] clock_checker:c2_.._with_observed_tick ... passed (0.001 sec)
% [30/324] clock_checker:d1_..e_edge_headed_plane ... passed (0.002 sec)
% [31/324] clock_checker:liv..abelled_not_refused ... passed (0.000 sec)
% [32/324] clock_checker:two..d_is_a_race_not_one ... passed (0.001 sec)
% [33-1/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-2/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-3/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-4/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-5/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-6/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-7/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-8/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-9/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-10/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-11/324] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [34-1/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-2/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-3/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-4/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-5/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-6/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-7/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-8/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-9/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-10/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-11/324] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [35/324] graph_module:cycl.._acyclic_singletons ... passed (0.000 sec)
% [36/324] graph_module:self.._a_cyclic_component ... passed (0.000 sec)
% [37/324] graph_module:acyc.._a_cyclic_component ... passed (0.000 sec)
% [38/324] graph_module:comp.._by_smallest_member ... passed (0.000 sec)
% [39/324] graph_module:comp..ontaining_component ... passed (0.000 sec)
% [40/324] graph_module:clos..ict_positive_length ... passed (0.000 sec)
% [41/324] graph_module:clos..s_a_node_on_a_cycle ... passed (0.000 sec)
% [42/324] graph_module:topo..ical_order_on_a_dag ... passed (0.000 sec)
% [43/324] graph_module:topo..er_fails_on_a_cycle ... passed (0.000 sec)
% [44/324] graph_module:topo..ails_on_a_self_loop ... passed (0.000 sec)
% [45/324] graph_module:isol..urvive_construction ... passed (0.000 sec)
% [46/324] graph_module:duplicate_edges_collapse .... passed (0.000 sec)
% [47/324] graph_module:comp..e_with_connectivity ... passed (0.000 sec)
% [48/324] diag_channel:one_based_to_zero_based ..... passed (0.000 sec)
% [49-1/324] diag_channel:json..e_equals_human_line .. passed (0.004 sec)
% [50/324] diag_channel:reco..round_trips_as_json ... passed (0.001 sec)
% [51/324] diag_channel:refu..not_earlier_mention ... passed (0.000 sec)
% [52/324] diag_channel:refu.._statement_position ... passed (0.000 sec)
% [53/324] diag_channel:pars.._is_exact_in_record ... passed (0.000 sec)
% [54/324] diag_channel:uri_..encoded_file_scheme ... passed (0.000 sec)
% [55/324] subscribe_cone:ze.._subscribes_nothing ... passed (0.000 sec)
% [56/324] subscribe_cone:hand_computed_cone ........ passed (0.000 sec)
% [57/324] subscribe_cone:sampler_included .......... passed (0.000 sec)
% [58/324] subscribe_cone:negation_included ......... passed (0.000 sec)
% [59/324] subscribe_cone:de..the_real_decl_forms ... passed (0.000 sec)
% [60/324] subscribe_cone:de..to_a_queryless_cone ... passed (0.000 sec)
% [61/324] subscribe_cone:ed..chain_including_pre ... passed (0.000 sec)
% [62/324] subscribe_cone:ne..and_combine_spliced ... passed (0.000 sec)
% [63/324] subscribe_cone:re..ides_what_a_read_is ... passed (0.000 sec)
% [64/324] subscribe_cone:go..lex_cone_invariants ... passed (0.296 sec)
% [65/324] subscribe_cone:em..ath_behind_the_flag ... passed (0.003 sec)
% [66/324] subscribe_cone:em..ents_name_their_rel ... passed (0.002 sec)
% [67/324] subscribe_cone:em.._hand_computed_cone ... passed (0.002 sec)
% [68/324] subscribe_cone:ze.._subscribes_nothing ... passed (0.002 sec)
% [69/324] stratum_order:swi..d_replace_one_group ... passed (0.001 sec)
% [70/324] stratum_order:dem.._laziness_one_group ... passed (0.001 sec)
% [71/324] stratum_order:swi.._replace_rule_order ... passed (0.001 sec)
% [72/324] stratum_order:dem..laziness_rule_order ... passed (0.001 sec)
% [73/324] stratum_order:sel..remains_in_p2_order ... passed (0.000 sec)
% [74/324] column_naming:swi..yed_replace_columns ... passed (0.001 sec)
% [75/324] column_naming:demand_laziness_columns .... passed (0.001 sec)
% [76/324] sql_text_snapshot..ed_replace_edge_sql ... passed (0.002 sec)
% [77/324] sql_text_snapshot..eplace_ddl_pk_shape ... passed (0.002 sec)
% [78/324] sql_text_snapshot..straint_and_replace ... passed (0.001 sec)
% [79/324] sql_text_snapshot..eplace_frontier_ddl ... passed (0.002 sec)
% [80/324] sql_text_snapshot..dered_snapshot_read ... passed (0.001 sec)
% [81/324] sql_text_snapshot.._mirrors_each_write ... passed (0.003 sec)
% [82/324] sql_text_snapshot..d_replace_level_sql ... passed (0.002 sec)
% [83/324] sql_text_snapshot..iness_no_edge_rules ... passed (0.001 sec)
% [84/324] sql_text_snapshot..one_batch_statement ... passed (0.001 sec)
% [85/324] sql_text_snapshot.._laziness_level_sql ... passed (0.001 sec)
% [86/324] sql_text_snapshot..s_promoted_frontier ... passed (0.001 sec)
% [87/324] sql_text_snapshot.._same_tick_frontier ... passed (0.001 sec)
% [88/324] sql_text_snapshot..l_column_expr_shape ... passed (0.000 sec)
% [89/324] sql_text_snapshot..elta_sql_open_scope ... passed (0.002 sec)
% [90/324] sql_text_snapshot..ql_route_change_log ... passed (0.002 sec)
% [91/324] sql_text_snapshot..n_both_sql_families ... passed (0.001 sec)
% [92/324] sql_text_snapshot.._departure_frontier ... passed (0.001 sec)
% [93/324] sql_text_snapshot..with_key_predicates ... passed (0.001 sec)
% [94/324] incremental_mode:..gram_is_incremental ... passed (0.002 sec)
% [95/324] incremental_mode:..cremental_reconcile ... passed (0.003 sec)
% [96/324] incremental_mode:..remental_carry_path ... passed (0.001 sec)
% [97/324] incremental_mode:..e_referee_available ... passed (0.003 sec)
% [98/324] incremental_mode:..tements_are_emitted ... passed (0.001 sec)
% [99/324] incremental_mode:..ecursive_cte_reseed ... passed (0.000 sec)
% [100/324] incremental_mode:..son_batch_statement .. passed (0.001 sec)
% [101/324] supported_subset_..ount_aggregate_head .. passed (0.000 sec)
% [102/324] supported_subset_.._variable_separator .. passed (0.000 sec)
% [103/324] supported_subset_..ate_non_int_ordinal .. passed (0.001 sec)
% [104/324] supported_subset_..gregate_wrong_arity .. passed (0.000 sec)
% [105/324] supported_subset_..rray_aggregate_head .. passed (0.000 sec)
% [106/324] supported_subset_..ject_aggregate_head .. passed (0.000 sec)
% [107/324] supported_subset_..ding_aggregate_head .. passed (0.000 sec)
% [108/324] supported_subset_..tern_on_arrival_rel .. passed (0.000 sec)
% [109/324] supported_subset_..tern_on_derived_rel .. passed (0.000 sec)
% [110/324] supported_subset_..eps_its_own_refusal .. passed (0.001 sec)
% [111/324] supported_subset_..tern_on_arrival_rel .. passed (0.000 sec)
% [112/324] supported_subset_..uard_under_negation .. passed (0.000 sec)
% [113/324] supported_subset_..d_atom_in_edge_body .. passed (0.000 sec)
% [114/324] supported_subset_..nction_in_edge_body .. passed (0.000 sec)
% [115/324] supported_subset_..d_bind_in_edge_body .. passed (0.000 sec)
% [116/324] supported_subset_..e_atom_in_edge_body .. passed (0.000 sec)
% [117/324] supported_subset_..ts_now_in_edge_body .. passed (0.000 sec)
% [118/324] supported_subset_..n_variable_argument .. passed (0.000 sec)
% [119/324] supported_subset_..s_now_in_level_rule .. passed (0.000 sec)
% [120/324] supported_subset_..typed_from_its_body .. passed (0.001 sec)
% [121/324] supported_subset_.._still_gets_a_table .. passed (0.000 sec)
% [122/324] supported_subset_..rival_fed_level_rel .. passed (0.000 sec)
% [123/324] supported_subset_.._plane_before_edges .. passed (0.004 sec)
% [124/324] supported_subset_.._edge_fed_level_rel .. passed (0.000 sec)
% [125/324] supported_subset_.._is_integer_storage .. passed (0.001 sec)
% [126/324] supported_subset_..sample_in_edge_body .. passed (0.000 sec)
% [127/324] supported_subset_..nction_in_edge_body .. passed (0.000 sec)
% [128/324] supported_subset_..on_level_headed_rel .. passed (0.000 sec)
% [129/324] supported_subset_..atest_in_level_rule .. passed (0.000 sec)
% [130/324] supported_subset_..s_pre_in_level_rule .. passed (0.000 sec)
% [131/324] supported_subset_..keep_on_non_log_rel .. passed (0.000 sec)
% [132/324] supported_subset_..erived_edge_trigger .. passed (0.000 sec)
% [133/324] supported_subset_..erived_edge_trigger .. passed (0.000 sec)
% [134/324] expression_miscom..t_not_text_collapse .. passed (0.001 sec)
% [135/324] expression_miscom..t_not_text_collapse .. passed (0.001 sec)
% [136/324] expression_miscom..t_column_stays_text .. passed (0.001 sec)
% [137/324] expression_miscom..eat_column_affinity .. passed (0.001 sec)
% [138/324] expression_miscom..mparison_is_refused .. passed (0.001 sec)
% [139/324] expression_miscom..ype_join_is_refused .. passed (0.001 sec)
% [140/324] expression_miscom..ype_reaches_the_ddl .. passed (0.001 sec)
% [141/324] expression_miscom.._floored_correction .. passed (0.001 sec)
% [142/324] enum_decl_expansi..semicolon_enum_decl ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1118:
Warning: [Thread main]     test enum_decl_expansion:parser_retains_semicolon_enum_decl: Test succeeded with choicepoint
% [143/324] enum_decl_expansi.._rels_and_tag_union .. passed (0.000 sec)
% [144/324] enum_decl_expansi..iant_name_collision .. passed (0.000 sec)
% [145/324] enum_decl_expansi..ger_keyed_edge_head ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1159:
Warning: [Thread main]     test enum_decl_expansion:enum_tag_view_can_trigger_keyed_edge_head: Test succeeded with choicepoint
% [146/324] match_block:share..dinary_rule_per_arm .. passed (0.000 sec)
% [147/324] match_block:enum_..uires_every_variant .. passed (0.000 sec)
% [148/324] match_block:keyed..med_compile_refusal .. passed (0.000 sec)
% [149/324] match_block:key_p..med_compile_refusal .. passed (0.000 sec)
% [150/324] match_block:key_p..med_compile_refusal .. passed (0.000 sec)
% [151/324] match_block:dupli..med_compile_refusal .. passed (0.000 sec)
% [152/324] match_block:keyed..d_remains_supported .. passed (0.000 sec)
% [153/324] match_block:match.._left_to_right_arms ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1244:
Warning: [Thread main]     test match_block:match_surface_round_trips_with_prefix_semicolon_and_left_to_right_arms: Test succeeded with choicepoint
% [154/324] match_block:match..ut_prefix_semicolon ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1264:
Warning: [Thread main]     test match_block:match_surface_allows_first_arm_without_prefix_semicolon: Test succeeded with choicepoint
% [155/324] match_block:seq_s.._parser_and_printer ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1275:
Warning: [Thread main]     test match_block:seq_surface_round_trips_through_parser_and_printer: Test succeeded with choicepoint
% [156/324] match_block:match.._first_arm_spelling .. passed (0.000 sec)
% [157/324] match_block:sugar..er_to_identical_sql .. passed (0.003 sec)
% [158/324] match_block:reten..ed_delete_statement .. passed (0.000 sec)
% [159/324] hosts_wiring:sele..surface_round_trips ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1314:
Warning: [Thread main]     test hosts_wiring:selected_surface_round_trips: Test succeeded with choicepoint
% [160/324] hosts_wiring:host..ull_type_vocabulary ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1360:
Warning: [Thread main]     test hosts_wiring:host_input_and_bind_columns_read_the_full_type_vocabulary: Test succeeded with choicepoint
% [161/324] hosts_wiring:host..ill_a_named_refusal ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1375:
Warning: [Thread main]     test hosts_wiring:host_input_column_wrapper_is_still_a_named_refusal: Test succeeded with choicepoint
% [162/324] hosts_wiring:rhs_.._marker_is_rejected .. passed (0.000 sec)
% [163/324] hosts_wiring:rhs_.._marker_is_rejected .. passed (0.000 sec)
% [164/324] hosts_wiring:plai..n_order_independent ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1397:
Warning: [Thread main]     test hosts_wiring:plain_host_resolution_is_declaration_order_independent: Test succeeded with choicepoint
% [165/324] hosts_wiring:remo..surface_is_rejected .. passed (0.000 sec)
% [166/324] hosts_wiring:plai..mains_relation_atom ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1421:
Warning: [Thread main]     test hosts_wiring:plain_non_host_rhs_remains_relation_atom: Test succeeded with choicepoint
% [167/324] hosts_wiring:plai..sting_named_refusal .. passed (0.000 sec)
% [168/324] hosts_wiring:remo..keyword_is_rejected .. passed (0.000 sec)
% [169/324] hosts_wiring:refe..arks_reference_edge ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1440:
Warning: [Thread main]     test hosts_wiring:referenced_rel_remains_queryable_and_marks_reference_edge: Test succeeded with choicepoint
% [170/324] hosts_wiring:name..omissions_are_fresh ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1452:
Warning: [Thread main]     test hosts_wiring:named_body_omissions_are_fresh: Test succeeded with choicepoint
% [171/324] hosts_wiring:name..ial_head_is_refused ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1464:
Warning: [Thread main]     test hosts_wiring:named_partial_head_is_refused: Test succeeded with choicepoint
% [172/324] hosts_wiring:host..enced_input_refusal .. passed (0.000 sec)
% [173/324] hosts_wiring:host..bsent_from_template .. passed (0.000 sec)
% [174/324] hosts_wiring:host..ypes_not_name_alone .. passed (0.000 sec)
% [175/324] hosts_wiring:host..t_reference_refusal .. passed (0.000 sec)
% [176/324] hosts_wiring:host..nown_column_refusal .. passed (0.000 sec)
% [177/324] hosts_wiring:host..ame_is_not_a_column .. passed (0.000 sec)
% [178/324] hosts_wiring:extr..iler_known_executor .. passed (0.000 sec)
% [179/324] hosts_wiring:name..e_selected_executor .. passed (0.000 sec)
% [180/324] hosts_wiring:extr..uses_non_path_input .. passed (0.000 sec)
% [181/324] hosts_wiring:host_overlap_refusal ....... passed (0.000 sec)
% [182/324] hosts_wiring:host..cate_column_refusal .. passed (0.000 sec)
% [183/324] hosts_wiring:host..s_and_lowers_as_ref .. passed (0.005 sec)
% [184/324] hosts_wiring:host..efuses_by_type_name .. passed (0.001 sec)
% [185/324] hosts_wiring:probe_arity_refusal ........ passed (0.000 sec)
% [186/324] hosts_wiring:bind..d_rule_head_refusal .. passed (0.000 sec)
% [187/324] hosts_wiring:nati..ts_query_exact_text .. passed (0.000 sec)
% [188/324] hosts_wiring:emit..lans_and_demand_sql .. passed (0.009 sec)
% [189/324] hosts_wiring:quer..and_bound_positions .. passed (0.002 sec)
% [190/324] hosts_wiring:back..low_the_stated_rule ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1741:
Warning: [Thread main]     test hosts_wiring:backslash_escapes_follow_the_stated_rule: Test succeeded with choicepoint
% [191/324] hosts_wiring:back..s_print_and_reparse ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:1763:
Warning: [Thread main]     test hosts_wiring:backslash_survives_print_and_reparse: Test succeeded with choicepoint
% [192/324] hosts_wiring:rese.._the_generated_ones .. passed (0.000 sec)
% [193/324] hosts_wiring:ever..umn_refuses_by_name .. passed (0.000 sec)
% [194/324] hosts_wiring:unpr..lares_its_relations .. passed (0.000 sec)
% [195/324] body_walk_charact..ation:body_ref_uses .. passed (0.000 sec)
% [196/324] body_walk_charact..n:conjunction_goals .. passed (0.000 sec)
% [197/324] body_walk_charact..ation:trigger_items .. passed (0.000 sec)
% [198/324] body_walk_charact..ngine_finalize_refs .. passed (0.000 sec)
% [199/324] body_walk_charact..ngine_latest_refs .. passed (0.000 sec)
% [200/324] body_walk_charact..ion:engine_pre_refs .. passed (0.000 sec)
% [201/324] body_walk_charact..analyze_latest_refs .. passed (0.000 sec)
% [202/324] body_walk_charact..on:analyze_pre_refs .. passed (0.000 sec)
% [203/324] body_walk_charact..ation:goal_rel_refs .. passed (0.000 sec)
% [204/324] body_walk_characterization:body_atoms ... passed (0.000 sec)
% [205/324] body_walk_charact..reserved_constructs .. passed (0.000 sec)
% [206/324] body_walk_charact..ion:forbidden_goals .. passed (0.000 sec)
% [207/324] body_walk_charact..ion:host_body_goals .. passed (0.000 sec)
% [208/324] body_walk_charact.._latest_scans_agree .. passed (0.000 sec)
% [209/324] body_walk_charact..ler_pre_scans_agree .. passed (0.000 sec)
% [210/324] body_walk_charact.._agree_across_doors .. passed (0.000 sec)
% [211/324] body_walk_charact..atches_the_registry .. passed (0.000 sec)
% [212/324] cross_plane_check..of_range_both_doors .. passed (0.000 sec)
% [213/324] cross_plane_check..uplicate_both_doors .. passed (0.000 sec)
% [214/324] cross_plane_check..vel_head_both_doors .. passed (0.000 sec)
% [215/324] cross_plane_check..ds_differ_by_design .. passed (0.000 sec)
% [216/324] cross_plane_check..aded_rel_both_doors .. passed (0.000 sec)
% [217/324] cross_plane_check.._log_rel_both_doors .. passed (0.000 sec)
% [218/324] cross_plane_check..ict_risk_both_doors .. passed (0.000 sec)
% [219/324] cross_plane_check..epted_at_both_doors .. passed (0.000 sec)
% [220/324] cross_plane_check..fuses_at_both_doors .. passed (0.000 sec)
% [221/324] cross_plane_check..rd_refusal_payloads .. passed (0.000 sec)
% [222/324] cross_plane_check..fuses_at_both_doors .. passed (0.000 sec)
% [223/324] cross_plane_check..vel_rule_both_doors .. passed (0.000 sec)
% [224/324] cross_plane_check..vel_rule_both_doors .. passed (0.000 sec)
% [225/324] cross_plane_check..agrees_across_doors .. passed (0.000 sec)
% [226/324] cross_plane_check..d_not_latest_parity .. passed (0.000 sec)
% [227/324] cross_plane_check..sted_not_pre_parity .. passed (0.000 sec)
% [228/324] cross_plane_check..fused_by_both_doors .. passed (0.000 sec)
% [229/324] cross_plane_check..g_without_retention .. passed (0.000 sec)
% [230/324] cross_plane_check..regate_in_edge_head .. passed (0.000 sec)
% [231/324] cross_plane_check.._at_the_oracle_door .. passed (0.000 sec)
% [232/324] cross_plane_check..program_at_lowering .. passed (0.001 sec)
% [233/324] cross_plane_check.._engine_value_guard .. passed (0.000 sec)
% [234/324] cross_plane_check..epted_at_both_doors .. passed (0.000 sec)
% [235/324] cross_plane_check..lumn_stays_accepted .. passed (0.000 sec)
% [236/324] cross_plane_check..epted_by_both_doors .. passed (0.000 sec)
% [237/324] refusal_messages:..al_renders_one_line .. passed (0.002 sec)
% [238/324] declaration_query..agrees_across_doors .. passed (0.000 sec)
% [239/324] declaration_query..agrees_across_doors .. passed (0.000 sec)
% [240/324] expansion_order:declared_phase_order .... passed (0.000 sec)
% [241/324] expansion_order:s..es_are_placeholders .. passed (0.000 sec)
% [242/324] expansion_order:s..r_rule_cursor_block .. passed (0.000 sec)
% [243/324] expansion_order:s..vel_rule_is_refused .. passed (0.000 sec)
% [244/324] expansion_order:c..reads_the_bare_atom .. passed (0.000 sec)
% [245/324] expansion_order:c..stead_of_triggering .. passed (0.000 sec)
% [246/324] expansion_order:c..on_spine_is_refused .. passed (0.000 sec)
% [247/324] expansion_order:l..s_target_membership .. passed (0.000 sec)
% [248/324] expansion_order:e..s_target_membership .. passed (0.000 sec)
% [249/324] expansion_order:e..p_is_not_duplicated .. passed (0.000 sec)
% [250/324] expansion_order:e..rves_expanded_terms .. passed (0.000 sec)
% [251/324] expansion_order:e..nonexhaustive_match .. passed (0.000 sec)
% [252/324] expansion_order:c..erated_variant_refs .. passed (0.000 sec)
% [253/324] expansion_order:p..m_has_empty_context .. passed (0.000 sec)
% [254/324] expansion_order:d.._and_match_together .. passed (0.000 sec)
% [255/324] expression_invent..y_the_expected_rows .. passed (0.000 sec)
% [256/324] expression_invent..s_with_surface_rows .. passed (0.000 sec)
% [257/324] expression_invent..recognizer_is_total .. passed (0.000 sec)
% [258/324] expression_invent..c_row_lowers_to_sql .. passed (0.000 sec)
% [259/324] expression_invent..wers_sign_corrected .. passed (0.000 sec)
% [260/324] expression_invent..ii_character_filter .. passed (0.000 sec)
% [261/324] expression_invent..ses_integer_operand .. passed (0.000 sec)
% [262/324] expression_invent..to_its_sql_operator .. passed (0.000 sec)
% [263/324] expression_invent..omes_from_the_table .. passed (0.000 sec)
% [264/324] phase5_value_plan..ut_surface_wrappers ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:3076:
Warning: [Thread main]     test phase5_value_plane:parser_and_printer_round_trip_bool_and_float_without_surface_wrappers: Test succeeded with choicepoint
% [265/324] phase5_value_plan..ool_literal_witness .. passed (0.000 sec)
% [266/324] phase5_value_plan..nstraints_are_exact .. passed (0.001 sec)
% [267/324] phase5_value_plan..ite_real_operations .. passed (0.001 sec)
% [268/324] phase5_value_plan.._scan_state_numeric .. passed (0.000 sec)
% [269/324] oracle_aggregate_..an_oracle_aggregate .. passed (0.000 sec)
% [270/324] oracle_aggregate_..ered_aggregate_rows .. passed (0.000 sec)
% [271/324] oracle_aggregate_..hree_distinct_roles ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:3206:
Warning: [Thread main]     test oracle_aggregate_classification:aggregate_axis_carries_three_distinct_roles: Test succeeded with choicepoint
% [272/324] oracle_aggregate_.._live_in_the_oracle .. passed (0.000 sec)
% [273/324] oracle_aggregate_..is_not_an_aggregate .. passed (0.000 sec)
% [274/324] oracle_aggregate_..rgument_stays_plain .. passed (0.000 sec)
% [275/324] relation_depth_lo..negation_is_refused .. passed (0.000 sec)
% [276/324] relation_depth_lo..dge_rule_is_refused .. passed (0.000 sec)
% [277/324] relation_depth_lo.._one_join_per_level .. passed (0.001 sec)
% [278/324] relation_depth_lo.._no_dictionary_join ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lab-gh304/v6/prolog/compile/test/plunit_tests.pl:3361:
Warning: [Thread main]     test relation_depth_lowering:head_value_that_is_a_body_atom_needs_no_dictionary_join: Test succeeded with choicepoint
% [279/324] json_grammar:unqu..entifier_keys_parse .. passed (0.000 sec)
% [280/324] json_grammar:trai.._comma_is_not_taken .. passed (0.000 sec)
% [281/324] json_grammar:both..s_give_the_same_key .. passed (0.000 sec)
% [282/324] json_grammar:quot.._is_a_literal_label .. passed (0.000 sec)
% [283/324] json_grammar:dollar_marks_a_key_hole .... passed (0.000 sec)
% [284/324] json_grammar:doll..the_bare_identifier .. passed (0.000 sec)
% [285/324] json_grammar:desc.._to_the_quoted_atom .. passed (0.000 sec)
% [286/324] json_grammar:gh_cache_flagship_parses ... passed (0.000 sec)
% [287/324] json_grammar:empt..the_arity_zero_atom .. passed (0.000 sec)
% [288/324] json_grammar:empt..y_is_the_empty_list .. passed (0.000 sec)
% [289/324] json_grammar:tagg..ith_a_named_refusal .. passed (0.000 sec)
% [290/324] json_grammar:unde..ith_a_named_refusal .. passed (0.000 sec)
% [291/324] json_grammar:ever..duction_round_trips .. passed (0.002 sec)
% [292/324] json_grammar:non_..r_key_prints_quoted .. passed (0.000 sec)
% [293/324] json_grammar:type..s_to_a_nested_colon .. passed (0.000 sec)
% [294/324] json_grammar:typed_capture_round_trips .. passed (0.001 sec)
% [295/324] json_grammar:unty..till_prints_untyped .. passed (0.000 sec)
% [296/324] json_grammar:capt.._agree_across_doors .. passed (0.000 sec)
% [297/324] json_grammar:unkn..sed_by_the_compiler .. passed (0.000 sec)
% [298/324] json_grammar:unkn..fused_by_the_oracle .. passed (0.000 sec)
% [299/324] json_grammar:orac..ir_json_type_answer .. passed (0.000 sec)
% [300-1/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-2/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-3/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-4/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-5/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-6/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-7/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-8/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-9/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-10/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-11/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-12/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-13/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-14/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-15/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-16/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-17/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-18/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-19/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-20/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-21/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-22/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-23/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-24/324] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [300-25/324] parse_error_posit..l_position_is_exact .. passed (0.003 sec)
% [301/324] parse_error_posit.._with_a_prefix_walk .. passed (0.000 sec)
% [302/324] dot_member_access..s_to_a_dot_get_nest .. passed (0.000 sec)
% [303/324] dot_member_access..rses_to_one_dot_get .. passed (0.000 sec)
% [304/324] dot_member_access..es_to_the_same_nest .. passed (0.000 sec)
% [305/324] dot_member_access..ot_still_terminates .. passed (0.000 sec)
% [306/324] dot_member_access..tatement_terminator .. passed (0.000 sec)
% [307/324] dot_member_access.._terminator_reading .. passed (0.000 sec)
% [308/324] dot_member_access..tays_a_syntax_error .. passed (0.000 sec)
% [309/324] dot_member_access..rals_are_unaffected .. passed (0.000 sec)
% [310/324] dot_member_access..through_the_printer .. passed (0.000 sec)
% [311/324] dot_member_access..through_the_printer .. passed (0.000 sec)
% [312/324] dot_member_access.._to_a_nested_decode .. passed (0.000 sec)
% [313/324] dot_member_access..e_author_could_type .. passed (0.000 sec)
% [314/324] dot_member_access..e_brace_decode_goal .. passed (0.001 sec)
% [315/324] dot_member_access..des_after_that_atom .. passed (0.000 sec)
% [316/324] dot_member_access..des_before_the_bind .. passed (0.000 sec)
% [317/324] dot_member_access..goal_still_resolves .. passed (0.000 sec)
% [318/324] dot_member_access..it_and_never_writes .. passed (0.000 sec)
% [319/324] dot_member_access.._returned_unchanged .. passed (0.000 sec)
% [320/324] dot_member_access..ind_refuses_by_name .. passed (0.000 sec)
% [321/324] dot_member_access..ead_refuses_by_name .. passed (0.000 sec)
% [322/324] dot_member_access..with_the_whole_path .. passed (0.000 sec)
% [323/324] dot_member_access..on_is_a_parse_error .. passed (0.000 sec)
% [324/324] dot_member_access..oal_refuses_by_name .. passed (0.000 sec)
```
