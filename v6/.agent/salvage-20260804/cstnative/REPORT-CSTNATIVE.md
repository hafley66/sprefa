# CST native syntax lane report

Worktree: `/Users/chrishafley/projects/sprefa-codex-cstnative`
Base: `f76ef6f4025130c2b73664ee2a61d9d0f9811d37`
No commit or merge was run.

Implemented under `v6/prolog/**`:

- Native `cst(path, digest, lang) { ... }` parsing with the existing query
  functors: `node/2`, `field/2`, `capture/2`, `group/2`, `quant/2`,
  `alternative/1`, `anonymous/1`, `predicate/3`, `capture_ref/1`,
  `wildcard`, `named_wildcard`, and `string/1`.
- Canonical serializer and CST normalization through the existing `ast/4`
  expansion.
- Capture and variable refusals.
- `#match?` and `#not-match?` checks through the existing regexp subset
  predicates.
- Fixture round-trip coverage and printer coverage.
- Malformed CST query blocks raise `dl_parse_error(cst_query, position(...))`.

Focused native suite:

```text
% [41/41] hosts_wiring:... passed
EXIT_CODE=0
```

Required gates, verbatim result lines:

```text
$ cd v6 && just conformance
PASS  tightened_baseline_catches_regrowth
EXIT_CODE=0
```

```text
$ cd v6 && just text-door
TEXT_DOOR compiled=417 byte_identical=416 failures=1
  TEXT_DOOR_FAIL json_nfc_and_nfd_keys_stay_distinct error(io_error(write,<stream>(0x6000035d5f00)),context(system:format/3,'Encoding cannot represent character'))
error: recipe `text-door` failed on line 48 with exit code 1
EXIT_CODE=1
```

```text
$ cd v6 && just plunit
% [54/345] diag_channel:uri_..encoded_file_scheme .. **FAILED (0.000 sec)
ERROR: [Thread main]     test diag_channel:uri_is_percent_encoded_file_scheme: failed
error: recipe `plunit` failed on line 53 with exit code 1
EXIT_CODE=1
```

The two failing required gates are outside the CST changes. The text-door
failure is an encoding error in `json_nfc_and_nfd_keys_stay_distinct`. The
plunit failure is `diag_channel:uri_is_percent_encoded_file_scheme`.

## Full gate output

The following blocks contain the captured stdout and stderr from the required
commands.

### `cd v6 && just conformance`

```text
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
cd /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/conformance && /Users/chrishafley/projects/sprefa-codex-cstnative/v6/tools/run-capped.sh "${CONFORMANCE_BUDGET_S:-300}" swipl -q -l go.pl -g go -g halt
bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
perl: warning: Setting locale failed.
perl: warning: Please check that your locale settings:
	LC_ALL = "C.UTF-8",
	LC_TERMINAL = "iTerm2",
	LC_CTYPE = "C.UTF-8",
	LANG = "C.UTF-8"
    are supported and installed on your system.
perl: warning: Falling back to the standard locale ("C").
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
PASS  regexp_positive_match
PASS  regexp_non_match
PASS  regexp_retraction_flip
PASS  regexp_pattern_not_literal
PASS  regexp_operand_not_text
PASS  regexp_pattern_outside_subset
PASS  regexp_pattern_invalid
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
PASS  edge_trigger_literal_filters_on_the_oracle_door
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
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
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

### `cd v6 && just text-door`

```text
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
cd /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog && bash compile/scripts/text_door_receipt.sh
bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
perl: warning: Setting locale failed.
perl: warning: Please check that your locale settings:
	LC_ALL = "C.UTF-8",
	LC_TERMINAL = "iTerm2",
	LC_CTYPE = "C.UTF-8",
	LANG = "C.UTF-8"
    are supported and installed on your system.
perl: warning: Falling back to the standard locale ("C").
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=0/0 plan=4/45359 lower=0/2547 boot=0/194 emit=2/10465 write=0/92 total=6/58657
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=0/0 plan=1/7626 lower=0/2543 boot=0/194 emit=2/10461 write=0/91 total=3/20915
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=2/4911 plan=0/7626 lower=1/2543 boot=0/194 emit=1/10461 write=0/91 total=4/25826
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=0/0 plan=1/7626 lower=1/2543 boot=0/194 emit=1/10461 write=0/91 total=3/20915
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=0/972 plan=1/7626 lower=0/2543 boot=0/194 emit=1/10461 write=0/91 total=2/21887
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/0 plan=0/7828 lower=1/2544 boot=0/194 emit=1/11601 write=0/91 total=2/22258
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/0 plan=1/7626 lower=0/2544 boot=0/194 emit=2/11601 write=0/91 total=3/22056
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/982 plan=1/7626 lower=0/2544 boot=0/194 emit=2/11601 write=0/91 total=3/23038
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/0 plan=1/7626 lower=0/2544 boot=0/194 emit=2/11601 write=0/91 total=3/22056
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/982 plan=1/7626 lower=0/2544 boot=0/194 emit=2/11601 write=0/91 total=3/23038
COMPILE-TRACE program=match_classify_response parse=0/0 plan=1/15199 lower=0/4794 boot=0/227 emit=3/22400 write=1/91 total=5/42711
COMPILE-TRACE program=match_classify_response parse=0/0 plan=1/14573 lower=1/4794 boot=0/227 emit=2/22400 write=1/91 total=5/42085
COMPILE-TRACE program=match_classify_response parse=0/6688 plan=1/14573 lower=1/4794 boot=0/227 emit=3/22400 write=0/91 total=5/48773
COMPILE-TRACE program=match_classify_response parse=0/0 plan=1/14573 lower=0/4794 boot=0/227 emit=3/22400 write=0/91 total=4/42085
COMPILE-TRACE program=match_classify_response parse=1/6687 plan=1/14573 lower=1/4794 boot=0/227 emit=3/22400 write=0/91 total=6/48772
COMPILE-TRACE program=match_classify_response_desugared parse=0/0 plan=0/15111 lower=1/4794 boot=0/227 emit=3/22400 write=0/91 total=4/42623
COMPILE-TRACE program=match_classify_response_desugared parse=0/0 plan=1/14643 lower=1/4794 boot=0/227 emit=3/22400 write=0/91 total=5/42155
COMPILE-TRACE program=match_classify_response_desugared parse=1/15665 plan=1/14643 lower=1/4794 boot=0/227 emit=3/22400 write=0/91 total=6/57820
COMPILE-TRACE program=match_classify_response_desugared parse=0/0 plan=1/14643 lower=1/4794 boot=0/227 emit=2/22400 write=1/91 total=5/42155
COMPILE-TRACE program=match_classify_response_desugared parse=1/15665 plan=1/14643 lower=1/4794 boot=0/227 emit=2/22400 write=1/91 total=6/57820
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=0/0 plan=1/4860 lower=0/1055 boot=0/158 emit=1/7991 write=1/91 total=3/14155
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=0/0 plan=1/4883 lower=0/1061 boot=0/160 emit=1/8044 write=0/91 total=2/14239
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=0/3000 plan=1/4883 lower=0/1061 boot=0/160 emit=1/8044 write=0/91 total=2/17239
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=0/0 plan=1/4883 lower=0/1061 boot=0/160 emit=1/8044 write=0/91 total=2/14239
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=1/3000 plan=0/4883 lower=0/1061 boot=0/160 emit=2/8044 write=0/91 total=3/17239
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=0/0 plan=0/4813 lower=0/1055 boot=0/158 emit=1/8147 write=1/91 total=2/14264
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=0/0 plan=1/4836 lower=0/1061 boot=0/160 emit=1/8201 write=0/91 total=2/14349
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=1/4256 plan=0/4836 lower=0/1061 boot=0/160 emit=1/8201 write=1/91 total=3/18605
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=0/0 plan=0/4836 lower=0/1061 boot=0/160 emit=1/8201 write=1/91 total=2/14349
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=0/4256 plan=0/4836 lower=1/1061 boot=0/160 emit=1/8201 write=0/91 total=2/18605
COMPILE-TRACE program=extraction_fork_callgraph parse=0/0 plan=2/43057 lower=3/16610 boot=0/393 emit=6/54968 write=1/91 total=12/115119
COMPILE-TRACE program=extraction_fork_callgraph parse=0/0 plan=2/42294 lower=3/16619 boot=0/283 emit=6/54852 write=0/91 total=11/114139
COMPILE-TRACE program=extraction_fork_callgraph parse=2/22431 plan=2/42294 lower=3/16619 boot=0/283 emit=6/54852 write=1/91 total=14/136570
COMPILE-TRACE program=extraction_fork_callgraph parse=0/0 plan=2/42294 lower=3/16619 boot=0/283 emit=5/54852 write=1/91 total=11/114139
COMPILE-TRACE program=extraction_fork_callgraph parse=1/22431 plan=3/42294 lower=2/16619 boot=1/283 emit=5/54852 write=1/91 total=13/136570
COMPILE-TRACE program=extraction_fork_span_line parse=0/0 plan=2/23189 lower=1/8581 boot=0/354 emit=4/38286 write=1/91 total=8/70501
COMPILE-TRACE program=extraction_fork_span_line parse=0/0 plan=1/22655 lower=2/8590 boot=0/250 emit=4/38167 write=0/91 total=7/69753
COMPILE-TRACE program=extraction_fork_span_line parse=1/10962 plan=1/22655 lower=2/8590 boot=0/250 emit=4/38167 write=0/91 total=8/80715
COMPILE-TRACE program=extraction_fork_span_line parse=0/0 plan=1/22655 lower=1/8590 boot=0/250 emit=5/38167 write=0/91 total=7/69753
COMPILE-TRACE program=extraction_fork_span_line parse=1/10962 plan=2/22655 lower=1/8590 boot=0/250 emit=4/38167 write=1/91 total=9/80715
COMPILE-TRACE program=native_ts_query_term parse=0/0 plan=2/26168 lower=1/9476 boot=0/472 emit=5/41302 write=0/91 total=8/77509
COMPILE-TRACE program=native_ts_query_term parse=0/0 plan=2/25794 lower=1/9482 boot=0/301 emit=5/41076 write=1/91 total=9/76744
COMPILE-TRACE program=native_ts_query_term parse=1/19101 plan=2/25794 lower=1/9482 boot=0/301 emit=5/41076 write=0/91 total=9/95845
COMPILE-TRACE program=native_ts_query_term parse=0/0 plan=1/25794 lower=2/9482 boot=0/301 emit=4/41076 write=1/91 total=8/76744
COMPILE-TRACE program=native_ts_query_term parse=1/19101 plan=2/25794 lower=2/9482 boot=0/301 emit=4/41076 write=0/91 total=9/95845
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=0/0 plan=1/14826 lower=0/5380 boot=1/224 emit=2/23394 write=0/91 total=4/43915
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=0/0 plan=1/13682 lower=1/5380 boot=0/224 emit=2/23394 write=1/91 total=5/42771
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=1/15223 plan=0/13682 lower=1/5380 boot=0/224 emit=3/23394 write=0/91 total=5/57994
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=0/0 plan=1/13682 lower=1/5380 boot=0/224 emit=2/23394 write=1/91 total=5/42771
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=0/15223 plan=1/13682 lower=1/5380 boot=0/224 emit=3/23394 write=0/91 total=5/57994
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=0/0 plan=1/12376 lower=0/3316 boot=0/223 emit=3/21145 write=0/91 total=4/37151
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=0/0 plan=1/11149 lower=1/3316 boot=0/223 emit=2/21145 write=1/91 total=5/35924
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=0/11411 plan=1/11149 lower=1/3316 boot=0/223 emit=2/21145 write=0/91 total=4/47335
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=0/0 plan=1/11149 lower=0/3316 boot=0/223 emit=3/21145 write=0/91 total=4/35924
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=1/11411 plan=0/11149 lower=1/3316 boot=0/223 emit=2/21145 write=1/91 total=5/47335
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=0/0 plan=1/13797 lower=1/4756 boot=0/206 emit=3/27637 write=1/91 total=6/46487
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=0/0 plan=1/13433 lower=1/4756 boot=0/206 emit=3/27637 write=0/91 total=5/46123
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=1/20551 plan=1/13433 lower=1/4756 boot=0/206 emit=3/27637 write=0/91 total=6/66674
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=0/0 plan=1/13433 lower=1/4756 boot=0/206 emit=3/27637 write=0/91 total=5/46123
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=1/20551 plan=1/13433 lower=1/4756 boot=0/206 emit=3/27637 write=1/91 total=7/66674
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=0/0 plan=1/2583 lower=0/890 boot=0/187 emit=1/6690 write=0/91 total=2/10441
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=0/0 plan=0/2444 lower=0/732 boot=0/187 emit=1/6690 write=0/91 total=1/10144
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=0/1397 plan=1/2444 lower=0/732 boot=0/187 emit=1/6690 write=0/91 total=2/11541
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=0/0 plan=0/2444 lower=0/732 boot=0/187 emit=1/6690 write=0/91 total=1/10144
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=1/1397 plan=0/2444 lower=0/732 boot=0/187 emit=1/6690 write=0/91 total=2/11541
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/0 plan=0/5831 lower=1/1794 boot=0/217 emit=1/10365 write=0/91 total=2/18298
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/0 plan=1/5538 lower=0/1794 boot=0/217 emit=1/10365 write=1/91 total=3/18005
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/4258 plan=0/5538 lower=1/1794 boot=0/217 emit=1/10365 write=0/91 total=2/22263
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/0 plan=1/5538 lower=0/1794 boot=0/217 emit=1/10365 write=1/91 total=3/18005
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/4258 plan=0/5538 lower=1/1794 boot=0/217 emit=1/10365 write=1/91 total=3/22263
COMPILE-TRACE program=struct_intern_order_a parse=0/0 plan=0/2515 lower=1/697 boot=0/186 emit=0/5426 write=1/91 total=2/8915
COMPILE-TRACE program=struct_intern_order_a parse=0/0 plan=0/2289 lower=0/697 boot=0/186 emit=1/5426 write=0/91 total=1/8689
COMPILE-TRACE program=struct_intern_order_a parse=1/1253 plan=0/2289 lower=0/697 boot=0/186 emit=1/5426 write=0/91 total=2/9942
COMPILE-TRACE program=struct_intern_order_a parse=0/0 plan=0/2289 lower=0/697 boot=0/186 emit=1/5426 write=0/91 total=1/8689
COMPILE-TRACE program=struct_intern_order_a parse=0/1253 plan=0/2289 lower=0/697 boot=0/186 emit=1/5426 write=1/91 total=2/9942
COMPILE-TRACE program=struct_intern_order_b parse=0/0 plan=0/2515 lower=1/697 boot=0/186 emit=0/5426 write=1/91 total=2/8915
COMPILE-TRACE program=struct_intern_order_b parse=0/0 plan=0/2289 lower=0/697 boot=0/186 emit=1/5426 write=0/91 total=1/8689
COMPILE-TRACE program=struct_intern_order_b parse=1/1253 plan=0/2289 lower=0/697 boot=0/186 emit=1/5426 write=0/91 total=2/9942
COMPILE-TRACE program=struct_intern_order_b parse=0/0 plan=0/2289 lower=0/697 boot=0/186 emit=1/5426 write=0/91 total=1/8689
COMPILE-TRACE program=struct_intern_order_b parse=0/1253 plan=0/2289 lower=0/697 boot=0/186 emit=1/5426 write=0/91 total=1/9942
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=0/0 plan=1/8281 lower=0/2786 boot=0/268 emit=2/14841 write=0/91 total=3/26267
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=0/0 plan=1/8070 lower=0/2785 boot=0/268 emit=2/14841 write=0/91 total=3/26055
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=1/5932 plan=1/8070 lower=0/2785 boot=0/268 emit=2/14841 write=0/91 total=4/31987
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=0/0 plan=1/8070 lower=1/2785 boot=0/268 emit=1/14841 write=1/91 total=4/26055
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=0/5932 plan=1/8070 lower=0/2785 boot=0/268 emit=2/14841 write=0/91 total=3/31987
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/0 plan=1/7277 lower=0/2519 boot=0/218 emit=2/12650 write=0/91 total=3/22755
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/0 plan=1/6976 lower=0/2519 boot=0/218 emit=2/12650 write=0/91 total=3/22454
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/5735 plan=1/6976 lower=0/2519 boot=0/218 emit=2/12650 write=0/91 total=3/28189
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/0 plan=1/6976 lower=0/2519 boot=0/218 emit=2/12650 write=0/91 total=3/22454
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/5735 plan=1/6976 lower=1/2519 boot=0/218 emit=1/12650 write=1/91 total=4/28189
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=0/0 plan=1/7972 lower=0/2560 boot=0/221 emit=1/12195 write=1/91 total=3/23039
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=0/0 plan=1/7608 lower=0/2560 boot=0/221 emit=2/12195 write=0/91 total=3/22675
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=1/6593 plan=0/7608 lower=1/2560 boot=0/221 emit=1/12195 write=1/91 total=4/29268
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=0/0 plan=0/7608 lower=1/2560 boot=0/221 emit=1/12195 write=1/91 total=3/22675
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=0/6593 plan=1/7608 lower=0/2560 boot=0/221 emit=2/12195 write=0/91 total=3/29268
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=0/0 plan=1/22818 lower=2/7596 boot=0/359 emit=3/31538 write=1/91 total=7/62402
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=0/0 plan=2/22460 lower=1/7596 boot=0/308 emit=4/31441 write=0/91 total=7/61896
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=1/11274 plan=1/22460 lower=2/7596 boot=0/308 emit=3/31441 write=1/91 total=8/73170
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=0/0 plan=1/22460 lower=2/7596 boot=0/308 emit=4/31441 write=0/91 total=7/61896
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=1/11274 plan=2/22460 lower=1/7596 boot=0/308 emit=4/31441 write=0/91 total=8/73170
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/0 plan=1/2985 lower=0/732 boot=0/187 emit=1/6645 write=0/91 total=2/10640
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/0 plan=1/2444 lower=0/732 boot=0/187 emit=1/6645 write=0/91 total=2/10099
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/1387 plan=0/2444 lower=1/732 boot=0/187 emit=1/6645 write=0/91 total=2/11486
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/0 plan=0/2444 lower=0/732 boot=0/187 emit=1/6645 write=0/91 total=1/10099
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/1387 plan=0/2444 lower=1/732 boot=0/187 emit=0/6645 write=1/91 total=2/11486
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=0/0 plan=1/5593 lower=0/1835 boot=0/217 emit=1/8894 write=1/91 total=3/16630
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=0/0 plan=1/5466 lower=0/1835 boot=0/217 emit=1/8894 write=1/91 total=3/16503
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=0/4440 plan=0/5466 lower=1/1835 boot=0/217 emit=1/8894 write=0/91 total=2/20943
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=0/0 plan=1/5466 lower=0/1835 boot=0/217 emit=1/8894 write=1/91 total=3/16503
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=0/4440 plan=0/5466 lower=1/1835 boot=0/217 emit=1/8894 write=0/91 total=2/20943
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/0 plan=0/4593 lower=0/1485 boot=0/167 emit=1/8002 write=1/91 total=2/14338
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/0 plan=0/4458 lower=1/1485 boot=0/167 emit=1/8002 write=0/91 total=2/14203
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/3443 plan=1/4458 lower=0/1485 boot=0/167 emit=1/8002 write=0/91 total=2/17646
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/0 plan=0/4458 lower=1/1485 boot=0/167 emit=1/8002 write=0/91 total=2/14203
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/3443 plan=1/4458 lower=0/1485 boot=0/167 emit=1/8002 write=0/91 total=2/17646
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=0/0 plan=0/4658 lower=0/1646 boot=1/167 emit=0/6568 write=1/91 total=2/13130
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=0/0 plan=0/4523 lower=1/1646 boot=0/167 emit=1/6568 write=0/91 total=2/12995
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=0/3639 plan=1/4523 lower=0/1646 boot=0/167 emit=1/6568 write=0/91 total=2/16634
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=0/0 plan=0/4523 lower=0/1646 boot=0/167 emit=1/6568 write=0/91 total=1/12995
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=1/3639 plan=0/4523 lower=0/1646 boot=0/167 emit=1/6568 write=1/91 total=3/16634
COMPILE-TRACE program=probe_output_comparison_guard parse=0/0 plan=1/14597 lower=1/5488 boot=0/270 emit=2/23208 write=1/91 total=5/43654
COMPILE-TRACE program=probe_output_comparison_guard parse=0/0 plan=1/14149 lower=1/5488 boot=0/225 emit=3/23126 write=0/91 total=5/43079
COMPILE-TRACE program=probe_output_comparison_guard parse=0/6533 plan=1/14149 lower=1/5488 boot=0/225 emit=3/23126 write=0/91 total=5/49612
COMPILE-TRACE program=probe_output_comparison_guard parse=0/0 plan=1/14149 lower=1/5488 boot=0/225 emit=3/23126 write=0/91 total=5/43079
COMPILE-TRACE program=probe_output_comparison_guard parse=0/6533 plan=1/14149 lower=1/5488 boot=0/225 emit=3/23126 write=1/91 total=6/49612
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=0/0 plan=1/4777 lower=0/1667 boot=0/165 emit=1/7610 write=0/91 total=2/14310
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=0/0 plan=0/4676 lower=0/1667 boot=0/165 emit=1/7610 write=1/91 total=2/14209
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=0/3678 plan=0/4676 lower=1/1667 boot=0/165 emit=1/7610 write=0/91 total=2/17887
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=0/0 plan=0/4676 lower=0/1667 boot=0/165 emit=1/7610 write=0/91 total=1/14209
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=1/3678 plan=0/4676 lower=0/1667 boot=0/165 emit=1/7610 write=1/91 total=3/17887
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=0/0 plan=1/9348 lower=0/3254 boot=0/283 emit=2/13496 write=1/91 total=4/26472
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=0/0 plan=1/9158 lower=0/3254 boot=0/227 emit=2/13389 write=0/91 total=3/26119
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=1/9258 plan=1/9158 lower=0/3254 boot=0/227 emit=2/13389 write=0/91 total=4/35377
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=0/0 plan=0/9158 lower=1/3254 boot=0/227 emit=2/13389 write=0/91 total=3/26119
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=1/9258 plan=0/9158 lower=1/3254 boot=0/227 emit=2/13389 write=0/91 total=4/35377
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=0/0 plan=1/15367 lower=1/4610 boot=0/222 emit=2/24493 write=1/91 total=5/44783
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=0/0 plan=1/13785 lower=1/4610 boot=0/222 emit=3/24493 write=0/91 total=5/43201
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=1/16495 plan=1/13785 lower=1/4610 boot=0/222 emit=3/24493 write=0/91 total=6/59696
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=0/0 plan=1/13785 lower=1/4610 boot=0/222 emit=2/24493 write=1/91 total=5/43201
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=1/16495 plan=0/13785 lower=1/4610 boot=0/222 emit=3/24493 write=0/91 total=5/59696
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=0/0 plan=2/28352 lower=2/9947 boot=0/279 emit=4/38414 write=1/91 total=9/77083
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=0/0 plan=2/26897 lower=1/9947 boot=0/279 emit=5/38414 write=0/91 total=8/75628
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=2/30308 plan=2/26897 lower=1/9947 boot=0/279 emit=4/38414 write=1/91 total=10/105936
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=0/0 plan=1/26897 lower=2/9947 boot=0/279 emit=5/38414 write=0/91 total=8/75628
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=2/30308 plan=2/26897 lower=2/9947 boot=0/279 emit=4/38414 write=0/91 total=10/105936
COMPILE-TRACE program=bool_literals_round_trip parse=0/0 plan=0/1690 lower=0/432 boot=0/183 emit=0/3839 write=0/91 total=0/6235
COMPILE-TRACE program=bool_literals_round_trip parse=0/0 plan=0/1543 lower=0/432 boot=0/136 emit=1/3738 write=0/91 total=1/5940
COMPILE-TRACE program=bool_literals_round_trip parse=0/811 plan=0/1543 lower=0/432 boot=0/136 emit=1/3738 write=0/91 total=1/6751
COMPILE-TRACE program=bool_literals_round_trip parse=0/0 plan=0/1543 lower=0/432 boot=0/136 emit=1/3738 write=0/91 total=1/5940
COMPILE-TRACE program=bool_literals_round_trip parse=0/811 plan=0/1543 lower=1/432 boot=0/136 emit=0/3738 write=0/91 total=1/6751
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/0 plan=0/5187 lower=1/1717 boot=0/266 emit=1/7954 write=0/91 total=2/15215
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/0 plan=0/5036 lower=0/1717 boot=0/166 emit=1/7753 write=1/91 total=2/14763
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/4453 plan=0/5036 lower=1/1717 boot=0/166 emit=1/7753 write=0/91 total=2/19216
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/0 plan=0/5036 lower=1/1717 boot=0/166 emit=0/7753 write=1/91 total=2/14763
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/4453 plan=0/5036 lower=1/1717 boot=0/166 emit=1/7753 write=0/91 total=2/19216
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=0/0 plan=1/6935 lower=0/1986 boot=0/378 emit=2/10727 write=0/91 total=3/20117
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=0/0 plan=0/6659 lower=1/1986 boot=0/188 emit=1/10347 write=0/91 total=2/19271
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=1/5264 plan=0/6659 lower=1/1986 boot=0/188 emit=1/10347 write=0/91 total=3/24535
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=0/0 plan=1/6659 lower=0/1986 boot=0/188 emit=2/10347 write=0/91 total=3/19271
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=0/5264 plan=1/6659 lower=0/1986 boot=0/188 emit=1/10347 write=1/91 total=3/24535
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=0/0 plan=0/4558 lower=1/1570 boot=0/217 emit=1/8896 write=0/91 total=2/15332
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=0/0 plan=0/4476 lower=0/1569 boot=0/167 emit=1/8795 write=1/91 total=2/15098
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=0/4151 plan=1/4476 lower=0/1569 boot=0/167 emit=1/8795 write=1/91 total=3/19249
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=0/0 plan=1/4476 lower=0/1569 boot=0/167 emit=1/8795 write=0/91 total=2/15098
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=1/4150 plan=0/4476 lower=0/1569 boot=0/167 emit=1/8795 write=1/91 total=3/19248
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=0/0 plan=1/4911 lower=0/1623 boot=0/229 emit=1/9348 write=0/91 total=2/16202
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=0/0 plan=0/4801 lower=1/1623 boot=0/168 emit=1/9225 write=0/91 total=2/15908
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=0/5014 plan=1/4801 lower=0/1623 boot=0/168 emit=1/9225 write=0/91 total=2/20922
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=0/0 plan=1/4801 lower=0/1623 boot=0/168 emit=1/9225 write=1/91 total=3/15908
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=0/5014 plan=1/4801 lower=0/1623 boot=0/168 emit=1/9225 write=0/91 total=2/20922
COMPILE-TRACE program=float_avg_is_grouped parse=0/0 plan=0/4737 lower=1/1801 boot=0/321 emit=1/9917 write=0/91 total=2/16867
COMPILE-TRACE program=float_avg_is_grouped parse=0/0 plan=0/4492 lower=0/1801 boot=0/171 emit=2/9623 write=0/91 total=2/16178
COMPILE-TRACE program=float_avg_is_grouped parse=0/4221 plan=0/4492 lower=1/1801 boot=0/171 emit=1/9623 write=0/91 total=2/20399
COMPILE-TRACE program=float_avg_is_grouped parse=0/0 plan=1/4492 lower=0/1801 boot=0/171 emit=1/9623 write=1/91 total=3/16178
COMPILE-TRACE program=float_avg_is_grouped parse=0/4221 plan=0/4492 lower=1/1801 boot=0/171 emit=1/9623 write=0/91 total=2/20399
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=0/0 plan=0/5163 lower=1/1729 boot=0/266 emit=1/8085 write=0/91 total=2/15334
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=0/0 plan=1/5008 lower=0/1729 boot=0/166 emit=1/7883 write=0/91 total=2/14877
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=1/4478 plan=0/5008 lower=0/1729 boot=0/166 emit=1/7883 write=1/91 total=3/19355
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=0/0 plan=0/5008 lower=0/1729 boot=0/166 emit=1/7883 write=1/91 total=2/14877
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=0/4478 plan=0/5008 lower=1/1729 boot=0/166 emit=1/7883 write=0/91 total=2/19355
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=0/0 plan=1/6779 lower=0/2338 boot=0/401 emit=2/11759 write=0/91 total=3/21368
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=0/0 plan=0/6437 lower=1/2338 boot=0/189 emit=1/11361 write=0/91 total=2/20416
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=1/6159 plan=0/6437 lower=1/2338 boot=0/189 emit=1/11361 write=1/91 total=4/26575
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=0/0 plan=0/6437 lower=1/2338 boot=0/189 emit=1/11361 write=1/91 total=3/20416
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=0/6159 plan=0/6437 lower=1/2338 boot=0/189 emit=1/11361 write=0/91 total=2/26575
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=0/0 plan=0/1472 lower=0/397 boot=0/171 emit=1/2901 write=0/91 total=1/5032
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=0/0 plan=0/1422 lower=0/397 boot=1/135 emit=0/2823 write=0/91 total=1/4868
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=0/698 plan=1/1422 lower=0/397 boot=0/135 emit=0/2823 write=0/91 total=1/5566
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/4868
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=1/698 plan=0/1422 lower=0/397 boot=0/135 emit=0/2823 write=4/91 total=5/5566
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/0 plan=0/1523 lower=0/397 boot=0/171 emit=0/2901 write=1/91 total=1/5083
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=1/91 total=2/4868
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/698 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/5566
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/4868
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/698 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/5566
COMPILE-TRACE program=float_shortest_round_trip_wire parse=0/0 plan=0/1574 lower=0/397 boot=0/171 emit=0/2923 write=1/91 total=1/5156
COMPILE-TRACE program=float_shortest_round_trip_wire parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=1/2843 write=0/91 total=1/4888
COMPILE-TRACE program=float_shortest_round_trip_wire parse=0/702 plan=0/1422 lower=1/397 boot=0/135 emit=0/2843 write=0/91 total=1/5590
COMPILE-TRACE program=float_shortest_round_trip_wire parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=1/2843 write=0/91 total=1/4888
COMPILE-TRACE program=float_shortest_round_trip_wire parse=0/702 plan=0/1422 lower=0/397 boot=0/135 emit=1/2843 write=0/91 total=1/5590
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/0 plan=0/5005 lower=0/1801 boot=0/321 emit=1/9917 write=0/91 total=1/17135
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/0 plan=0/4492 lower=0/1801 boot=0/171 emit=2/9623 write=0/91 total=2/16178
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/4221 plan=1/4492 lower=0/1801 boot=0/171 emit=1/9623 write=1/91 total=3/20399
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/0 plan=1/4492 lower=0/1801 boot=0/171 emit=1/9623 write=1/91 total=3/16178
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/4221 plan=0/4492 lower=1/1801 boot=0/171 emit=1/9623 write=0/91 total=2/20399
COMPILE-TRACE program=int_accepts_integral_float parse=0/0 plan=0/1474 lower=1/397 boot=0/171 emit=0/2535 write=0/91 total=1/4668
COMPILE-TRACE program=int_accepts_integral_float parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=0/2453 write=1/91 total=1/4498
COMPILE-TRACE program=int_accepts_integral_float parse=0/694 plan=0/1422 lower=0/397 boot=0/135 emit=1/2453 write=0/91 total=1/5192
COMPILE-TRACE program=int_accepts_integral_float parse=0/0 plan=1/1422 lower=0/397 boot=0/135 emit=0/2453 write=1/91 total=2/4498
COMPILE-TRACE program=int_accepts_integral_float parse=0/694 plan=0/1422 lower=0/397 boot=0/135 emit=0/2453 write=1/91 total=1/5192
COMPILE-TRACE program=float_widens_integer_ingress parse=0/0 plan=0/1473 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/4919
COMPILE-TRACE program=float_widens_integer_ingress parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=0/2823 write=1/91 total=1/4868
COMPILE-TRACE program=float_widens_integer_ingress parse=0/698 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/5566
COMPILE-TRACE program=float_widens_integer_ingress parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/4868
COMPILE-TRACE program=float_widens_integer_ingress parse=0/698 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/5566
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/0 plan=0/1489 lower=0/397 boot=0/171 emit=0/2901 write=1/91 total=1/5049
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/4868
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/698 plan=0/1422 lower=0/397 boot=0/135 emit=0/2823 write=1/91 total=1/5566
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=0/2823 write=1/91 total=1/4868
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/698 plan=0/1422 lower=0/397 boot=0/135 emit=1/2823 write=0/91 total=1/5566
COMPILE-TRACE program=head_column_int_widens_into_float parse=0/0 plan=1/3898 lower=0/1301 boot=0/204 emit=1/5821 write=0/91 total=2/11315
COMPILE-TRACE program=head_column_int_widens_into_float parse=0/0 plan=1/3842 lower=0/1301 boot=0/165 emit=1/5741 write=0/91 total=2/11140
COMPILE-TRACE program=head_column_int_widens_into_float parse=0/2907 plan=1/3842 lower=0/1301 boot=0/165 emit=1/5741 write=0/91 total=2/14047
COMPILE-TRACE program=head_column_int_widens_into_float parse=0/0 plan=0/3842 lower=0/1301 boot=0/165 emit=1/5741 write=0/91 total=1/11140
COMPILE-TRACE program=head_column_int_widens_into_float parse=0/2907 plan=1/3842 lower=0/1301 boot=0/165 emit=1/5741 write=0/91 total=2/14047
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=0/0 plan=0/3895 lower=0/1301 boot=0/244 emit=1/5554 write=0/91 total=1/11085
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=0/0 plan=0/3841 lower=0/1301 boot=0/165 emit=1/5461 write=0/91 total=1/10859
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=1/2961 plan=0/3841 lower=0/1301 boot=0/165 emit=1/5461 write=0/91 total=2/13820
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=0/0 plan=0/3841 lower=1/1301 boot=0/165 emit=0/5461 write=1/91 total=2/10859
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=0/2961 plan=0/3841 lower=0/1301 boot=0/165 emit=0/5461 write=1/91 total=1/13820
COMPILE-TRACE program=relation_depth2_construct_and_read parse=0/0 plan=2/28931 lower=1/12832 boot=0/369 emit=4/30349 write=1/91 total=8/72572
COMPILE-TRACE program=relation_depth2_construct_and_read parse=0/0 plan=2/28722 lower=1/12832 boot=0/369 emit=3/30349 write=1/91 total=7/72363
COMPILE-TRACE program=relation_depth2_construct_and_read parse=2/25564 plan=2/28722 lower=1/12832 boot=0/369 emit=4/30349 write=0/91 total=9/97927
COMPILE-TRACE program=relation_depth2_construct_and_read parse=0/0 plan=2/28722 lower=2/12832 boot=0/369 emit=3/30349 write=1/91 total=8/72363
COMPILE-TRACE program=relation_depth2_construct_and_read parse=2/25564 plan=1/28722 lower=2/12832 boot=0/369 emit=4/30349 write=0/91 total=9/97927
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=0/0 plan=2/30898 lower=2/15162 boot=0/397 emit=4/31618 write=0/91 total=8/78166
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=0/0 plan=1/30696 lower=3/15162 boot=0/397 emit=3/31618 write=1/91 total=8/77964
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=2/29346 plan=2/30696 lower=2/15162 boot=0/397 emit=4/31618 write=0/91 total=10/107310
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=0/0 plan=2/30696 lower=2/15162 boot=0/397 emit=4/31618 write=1/91 total=9/77964
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=2/29346 plan=1/30696 lower=2/15162 boot=0/397 emit=4/31618 write=1/91 total=10/107310
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=0/0 plan=2/29297 lower=2/12832 boot=0/369 emit=4/30349 write=0/91 total=8/72938
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=0/0 plan=2/28722 lower=2/12832 boot=0/369 emit=3/30349 write=1/91 total=8/72363
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=2/25564 plan=2/28722 lower=1/12832 boot=0/369 emit=4/30349 write=0/91 total=9/97927
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=0/0 plan=2/28722 lower=2/12832 boot=0/369 emit=3/30349 write=1/91 total=8/72363
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=1/25564 plan=2/28722 lower=2/12832 boot=0/369 emit=3/30349 write=1/91 total=9/97927
COMPILE-TRACE program=relation_depth2_chained_decode parse=0/0 plan=2/31599 lower=1/12993 boot=0/369 emit=4/30525 write=0/91 total=7/75577
COMPILE-TRACE program=relation_depth2_chained_decode parse=0/0 plan=1/31390 lower=2/12993 boot=0/369 emit=4/30525 write=0/91 total=7/75368
COMPILE-TRACE program=relation_depth2_chained_decode parse=2/25747 plan=2/31390 lower=1/12993 boot=0/369 emit=4/30525 write=0/91 total=9/101115
COMPILE-TRACE program=relation_depth2_chained_decode parse=0/0 plan=2/31390 lower=2/12993 boot=0/369 emit=4/30525 write=0/91 total=8/75368
COMPILE-TRACE program=relation_depth2_chained_decode parse=2/25747 plan=2/31390 lower=2/12993 boot=0/369 emit=4/30525 write=0/91 total=10/101115
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=0/0 plan=2/30347 lower=1/12853 boot=0/369 emit=4/30452 write=0/91 total=7/74112
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=0/0 plan=1/30108 lower=2/12853 boot=0/369 emit=4/30452 write=0/91 total=7/73873
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=2/25748 plan=2/30108 lower=2/12853 boot=0/369 emit=4/30452 write=0/91 total=10/99621
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=0/0 plan=2/30108 lower=1/12853 boot=0/369 emit=4/30452 write=0/91 total=7/73873
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=2/25748 plan=2/30108 lower=2/12853 boot=0/369 emit=4/30452 write=0/91 total=10/99621
COMPILE-TRACE program=relation_depth2_dot_read parse=0/0 plan=2/30345 lower=2/12853 boot=0/369 emit=3/30452 write=1/91 total=8/74110
COMPILE-TRACE program=relation_depth2_dot_read parse=0/0 plan=2/30106 lower=2/12853 boot=0/369 emit=3/30452 write=0/91 total=7/73871
COMPILE-TRACE program=relation_depth2_dot_read parse=2/23785 plan=2/30106 lower=2/12853 boot=0/369 emit=4/30452 write=0/91 total=10/97656
COMPILE-TRACE program=relation_depth2_dot_read parse=0/0 plan=2/30106 lower=2/12853 boot=0/369 emit=4/30452 write=0/91 total=8/73871
COMPILE-TRACE program=relation_depth2_dot_read parse=2/23785 plan=1/30106 lower=2/12853 boot=0/369 emit=4/30452 write=0/91 total=9/97656
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=0/0 plan=2/30456 lower=1/12853 boot=0/369 emit=4/30452 write=1/91 total=8/74221
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=0/0 plan=2/30217 lower=1/12853 boot=0/369 emit=4/30452 write=0/91 total=7/73982
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=2/24409 plan=2/30217 lower=2/12853 boot=0/369 emit=3/30452 write=1/91 total=10/98391
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=0/0 plan=1/30217 lower=2/12853 boot=0/369 emit=4/30452 write=1/91 total=8/73982
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=2/24409 plan=2/30217 lower=1/12853 boot=1/369 emit=3/30452 write=1/91 total=10/98391
COMPILE-TRACE program=relation_depth3_construct_and_read parse=0/0 plan=2/37564 lower=2/18913 boot=0/432 emit=4/39788 write=1/91 total=9/96788
COMPILE-TRACE program=relation_depth3_construct_and_read parse=0/0 plan=2/37321 lower=3/18913 boot=0/432 emit=5/39788 write=0/91 total=10/96545
COMPILE-TRACE program=relation_depth3_construct_and_read parse=3/35299 plan=2/37321 lower=2/18913 boot=0/432 emit=5/39788 write=0/91 total=12/131844
COMPILE-TRACE program=relation_depth3_construct_and_read parse=0/0 plan=2/37321 lower=2/18913 boot=0/432 emit=5/39788 write=1/91 total=10/96545
COMPILE-TRACE program=relation_depth3_construct_and_read parse=3/35299 plan=2/37321 lower=2/18913 boot=0/432 emit=5/39788 write=0/91 total=12/131844
COMPILE-TRACE program=relation_depth3_chained_decode parse=0/0 plan=2/49214 lower=3/22528 boot=0/463 emit=5/46040 write=1/91 total=11/118336
COMPILE-TRACE program=relation_depth3_chained_decode parse=0/0 plan=3/48957 lower=3/22528 boot=0/463 emit=5/46040 write=1/91 total=12/118079
COMPILE-TRACE program=relation_depth3_chained_decode parse=3/42102 plan=2/48957 lower=3/22528 boot=0/463 emit=5/46040 write=1/91 total=14/160181
COMPILE-TRACE program=relation_depth3_chained_decode parse=0/0 plan=3/48957 lower=2/22528 boot=0/463 emit=6/46040 write=1/91 total=12/118079
COMPILE-TRACE program=relation_depth3_chained_decode parse=3/42102 plan=3/48957 lower=3/22528 boot=0/463 emit=5/46040 write=1/91 total=15/160181
COMPILE-TRACE program=relation_depth3_many_rows parse=0/0 plan=3/37992 lower=2/18913 boot=0/432 emit=4/39788 write=1/91 total=10/97216
COMPILE-TRACE program=relation_depth3_many_rows parse=0/0 plan=2/37321 lower=2/18913 boot=0/432 emit=5/39788 write=0/91 total=9/96545
COMPILE-TRACE program=relation_depth3_many_rows parse=3/35299 plan=2/37321 lower=3/18913 boot=0/432 emit=4/39788 write=0/91 total=12/131844
COMPILE-TRACE program=relation_depth3_many_rows parse=0/0 plan=3/37321 lower=2/18913 boot=0/432 emit=5/39788 write=0/91 total=10/96545
COMPILE-TRACE program=relation_depth3_many_rows parse=3/35299 plan=2/37321 lower=2/18913 boot=1/432 emit=4/39788 write=1/91 total=13/131844
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=0/0 plan=1/12241 lower=0/4681 boot=0/251 emit=1/14535 write=1/91 total=3/31799
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=0/0 plan=1/12134 lower=0/4681 boot=0/251 emit=2/14535 write=1/91 total=4/31692
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=0/9648 plan=1/12134 lower=1/4681 boot=0/251 emit=2/14535 write=0/91 total=4/41340
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=0/0 plan=1/12134 lower=1/4681 boot=0/251 emit=2/14535 write=0/91 total=4/31692
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=1/9648 plan=1/12134 lower=0/4681 boot=0/251 emit=2/14535 write=1/91 total=5/41340
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=0/0 plan=1/10831 lower=1/3882 boot=0/328 emit=2/14355 write=0/91 total=4/29487
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=0/0 plan=1/10626 lower=0/3882 boot=0/191 emit=2/14061 write=0/91 total=3/28851
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=1/4727 plan=0/10626 lower=1/3882 boot=0/191 emit=2/14061 write=0/91 total=4/33578
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=0/0 plan=1/10626 lower=0/3882 boot=0/191 emit=2/14061 write=0/91 total=3/28851
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=1/4727 plan=0/10626 lower=1/3882 boot=0/191 emit=1/14061 write=1/91 total=4/33578
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=0/0 plan=1/10871 lower=1/3882 boot=0/233 emit=1/14145 write=0/91 total=3/29222
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=0/0 plan=1/10626 lower=1/3882 boot=0/191 emit=1/14061 write=1/91 total=4/28851
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=0/4727 plan=1/10626 lower=1/3882 boot=0/191 emit=1/14061 write=1/91 total=4/33578
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=0/0 plan=1/10626 lower=0/3882 boot=0/191 emit=2/14061 write=0/91 total=3/28851
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=1/4727 plan=0/10626 lower=1/3882 boot=0/191 emit=2/14061 write=0/91 total=4/33578
COMPILE-TRACE program=coalesce_over_derived_source parse=0/0 plan=1/15242 lower=0/5291 boot=1/469 emit=1/13341 write=1/91 total=4/34434
COMPILE-TRACE program=coalesce_over_derived_source parse=0/0 plan=1/14810 lower=1/5291 boot=0/222 emit=1/12921 write=1/91 total=4/33335
COMPILE-TRACE program=coalesce_over_derived_source parse=1/8711 plan=1/14810 lower=1/5291 boot=0/222 emit=1/12921 write=0/91 total=4/42046
COMPILE-TRACE program=coalesce_over_derived_source parse=0/0 plan=1/14810 lower=0/5291 boot=0/222 emit=2/12921 write=0/91 total=3/33335
COMPILE-TRACE program=coalesce_over_derived_source parse=1/8711 plan=1/14810 lower=1/5291 boot=0/222 emit=1/12921 write=1/91 total=5/42046
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/0 plan=1/12715 lower=0/2698 boot=0/238 emit=2/9005 write=0/91 total=3/24747
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/0 plan=1/12277 lower=0/2698 boot=1/185 emit=1/8905 write=1/91 total=4/24156
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/4947 plan=1/12277 lower=1/2698 boot=0/185 emit=1/8905 write=0/91 total=3/29103
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/0 plan=1/12277 lower=1/2698 boot=0/185 emit=1/8905 write=0/91 total=3/24156
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/4947 plan=1/12277 lower=1/2698 boot=0/185 emit=1/8905 write=0/91 total=3/29103
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/0 plan=0/4123 lower=0/1303 boot=0/165 emit=1/6798 write=0/91 total=1/12480
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/0 plan=0/3830 lower=0/1303 boot=0/165 emit=1/6798 write=1/91 total=2/12187
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/2846 plan=1/3830 lower=0/1303 boot=0/165 emit=1/6798 write=1/91 total=3/15033
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/0 plan=1/3830 lower=0/1303 boot=0/165 emit=1/6798 write=0/91 total=2/12187
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/2846 plan=1/3830 lower=0/1303 boot=0/165 emit=1/6798 write=0/91 total=2/15033
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=0/0 plan=0/3920 lower=0/1301 boot=0/497 emit=1/5568 write=0/91 total=1/11377
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=0/0 plan=0/3826 lower=0/1301 boot=0/165 emit=1/5420 write=0/91 total=1/10803
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=1/2873 plan=0/3826 lower=0/1301 boot=0/165 emit=1/5420 write=0/91 total=2/13676
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=0/0 plan=0/3826 lower=1/1301 boot=0/165 emit=0/5420 write=1/91 total=2/10803
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=0/2873 plan=0/3826 lower=0/1301 boot=0/165 emit=1/5420 write=0/91 total=1/13676
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=0/0 plan=0/3931 lower=0/1301 boot=0/319 emit=1/5531 write=0/91 total=1/11173
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=0/0 plan=0/3826 lower=0/1301 boot=0/165 emit=1/5420 write=1/91 total=2/10803
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=0/2873 plan=0/3826 lower=0/1301 boot=0/165 emit=1/5420 write=0/91 total=1/13676
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=0/0 plan=0/3826 lower=1/1301 boot=0/165 emit=0/5420 write=1/91 total=2/10803
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=0/2873 plan=0/3826 lower=0/1301 boot=0/165 emit=1/5420 write=0/91 total=1/13676
COMPILE-TRACE program=json_empty_string_key_round_trips parse=0/0 plan=0/5124 lower=0/1799 boot=0/264 emit=1/7221 write=0/91 total=1/14499
COMPILE-TRACE program=json_empty_string_key_round_trips parse=0/0 plan=0/5041 lower=0/1799 boot=0/166 emit=1/7124 write=1/91 total=2/14221
COMPILE-TRACE program=json_empty_string_key_round_trips parse=0/4227 plan=0/5041 lower=1/1799 boot=0/166 emit=1/7124 write=0/91 total=2/18448
COMPILE-TRACE program=json_empty_string_key_round_trips parse=0/0 plan=0/5041 lower=0/1799 boot=0/166 emit=0/7124 write=1/91 total=1/14221
COMPILE-TRACE program=json_empty_string_key_round_trips parse=0/4227 plan=0/5041 lower=1/1799 boot=0/166 emit=1/7124 write=0/91 total=2/18448
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=0/0 plan=1/5137 lower=0/1799 boot=0/332 emit=1/7236 write=0/91 total=2/14595
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=0/0 plan=0/5041 lower=1/1799 boot=0/166 emit=1/7124 write=0/91 total=2/14221
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=0/4227 plan=1/5041 lower=0/1799 boot=0/166 emit=1/7124 write=0/91 total=2/18448
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=0/0 plan=1/5041 lower=0/1799 boot=0/166 emit=1/7124 write=0/91 total=2/14221
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=0/4227 plan=1/5041 lower=0/1799 boot=0/166 emit=1/7124 write=0/91 total=2/18448
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=0/0 plan=0/4684 lower=0/1429 boot=0/167 emit=1/7842 write=0/91 total=1/14213
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=0/0 plan=1/4424 lower=0/1429 boot=0/167 emit=1/7842 write=0/91 total=2/13953
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=1/3989 plan=0/4424 lower=0/1429 boot=0/167 emit=1/7842 write=1/91 total=3/17942
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=0/0 plan=1/4424 lower=0/1429 boot=0/167 emit=1/7842 write=0/91 total=2/13953
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=1/3989 plan=0/4424 lower=0/1429 boot=0/167 emit=1/7842 write=1/91 total=3/17942
COMPILE-TRACE program=json_empty_containers_nest parse=0/0 plan=0/3954 lower=1/1301 boot=0/502 emit=0/5561 write=1/91 total=2/11409
COMPILE-TRACE program=json_empty_containers_nest parse=0/0 plan=0/3826 lower=1/1301 boot=0/165 emit=1/5420 write=0/91 total=2/10803
COMPILE-TRACE program=json_empty_containers_nest parse=0/2873 plan=0/3826 lower=1/1301 boot=0/165 emit=0/5420 write=1/91 total=2/13676
COMPILE-TRACE program=json_empty_containers_nest parse=0/0 plan=1/3826 lower=0/1301 boot=0/165 emit=1/5420 write=0/91 total=2/10803
COMPILE-TRACE program=json_empty_containers_nest parse=0/2873 plan=0/3826 lower=1/1301 boot=0/165 emit=1/5420 write=0/91 total=2/13676
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=0/0 plan=1/5166 lower=0/2407 boot=0/518 emit=1/8365 write=0/91 total=2/16547
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=0/0 plan=1/5002 lower=0/2407 boot=0/165 emit=1/8231 write=0/91 total=2/15896
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=5/118918 plan=0/5002 lower=1/2407 boot=0/165 emit=1/8231 write=0/91 total=7/134814
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=0/0 plan=0/5002 lower=1/2407 boot=0/165 emit=1/8231 write=0/91 total=2/15896
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=5/118918 plan=0/5002 lower=1/2407 boot=0/165 emit=1/8231 write=0/91 total=7/134814
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=0/0 plan=0/8859 lower=1/2670 boot=0/221 emit=2/16593 write=0/91 total=3/28434
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=0/0 plan=1/8241 lower=0/2670 boot=0/221 emit=2/16593 write=1/91 total=4/27816
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=0/7668 plan=1/8241 lower=0/2670 boot=0/221 emit=2/16593 write=1/91 total=4/35484
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=0/0 plan=0/8241 lower=1/2670 boot=0/221 emit=2/16593 write=0/91 total=3/27816
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=1/7668 plan=0/8241 lower=1/2670 boot=0/221 emit=2/16593 write=0/91 total=4/35484
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=0/0 plan=1/7731 lower=0/2965 boot=0/168 emit=1/9663 write=1/91 total=3/20618
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=0/0 plan=0/7658 lower=1/2965 boot=0/168 emit=1/9663 write=0/91 total=2/20545
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=1/7135 plan=0/7658 lower=1/2965 boot=0/168 emit=1/9663 write=0/91 total=3/27680
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=0/0 plan=1/7658 lower=0/2965 boot=0/168 emit=2/9663 write=0/91 total=3/20545
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=0/7135 plan=1/7658 lower=0/2965 boot=0/168 emit=2/9663 write=0/91 total=3/27680
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=0/0 plan=1/7730 lower=1/3096 boot=0/576 emit=1/10183 write=0/91 total=3/21676
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=0/0 plan=0/7543 lower=1/3096 boot=0/167 emit=1/10034 write=1/91 total=3/20931
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=0/9434 plan=1/7543 lower=0/3096 boot=0/167 emit=1/10034 write=1/91 total=3/30365
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=0/0 plan=0/7543 lower=1/3096 boot=0/167 emit=1/10034 write=0/91 total=2/20931
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=1/9434 plan=0/7543 lower=1/3096 boot=0/167 emit=1/10034 write=0/91 total=3/30365
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=0/0 plan=1/15402 lower=1/4203 boot=0/190 emit=1/12915 write=1/91 total=4/32801
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=0/0 plan=1/15086 lower=1/4203 boot=0/190 emit=2/12915 write=0/91 total=4/32485
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=1/10364 plan=1/15086 lower=0/4203 boot=0/190 emit=2/12915 write=0/91 total=4/42849
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=0/0 plan=1/15086 lower=1/4203 boot=0/190 emit=2/12915 write=0/91 total=4/32485
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=1/10364 plan=1/15086 lower=1/4203 boot=0/190 emit=1/12915 write=1/91 total=5/42849
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=0/0 plan=0/5486 lower=0/1909 boot=0/168 emit=1/7686 write=1/91 total=2/15340
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=0/0 plan=1/5295 lower=0/1909 boot=0/168 emit=1/7686 write=0/91 total=2/15149
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=1/4860 plan=0/5295 lower=1/1909 boot=0/168 emit=1/7686 write=0/91 total=3/20009
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=0/0 plan=0/5295 lower=1/1909 boot=0/168 emit=1/7686 write=0/91 total=2/15149
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=0/4860 plan=1/5295 lower=0/1909 boot=0/168 emit=1/7686 write=1/91 total=3/20009
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/0 plan=1/4980 lower=0/1694 boot=0/167 emit=1/6717 write=0/91 total=2/13649
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/0 plan=0/4874 lower=0/1694 boot=0/167 emit=1/6717 write=1/91 total=2/13543
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/4092 plan=0/4874 lower=1/1694 boot=0/167 emit=0/6717 write=1/91 total=2/17635
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/0 plan=1/4874 lower=0/1694 boot=0/167 emit=1/6717 write=0/91 total=2/13543
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/4092 plan=0/4874 lower=0/1694 boot=0/167 emit=1/6717 write=0/91 total=1/17635
COMPILE-TRACE program=ordered_json_group_array_value parse=0/0 plan=0/4555 lower=1/1413 boot=0/313 emit=1/8920 write=0/91 total=2/15292
COMPILE-TRACE program=ordered_json_group_array_value parse=0/0 plan=0/4543 lower=0/1419 boot=0/165 emit=2/8644 write=0/91 total=2/14862
COMPILE-TRACE program=ordered_json_group_array_value parse=0/3732 plan=0/4543 lower=1/1419 boot=0/165 emit=1/8644 write=0/91 total=2/18594
COMPILE-TRACE program=ordered_json_group_array_value parse=0/0 plan=0/4543 lower=1/1419 boot=0/165 emit=1/8644 write=0/91 total=2/14862
COMPILE-TRACE program=ordered_json_group_array_value parse=0/3732 plan=1/4543 lower=0/1419 boot=0/165 emit=1/8644 write=0/91 total=2/18594
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=0/0 plan=0/4506 lower=1/1412 boot=0/263 emit=1/7959 write=0/91 total=2/14231
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=0/0 plan=0/4543 lower=0/1418 boot=0/165 emit=1/7812 write=1/91 total=2/14029
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=0/3733 plan=0/4543 lower=0/1418 boot=0/165 emit=2/7812 write=0/91 total=2/17762
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=0/0 plan=0/4543 lower=0/1418 boot=0/165 emit=1/7812 write=0/91 total=1/14029
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=1/3733 plan=0/4543 lower=0/1418 boot=0/165 emit=2/7812 write=0/91 total=3/17762
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/0 plan=1/4867 lower=0/1500 boot=0/346 emit=1/9398 write=1/91 total=3/16202
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/0 plan=1/4881 lower=0/1509 boot=0/166 emit=1/9094 write=1/91 total=3/15741
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/4749 plan=0/4881 lower=1/1509 boot=0/166 emit=1/9094 write=0/91 total=2/20490
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/0 plan=0/4881 lower=1/1509 boot=0/166 emit=1/9094 write=0/91 total=2/15741
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/4749 plan=1/4881 lower=0/1509 boot=0/166 emit=1/9094 write=0/91 total=2/20490
COMPILE-TRACE program=ordered_group_concat_value parse=0/0 plan=0/4577 lower=0/1452 boot=0/313 emit=2/9703 write=0/91 total=2/16136
COMPILE-TRACE program=ordered_group_concat_value parse=0/0 plan=1/4565 lower=0/1458 boot=0/165 emit=1/9430 write=0/91 total=2/15709
COMPILE-TRACE program=ordered_group_concat_value parse=1/3855 plan=0/4565 lower=0/1458 boot=1/165 emit=1/9430 write=0/91 total=3/19564
COMPILE-TRACE program=ordered_group_concat_value parse=0/0 plan=1/4565 lower=0/1458 boot=0/165 emit=1/9430 write=0/91 total=2/15709
COMPILE-TRACE program=ordered_group_concat_value parse=0/3855 plan=0/4565 lower=1/1458 boot=0/165 emit=1/9430 write=0/91 total=2/19564
COMPILE-TRACE program=ordered_group_concat_ordinal parse=0/0 plan=1/4889 lower=0/1539 boot=0/346 emit=1/10184 write=1/91 total=3/17049
COMPILE-TRACE program=ordered_group_concat_ordinal parse=0/0 plan=1/4903 lower=0/1548 boot=0/166 emit=1/9880 write=0/91 total=2/16588
COMPILE-TRACE program=ordered_group_concat_ordinal parse=1/4872 plan=0/4903 lower=0/1548 boot=0/166 emit=1/9880 write=1/91 total=3/21460
COMPILE-TRACE program=ordered_group_concat_ordinal parse=0/0 plan=0/4903 lower=1/1548 boot=0/166 emit=1/9880 write=0/91 total=2/16588
COMPILE-TRACE program=ordered_group_concat_ordinal parse=0/4872 plan=1/4903 lower=0/1548 boot=0/166 emit=1/9880 write=1/91 total=3/21460
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/0 plan=0/5180 lower=1/1500 boot=0/346 emit=1/9398 write=0/91 total=2/16515
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/0 plan=0/4881 lower=1/1509 boot=0/166 emit=1/9094 write=0/91 total=2/15741
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/4749 plan=1/4881 lower=0/1509 boot=0/166 emit=1/9094 write=1/91 total=3/20490
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/0 plan=0/4881 lower=0/1509 boot=0/166 emit=2/9094 write=0/91 total=2/15741
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/4749 plan=1/4881 lower=0/1509 boot=0/166 emit=1/9094 write=0/91 total=2/20490
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=0/0 plan=0/4723 lower=1/1420 boot=0/359 emit=1/8018 write=0/91 total=2/14611
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=0/0 plan=0/4553 lower=0/1420 boot=0/165 emit=1/7776 write=0/91 total=1/14005
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=1/3803 plan=0/4553 lower=0/1420 boot=0/165 emit=1/7776 write=1/91 total=3/17808
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=0/0 plan=1/4553 lower=0/1420 boot=0/165 emit=1/7776 write=0/91 total=2/14005
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=1/3803 plan=0/4553 lower=0/1420 boot=0/165 emit=1/7776 write=0/91 total=2/17808
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=0/0 plan=0/4895 lower=1/1539 boot=0/285 emit=1/10906 write=0/91 total=2/17716
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=0/0 plan=0/4981 lower=1/1548 boot=0/166 emit=1/10676 write=0/91 total=2/17462
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=1/5232 plan=0/4981 lower=1/1548 boot=0/166 emit=1/10676 write=0/91 total=3/22694
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=0/0 plan=1/4981 lower=0/1548 boot=0/166 emit=1/10676 write=1/91 total=3/17462
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=0/5232 plan=0/4981 lower=1/1548 boot=0/166 emit=1/10676 write=0/91 total=2/22694
COMPILE-TRACE program=ordered_fragment_line_assembly parse=0/0 plan=0/4943 lower=0/1539 boot=0/285 emit=2/11377 write=0/91 total=2/18235
COMPILE-TRACE program=ordered_fragment_line_assembly parse=0/0 plan=0/5029 lower=0/1548 boot=0/166 emit=2/11116 write=0/91 total=2/17950
COMPILE-TRACE program=ordered_fragment_line_assembly parse=0/5366 plan=1/5029 lower=0/1548 boot=0/166 emit=1/11116 write=1/91 total=3/23316
COMPILE-TRACE program=ordered_fragment_line_assembly parse=0/0 plan=0/5029 lower=0/1548 boot=0/166 emit=2/11116 write=0/91 total=2/17950
COMPILE-TRACE program=ordered_fragment_line_assembly parse=0/5366 plan=1/5029 lower=0/1548 boot=0/166 emit=1/11116 write=1/91 total=3/23316
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=0/0 plan=0/5315 lower=1/1540 boot=0/379 emit=1/13415 write=1/91 total=3/20740
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=0/0 plan=1/5375 lower=0/1552 boot=0/167 emit=2/12908 write=0/91 total=3/20093
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=0/6032 plan=1/5375 lower=0/1552 boot=0/167 emit=2/12908 write=0/91 total=3/26125
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=0/0 plan=0/5375 lower=1/1552 boot=0/167 emit=1/12908 write=0/91 total=2/20093
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=1/6032 plan=0/5375 lower=1/1552 boot=0/167 emit=1/12908 write=1/91 total=4/26125
COMPILE-TRACE program=ordered_group_rels_json_head parse=0/0 plan=1/5315 lower=0/1540 boot=0/379 emit=2/13570 write=0/91 total=3/20895
COMPILE-TRACE program=ordered_group_rels_json_head parse=0/0 plan=0/5375 lower=1/1552 boot=0/167 emit=1/13063 write=0/91 total=2/20248
COMPILE-TRACE program=ordered_group_rels_json_head parse=1/6043 plan=0/5375 lower=1/1552 boot=0/167 emit=1/13063 write=1/91 total=4/26291
COMPILE-TRACE program=ordered_group_rels_json_head parse=0/0 plan=0/5375 lower=0/1552 boot=0/167 emit=2/13063 write=0/91 total=2/20248
COMPILE-TRACE program=ordered_group_rels_json_head parse=1/6043 plan=0/5375 lower=0/1552 boot=0/167 emit=2/13063 write=0/91 total=3/26291
COMPILE-TRACE program=regexp_positive_match parse=0/0 plan=0/5071 lower=1/1607 boot=0/281 emit=1/7407 write=0/91 total=2/14457
COMPILE-TRACE program=regexp_positive_match parse=0/0 plan=0/4923 lower=1/1607 boot=0/164 emit=1/7149 write=0/91 total=2/13934
COMPILE-TRACE program=regexp_positive_match parse=0/2980 plan=0/4923 lower=1/1607 boot=0/164 emit=1/7149 write=0/91 total=2/16914
COMPILE-TRACE program=regexp_positive_match parse=0/0 plan=0/4923 lower=0/1607 boot=0/164 emit=1/7149 write=0/91 total=1/13934
COMPILE-TRACE program=regexp_positive_match parse=1/2980 plan=0/4923 lower=0/1607 boot=0/164 emit=1/7149 write=0/91 total=2/16914
COMPILE-TRACE program=regexp_non_match parse=0/0 plan=0/5023 lower=0/1607 boot=0/242 emit=1/7316 write=1/91 total=2/14279
COMPILE-TRACE program=regexp_non_match parse=0/0 plan=1/4925 lower=0/1607 boot=0/164 emit=1/7144 write=0/91 total=2/13931
COMPILE-TRACE program=regexp_non_match parse=1/2977 plan=0/4925 lower=0/1607 boot=0/164 emit=1/7144 write=1/91 total=3/16908
COMPILE-TRACE program=regexp_non_match parse=0/0 plan=0/4925 lower=0/1607 boot=0/164 emit=1/7144 write=0/91 total=1/13931
COMPILE-TRACE program=regexp_non_match parse=1/2977 plan=0/4925 lower=0/1607 boot=0/164 emit=1/7144 write=1/91 total=3/16908
COMPILE-TRACE program=regexp_retraction_flip parse=0/0 plan=1/5076 lower=0/1607 boot=0/203 emit=1/7235 write=0/91 total=2/14212
COMPILE-TRACE program=regexp_retraction_flip parse=0/0 plan=0/4923 lower=0/1607 boot=0/164 emit=1/7149 write=0/91 total=1/13934
COMPILE-TRACE program=regexp_retraction_flip parse=1/2980 plan=0/4923 lower=0/1607 boot=0/164 emit=1/7149 write=0/91 total=2/16914
COMPILE-TRACE program=regexp_retraction_flip parse=0/0 plan=1/4923 lower=0/1607 boot=0/164 emit=1/7149 write=0/91 total=2/13934
COMPILE-TRACE program=regexp_retraction_flip parse=0/2980 plan=1/4923 lower=0/1607 boot=0/164 emit=1/7149 write=0/91 total=2/16914
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/0 plan=1/4119 lower=0/1274 boot=0/165 emit=1/5429 write=0/91 total=2/11078
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/0 plan=1/4152 lower=0/1277 boot=0/166 emit=1/5461 write=0/91 total=2/11147
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/2788 plan=1/4152 lower=0/1277 boot=0/166 emit=1/5461 write=0/91 total=2/13935
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/0 plan=0/4152 lower=0/1277 boot=0/166 emit=1/5461 write=0/91 total=1/11147
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/2788 plan=1/4152 lower=0/1277 boot=0/166 emit=1/5461 write=0/91 total=2/13935
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/0 plan=1/4293 lower=0/1300 boot=0/164 emit=1/7236 write=0/91 total=2/13084
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/0 plan=0/4000 lower=0/1300 boot=0/164 emit=1/7236 write=0/91 total=1/12791
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/2529 plan=1/4000 lower=0/1300 boot=0/164 emit=1/7236 write=0/91 total=2/15320
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/0 plan=0/4000 lower=1/1300 boot=0/164 emit=0/7236 write=1/91 total=2/12791
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/2529 plan=0/4000 lower=0/1300 boot=0/164 emit=1/7236 write=1/91 total=2/15320
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/0 plan=0/6889 lower=1/2482 boot=0/184 emit=1/7490 write=0/91 total=2/17136
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/0 plan=0/6946 lower=0/2488 boot=0/186 emit=1/7556 write=1/91 total=2/17267
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/2489 plan=0/6946 lower=1/2488 boot=0/186 emit=1/7556 write=0/91 total=2/19756
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/0 plan=0/6946 lower=1/2488 boot=0/186 emit=1/7556 write=0/91 total=2/17267
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/2489 plan=1/6946 lower=0/2488 boot=0/186 emit=1/7556 write=1/91 total=3/19756
COMPILE-TRACE program=conjunction_level_control_for_combine parse=0/0 plan=1/6137 lower=0/2220 boot=0/184 emit=1/7417 write=0/91 total=2/16049
COMPILE-TRACE program=conjunction_level_control_for_combine parse=0/0 plan=0/6194 lower=1/2226 boot=0/186 emit=1/7483 write=0/91 total=2/16180
COMPILE-TRACE program=conjunction_level_control_for_combine parse=0/4544 plan=1/6194 lower=0/2226 boot=0/186 emit=1/7483 write=1/91 total=3/20724
COMPILE-TRACE program=conjunction_level_control_for_combine parse=0/0 plan=0/6194 lower=0/2226 boot=0/186 emit=1/7483 write=1/91 total=2/16180
COMPILE-TRACE program=conjunction_level_control_for_combine parse=0/4544 plan=0/6194 lower=1/2226 boot=0/186 emit=1/7483 write=0/91 total=2/20724
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=0/0 plan=1/7414 lower=0/1839 boot=0/177 emit=1/6650 write=1/91 total=3/16171
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=0/0 plan=1/7471 lower=0/1845 boot=0/179 emit=1/6716 write=0/91 total=2/16302
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=1/3060 plan=0/7471 lower=0/1845 boot=0/179 emit=1/6716 write=1/91 total=3/19362
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=0/0 plan=1/7471 lower=0/1845 boot=0/179 emit=1/6716 write=0/91 total=2/16302
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=0/3060 plan=1/7471 lower=0/1845 boot=0/179 emit=1/6716 write=1/91 total=3/19362
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/0 plan=1/6539 lower=0/1774 boot=0/177 emit=1/6661 write=0/91 total=2/15242
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/0 plan=0/6596 lower=0/1780 boot=0/179 emit=2/6727 write=0/91 total=2/15373
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/5117 plan=1/6596 lower=0/1780 boot=0/179 emit=1/6727 write=1/91 total=3/20490
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/0 plan=0/6596 lower=0/1780 boot=0/179 emit=2/6727 write=0/91 total=2/15373
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/5117 plan=1/6596 lower=0/1780 boot=0/179 emit=1/6727 write=0/91 total=2/20490
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/0 plan=0/4656 lower=0/1507 boot=0/163 emit=1/5167 write=0/91 total=1/11584
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/0 plan=1/4690 lower=0/1510 boot=0/164 emit=1/5199 write=0/91 total=2/11654
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/1484 plan=1/4690 lower=0/1510 boot=0/164 emit=1/5199 write=0/91 total=2/13138
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/0 plan=0/4690 lower=0/1510 boot=0/164 emit=1/5199 write=0/91 total=1/11654
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/1484 plan=1/4690 lower=0/1510 boot=0/164 emit=1/5199 write=0/91 total=2/13138
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/0 plan=1/4966 lower=0/1069 boot=0/156 emit=1/4127 write=0/91 total=2/10409
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/0 plan=1/5000 lower=0/1072 boot=0/157 emit=1/4159 write=0/91 total=2/10479
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/1985 plan=1/5000 lower=0/1072 boot=0/157 emit=1/4159 write=0/91 total=2/12464
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/0 plan=0/5000 lower=1/1072 boot=0/157 emit=0/4159 write=1/91 total=2/10479
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/1985 plan=1/5000 lower=0/1072 boot=0/157 emit=1/4159 write=0/91 total=2/12464
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=0/0 plan=3/56739 lower=2/12773 boot=0/346 emit=5/45345 write=0/91 total=10/115294
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=0/0 plan=3/53984 lower=2/12794 boot=0/353 emit=5/45574 write=1/91 total=11/112796
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=3/37909 plan=3/53984 lower=2/12794 boot=0/353 emit=5/45574 write=1/91 total=14/150705
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=0/0 plan=3/53984 lower=3/12794 boot=0/353 emit=5/45574 write=0/91 total=11/112796
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=3/37909 plan=3/53984 lower=2/12794 boot=0/353 emit=5/45574 write=1/91 total=14/150705
COMPILE-TRACE program=clock_rel_join_storms parse=0/0 plan=1/25636 lower=2/4867 boot=0/252 emit=3/25563 write=0/91 total=6/56409
COMPILE-TRACE program=clock_rel_join_storms parse=0/0 plan=1/22314 lower=1/4885 boot=0/258 emit=3/25746 write=1/91 total=6/53294
COMPILE-TRACE program=clock_rel_join_storms parse=1/18245 plan=1/22314 lower=1/4885 boot=0/258 emit=3/25746 write=0/91 total=6/71539
COMPILE-TRACE program=clock_rel_join_storms parse=0/0 plan=1/22314 lower=1/4885 boot=0/258 emit=3/25746 write=0/91 total=5/53294
COMPILE-TRACE program=clock_rel_join_storms parse=1/18245 plan=2/22314 lower=1/4885 boot=0/258 emit=3/25746 write=0/91 total=7/71539
COMPILE-TRACE program=retention_count_prunes_oldest parse=0/0 plan=0/1603 lower=1/389 boot=0/138 emit=0/3192 write=0/91 total=1/5413
COMPILE-TRACE program=retention_count_prunes_oldest parse=0/0 plan=0/1553 lower=0/392 boot=0/139 emit=1/3224 write=0/91 total=1/5399
COMPILE-TRACE program=retention_count_prunes_oldest parse=0/876 plan=1/1553 lower=0/392 boot=0/139 emit=0/3224 write=1/91 total=2/6275
COMPILE-TRACE program=retention_count_prunes_oldest parse=0/0 plan=1/1553 lower=0/392 boot=0/139 emit=0/3224 write=1/91 total=2/5399
COMPILE-TRACE program=retention_count_prunes_oldest parse=0/876 plan=0/1553 lower=0/392 boot=0/139 emit=1/3224 write=0/91 total=1/6275
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=0/0 plan=1/1603 lower=0/389 boot=0/138 emit=0/3192 write=1/91 total=2/5413
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=0/0 plan=0/1553 lower=0/392 boot=0/139 emit=1/3224 write=0/91 total=1/5399
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=0/876 plan=0/1553 lower=0/392 boot=0/139 emit=0/3224 write=1/91 total=1/6275
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=0/0 plan=0/1553 lower=0/392 boot=0/139 emit=1/3224 write=0/91 total=1/5399
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=0/876 plan=1/1553 lower=0/392 boot=0/139 emit=0/3224 write=1/91 total=2/6275
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/0 plan=0/5373 lower=1/1022 boot=0/161 emit=1/6848 write=0/91 total=2/13495
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/0 plan=0/5089 lower=1/1028 boot=0/163 emit=1/6901 write=0/91 total=2/13272
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/2870 plan=1/5089 lower=0/1028 boot=0/163 emit=1/6901 write=0/91 total=2/16142
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/0 plan=0/5089 lower=1/1028 boot=0/163 emit=1/6901 write=0/91 total=2/13272
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/2870 plan=0/5089 lower=1/1028 boot=0/163 emit=1/6901 write=0/91 total=2/16142
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=0/0 plan=1/13371 lower=1/3074 boot=0/158 emit=1/9980 write=1/91 total=4/26674
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=0/0 plan=1/13281 lower=0/3080 boot=0/160 emit=2/10033 write=0/91 total=3/26645
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=1/13281 plan=1/13281 lower=0/3080 boot=0/160 emit=2/10033 write=0/91 total=4/39926
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=0/0 plan=1/13281 lower=0/3080 boot=0/160 emit=1/10033 write=0/91 total=2/26645
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=1/13281 plan=1/13281 lower=0/3080 boot=0/160 emit=2/10033 write=0/91 total=4/39926
COMPILE-TRACE program=log_retraction_rejected parse=0/0 plan=1/1551 lower=0/374 boot=0/136 emit=0/3037 write=1/91 total=2/5189
COMPILE-TRACE program=log_retraction_rejected parse=0/0 plan=0/1540 lower=0/377 boot=0/137 emit=1/3069 write=0/91 total=1/5214
COMPILE-TRACE program=log_retraction_rejected parse=0/810 plan=0/1540 lower=0/377 boot=0/137 emit=1/3069 write=0/91 total=1/6024
COMPILE-TRACE program=log_retraction_rejected parse=0/0 plan=0/1540 lower=0/377 boot=0/137 emit=1/3069 write=1/91 total=2/5214
COMPILE-TRACE program=log_retraction_rejected parse=0/810 plan=0/1540 lower=0/377 boot=0/137 emit=0/3069 write=1/91 total=1/6024
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/0 plan=1/1607 lower=0/476 boot=0/135 emit=0/3757 write=1/91 total=2/6066
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/0 plan=0/1625 lower=0/482 boot=0/137 emit=1/3808 write=1/91 total=2/6143
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/953 plan=0/1625 lower=0/482 boot=0/137 emit=1/3808 write=0/91 total=1/7096
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/0 plan=1/1625 lower=0/482 boot=0/137 emit=0/3808 write=1/91 total=2/6143
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/953 plan=0/1625 lower=0/482 boot=0/137 emit=1/3808 write=0/91 total=1/7096
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/0 plan=1/4469 lower=0/940 boot=0/161 emit=1/5920 write=0/91 total=2/11581
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/0 plan=0/4413 lower=0/943 boot=0/162 emit=1/5953 write=1/91 total=2/11562
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/3213 plan=0/4413 lower=0/943 boot=0/162 emit=1/5953 write=1/91 total=2/14775
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/0 plan=1/4413 lower=0/943 boot=0/162 emit=1/5953 write=0/91 total=2/11562
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/3213 plan=1/4413 lower=0/943 boot=0/162 emit=1/5953 write=0/91 total=2/14775
COMPILE-TRACE program=now_reads_the_tick parse=0/0 plan=1/5971 lower=0/1264 boot=0/159 emit=1/6038 write=0/91 total=2/13523
COMPILE-TRACE program=now_reads_the_tick parse=0/0 plan=0/5949 lower=0/1267 boot=0/160 emit=1/6071 write=1/91 total=2/13538
COMPILE-TRACE program=now_reads_the_tick parse=0/3723 plan=0/5949 lower=0/1267 boot=1/160 emit=1/6071 write=0/91 total=2/17261
COMPILE-TRACE program=now_reads_the_tick parse=0/0 plan=0/5949 lower=1/1267 boot=0/160 emit=0/6071 write=1/91 total=2/13538
COMPILE-TRACE program=now_reads_the_tick parse=0/3723 plan=0/5949 lower=1/1267 boot=0/160 emit=1/6071 write=0/91 total=2/17261
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=0/0 plan=1/7716 lower=0/1482 boot=0/182 emit=1/8216 write=1/91 total=3/17687
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=0/0 plan=1/7753 lower=0/1485 boot=0/183 emit=2/8250 write=0/91 total=3/17762
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=0/5481 plan=1/7753 lower=0/1485 boot=0/183 emit=1/8250 write=0/91 total=2/23243
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=0/0 plan=0/7753 lower=1/1485 boot=0/183 emit=1/8250 write=0/91 total=2/17762
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=0/5481 plan=1/7753 lower=0/1485 boot=0/183 emit=2/8250 write=0/91 total=3/23243
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/0 plan=0/7179 lower=0/1490 boot=0/182 emit=2/9191 write=0/91 total=2/18133
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/0 plan=0/7170 lower=0/1496 boot=0/184 emit=2/9259 write=0/91 total=2/18200
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/4447 plan=1/7170 lower=0/1496 boot=0/184 emit=1/9259 write=0/91 total=2/22647
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/0 plan=0/7170 lower=1/1496 boot=0/184 emit=1/9259 write=0/91 total=2/18200
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/4447 plan=1/7170 lower=0/1496 boot=0/184 emit=1/9259 write=1/91 total=3/22647
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=0/0 plan=0/6952 lower=0/1652 boot=0/182 emit=2/9714 write=0/91 total=2/18591
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=0/0 plan=0/6943 lower=0/1658 boot=0/184 emit=2/9782 write=0/91 total=2/18658
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=0/5493 plan=1/6943 lower=0/1658 boot=0/184 emit=1/9782 write=1/91 total=3/24151
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=0/0 plan=0/6943 lower=1/1658 boot=0/184 emit=1/9782 write=0/91 total=2/18658
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=1/5493 plan=0/6943 lower=1/1658 boot=0/184 emit=1/9782 write=0/91 total=3/24151
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/0 plan=0/4013 lower=1/1297 boot=0/163 emit=1/6950 write=0/91 total=2/12514
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/0 plan=0/3940 lower=0/1300 boot=0/164 emit=1/6983 write=1/91 total=2/12478
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/2511 plan=0/3940 lower=0/1300 boot=0/164 emit=1/6983 write=1/91 total=2/14989
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/0 plan=1/3940 lower=0/1300 boot=0/164 emit=1/6983 write=0/91 total=2/12478
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/2511 plan=0/3940 lower=1/1300 boot=0/164 emit=1/6983 write=0/91 total=2/14989
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/0 plan=1/8637 lower=0/2073 boot=0/186 emit=2/10386 write=0/91 total=3/21373
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/0 plan=1/8617 lower=0/2076 boot=0/187 emit=2/10420 write=0/91 total=3/21391
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/4447 plan=1/8617 lower=0/2076 boot=0/187 emit=2/10420 write=0/91 total=3/25838
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/0 plan=1/8617 lower=0/2076 boot=0/187 emit=2/10420 write=0/91 total=3/21391
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/4447 plan=1/8617 lower=0/2076 boot=0/187 emit=2/10420 write=0/91 total=3/25838
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=0/0 plan=0/8620 lower=1/1663 boot=0/181 emit=1/12281 write=1/91 total=3/22836
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=0/0 plan=0/8627 lower=1/1669 boot=0/183 emit=1/12337 write=1/91 total=3/22907
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=0/6106 plan=1/8627 lower=0/1669 boot=0/183 emit=2/12337 write=0/91 total=3/29013
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=0/0 plan=0/8627 lower=1/1669 boot=0/183 emit=1/12337 write=1/91 total=3/22907
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=0/6106 plan=1/8627 lower=0/1669 boot=0/183 emit=2/12337 write=0/91 total=3/29013
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=0/0 plan=0/6775 lower=1/1382 boot=0/158 emit=1/7633 write=0/91 total=2/16039
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=0/0 plan=0/6703 lower=0/1388 boot=0/160 emit=1/7686 write=1/91 total=2/16028
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=0/5255 plan=1/6703 lower=0/1388 boot=0/160 emit=1/7686 write=0/91 total=2/21283
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=0/0 plan=1/6703 lower=0/1388 boot=0/160 emit=1/7686 write=0/91 total=2/16028
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=1/5255 plan=0/6703 lower=0/1388 boot=0/160 emit=2/7686 write=0/91 total=3/21283
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=0/0 plan=0/6787 lower=0/1382 boot=0/158 emit=1/7633 write=1/91 total=2/16051
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=0/0 plan=1/6703 lower=0/1388 boot=0/160 emit=1/7686 write=0/91 total=2/16028
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=1/5255 plan=0/6703 lower=0/1388 boot=0/160 emit=2/7686 write=0/91 total=3/21283
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=0/0 plan=0/6703 lower=1/1388 boot=0/160 emit=1/7686 write=0/91 total=2/16028
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=0/5255 plan=1/6703 lower=0/1388 boot=0/160 emit=1/7686 write=0/91 total=2/21283
COMPILE-TRACE program=set_dedups_log_stacks parse=0/0 plan=1/7820 lower=0/1654 boot=0/203 emit=2/10514 write=0/91 total=3/20282
COMPILE-TRACE program=set_dedups_log_stacks parse=0/0 plan=1/7790 lower=0/1660 boot=0/205 emit=2/10584 write=0/91 total=3/20330
COMPILE-TRACE program=set_dedups_log_stacks parse=0/5768 plan=1/7790 lower=0/1660 boot=0/205 emit=2/10584 write=0/91 total=3/26098
COMPILE-TRACE program=set_dedups_log_stacks parse=0/0 plan=1/7790 lower=0/1660 boot=0/205 emit=1/10584 write=1/91 total=3/20330
COMPILE-TRACE program=set_dedups_log_stacks parse=1/5768 plan=0/7790 lower=1/1660 boot=0/205 emit=1/10584 write=1/91 total=4/26098
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=0/0 plan=0/9441 lower=1/3919 boot=0/471 emit=2/15551 write=0/91 total=3/29473
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=0/0 plan=1/9477 lower=0/3934 boot=0/189 emit=2/15026 write=0/91 total=3/28717
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=1/9937 plan=1/9477 lower=0/3934 boot=0/189 emit=2/15026 write=0/91 total=4/38654
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=0/0 plan=0/9477 lower=1/3934 boot=0/189 emit=1/15026 write=1/91 total=3/28717
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=1/9937 plan=0/9477 lower=1/3934 boot=0/189 emit=2/15026 write=0/91 total=4/38654
COMPILE-TRACE program=comparison_filters_rows parse=0/0 plan=1/17489 lower=1/7385 boot=0/515 emit=3/22452 write=0/91 total=5/47932
COMPILE-TRACE program=comparison_filters_rows parse=0/0 plan=1/17491 lower=1/7400 boot=0/218 emit=2/21932 write=1/91 total=5/47132
COMPILE-TRACE program=comparison_filters_rows parse=1/18632 plan=1/17491 lower=1/7400 boot=0/218 emit=2/21932 write=1/91 total=6/65764
COMPILE-TRACE program=comparison_filters_rows parse=0/0 plan=1/17491 lower=1/7400 boot=0/218 emit=3/21932 write=0/91 total=5/47132
COMPILE-TRACE program=comparison_filters_rows parse=1/18632 plan=1/17491 lower=1/7400 boot=0/218 emit=3/21932 write=0/91 total=6/65764
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
COMPILE-TRACE program=range_join_over_arithmetic parse=0/0 plan=1/9279 lower=0/3437 boot=0/343 emit=2/13163 write=0/91 total=3/26313
COMPILE-TRACE program=range_join_over_arithmetic parse=0/0 plan=0/9410 lower=1/3449 boot=0/188 emit=2/12882 write=0/91 total=3/26020
COMPILE-TRACE program=range_join_over_arithmetic parse=1/8396 plan=0/9410 lower=1/3449 boot=0/188 emit=1/12882 write=1/91 total=4/34416
COMPILE-TRACE program=range_join_over_arithmetic parse=0/0 plan=0/9410 lower=1/3449 boot=0/188 emit=1/12882 write=1/91 total=3/26020
COMPILE-TRACE program=range_join_over_arithmetic parse=0/8396 plan=1/9410 lower=1/3449 boot=0/188 emit=1/12882 write=1/91 total=4/34416
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=0/0 plan=1/9718 lower=1/3506 boot=0/418 emit=1/12852 write=1/91 total=4/26585
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=0/0 plan=1/9836 lower=1/3521 boot=0/189 emit=1/12544 write=1/91 total=4/26181
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=0/8104 plan=1/9836 lower=0/3521 boot=0/189 emit=1/12544 write=0/91 total=2/34285
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=0/0 plan=0/9836 lower=1/3521 boot=0/189 emit=2/12544 write=0/91 total=3/26181
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=0/8104 plan=1/9836 lower=1/3521 boot=0/189 emit=1/12544 write=0/91 total=3/34285
COMPILE-TRACE program=interpolation_desugars_to_concat parse=0/0 plan=0/5817 lower=1/1987 boot=0/213 emit=1/10108 write=0/91 total=2/18216
COMPILE-TRACE program=interpolation_desugars_to_concat parse=0/0 plan=1/5909 lower=0/1993 boot=0/165 emit=1/10036 write=0/91 total=2/18194
COMPILE-TRACE program=interpolation_desugars_to_concat parse=0/5457 plan=0/5909 lower=1/1993 boot=0/165 emit=1/10036 write=0/91 total=2/23651
COMPILE-TRACE program=interpolation_desugars_to_concat parse=0/0 plan=1/5909 lower=0/1993 boot=0/165 emit=1/10036 write=0/91 total=2/18194
COMPILE-TRACE program=interpolation_desugars_to_concat parse=1/5457 plan=0/5909 lower=0/1993 boot=0/165 emit=2/10036 write=0/91 total=3/23651
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=0/0 plan=1/7573 lower=0/2426 boot=0/407 emit=2/10477 write=0/91 total=3/20974
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=0/0 plan=0/7491 lower=1/2435 boot=0/166 emit=1/9947 write=0/91 total=2/20130
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=1/6410 plan=0/7491 lower=1/2435 boot=0/166 emit=1/9947 write=0/91 total=3/26540
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=0/0 plan=1/7491 lower=0/2435 boot=0/166 emit=1/9947 write=1/91 total=3/20130
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=0/6410 plan=1/7491 lower=0/2435 boot=0/166 emit=1/9947 write=1/91 total=3/26540
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/0 plan=0/1422 lower=0/397 boot=0/135 emit=1/2533 write=0/91 total=1/4578
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/0 plan=0/1422 lower=1/397 boot=0/135 emit=0/2533 write=1/91 total=2/4578
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/712 plan=0/1422 lower=0/397 boot=0/135 emit=0/2533 write=1/91 total=1/5290
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/0 plan=1/1422 lower=0/397 boot=0/135 emit=0/2533 write=1/91 total=2/4578
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/712 plan=0/1422 lower=0/397 boot=0/135 emit=0/2533 write=1/91 total=1/5290
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=0/0 plan=0/5707 lower=1/2175 boot=0/755 emit=1/10638 write=0/91 total=2/19366
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=0/0 plan=0/5539 lower=1/2175 boot=0/167 emit=1/10450 write=0/91 total=2/18422
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=0/6411 plan=0/5539 lower=1/2175 boot=0/167 emit=1/10450 write=0/91 total=2/24833
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=0/0 plan=1/5539 lower=0/2175 boot=0/167 emit=1/10450 write=1/91 total=3/18422
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=0/6411 plan=1/5539 lower=0/2175 boot=0/167 emit=2/10450 write=0/91 total=3/24833
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=0/0 plan=1/4848 lower=0/1763 boot=0/459 emit=1/6428 write=1/91 total=3/13589
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=0/0 plan=0/4737 lower=1/1763 boot=0/165 emit=1/6300 write=1/91 total=3/13056
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=0/4091 plan=0/4737 lower=1/1763 boot=0/165 emit=1/6300 write=0/91 total=2/17147
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=0/0 plan=0/4737 lower=0/1763 boot=0/165 emit=1/6300 write=0/91 total=1/13056
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=1/4091 plan=0/4737 lower=0/1763 boot=0/165 emit=1/6300 write=0/91 total=2/17147
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/0 plan=0/5118 lower=1/1800 boot=0/353 emit=1/8109 write=0/91 total=2/15471
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/0 plan=1/5041 lower=0/1800 boot=0/166 emit=1/7995 write=1/91 total=3/15093
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/4230 plan=1/5041 lower=0/1800 boot=0/166 emit=1/7995 write=0/91 total=2/19323
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/0 plan=0/5041 lower=1/1800 boot=0/166 emit=1/7995 write=0/91 total=2/15093
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/4230 plan=1/5041 lower=0/1800 boot=0/166 emit=1/7995 write=0/91 total=2/19323
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=0/0 plan=0/5406 lower=1/2156 boot=0/945 emit=1/10627 write=0/91 total=2/19225
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=0/0 plan=1/5234 lower=0/2156 boot=0/164 emit=1/10410 write=0/91 total=2/18055
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=0/10495 plan=1/5234 lower=0/2156 boot=0/164 emit=2/10410 write=0/91 total=3/28550
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=0/0 plan=0/5234 lower=1/2156 boot=0/164 emit=1/10410 write=0/91 total=2/18055
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=1/10495 plan=0/5234 lower=1/2156 boot=0/164 emit=1/10410 write=0/91 total=3/28550
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=0/0 plan=0/5178 lower=1/2037 boot=0/846 emit=1/9658 write=0/91 total=2/17810
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=0/0 plan=0/5013 lower=0/2037 boot=0/164 emit=2/9456 write=0/91 total=2/16761
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=0/7658 plan=1/5013 lower=0/2037 boot=0/164 emit=1/9456 write=1/91 total=3/24419
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=0/0 plan=0/5013 lower=1/2037 boot=0/164 emit=1/9456 write=1/91 total=3/16761
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=0/7658 plan=1/5013 lower=0/2037 boot=0/164 emit=1/9456 write=0/91 total=2/24419
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=0/0 plan=0/4798 lower=1/1785 boot=0/440 emit=1/7106 write=0/91 total=2/14220
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=0/0 plan=1/4668 lower=0/1785 boot=0/164 emit=1/6981 write=1/91 total=3/13689
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=0/4322 plan=0/4668 lower=1/1785 boot=0/164 emit=1/6981 write=0/91 total=2/18011
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=0/0 plan=0/4668 lower=0/1785 boot=0/164 emit=1/6981 write=1/91 total=2/13689
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=0/4322 plan=0/4668 lower=1/1785 boot=0/164 emit=1/6981 write=0/91 total=2/18011
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/0 plan=1/5446 lower=0/1692 boot=0/382 emit=1/8024 write=0/91 total=2/15635
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/0 plan=0/5205 lower=1/1692 boot=0/165 emit=1/7694 write=0/91 total=2/14847
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/3729 plan=1/5205 lower=0/1692 boot=0/165 emit=1/7694 write=0/91 total=2/18576
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/0 plan=1/5205 lower=0/1692 boot=0/165 emit=1/7694 write=0/91 total=2/14847
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/3729 plan=1/5205 lower=0/1692 boot=0/165 emit=1/7694 write=0/91 total=2/18576
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/0 plan=0/5500 lower=1/1825 boot=0/331 emit=1/9397 write=0/91 total=2/17144
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/0 plan=0/5342 lower=1/1825 boot=0/167 emit=1/9180 write=0/91 total=2/16605
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/4782 plan=1/5342 lower=0/1825 boot=0/167 emit=1/9180 write=0/91 total=2/21387
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/0 plan=1/5342 lower=0/1825 boot=0/167 emit=1/9180 write=0/91 total=2/16605
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/4782 plan=0/5342 lower=1/1825 boot=0/167 emit=1/9180 write=0/91 total=2/21387
COMPILE-TRACE program=count_is_bag_of_derivations parse=0/0 plan=0/4799 lower=0/1513 boot=1/346 emit=1/7693 write=0/91 total=2/14442
COMPILE-TRACE program=count_is_bag_of_derivations parse=0/0 plan=1/4804 lower=0/1522 boot=0/166 emit=1/7431 write=0/91 total=2/14014
COMPILE-TRACE program=count_is_bag_of_derivations parse=0/4224 plan=1/4804 lower=0/1522 boot=0/166 emit=1/7431 write=0/91 total=2/18238
COMPILE-TRACE program=count_is_bag_of_derivations parse=0/0 plan=1/4804 lower=0/1522 boot=0/166 emit=1/7431 write=1/91 total=3/14014
COMPILE-TRACE program=count_is_bag_of_derivations parse=1/4224 plan=0/4804 lower=0/1522 boot=0/166 emit=1/7431 write=1/91 total=3/18238
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=0/0 plan=0/5185 lower=1/1736 boot=0/313 emit=1/8194 write=0/91 total=2/15519
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=0/0 plan=0/5146 lower=0/1742 boot=0/165 emit=1/7930 write=1/91 total=2/15074
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=0/4571 plan=0/5146 lower=1/1742 boot=0/165 emit=1/7930 write=0/91 total=2/19645
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=0/0 plan=0/5146 lower=1/1742 boot=0/165 emit=1/7930 write=0/91 total=2/15074
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=0/4571 plan=1/5146 lower=0/1742 boot=0/165 emit=1/7930 write=1/91 total=3/19645
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=0/0 plan=0/5334 lower=0/1724 boot=0/163 emit=1/7849 write=1/91 total=2/15161
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=0/0 plan=1/5122 lower=0/1730 boot=0/165 emit=1/7902 write=0/91 total=2/15010
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=1/4583 plan=0/5122 lower=0/1730 boot=0/165 emit=2/7902 write=0/91 total=3/19593
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=0/0 plan=0/5122 lower=0/1730 boot=0/165 emit=1/7902 write=0/91 total=1/15010
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=1/4583 plan=0/5122 lower=0/1730 boot=0/165 emit=1/7902 write=1/91 total=3/19593
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=0/0 plan=1/5453 lower=0/1724 boot=0/363 emit=1/8270 write=0/91 total=2/15901
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=0/0 plan=0/5122 lower=1/1730 boot=0/165 emit=1/7902 write=0/91 total=2/15010
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=1/4583 plan=0/5122 lower=0/1730 boot=0/165 emit=1/7902 write=1/91 total=3/19593
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=0/0 plan=0/5122 lower=0/1730 boot=0/165 emit=1/7902 write=1/91 total=2/15010
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=0/4583 plan=0/5122 lower=1/1730 boot=0/165 emit=1/7902 write=0/91 total=2/19593
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=0/0 plan=0/4947 lower=0/1526 boot=0/285 emit=1/8648 write=1/91 total=2/15497
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=0/0 plan=1/4868 lower=0/1535 boot=0/166 emit=1/8479 write=0/91 total=2/15139
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=1/4424 plan=0/4868 lower=0/1535 boot=0/166 emit=1/8479 write=1/91 total=3/19563
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=0/0 plan=1/4868 lower=0/1535 boot=0/166 emit=1/8479 write=0/91 total=2/15139
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=1/4424 plan=0/4868 lower=1/1535 boot=0/166 emit=1/8479 write=0/91 total=3/19563
COMPILE-TRACE program=merge_batches_per_tick parse=0/0 plan=1/7227 lower=0/1505 boot=0/182 emit=1/8192 write=0/91 total=2/17197
COMPILE-TRACE program=merge_batches_per_tick parse=0/0 plan=0/7190 lower=1/1511 boot=0/184 emit=1/8260 write=0/91 total=2/17236
COMPILE-TRACE program=merge_batches_per_tick parse=0/5381 plan=1/7190 lower=0/1511 boot=0/184 emit=1/8260 write=1/91 total=3/22617
COMPILE-TRACE program=merge_batches_per_tick parse=0/0 plan=0/7190 lower=0/1511 boot=0/184 emit=2/8260 write=0/91 total=2/17236
COMPILE-TRACE program=merge_batches_per_tick parse=0/5381 plan=1/7190 lower=0/1511 boot=0/184 emit=1/8260 write=0/91 total=2/22617
COMPILE-TRACE program=merge_never_retracts parse=0/0 plan=0/7127 lower=1/1505 boot=0/182 emit=1/8192 write=0/91 total=2/17097
COMPILE-TRACE program=merge_never_retracts parse=0/0 plan=0/7190 lower=0/1511 boot=0/184 emit=2/8260 write=0/91 total=2/17236
COMPILE-TRACE program=merge_never_retracts parse=0/5381 plan=1/7190 lower=0/1511 boot=0/184 emit=1/8260 write=0/91 total=2/22617
COMPILE-TRACE program=merge_never_retracts parse=0/0 plan=1/7190 lower=0/1511 boot=0/184 emit=1/8260 write=0/91 total=2/17236
COMPILE-TRACE program=merge_never_retracts parse=0/5381 plan=0/7190 lower=1/1511 boot=0/184 emit=1/8260 write=0/91 total=2/22617
COMPILE-TRACE program=key_last_write_wins parse=0/0 plan=0/8132 lower=1/1727 boot=0/181 emit=2/11936 write=0/91 total=3/22067
COMPILE-TRACE program=key_last_write_wins parse=0/0 plan=0/8223 lower=1/1739 boot=0/185 emit=1/12048 write=1/91 total=3/22286
COMPILE-TRACE program=key_last_write_wins parse=0/7523 plan=1/8223 lower=0/1739 boot=0/185 emit=2/12048 write=0/91 total=3/29809
COMPILE-TRACE program=key_last_write_wins parse=0/0 plan=1/8223 lower=0/1739 boot=0/185 emit=2/12048 write=0/91 total=3/22286
COMPILE-TRACE program=key_last_write_wins parse=0/7523 plan=0/8223 lower=0/1739 boot=1/185 emit=1/12048 write=0/91 total=2/29809
COMPILE-TRACE program=key_identical_write_is_silent parse=0/0 plan=0/8050 lower=1/1727 boot=0/181 emit=1/11936 write=1/91 total=3/21985
COMPILE-TRACE program=key_identical_write_is_silent parse=0/0 plan=1/8223 lower=0/1739 boot=0/185 emit=2/12048 write=0/91 total=3/22286
COMPILE-TRACE program=key_identical_write_is_silent parse=1/7523 plan=0/8223 lower=0/1739 boot=0/185 emit=1/12048 write=0/91 total=2/29809
COMPILE-TRACE program=key_identical_write_is_silent parse=0/0 plan=0/8223 lower=1/1739 boot=0/185 emit=1/12048 write=0/91 total=2/22286
COMPILE-TRACE program=key_identical_write_is_silent parse=1/7523 plan=0/8223 lower=1/1739 boot=0/185 emit=1/12048 write=0/91 total=3/29809
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=0/0 plan=1/8036 lower=0/1727 boot=0/181 emit=1/11936 write=0/91 total=2/21971
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=0/0 plan=0/8223 lower=1/1739 boot=0/185 emit=1/12048 write=1/91 total=3/22286
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=0/7523 plan=1/8223 lower=0/1739 boot=0/185 emit=2/12048 write=0/91 total=3/29809
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=0/0 plan=0/8223 lower=1/1739 boot=0/185 emit=2/12048 write=0/91 total=3/22286
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=0/7523 plan=1/8223 lower=0/1739 boot=0/185 emit=2/12048 write=0/91 total=3/29809
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=0/0 plan=1/17839 lower=1/4453 boot=0/266 emit=2/16719 write=0/91 total=4/39368
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=0/0 plan=1/18885 lower=1/4465 boot=0/214 emit=2/16730 write=0/91 total=4/40385
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=1/13148 plan=1/18885 lower=1/4465 boot=0/214 emit=2/16730 write=1/91 total=6/53533
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=0/0 plan=1/18885 lower=1/4465 boot=0/214 emit=2/16730 write=1/91 total=5/40385
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=1/13148 plan=1/18885 lower=0/4465 boot=0/214 emit=3/16730 write=0/91 total=5/53533
COMPILE-TRACE program=batched_increments_both_count parse=0/0 plan=1/6953 lower=0/1764 boot=0/208 emit=2/8404 write=0/91 total=3/17420
COMPILE-TRACE program=batched_increments_both_count parse=0/0 plan=0/7367 lower=0/1770 boot=1/160 emit=1/8353 write=0/91 total=2/17741
COMPILE-TRACE program=batched_increments_both_count parse=0/5414 plan=1/7367 lower=0/1770 boot=0/160 emit=2/8353 write=0/91 total=3/23155
COMPILE-TRACE program=batched_increments_both_count parse=0/0 plan=1/7367 lower=0/1770 boot=0/160 emit=1/8353 write=1/91 total=3/17741
COMPILE-TRACE program=batched_increments_both_count parse=0/5414 plan=0/7367 lower=1/1770 boot=0/160 emit=1/8353 write=0/91 total=2/23155
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=0/0 plan=1/12715 lower=0/3120 boot=0/234 emit=2/13028 write=1/91 total=4/29188
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=0/0 plan=1/13692 lower=0/3132 boot=0/185 emit=2/13035 write=0/91 total=3/30135
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=1/9967 plan=1/13692 lower=0/3132 boot=0/185 emit=1/13035 write=1/91 total=4/40102
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=0/0 plan=1/13692 lower=1/3132 boot=0/185 emit=2/13035 write=0/91 total=4/30135
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=1/9967 plan=0/13692 lower=1/3132 boot=0/185 emit=2/13035 write=0/91 total=4/40102
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/0 plan=0/6620 lower=0/1719 boot=0/208 emit=1/7273 write=1/91 total=2/15911
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/0 plan=1/6973 lower=0/1722 boot=0/159 emit=2/7201 write=0/91 total=3/16146
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/4767 plan=1/6973 lower=0/1722 boot=0/159 emit=1/7201 write=1/91 total=3/20913
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/0 plan=0/6973 lower=0/1722 boot=0/159 emit=2/7201 write=0/91 total=2/16146
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/4767 plan=1/6973 lower=0/1722 boot=0/159 emit=1/7201 write=1/91 total=3/20913
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=0/0 plan=1/6663 lower=0/1719 boot=0/208 emit=1/7273 write=0/91 total=2/15954
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=0/0 plan=0/6973 lower=1/1722 boot=0/159 emit=1/7201 write=0/91 total=2/16146
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=0/4767 plan=1/6973 lower=0/1722 boot=0/159 emit=1/7201 write=1/91 total=3/20913
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=0/0 plan=0/6973 lower=1/1722 boot=0/159 emit=1/7201 write=0/91 total=2/16146
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=1/4767 plan=0/6973 lower=0/1722 boot=0/159 emit=2/7201 write=0/91 total=3/20913
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/0 plan=0/5876 lower=0/1470 boot=0/208 emit=2/9160 write=0/91 total=2/16805
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/0 plan=0/6191 lower=0/1476 boot=0/160 emit=2/9108 write=0/91 total=2/17026
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/4893 plan=1/6191 lower=0/1476 boot=0/160 emit=2/9108 write=0/91 total=3/21919
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/0 plan=0/6191 lower=0/1476 boot=0/160 emit=2/9108 write=0/91 total=2/17026
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/4893 plan=1/6191 lower=0/1476 boot=0/160 emit=1/9108 write=1/91 total=3/21919
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=0/0 plan=1/7041 lower=0/1762 boot=0/208 emit=2/9672 write=0/91 total=3/18774
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=0/0 plan=0/7445 lower=1/1768 boot=0/160 emit=1/9614 write=0/91 total=2/19078
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=1/6140 plan=0/7445 lower=1/1768 boot=0/160 emit=1/9614 write=0/91 total=3/25218
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=0/0 plan=0/7445 lower=0/1768 boot=1/160 emit=1/9614 write=0/91 total=2/19078
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=1/6140 plan=0/7445 lower=0/1768 boot=0/160 emit=2/9614 write=0/91 total=3/25218
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=0/0 plan=0/7041 lower=0/1762 boot=0/208 emit=2/9672 write=0/91 total=2/18774
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=0/0 plan=0/7445 lower=1/1768 boot=0/160 emit=1/9614 write=0/91 total=2/19078
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=1/6140 plan=0/7445 lower=1/1768 boot=0/160 emit=1/9614 write=0/91 total=3/25218
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=0/0 plan=1/7445 lower=0/1768 boot=0/160 emit=1/9614 write=1/91 total=3/19078
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=0/6140 plan=1/7445 lower=0/1768 boot=0/160 emit=2/9614 write=0/91 total=3/25218
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=0/0 plan=0/4707 lower=0/1378 boot=0/165 emit=2/8004 write=0/91 total=2/14345
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=0/0 plan=1/4720 lower=0/1387 boot=0/168 emit=1/8078 write=0/91 total=2/14444
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=1/4243 plan=0/4720 lower=0/1387 boot=0/168 emit=1/8078 write=1/91 total=3/18687
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=0/0 plan=0/4720 lower=0/1387 boot=0/168 emit=1/8078 write=0/91 total=1/14444
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=1/4243 plan=0/4720 lower=0/1387 boot=0/168 emit=1/8078 write=1/91 total=3/18687
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/0 plan=1/4707 lower=0/1378 boot=0/165 emit=1/8004 write=0/91 total=2/14345
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/0 plan=0/4720 lower=0/1387 boot=0/168 emit=1/8078 write=1/91 total=2/14444
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/4243 plan=0/4720 lower=1/1387 boot=0/168 emit=1/8078 write=0/91 total=2/18687
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/0 plan=0/4720 lower=0/1387 boot=0/168 emit=1/8078 write=1/91 total=2/14444
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/4243 plan=0/4720 lower=1/1387 boot=0/168 emit=1/8078 write=0/91 total=2/18687
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/0 plan=0/4646 lower=0/1378 boot=0/165 emit=1/8004 write=0/91 total=1/14284
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/0 plan=0/4720 lower=1/1387 boot=0/168 emit=1/8078 write=0/91 total=2/14444
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/4243 plan=1/4720 lower=0/1387 boot=0/168 emit=1/8078 write=0/91 total=2/18687
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/0 plan=1/4720 lower=0/1387 boot=0/168 emit=1/8078 write=0/91 total=2/14444
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/4243 plan=1/4720 lower=0/1387 boot=0/168 emit=1/8078 write=0/91 total=2/18687
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=0/0 plan=0/7235 lower=1/1839 boot=0/188 emit=1/9949 write=0/91 total=2/19302
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=0/0 plan=0/7220 lower=1/1842 boot=0/189 emit=1/9983 write=0/91 total=2/19325
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=1/5128 plan=0/7220 lower=0/1842 boot=0/189 emit=2/9983 write=0/91 total=3/24453
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=0/0 plan=1/7220 lower=0/1842 boot=0/189 emit=1/9983 write=0/91 total=2/19325
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=1/5128 plan=0/7220 lower=1/1842 boot=0/189 emit=1/9983 write=1/91 total=4/24453
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=0/0 plan=1/4409 lower=0/925 boot=0/159 emit=1/5632 write=0/91 total=2/11216
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=0/0 plan=0/4359 lower=0/928 boot=0/160 emit=1/5665 write=0/91 total=1/11203
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=1/3078 plan=0/4359 lower=0/928 boot=0/160 emit=1/5665 write=0/91 total=2/14281
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=0/0 plan=1/4359 lower=0/928 boot=0/160 emit=1/5665 write=0/91 total=2/11203
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=0/3078 plan=0/4359 lower=1/928 boot=0/160 emit=1/5665 write=0/91 total=2/14281
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=0/0 plan=1/4225 lower=0/946 boot=0/157 emit=1/5660 write=0/91 total=2/11079
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=0/0 plan=1/4224 lower=0/949 boot=0/158 emit=1/5693 write=0/91 total=2/11115
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=0/2875 plan=1/4224 lower=0/949 boot=0/158 emit=1/5693 write=0/91 total=2/13990
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=0/0 plan=0/4224 lower=0/949 boot=0/158 emit=1/5693 write=0/91 total=1/11115
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=0/2875 plan=1/4224 lower=0/949 boot=0/158 emit=1/5693 write=0/91 total=2/13990
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/0 plan=0/4363 lower=0/925 boot=0/159 emit=1/5469 write=1/91 total=2/11007
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/0 plan=0/4359 lower=1/928 boot=0/160 emit=1/5502 write=0/91 total=2/11040
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/3004 plan=0/4359 lower=1/928 boot=0/160 emit=0/5502 write=1/91 total=2/14044
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/0 plan=1/4359 lower=0/928 boot=0/160 emit=1/5502 write=0/91 total=2/11040
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/3004 plan=0/4359 lower=1/928 boot=0/160 emit=0/5502 write=1/91 total=2/14044
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=0/0 plan=1/7380 lower=0/1616 boot=0/178 emit=1/7964 write=1/91 total=3/17229
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=0/0 plan=1/7437 lower=0/1622 boot=0/180 emit=1/8030 write=1/91 total=3/17360
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=0/6069 plan=1/7437 lower=0/1622 boot=0/180 emit=1/8030 write=2/91 total=4/23429
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=0/0 plan=1/7437 lower=0/1622 boot=0/180 emit=2/8030 write=1/91 total=4/17360
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=0/6069 plan=1/7437 lower=0/1622 boot=0/180 emit=1/8030 write=3/91 total=5/23429
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=0/0 plan=1/7338 lower=0/1700 boot=0/177 emit=2/8229 write=0/91 total=3/17535
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=0/0 plan=1/7395 lower=0/1706 boot=0/179 emit=1/8295 write=1/91 total=3/17666
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=0/6085 plan=1/7395 lower=0/1706 boot=0/179 emit=1/8295 write=1/91 total=3/23751
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=0/0 plan=0/7395 lower=1/1706 boot=0/179 emit=1/8295 write=0/91 total=2/17666
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=0/6085 plan=1/7395 lower=0/1706 boot=0/179 emit=1/8295 write=1/91 total=3/23751
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=0/0 plan=1/11776 lower=1/2440 boot=0/178 emit=1/9779 write=0/91 total=3/24264
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=0/0 plan=0/11833 lower=1/2446 boot=0/180 emit=1/9845 write=1/91 total=3/24395
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=0/10824 plan=1/11833 lower=1/2446 boot=0/180 emit=1/9845 write=1/91 total=4/35219
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=0/0 plan=1/11833 lower=0/2446 boot=0/180 emit=2/9845 write=0/91 total=3/24395
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=1/10824 plan=0/11833 lower=1/2446 boot=0/180 emit=1/9845 write=1/91 total=4/35219
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=0/0 plan=1/11776 lower=0/2440 boot=0/178 emit=2/9779 write=0/91 total=3/24264
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=0/0 plan=1/11833 lower=0/2446 boot=0/180 emit=2/9845 write=0/91 total=3/24395
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=1/10824 plan=1/11833 lower=0/2446 boot=0/180 emit=2/9845 write=0/91 total=4/35219
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=0/0 plan=0/11833 lower=1/2446 boot=0/180 emit=1/9845 write=1/91 total=3/24395
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=0/10824 plan=1/11833 lower=1/2446 boot=0/180 emit=1/9845 write=0/91 total=3/35219
COMPILE-TRACE program=filter_map_is_a_level_rule parse=0/0 plan=1/6658 lower=0/2225 boot=0/263 emit=1/8178 write=0/91 total=2/17415
COMPILE-TRACE program=filter_map_is_a_level_rule parse=0/0 plan=0/6563 lower=1/2231 boot=0/165 emit=1/8024 write=0/91 total=2/17074
COMPILE-TRACE program=filter_map_is_a_level_rule parse=1/4908 plan=0/6563 lower=0/2231 boot=0/165 emit=2/8024 write=0/91 total=3/21982
COMPILE-TRACE program=filter_map_is_a_level_rule parse=0/0 plan=0/6563 lower=0/2231 boot=1/165 emit=1/8024 write=0/91 total=2/17074
COMPILE-TRACE program=filter_map_is_a_level_rule parse=0/4908 plan=1/6563 lower=0/2231 boot=0/165 emit=1/8024 write=0/91 total=2/21982
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=0/0 plan=0/8250 lower=1/1952 boot=0/159 emit=0/5248 write=1/91 total=2/15700
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=0/0 plan=1/8289 lower=0/1955 boot=0/160 emit=1/5281 write=0/91 total=2/15776
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=1/6101 plan=0/8289 lower=0/1955 boot=0/160 emit=1/5281 write=0/91 total=2/21877
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=0/0 plan=1/8289 lower=0/1955 boot=0/160 emit=1/5281 write=0/91 total=2/15776
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=0/6101 plan=1/8289 lower=0/1955 boot=0/160 emit=1/5281 write=0/91 total=2/21877
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=0/0 plan=0/6187 lower=1/2224 boot=0/184 emit=1/11577 write=0/91 total=2/20263
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=0/0 plan=1/6230 lower=0/2230 boot=0/186 emit=1/11645 write=1/91 total=3/20382
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=0/4656 plan=1/6230 lower=0/2230 boot=0/186 emit=1/11645 write=1/91 total=3/25038
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=0/0 plan=0/6230 lower=1/2230 boot=0/186 emit=1/11645 write=0/91 total=2/20382
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=1/4656 plan=0/6230 lower=0/2230 boot=0/186 emit=1/11645 write=0/91 total=2/25038
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=0/0 plan=2/37420 lower=1/8681 boot=0/234 emit=4/22807 write=1/91 total=8/69233
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=0/0 plan=2/37290 lower=1/8693 boot=0/238 emit=3/22911 write=1/91 total=7/69223
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=1/17009 plan=2/37290 lower=1/8693 boot=0/238 emit=3/22911 write=1/91 total=8/86232
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=0/0 plan=2/37290 lower=1/8693 boot=0/238 emit=3/22911 write=1/91 total=7/69223
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=1/17009 plan=2/37290 lower=1/8693 boot=0/238 emit=3/22911 write=1/91 total=8/86232
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=0/0 plan=1/10261 lower=0/3951 boot=0/165 emit=1/9829 write=1/91 total=3/24297
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=0/0 plan=0/10213 lower=1/3963 boot=0/169 emit=1/9921 write=0/91 total=2/24357
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=0/13481 plan=1/10213 lower=1/3963 boot=0/169 emit=1/9921 write=0/91 total=3/37838
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=0/0 plan=1/10213 lower=1/3963 boot=0/169 emit=1/9921 write=0/91 total=3/24357
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=1/13481 plan=1/10213 lower=0/3963 boot=0/169 emit=2/9921 write=0/91 total=4/37838
COMPILE-TRACE program=switch_as_keyed_replace parse=0/0 plan=1/14641 lower=1/4199 boot=0/355 emit=3/24814 write=0/91 total=5/44100
COMPILE-TRACE program=switch_as_keyed_replace parse=0/0 plan=1/14636 lower=1/4211 boot=0/241 emit=3/24672 write=1/91 total=6/43851
COMPILE-TRACE program=switch_as_keyed_replace parse=0/13505 plan=1/14636 lower=1/4211 boot=0/241 emit=3/24672 write=1/91 total=6/57356
COMPILE-TRACE program=switch_as_keyed_replace parse=0/0 plan=1/14636 lower=1/4211 boot=0/241 emit=3/24672 write=1/91 total=6/43851
COMPILE-TRACE program=switch_as_keyed_replace parse=0/13505 plan=1/14636 lower=1/4211 boot=0/241 emit=3/24672 write=1/91 total=6/57356
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=0/0 plan=1/1705 lower=0/477 boot=0/135 emit=0/4689 write=1/91 total=2/7097
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=0/0 plan=0/1625 lower=0/483 boot=0/137 emit=1/4741 write=0/91 total=1/7077
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=0/968 plan=0/1625 lower=1/483 boot=0/137 emit=0/4741 write=0/91 total=1/8045
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=0/0 plan=0/1625 lower=0/483 boot=0/137 emit=1/4741 write=0/91 total=1/7077
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=0/968 plan=0/1625 lower=1/483 boot=0/137 emit=0/4741 write=1/91 total=2/8045
COMPILE-TRACE program=merge_policy parse=0/0 plan=2/28856 lower=1/6698 boot=0/448 emit=4/38063 write=1/91 total=8/74156
COMPILE-TRACE program=merge_policy parse=0/0 plan=1/28869 lower=2/6716 boot=0/318 emit=5/38029 write=0/91 total=8/74023
COMPILE-TRACE program=merge_policy parse=2/22352 plan=1/28869 lower=2/6716 boot=0/318 emit=4/38029 write=1/91 total=10/96375
COMPILE-TRACE program=merge_policy parse=0/0 plan=1/28869 lower=1/6716 boot=0/318 emit=4/38029 write=1/91 total=7/74023
COMPILE-TRACE program=merge_policy parse=1/22352 plan=2/28869 lower=1/6716 boot=0/318 emit=5/38029 write=0/91 total=9/96375
COMPILE-TRACE program=exhaust_policy parse=0/0 plan=2/32668 lower=2/7110 boot=0/448 emit=4/38287 write=1/91 total=9/78604
COMPILE-TRACE program=exhaust_policy parse=0/0 plan=2/32503 lower=1/7128 boot=0/318 emit=5/38253 write=0/91 total=8/78293
COMPILE-TRACE program=exhaust_policy parse=2/24365 plan=2/32503 lower=1/7128 boot=0/318 emit=5/38253 write=1/91 total=11/102658
COMPILE-TRACE program=exhaust_policy parse=0/0 plan=2/32503 lower=2/7128 boot=0/318 emit=4/38253 write=1/91 total=9/78293
COMPILE-TRACE program=exhaust_policy parse=1/24365 plan=2/32503 lower=2/7128 boot=0/318 emit=4/38253 write=1/91 total=10/102658
COMPILE-TRACE program=concat_program_queue parse=0/0 plan=4/93558 lower=3/16452 boot=0/769 emit=8/66408 write=1/91 total=16/177278
COMPILE-TRACE program=concat_program_queue parse=0/0 plan=5/92559 lower=3/16470 boot=0/443 emit=8/66166 write=1/91 total=17/175729
COMPILE-TRACE program=concat_program_queue parse=5/51331 plan=5/92559 lower=3/16470 boot=0/443 emit=8/66166 write=1/91 total=22/227060
COMPILE-TRACE program=concat_program_queue parse=0/0 plan=5/92559 lower=2/16470 boot=0/443 emit=9/66166 write=0/91 total=16/175729
COMPILE-TRACE program=concat_program_queue parse=5/51331 plan=5/92559 lower=3/16470 boot=0/443 emit=8/66166 write=1/91 total=22/227060
COMPILE-TRACE program=completion_propagation_lattice_tick parse=0/0 plan=2/31194 lower=2/8100 boot=0/592 emit=4/35273 write=1/91 total=9/75250
COMPILE-TRACE program=completion_propagation_lattice_tick parse=0/0 plan=2/30861 lower=1/8118 boot=0/336 emit=5/35035 write=0/91 total=8/74441
COMPILE-TRACE program=completion_propagation_lattice_tick parse=2/22214 plan=2/30861 lower=1/8118 boot=0/336 emit=4/35035 write=1/91 total=10/96655
COMPILE-TRACE program=completion_propagation_lattice_tick parse=0/0 plan=2/30861 lower=1/8118 boot=0/336 emit=5/35035 write=0/91 total=8/74441
COMPILE-TRACE program=completion_propagation_lattice_tick parse=2/22214 plan=2/30861 lower=1/8118 boot=0/336 emit=5/35035 write=0/91 total=10/96655
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=0/0 plan=2/34396 lower=1/7552 boot=0/355 emit=4/31272 write=1/91 total=8/73666
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=0/0 plan=2/34311 lower=1/7561 boot=0/293 emit=5/31259 write=0/91 total=8/73515
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=2/20634 plan=2/34311 lower=1/7561 boot=0/293 emit=4/31259 write=1/91 total=10/94149
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=0/0 plan=2/34311 lower=1/7561 boot=0/293 emit=4/31259 write=1/91 total=8/73515
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=1/20634 plan=2/34311 lower=2/7561 boot=0/293 emit=4/31259 write=0/91 total=9/94149
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=0/0 plan=2/32605 lower=1/7552 boot=0/409 emit=4/31379 write=1/91 total=8/72036
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=0/0 plan=2/34311 lower=1/7561 boot=0/293 emit=5/31259 write=0/91 total=8/73515
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=2/20634 plan=2/34311 lower=1/7561 boot=0/293 emit=4/31259 write=1/91 total=10/94149
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=0/0 plan=2/34311 lower=1/7561 boot=0/293 emit=4/31259 write=1/91 total=8/73515
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=1/20634 plan=2/34311 lower=1/7561 boot=0/293 emit=4/31259 write=0/91 total=8/94149
COMPILE-TRACE program=fill_as_cache_update_swr parse=0/0 plan=1/14362 lower=0/4139 boot=0/238 emit=2/23398 write=1/91 total=4/42228
COMPILE-TRACE program=fill_as_cache_update_swr parse=0/0 plan=1/14311 lower=1/4151 boot=0/242 emit=3/23518 write=0/91 total=5/42313
COMPILE-TRACE program=fill_as_cache_update_swr parse=1/12485 plan=1/14311 lower=1/4151 boot=0/242 emit=2/23518 write=1/91 total=6/54798
COMPILE-TRACE program=fill_as_cache_update_swr parse=0/0 plan=0/14311 lower=1/4151 boot=0/242 emit=3/23518 write=1/91 total=5/42313
COMPILE-TRACE program=fill_as_cache_update_swr parse=0/12485 plan=1/14311 lower=1/4151 boot=0/242 emit=3/23518 write=0/91 total=5/54798
COMPILE-TRACE program=demand_laziness_effect_rows parse=0/0 plan=1/7756 lower=0/2417 boot=0/193 emit=2/14311 write=0/91 total=3/24768
COMPILE-TRACE program=demand_laziness_effect_rows parse=0/0 plan=0/7514 lower=1/2423 boot=0/195 emit=2/14367 write=0/91 total=3/24590
COMPILE-TRACE program=demand_laziness_effect_rows parse=1/6123 plan=0/7514 lower=1/2423 boot=0/195 emit=2/14367 write=0/91 total=4/30713
COMPILE-TRACE program=demand_laziness_effect_rows parse=0/0 plan=1/7514 lower=0/2423 boot=0/195 emit=2/14367 write=0/91 total=3/24590
COMPILE-TRACE program=demand_laziness_effect_rows parse=1/6123 plan=0/7514 lower=1/2423 boot=0/195 emit=2/14367 write=0/91 total=4/30713
COMPILE-TRACE program=shared_demand_refcount parse=0/0 plan=0/7617 lower=1/2417 boot=0/193 emit=2/14311 write=0/91 total=3/24629
COMPILE-TRACE program=shared_demand_refcount parse=0/0 plan=0/7514 lower=1/2423 boot=0/195 emit=1/14367 write=1/91 total=3/24590
COMPILE-TRACE program=shared_demand_refcount parse=0/6123 plan=1/7514 lower=0/2423 boot=0/195 emit=2/14367 write=0/91 total=3/30713
COMPILE-TRACE program=shared_demand_refcount parse=0/0 plan=1/7514 lower=0/2423 boot=0/195 emit=2/14367 write=0/91 total=3/24590
COMPILE-TRACE program=shared_demand_refcount parse=0/6123 plan=1/7514 lower=0/2423 boot=0/195 emit=2/14367 write=0/91 total=3/30713
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=0/0 plan=1/15512 lower=2/4990 boot=0/265 emit=3/29725 write=1/91 total=7/50583
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=0/0 plan=1/15507 lower=1/5008 boot=0/271 emit=3/29911 write=1/91 total=6/50788
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=1/13314 plan=0/15507 lower=1/5008 boot=0/271 emit=3/29911 write=1/91 total=6/64102
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=0/0 plan=1/15507 lower=1/5008 boot=0/271 emit=4/29911 write=0/91 total=6/50788
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=1/13314 plan=1/15507 lower=1/5008 boot=0/271 emit=3/29911 write=1/91 total=7/64102
COMPILE-TRACE program=seq_wire_surface parse=0/0 plan=1/20245 lower=0/4818 boot=0/184 emit=1/13596 write=1/91 total=3/38934
COMPILE-TRACE program=seq_wire_surface parse=0/0 plan=2/20012 lower=0/4818 boot=0/184 emit=2/13596 write=1/91 total=5/38701
COMPILE-TRACE program=seq_wire_surface parse=0/4210 plan=1/20012 lower=1/4818 boot=0/184 emit=2/13596 write=0/91 total=4/42911
COMPILE-TRACE program=seq_wire_surface parse=0/0 plan=2/20012 lower=0/4818 boot=0/184 emit=2/13596 write=0/91 total=4/38701
COMPILE-TRACE program=seq_wire_surface parse=1/4210 plan=1/20012 lower=0/4818 boot=0/184 emit=2/13596 write=1/91 total=5/42911
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
COMPILE-TRACE program=seq_wire_hand parse=0/0 plan=1/20385 lower=1/4818 boot=0/184 emit=2/13596 write=1/91 total=5/39074
COMPILE-TRACE program=seq_wire_hand parse=0/0 plan=1/20152 lower=1/4818 boot=0/184 emit=2/13596 write=0/91 total=4/38841
COMPILE-TRACE program=seq_wire_hand parse=2/16593 plan=1/20152 lower=1/4818 boot=0/184 emit=2/13596 write=0/91 total=6/55434
COMPILE-TRACE program=seq_wire_hand parse=0/0 plan=1/20152 lower=1/4818 boot=0/184 emit=2/13596 write=0/91 total=4/38841
COMPILE-TRACE program=seq_wire_hand parse=1/16593 plan=2/20152 lower=0/4818 boot=0/184 emit=2/13596 write=1/91 total=6/55434
COMPILE-TRACE program=identical_demand_dedups parse=0/0 plan=1/12204 lower=1/2387 boot=0/204 emit=2/18658 write=1/91 total=5/33544
COMPILE-TRACE program=identical_demand_dedups parse=0/0 plan=0/12402 lower=1/2405 boot=0/210 emit=2/18820 write=1/91 total=4/33928
COMPILE-TRACE program=identical_demand_dedups parse=0/10545 plan=1/12402 lower=1/2405 boot=0/210 emit=2/18820 write=0/91 total=4/44473
COMPILE-TRACE program=identical_demand_dedups parse=0/0 plan=1/12402 lower=0/2405 boot=0/210 emit=2/18820 write=1/91 total=4/33928
COMPILE-TRACE program=identical_demand_dedups parse=0/10545 plan=1/12402 lower=1/2405 boot=0/210 emit=2/18820 write=0/91 total=4/44473
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=0/0 plan=0/5134 lower=1/1097 boot=0/158 emit=1/9155 write=0/91 total=2/15635
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=0/0 plan=0/5118 lower=1/1106 boot=0/161 emit=1/9230 write=0/91 total=2/15706
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=1/4921 plan=0/5118 lower=0/1106 boot=0/161 emit=2/9230 write=0/91 total=3/20627
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=0/0 plan=0/5118 lower=0/1106 boot=0/161 emit=2/9230 write=0/91 total=2/15706
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=0/4921 plan=1/5118 lower=0/1106 boot=0/161 emit=1/9230 write=0/91 total=2/20627
COMPILE-TRACE program=terminal_is_terminal parse=0/0 plan=1/10391 lower=0/2908 boot=0/190 emit=2/14206 write=0/91 total=3/27786
COMPILE-TRACE program=terminal_is_terminal parse=0/0 plan=1/10528 lower=0/2923 boot=0/195 emit=2/14339 write=0/91 total=3/28076
COMPILE-TRACE program=terminal_is_terminal parse=1/9360 plan=1/10528 lower=0/2923 boot=0/195 emit=2/14339 write=0/91 total=4/37436
COMPILE-TRACE program=terminal_is_terminal parse=0/0 plan=1/10528 lower=0/2923 boot=1/195 emit=1/14339 write=0/91 total=3/28076
COMPILE-TRACE program=terminal_is_terminal parse=1/9360 plan=1/10528 lower=0/2923 boot=0/195 emit=2/14339 write=0/91 total=4/37436
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/0 plan=0/2164 lower=0/599 boot=0/159 emit=1/7840 write=0/91 total=1/10853
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/0 plan=0/2290 lower=0/614 boot=0/164 emit=1/7968 write=1/91 total=2/11127
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/1757 plan=0/2290 lower=0/614 boot=0/164 emit=1/7968 write=0/91 total=1/12884
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/0 plan=0/2290 lower=1/614 boot=0/164 emit=1/7968 write=0/91 total=2/11127
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/1757 plan=0/2290 lower=0/614 boot=0/164 emit=1/7968 write=1/91 total=2/12884
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=0/0 plan=0/8465 lower=1/2130 boot=0/187 emit=2/14433 write=0/91 total=3/25306
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=0/0 plan=1/8422 lower=0/2139 boot=0/190 emit=2/14511 write=0/91 total=3/25353
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=1/8182 plan=0/8422 lower=1/2139 boot=0/190 emit=2/14511 write=0/91 total=4/33535
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=0/0 plan=1/8422 lower=0/2139 boot=0/190 emit=2/14511 write=1/91 total=4/25353
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=0/8182 plan=1/8422 lower=0/2139 boot=0/190 emit=2/14511 write=0/91 total=3/33535
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=0/0 plan=0/5332 lower=1/1147 boot=0/158 emit=1/10574 write=0/91 total=2/17302
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=0/0 plan=0/5389 lower=1/1156 boot=0/161 emit=1/10649 write=0/91 total=2/17446
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=0/5380 plan=0/5389 lower=1/1156 boot=0/161 emit=1/10649 write=0/91 total=2/22826
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=0/0 plan=1/5389 lower=0/1156 boot=0/161 emit=1/10649 write=1/91 total=3/17446
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=0/5380 plan=0/5389 lower=1/1156 boot=0/161 emit=1/10649 write=0/91 total=2/22826
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=0/0 plan=1/14722 lower=1/4990 boot=0/419 emit=2/19969 write=1/91 total=5/40191
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=0/0 plan=1/15013 lower=1/5011 boot=0/238 emit=2/19790 write=1/91 total=5/40143
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=1/14254 plan=1/15013 lower=0/5011 boot=0/238 emit=3/19790 write=0/91 total=5/54397
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=0/0 plan=1/15013 lower=1/5011 boot=0/238 emit=2/19790 write=1/91 total=5/40143
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=1/14254 plan=1/15013 lower=1/5011 boot=0/238 emit=2/19790 write=0/91 total=5/54397
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=0/0 plan=1/22627 lower=2/6322 boot=0/483 emit=3/25216 write=0/91 total=6/54739
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=0/0 plan=1/23869 lower=1/6343 boot=0/284 emit=4/25063 write=1/91 total=7/55650
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=1/21899 plan=2/23869 lower=1/6343 boot=0/284 emit=3/25063 write=1/91 total=8/77549
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=0/0 plan=2/23869 lower=1/6343 boot=0/284 emit=3/25063 write=0/91 total=6/55650
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=2/21899 plan=1/23869 lower=2/6343 boot=0/284 emit=3/25063 write=0/91 total=8/77549
COMPILE-TRACE program=head_move_replaces_key parse=0/0 plan=0/4701 lower=1/1051 boot=0/208 emit=1/4707 write=0/91 total=2/10758
COMPILE-TRACE program=head_move_replaces_key parse=0/0 plan=1/4872 lower=0/1057 boot=0/160 emit=1/4665 write=0/91 total=2/10845
COMPILE-TRACE program=head_move_replaces_key parse=0/4264 plan=1/4872 lower=0/1057 boot=0/160 emit=1/4665 write=0/91 total=2/15109
COMPILE-TRACE program=head_move_replaces_key parse=0/0 plan=0/4872 lower=0/1057 boot=0/160 emit=1/4665 write=0/91 total=1/10845
COMPILE-TRACE program=head_move_replaces_key parse=1/4264 plan=0/4872 lower=0/1057 boot=0/160 emit=1/4665 write=0/91 total=2/15109
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=0/0 plan=1/11093 lower=0/3162 boot=0/600 emit=2/15284 write=1/91 total=4/30230
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=0/0 plan=0/10898 lower=1/3177 boot=0/214 emit=2/14631 write=0/91 total=3/29011
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=0/10522 plan=1/10898 lower=1/3177 boot=0/214 emit=1/14631 write=1/91 total=4/39533
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=0/0 plan=1/10898 lower=0/3177 boot=0/214 emit=2/14631 write=1/91 total=4/29011
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=0/10522 plan=1/10898 lower=1/3177 boot=0/214 emit=2/14631 write=0/91 total=4/39533
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=0/0 plan=0/12742 lower=1/3047 boot=0/253 emit=2/17170 write=1/91 total=4/33303
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=0/0 plan=1/12862 lower=0/3065 boot=0/214 emit=2/17235 write=1/91 total=4/33467
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=1/14222 plan=1/12862 lower=0/3065 boot=0/214 emit=2/17235 write=1/91 total=5/47689
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=0/0 plan=1/12862 lower=1/3065 boot=0/214 emit=2/17235 write=0/91 total=4/33467
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=1/14222 plan=1/12862 lower=1/3065 boot=0/214 emit=2/17235 write=0/91 total=5/47689
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=0/0 plan=1/11865 lower=0/2395 boot=0/331 emit=2/17019 write=0/91 total=3/31701
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=0/0 plan=1/12075 lower=1/2419 boot=0/232 emit=2/17060 write=0/91 total=4/31877
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=1/12535 plan=1/12075 lower=1/2419 boot=0/232 emit=2/17060 write=0/91 total=5/44412
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=0/0 plan=1/12075 lower=1/2419 boot=0/232 emit=2/17060 write=0/91 total=4/31877
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=1/12535 plan=1/12075 lower=1/2419 boot=0/232 emit=2/17060 write=0/91 total=5/44412
COMPILE-TRACE program=changed_since_spans_two_turns parse=0/0 plan=1/19460 lower=1/4600 boot=0/234 emit=3/19858 write=0/91 total=5/44243
COMPILE-TRACE program=changed_since_spans_two_turns parse=0/0 plan=1/19219 lower=1/4609 boot=0/237 emit=2/19954 write=1/91 total=5/44110
COMPILE-TRACE program=changed_since_spans_two_turns parse=1/14896 plan=1/19219 lower=1/4609 boot=0/237 emit=2/19954 write=1/91 total=6/59006
COMPILE-TRACE program=changed_since_spans_two_turns parse=0/0 plan=1/19219 lower=1/4609 boot=0/237 emit=2/19954 write=1/91 total=5/44110
COMPILE-TRACE program=changed_since_spans_two_turns parse=1/14896 plan=1/19219 lower=1/4609 boot=0/237 emit=2/19954 write=0/91 total=5/59006
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=0/0 plan=1/19248 lower=1/4600 boot=0/304 emit=3/19992 write=0/91 total=5/44235
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=0/0 plan=1/19219 lower=1/4609 boot=0/237 emit=2/19954 write=1/91 total=5/44110
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=0/14896 plan=1/19219 lower=0/4609 boot=0/237 emit=2/19954 write=0/91 total=3/59006
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=0/0 plan=2/19219 lower=0/4609 boot=0/237 emit=3/19954 write=0/91 total=5/44110
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=1/14896 plan=1/19219 lower=1/4609 boot=0/237 emit=3/19954 write=0/91 total=6/59006
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=0/0 plan=1/18891 lower=1/4013 boot=0/232 emit=3/19189 write=0/91 total=5/42416
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=0/0 plan=1/19099 lower=1/4034 boot=0/239 emit=2/19376 write=1/91 total=5/42839
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=1/17046 plan=1/19099 lower=1/4034 boot=0/239 emit=3/19376 write=0/91 total=6/59885
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=0/0 plan=1/19099 lower=1/4034 boot=0/239 emit=2/19376 write=1/91 total=5/42839
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=1/17046 plan=1/19099 lower=1/4034 boot=0/239 emit=2/19376 write=1/91 total=6/59885
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=0/0 plan=1/18769 lower=1/4013 boot=0/232 emit=2/19189 write=0/91 total=4/42294
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=0/0 plan=1/19099 lower=1/4034 boot=0/239 emit=2/19376 write=1/91 total=5/42839
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=1/17046 plan=1/19099 lower=1/4034 boot=0/239 emit=2/19376 write=1/91 total=6/59885
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=0/0 plan=1/19099 lower=0/4034 boot=0/239 emit=2/19376 write=1/91 total=4/42839
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=1/17046 plan=1/19099 lower=0/4034 boot=0/239 emit=3/19376 write=0/91 total=5/59885
COMPILE-TRACE program=clean_state_no_diags parse=0/0 plan=4/80948 lower=4/19432 boot=0/1674 emit=7/64578 write=1/91 total=16/166723
COMPILE-TRACE program=clean_state_no_diags parse=0/0 plan=4/79972 lower=4/19465 boot=0/430 emit=7/62928 write=1/91 total=16/162886
COMPILE-TRACE program=clean_state_no_diags parse=6/52309 plan=4/79972 lower=3/19465 boot=0/430 emit=8/62928 write=1/91 total=22/215195
COMPILE-TRACE program=clean_state_no_diags parse=0/0 plan=4/79972 lower=3/19465 boot=0/430 emit=7/62928 write=1/91 total=15/162886
COMPILE-TRACE program=clean_state_no_diags parse=6/52309 plan=4/79972 lower=3/19465 boot=0/430 emit=7/62928 write=1/91 total=21/215195
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=0/0 plan=8/157492 lower=6/34918 boot=0/3267 emit=12/98198 write=1/91 total=27/293966
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=0/0 plan=8/155309 lower=5/34972 boot=0/641 emit=12/95688 write=1/91 total=26/286701
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=11/84321 plan=8/155309 lower=5/34972 boot=0/641 emit=11/95688 write=1/91 total=36/371022
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=0/0 plan=8/155309 lower=5/34972 boot=0/641 emit=11/95688 write=1/91 total=25/286701
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=11/84321 plan=7/155309 lower=6/34972 boot=0/641 emit=10/95688 write=1/91 total=35/371022
COMPILE-TRACE program=waiver_range_join_exact_rows parse=0/0 plan=1/27891 lower=2/8120 boot=0/749 emit=3/32418 write=1/91 total=7/69269
COMPILE-TRACE program=waiver_range_join_exact_rows parse=0/0 plan=2/27638 lower=2/8132 boot=0/298 emit=3/31617 write=1/91 total=8/67776
COMPILE-TRACE program=waiver_range_join_exact_rows parse=1/22038 plan=1/27638 lower=2/8132 boot=0/298 emit=3/31617 write=1/91 total=8/89814
COMPILE-TRACE program=waiver_range_join_exact_rows parse=0/0 plan=2/27638 lower=1/8132 boot=0/298 emit=4/31617 write=0/91 total=7/67776
COMPILE-TRACE program=waiver_range_join_exact_rows parse=2/22038 plan=1/27638 lower=2/8132 boot=0/298 emit=4/31617 write=0/91 total=9/89814
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=0/0 plan=3/52482 lower=2/13118 boot=0/1198 emit=6/50985 write=0/91 total=11/117874
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=0/0 plan=2/51667 lower=3/13136 boot=0/352 emit=5/49570 write=1/91 total=11/114816
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=3/35802 plan=3/51667 lower=2/13136 boot=0/352 emit=6/49570 write=1/91 total=15/150618
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=0/0 plan=3/51667 lower=2/13136 boot=0/352 emit=5/49570 write=1/91 total=11/114816
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=3/35802 plan=2/51667 lower=3/13136 boot=0/352 emit=5/49570 write=1/91 total=14/150618
COMPILE-TRACE program=over_baseline_count_row parse=0/0 plan=2/27979 lower=1/8120 boot=0/814 emit=4/32550 write=0/91 total=7/69554
COMPILE-TRACE program=over_baseline_count_row parse=0/0 plan=2/27638 lower=1/8132 boot=0/298 emit=4/31617 write=1/91 total=8/67776
COMPILE-TRACE program=over_baseline_count_row parse=1/22038 plan=2/27638 lower=1/8132 boot=0/298 emit=4/31617 write=1/91 total=9/89814
COMPILE-TRACE program=over_baseline_count_row parse=0/0 plan=1/27638 lower=2/8132 boot=0/298 emit=3/31617 write=1/91 total=7/67776
COMPILE-TRACE program=over_baseline_count_row parse=1/22038 plan=2/27638 lower=1/8132 boot=0/298 emit=4/31617 write=1/91 total=9/89814
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=0/0 plan=6/120214 lower=4/27752 boot=1/2344 emit=9/84182 write=1/91 total=21/234583
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=0/0 plan=6/118790 lower=5/27791 boot=0/563 emit=9/82249 write=1/91 total=21/229484
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=8/67729 plan=6/118790 lower=4/27791 boot=0/563 emit=10/82249 write=0/91 total=28/297213
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=0/0 plan=6/118790 lower=3/27791 boot=1/563 emit=9/82249 write=1/91 total=20/229484
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=8/67729 plan=6/118790 lower=5/27791 boot=0/563 emit=9/82249 write=1/91 total=29/297213
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=0/0 plan=6/120951 lower=4/27751 boot=0/2629 emit=9/83585 write=1/91 total=20/235007
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=0/0 plan=7/119317 lower=4/27796 boot=0/565 emit=9/81313 write=0/91 total=20/229082
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=9/68374 plan=6/119317 lower=4/27796 boot=0/565 emit=10/81313 write=1/91 total=30/297456
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=0/0 plan=7/119317 lower=4/27796 boot=0/565 emit=9/81313 write=1/91 total=21/229082
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=8/68374 plan=6/119317 lower=5/27796 boot=0/565 emit=8/81313 write=1/91 total=28/297456
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=0/0 plan=3/52597 lower=2/13118 boot=0/1269 emit=6/51111 write=0/91 total=11/118186
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=0/0 plan=2/51667 lower=3/13136 boot=0/352 emit=5/49570 write=0/91 total=10/114816
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=4/35802 plan=2/51667 lower=3/13136 boot=0/352 emit=5/49570 write=1/91 total=15/150618
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=0/0 plan=2/51667 lower=3/13136 boot=0/352 emit=5/49570 write=1/91 total=11/114816
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=3/35802 plan=3/51667 lower=2/13136 boot=0/352 emit=6/49570 write=1/91 total=15/150618
COMPILE-TRACE program=new_file_no_exceeded_diag parse=0/0 plan=3/52597 lower=2/13118 boot=0/1269 emit=6/51111 write=1/91 total=12/118186
COMPILE-TRACE program=new_file_no_exceeded_diag parse=0/0 plan=3/51667 lower=2/13136 boot=0/352 emit=6/49570 write=0/91 total=11/114816
COMPILE-TRACE program=new_file_no_exceeded_diag parse=4/35802 plan=3/51667 lower=2/13136 boot=0/352 emit=5/49570 write=1/91 total=15/150618
COMPILE-TRACE program=new_file_no_exceeded_diag parse=0/0 plan=2/51667 lower=3/13136 boot=0/352 emit=5/49570 write=1/91 total=11/114816
COMPILE-TRACE program=new_file_no_exceeded_diag parse=3/35802 plan=3/51667 lower=3/13136 boot=0/352 emit=5/49570 write=0/91 total=14/150618
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=0/0 plan=1/19412 lower=1/6256 boot=0/1194 emit=3/23901 write=0/91 total=5/50854
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=0/0 plan=1/18365 lower=1/6271 boot=0/218 emit=2/22117 write=1/91 total=5/47062
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=1/16825 plan=1/18365 lower=1/6271 boot=0/218 emit=2/22117 write=1/91 total=6/63887
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=0/0 plan=1/18365 lower=1/6271 boot=0/218 emit=2/22117 write=1/91 total=5/47062
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=1/16825 plan=1/18365 lower=1/6271 boot=0/218 emit=2/22117 write=1/91 total=6/63887
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=0/0 plan=1/20804 lower=1/6256 boot=0/2130 emit=3/25665 write=1/91 total=6/54946
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=0/0 plan=1/18365 lower=1/6271 boot=0/218 emit=2/22117 write=1/91 total=5/47062
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=1/16825 plan=1/18365 lower=1/6271 boot=0/218 emit=3/22117 write=0/91 total=6/63887
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=0/0 plan=1/18365 lower=1/6271 boot=0/218 emit=3/22117 write=0/91 total=5/47062
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=1/16825 plan=1/18365 lower=1/6271 boot=0/218 emit=3/22117 write=0/91 total=6/63887
COMPILE-TRACE program=unwrap_below_budget_silent parse=0/0 plan=1/18368 lower=1/6256 boot=1/492 emit=2/22542 write=1/91 total=6/47749
COMPILE-TRACE program=unwrap_below_budget_silent parse=0/0 plan=1/18365 lower=1/6271 boot=0/218 emit=3/22117 write=0/91 total=5/47062
COMPILE-TRACE program=unwrap_below_budget_silent parse=1/16825 plan=1/18365 lower=1/6271 boot=0/218 emit=3/22117 write=1/91 total=7/63887
COMPILE-TRACE program=unwrap_below_budget_silent parse=0/0 plan=1/18365 lower=1/6271 boot=0/218 emit=2/22117 write=1/91 total=5/47062
COMPILE-TRACE program=unwrap_below_budget_silent parse=1/16825 plan=1/18365 lower=1/6271 boot=0/218 emit=2/22117 write=1/91 total=6/63887
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=0/0 plan=4/75785 lower=2/17988 boot=1/1376 emit=6/62392 write=1/91 total=14/157632
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=0/0 plan=4/74972 lower=3/18009 boot=0/434 emit=7/61070 write=0/91 total=14/154576
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=5/47702 plan=4/74972 lower=3/18009 boot=0/434 emit=7/61070 write=1/91 total=20/202278
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=0/0 plan=4/74972 lower=3/18009 boot=0/434 emit=6/61070 write=1/91 total=14/154576
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=5/47702 plan=4/74972 lower=3/18009 boot=0/434 emit=7/61070 write=0/91 total=19/202278
TEXT_DOOR compiled=417 byte_identical=416 failures=1
  TEXT_DOOR_FAIL json_nfc_and_nfd_keys_stay_distinct error(io_error(write,<stream>(0x6000035d5f00)),context(system:format/3,'Encoding cannot represent character'))
error: recipe `text-door` failed on line 48 with exit code 1
```

### `cd v6 && just plunit`

```text
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
cd /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile && /Users/chrishafley/projects/sprefa-codex-cstnative/v6/tools/run-capped.sh "${PLUNIT_BUDGET_S:-600}" swipl -q -l test/plunit_tests.pl -g run_tests -g halt
bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
perl: warning: Setting locale failed.
perl: warning: Please check that your locale settings:
	LC_ALL = "C.UTF-8",
	LC_TERMINAL = "iTerm2",
	LC_CTYPE = "C.UTF-8",
	LANG = "C.UTF-8"
    are supported and installed on your system.
perl: warning: Falling back to the standard locale ("C").
Warning: /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/2_subscribe.plt:81:
Warning:    Singleton variables: [Id]
Warning: /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3196:
Warning:    Singleton variables: [Pattern]
% [1/345] clock_checker:reg.._roles_are_complete .... passed (0.002 sec)
% [2/345] clock_checker:edg.._labels_and_offsets .... passed (0.000 sec)
% [3/345] clock_checker:sin.._fact_are_queryable .... passed (0.000 sec)
% [4/345] clock_checker:pip..atch_observed_ticks .... passed (0.001 sec)
% [5/345] clock_checker:equ..rade_diamond_passes .... passed (0.000 sec)
% [6/345] clock_checker:une..ade_diamond_refuses .... passed (0.000 sec)
% [7/345] clock_checker:pos..scc_is_constructive .... passed (0.000 sec)
% [8/345] clock_checker:pos..y_scc_is_productive .... passed (0.000 sec)
% [9/345] clock_checker:clo..ear_in_chain_length .... passed (0.004 sec)
% [10/345] clock_checker:clo.._in_parallel_routes ... passed (0.003 sec)
% [11/345] clock_checker:zer..egative_scc_refuses ... passed (0.000 sec)
% [12/345] clock_checker:com.._runs_clock_checker ... passed (0.000 sec)
% [13/345] clock_checker:ora.._runs_clock_checker ... passed (0.000 sec)
% [14/345] clock_checker:fiv..classes_are_derived ... passed (0.000 sec)
% [15/345] clock_checker:his..ed_ids_and_programs ... passed (0.000 sec)
% [16/345] clock_checker:his.._partition_is_exact ... passed (0.000 sec)
% [17/345] clock_checker:his.._partition_is_exact ... passed (0.000 sec)
% [18/345] clock_checker:a2_..t_provable_boundary ... passed (0.000 sec)
% [19/345] clock_checker:sin..gger_batch_boundary ... passed (0.000 sec)
% [20/345] clock_checker:a4_..no_rule_clock_claim ... passed (0.000 sec)
% [21/345] clock_checker:a5_..arity_refs_distinct ... passed (0.000 sec)
% [22/345] clock_checker:a6_..untime_crosschecked ... passed (0.001 sec)
% [23/345] clock_checker:a7_..no_rule_clock_claim ... passed (0.000 sec)
% [24/345] clock_checker:a8_..ing_partition_clock ... passed (0.000 sec)
% [25/345] clock_checker:a9_..no_rule_clock_claim ... passed (0.000 sec)
% [26/345] clock_checker:a11..ggregate_dependency ... passed (0.000 sec)
% [27/345] clock_checker:a4_..g_b_and_stops_there ... passed (0.000 sec)
% [28/345] clock_checker:a12..eplay_from_sampling ... passed (0.001 sec)
% [29/345] clock_checker:c2_.._with_observed_tick ... passed (0.001 sec)
% [30/345] clock_checker:d1_..e_edge_headed_plane ... passed (0.002 sec)
% [31/345] clock_checker:liv..abelled_not_refused ... passed (0.000 sec)
% [32/345] clock_checker:two..d_is_a_race_not_one ... passed (0.001 sec)
% [33-1/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-2/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-3/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-4/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-5/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-6/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-7/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-8/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-9/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-10/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-11/345] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [34-1/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-2/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-3/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-4/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-5/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-6/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-7/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-8/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-9/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-10/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-11/345] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [35/345] graph_module:cycl.._acyclic_singletons ... passed (0.000 sec)
% [36/345] graph_module:self.._a_cyclic_component ... passed (0.000 sec)
% [37/345] graph_module:acyc.._a_cyclic_component ... passed (0.000 sec)
% [38/345] graph_module:comp.._by_smallest_member ... passed (0.000 sec)
% [39/345] graph_module:comp..ontaining_component ... passed (0.000 sec)
% [40/345] graph_module:clos..ict_positive_length ... passed (0.000 sec)
% [41/345] graph_module:clos..s_a_node_on_a_cycle ... passed (0.000 sec)
% [42/345] graph_module:topo..ical_order_on_a_dag ... passed (0.000 sec)
% [43/345] graph_module:topo..er_fails_on_a_cycle ... passed (0.000 sec)
% [44/345] graph_module:topo..ails_on_a_self_loop ... passed (0.000 sec)
% [45/345] graph_module:isol..urvive_construction ... passed (0.000 sec)
% [46/345] graph_module:duplicate_edges_collapse .... passed (0.000 sec)
% [47/345] graph_module:comp..e_with_connectivity ... passed (0.000 sec)
% [48/345] diag_channel:one_based_to_zero_based ..... passed (0.000 sec)
% [49-1/345] diag_channel:json..e_equals_human_line .. passed (0.004 sec)
% [50/345] diag_channel:reco..round_trips_as_json ... passed (0.000 sec)
% [51/345] diag_channel:refu..not_earlier_mention ... passed (0.000 sec)
% [52/345] diag_channel:refu.._statement_position ... passed (0.000 sec)
% [53/345] diag_channel:pars.._is_exact_in_record ... passed (0.000 sec)
% [54/345] diag_channel:uri_..encoded_file_scheme .. **FAILED (0.000 sec)
ERROR: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/diag.test.pl:113:
ERROR: [Thread main]     test diag_channel:uri_is_percent_encoded_file_scheme: failed
% [55/345] subscribe_cone:ze.._subscribes_nothing ... passed (0.000 sec)
% [56/345] subscribe_cone:hand_computed_cone ........ passed (0.000 sec)
% [57/345] subscribe_cone:sampler_included .......... passed (0.000 sec)
% [58/345] subscribe_cone:negation_included ......... passed (0.000 sec)
% [59/345] subscribe_cone:de..the_real_decl_forms ... passed (0.000 sec)
% [60/345] subscribe_cone:de..to_a_queryless_cone ... passed (0.000 sec)
% [61/345] subscribe_cone:ed..chain_including_pre ... passed (0.000 sec)
% [62/345] subscribe_cone:ne..and_combine_spliced ... passed (0.000 sec)
% [63/345] subscribe_cone:re..ides_what_a_read_is ... passed (0.000 sec)
% [64/345] subscribe_cone:go..lex_cone_invariants ... passed (0.308 sec)
% [65/345] subscribe_cone:em..ath_behind_the_flag ... passed (0.003 sec)
% [66/345] subscribe_cone:em..ents_name_their_rel ... passed (0.002 sec)
% [67/345] subscribe_cone:em.._hand_computed_cone ... passed (0.002 sec)
% [68/345] subscribe_cone:ze.._subscribes_nothing ... passed (0.002 sec)
% [69/345] stratum_order:swi..d_replace_one_group ... passed (0.001 sec)
% [70/345] stratum_order:dem.._laziness_one_group ... passed (0.001 sec)
% [71/345] stratum_order:swi.._replace_rule_order ... passed (0.001 sec)
% [72/345] stratum_order:dem..laziness_rule_order ... passed (0.001 sec)
% [73/345] stratum_order:sel..remains_in_p2_order ... passed (0.000 sec)
% [74/345] column_naming:swi..yed_replace_columns ... passed (0.001 sec)
% [75/345] column_naming:demand_laziness_columns .... passed (0.001 sec)
% [76/345] sql_text_snapshot..ed_replace_edge_sql ... passed (0.002 sec)
% [77/345] sql_text_snapshot..eplace_ddl_pk_shape ... passed (0.002 sec)
% [78/345] sql_text_snapshot..straint_and_replace ... passed (0.000 sec)
% [79/345] sql_text_snapshot..eplace_frontier_ddl ... passed (0.002 sec)
% [80/345] sql_text_snapshot..dered_snapshot_read ... passed (0.001 sec)
% [81/345] sql_text_snapshot.._mirrors_each_write ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:297:
Warning: [Thread main]     test sql_text_snapshots:ordered_pre_snapshots_once_then_mirrors_each_write: Test succeeded with choicepoint
% [82/345] sql_text_snapshot..d_replace_level_sql ... passed (0.002 sec)
% [83/345] sql_text_snapshot..iness_no_edge_rules ... passed (0.001 sec)
% [84/345] sql_text_snapshot..one_batch_statement ... passed (0.001 sec)
% [85/345] sql_text_snapshot.._laziness_level_sql ... passed (0.001 sec)
% [86/345] sql_text_snapshot..s_promoted_frontier ... passed (0.001 sec)
% [87/345] sql_text_snapshot.._same_tick_frontier ... passed (0.001 sec)
% [88/345] sql_text_snapshot..l_column_expr_shape ... passed (0.000 sec)
% [89/345] sql_text_snapshot..elta_sql_open_scope ... passed (0.002 sec)
% [90/345] sql_text_snapshot..ql_route_change_log ... passed (0.002 sec)
% [91/345] sql_text_snapshot..n_both_sql_families ... passed (0.001 sec)
% [92/345] sql_text_snapshot.._departure_frontier ... passed (0.001 sec)
% [93/345] sql_text_snapshot..with_key_predicates ... passed (0.002 sec)
% [94/345] incremental_mode:..gram_is_incremental ... passed (0.002 sec)
% [95/345] incremental_mode:..cremental_reconcile ... passed (0.003 sec)
% [96/345] incremental_mode:..remental_carry_path ... passed (0.001 sec)
% [97/345] incremental_mode:..e_referee_available ... passed (0.002 sec)
% [98/345] incremental_mode:..tements_are_emitted ... passed (0.001 sec)
% [99/345] incremental_mode:..ecursive_cte_reseed ... passed (0.000 sec)
% [100/345] incremental_mode:..son_batch_statement .. passed (0.001 sec)
% [101/345] supported_subset_..ount_aggregate_head ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:572:
Warning: [Thread main]     test supported_subset_gate:accepts_count_aggregate_head: Test succeeded with choicepoint
% [102/345] supported_subset_.._variable_separator .. passed (0.000 sec)
% [103/345] supported_subset_..ate_non_int_ordinal .. passed (0.001 sec)
% [104/345] supported_subset_..gregate_wrong_arity .. passed (0.000 sec)
% [105/345] supported_subset_..rray_aggregate_head .. passed (0.000 sec)
% [106/345] supported_subset_..ject_aggregate_head .. passed (0.000 sec)
% [107/345] supported_subset_..ding_aggregate_head .. passed (0.000 sec)
% [108/345] supported_subset_..tern_on_arrival_rel .. passed (0.000 sec)
% [109/345] supported_subset_..tern_on_derived_rel ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:664:
Warning: [Thread main]     test supported_subset_gate:accepts_compound_pattern_on_derived_rel: Test succeeded with choicepoint
% [110/345] supported_subset_..eps_its_own_refusal .. passed (0.001 sec)
% [111/345] supported_subset_..tern_on_arrival_rel ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:698:
Warning: [Thread main]     test supported_subset_gate:accepts_bool_literal_pattern_on_arrival_rel: Test succeeded with choicepoint
% [112/345] supported_subset_..uard_under_negation .. passed (0.000 sec)
% [113/345] supported_subset_..d_atom_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:727:
Warning: [Thread main]     test supported_subset_gate:accepts_negated_atom_in_edge_body: Test succeeded with choicepoint
% [114/345] supported_subset_..nction_in_edge_body .. passed (0.000 sec)
% [115/345] supported_subset_..d_bind_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:743:
Warning: [Thread main]     test supported_subset_gate:accepts_comparison_and_bind_in_edge_body: Test succeeded with choicepoint
% [116/345] supported_subset_..e_atom_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:750:
Warning: [Thread main]     test supported_subset_gate:accepts_plain_pre_atom_in_edge_body: Test succeeded with choicepoint
% [117/345] supported_subset_..ts_now_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:767:
Warning: [Thread main]     test supported_subset_gate:accepts_now_in_edge_body: Test succeeded with choicepoint
% [118/345] supported_subset_..n_variable_argument .. passed (0.000 sec)
% [119/345] supported_subset_..s_now_in_level_rule .. passed (0.000 sec)
% [120/345] supported_subset_..typed_from_its_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:799:
Warning: [Thread main]     test supported_subset_gate:accepts_edge_head_column_typed_from_its_body: Test succeeded with choicepoint
% [121/345] supported_subset_.._still_gets_a_table ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:810:
Warning: [Thread main]     test supported_subset_gate:initial_only_ref_still_gets_a_table: Test succeeded with choicepoint
% [122/345] supported_subset_..rival_fed_level_rel ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:829:
Warning: [Thread main]     test supported_subset_gate:accepts_edge_join_against_an_arrival_fed_level_rel: Test succeeded with choicepoint
% [123/345] supported_subset_.._plane_before_edges .. passed (0.004 sec)
% [124/345] supported_subset_.._edge_fed_level_rel ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:864:
Warning: [Thread main]     test supported_subset_gate:accepts_edge_join_against_an_edge_fed_level_rel: Test succeeded with choicepoint
% [125/345] supported_subset_.._is_integer_storage ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:879:
Warning: [Thread main]     test supported_subset_gate:now_bound_head_column_is_integer_storage: Test succeeded with choicepoint
% [126/345] supported_subset_..sample_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:900:
Warning: [Thread main]     test supported_subset_gate:accepts_latest_plain_rel_sample_in_edge_body: Test succeeded with choicepoint
% [127/345] supported_subset_..nction_in_edge_body .. passed (0.000 sec)
% [128/345] supported_subset_..on_level_headed_rel .. passed (0.000 sec)
% [129/345] supported_subset_..atest_in_level_rule .. passed (0.000 sec)
% [130/345] supported_subset_..s_pre_in_level_rule .. passed (0.000 sec)
% [131/345] supported_subset_..keep_on_non_log_rel .. passed (0.000 sec)
% [132/345] supported_subset_..erived_edge_trigger ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:965:
Warning: [Thread main]     test supported_subset_gate:accepts_level_derived_edge_trigger: Test succeeded with choicepoint
% [133/345] supported_subset_..erived_edge_trigger ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:972:
Warning: [Thread main]     test supported_subset_gate:accepts_edge_derived_edge_trigger: Test succeeded with choicepoint
% [134/345] expression_miscom..t_not_text_collapse .. passed (0.001 sec)
% [135/345] expression_miscom..t_not_text_collapse .. passed (0.001 sec)
% [136/345] expression_miscom..t_column_stays_text .. passed (0.001 sec)
% [137/345] expression_miscom..eat_column_affinity .. passed (0.001 sec)
% [138/345] expression_miscom..mparison_is_refused .. passed (0.001 sec)
% [139/345] expression_miscom..ype_join_is_refused .. passed (0.001 sec)
% [140/345] expression_miscom..ype_reaches_the_ddl .. passed (0.001 sec)
% [141/345] expression_miscom.._floored_correction .. passed (0.001 sec)
% [142/345] enum_decl_expansi..semicolon_enum_decl ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1124:
Warning: [Thread main]     test enum_decl_expansion:parser_retains_semicolon_enum_decl: Test succeeded with choicepoint
% [143/345] enum_decl_expansi.._rels_and_tag_union .. passed (0.000 sec)
% [144/345] enum_decl_expansi..iant_name_collision .. passed (0.000 sec)
% [145/345] enum_decl_expansi..ger_keyed_edge_head ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1165:
Warning: [Thread main]     test enum_decl_expansion:enum_tag_view_can_trigger_keyed_edge_head: Test succeeded with choicepoint
% [146/345] match_block:share..dinary_rule_per_arm .. passed (0.000 sec)
% [147/345] match_block:enum_..uires_every_variant .. passed (0.000 sec)
% [148/345] match_block:keyed..med_compile_refusal .. passed (0.000 sec)
% [149/345] match_block:key_p..med_compile_refusal .. passed (0.000 sec)
% [150/345] match_block:key_p..med_compile_refusal .. passed (0.000 sec)
% [151/345] match_block:dupli..med_compile_refusal .. passed (0.000 sec)
% [152/345] match_block:keyed..d_remains_supported ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1240:
Warning: [Thread main]     test match_block:keyed_edge_head_remains_supported: Test succeeded with choicepoint
% [153/345] match_block:match.._left_to_right_arms ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1250:
Warning: [Thread main]     test match_block:match_surface_round_trips_with_prefix_semicolon_and_left_to_right_arms: Test succeeded with choicepoint
% [154/345] match_block:match..ut_prefix_semicolon ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1270:
Warning: [Thread main]     test match_block:match_surface_allows_first_arm_without_prefix_semicolon: Test succeeded with choicepoint
% [155/345] match_block:seq_s.._parser_and_printer ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1281:
Warning: [Thread main]     test match_block:seq_surface_round_trips_through_parser_and_printer: Test succeeded with choicepoint
% [156/345] match_block:match.._first_arm_spelling .. passed (0.000 sec)
% [157/345] match_block:sugar..er_to_identical_sql .. passed (0.004 sec)
% [158/345] match_block:reten..ed_delete_statement .. passed (0.000 sec)
% [159/345] hosts_wiring:sele..surface_round_trips ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1320:
Warning: [Thread main]     test hosts_wiring:selected_surface_round_trips: Test succeeded with choicepoint
% [160/345] hosts_wiring:host..ull_type_vocabulary ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1366:
Warning: [Thread main]     test hosts_wiring:host_input_and_bind_columns_read_the_full_type_vocabulary: Test succeeded with choicepoint
% [161/345] hosts_wiring:host..ill_a_named_refusal ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1381:
Warning: [Thread main]     test hosts_wiring:host_input_column_wrapper_is_still_a_named_refusal: Test succeeded with choicepoint
% [162/345] hosts_wiring:rhs_.._marker_is_rejected .. passed (0.000 sec)
% [163/345] hosts_wiring:rhs_.._marker_is_rejected .. passed (0.000 sec)
% [164/345] hosts_wiring:plai..n_order_independent ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1403:
Warning: [Thread main]     test hosts_wiring:plain_host_resolution_is_declaration_order_independent: Test succeeded with choicepoint
% [165/345] hosts_wiring:remo..surface_is_rejected .. passed (0.000 sec)
% [166/345] hosts_wiring:plai..mains_relation_atom ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1427:
Warning: [Thread main]     test hosts_wiring:plain_non_host_rhs_remains_relation_atom: Test succeeded with choicepoint
% [167/345] hosts_wiring:plai..sting_named_refusal .. passed (0.000 sec)
% [168/345] hosts_wiring:remo..keyword_is_rejected .. passed (0.000 sec)
% [169/345] hosts_wiring:refe..arks_reference_edge ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1446:
Warning: [Thread main]     test hosts_wiring:referenced_rel_remains_queryable_and_marks_reference_edge: Test succeeded with choicepoint
% [170/345] hosts_wiring:name..omissions_are_fresh ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1458:
Warning: [Thread main]     test hosts_wiring:named_body_omissions_are_fresh: Test succeeded with choicepoint
% [171/345] hosts_wiring:name..ial_head_is_refused ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1470:
Warning: [Thread main]     test hosts_wiring:named_partial_head_is_refused: Test succeeded with choicepoint
% [172/345] hosts_wiring:host..enced_input_refusal .. passed (0.000 sec)
% [173/345] hosts_wiring:host..bsent_from_template .. passed (0.000 sec)
% [174/345] hosts_wiring:host..ypes_not_name_alone .. passed (0.000 sec)
% [175/345] hosts_wiring:host..t_reference_refusal .. passed (0.000 sec)
% [176/345] hosts_wiring:host..nown_column_refusal .. passed (0.000 sec)
% [177/345] hosts_wiring:host..ame_is_not_a_column .. passed (0.000 sec)
% [178/345] hosts_wiring:extr..iler_known_executor .. passed (0.000 sec)
% [179/345] hosts_wiring:name..e_selected_executor .. passed (0.000 sec)
% [180/345] hosts_wiring:extr..uses_non_path_input .. passed (0.000 sec)
% [181/345] hosts_wiring:host_overlap_refusal ....... passed (0.000 sec)
% [182/345] hosts_wiring:host..cate_column_refusal .. passed (0.000 sec)
% [183/345] hosts_wiring:host..s_and_lowers_as_ref .. passed (0.006 sec)
% [184/345] hosts_wiring:host..efuses_by_type_name .. passed (0.001 sec)
% [185/345] hosts_wiring:probe_arity_refusal ........ passed (0.000 sec)
% [186/345] hosts_wiring:bind..d_rule_head_refusal .. passed (0.000 sec)
% [187/345] hosts_wiring:nati..ts_query_exact_text .. passed (0.000 sec)
% [188/345] hosts_wiring:nati.._parses_to_ts_query ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1665:
Warning: [Thread main]     test hosts_wiring:native_cst_block_parses_to_ts_query: Test succeeded with choicepoint
% [189/345] hosts_wiring:nati..round_trips_fixture ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1682:
Warning: [Thread main]     test hosts_wiring:native_cst_query_round_trips_fixture: Test succeeded with choicepoint
% [190/345] hosts_wiring:native_cst_capture_unused .. passed (0.000 sec)
% [191/345] hosts_wiring:nati..variable_uncaptured .. passed (0.000 sec)
% [192/345] hosts_wiring:nati.._uses_regexp_subset .. passed (0.000 sec)
% [193/345] hosts_wiring:emit..lans_and_demand_sql .. passed (0.008 sec)
% [194/345] hosts_wiring:quer..and_bound_positions .. passed (0.002 sec)
% [195/345] hosts_wiring:back..low_the_stated_rule ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1795:
Warning: [Thread main]     test hosts_wiring:backslash_escapes_follow_the_stated_rule: Test succeeded with choicepoint
% [196/345] hosts_wiring:back..s_print_and_reparse ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:1817:
Warning: [Thread main]     test hosts_wiring:backslash_survives_print_and_reparse: Test succeeded with choicepoint
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
/opt/homebrew/bin/bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory
% [197/345] hosts_wiring:rese.._the_generated_ones .. passed (0.000 sec)
% [198/345] hosts_wiring:ever..umn_refuses_by_name .. passed (0.000 sec)
% [199/345] hosts_wiring:unpr..lares_its_relations .. passed (0.000 sec)
% [200/345] body_walk_charact..ation:body_ref_uses .. passed (0.000 sec)
% [201/345] body_walk_charact..n:conjunction_goals .. passed (0.000 sec)
% [202/345] body_walk_charact..ation:trigger_items .. passed (0.000 sec)
% [203/345] body_walk_charact..ngine_finalize_refs .. passed (0.000 sec)
% [204/345] body_walk_charact..:engine_latest_refs .. passed (0.000 sec)
% [205/345] body_walk_charact..ion:engine_pre_refs .. passed (0.000 sec)
% [206/345] body_walk_charact..analyze_latest_refs .. passed (0.000 sec)
% [207/345] body_walk_charact..on:analyze_pre_refs .. passed (0.000 sec)
% [208/345] body_walk_charact..ation:goal_rel_refs .. passed (0.000 sec)
% [209/345] body_walk_characterization:body_atoms ... passed (0.000 sec)
% [210/345] body_walk_charact..reserved_constructs .. passed (0.000 sec)
% [211/345] body_walk_charact..ion:forbidden_goals .. passed (0.000 sec)
% [212/345] body_walk_charact..ion:host_body_goals .. passed (0.000 sec)
% [213/345] body_walk_charact.._latest_scans_agree .. passed (0.000 sec)
% [214/345] body_walk_charact..ler_pre_scans_agree .. passed (0.000 sec)
% [215/345] body_walk_charact.._agree_across_doors .. passed (0.000 sec)
% [216/345] body_walk_charact..atches_the_registry .. passed (0.000 sec)
% [217/345] cross_plane_check..of_range_both_doors .. passed (0.000 sec)
% [218/345] cross_plane_check..uplicate_both_doors .. passed (0.000 sec)
% [219/345] cross_plane_check..vel_head_both_doors .. passed (0.000 sec)
% [220/345] cross_plane_check..ds_differ_by_design .. passed (0.000 sec)
% [221/345] cross_plane_check..aded_rel_both_doors .. passed (0.000 sec)
% [222/345] cross_plane_check.._log_rel_both_doors .. passed (0.000 sec)
% [223/345] cross_plane_check..ict_risk_both_doors .. passed (0.000 sec)
% [224/345] cross_plane_check..epted_at_both_doors .. passed (0.000 sec)
% [225/345] cross_plane_check..fuses_at_both_doors .. passed (0.000 sec)
% [226/345] cross_plane_check..rd_refusal_payloads .. passed (0.000 sec)
% [227/345] cross_plane_check..fuses_at_both_doors .. passed (0.001 sec)
% [228/345] cross_plane_check..vel_rule_both_doors .. passed (0.000 sec)
% [229/345] cross_plane_check..vel_rule_both_doors .. passed (0.000 sec)
% [230/345] cross_plane_check..agrees_across_doors .. passed (0.000 sec)
% [231/345] cross_plane_check..d_not_latest_parity .. passed (0.000 sec)
% [232/345] cross_plane_check..sted_not_pre_parity .. passed (0.000 sec)
% [233/345] cross_plane_check..fused_by_both_doors .. passed (0.000 sec)
% [234/345] cross_plane_check..g_without_retention .. passed (0.000 sec)
% [235/345] cross_plane_check..regate_in_edge_head .. passed (0.000 sec)
% [236/345] cross_plane_check.._at_the_oracle_door .. passed (0.000 sec)
% [237/345] cross_plane_check..program_at_lowering .. passed (0.001 sec)
% [238/345] cross_plane_check.._engine_value_guard .. passed (0.000 sec)
% [239/345] cross_plane_check..epted_at_both_doors .. passed (0.001 sec)
% [240/345] cross_plane_check..lumn_stays_accepted .. passed (0.000 sec)
% [241/345] cross_plane_check..epted_by_both_doors .. passed (0.000 sec)
% [242/345] refusal_messages:..al_renders_one_line .. passed (0.002 sec)
% [243/345] declaration_query..agrees_across_doors .. passed (0.000 sec)
% [244/345] declaration_query..agrees_across_doors .. passed (0.000 sec)
% [245/345] expansion_order:declared_phase_order .... passed (0.000 sec)
% [246/345] expansion_order:s..es_are_placeholders .. passed (0.000 sec)
% [247/345] expansion_order:a..d_rewrites_the_rule .. passed (0.000 sec)
% [248/345] expansion_order:a.._language_and_query .. passed (0.000 sec)
% [249/345] expansion_order:a..ry_must_be_a_string .. passed (0.000 sec)
% [250/345] expansion_order:a..guage_is_restricted .. passed (0.000 sec)
% [251/345] expansion_order:a..ust_be_a_known_atom .. passed (0.000 sec)
% [252/345] expansion_order:a..ejects_single_quote .. passed (0.000 sec)
% [253/345] expansion_order:a..res_a_named_capture .. passed (0.000 sec)
% [254/345] expansion_order:s..r_rule_cursor_block ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:2958:
Warning: [Thread main]     test expansion_order:seq_expands_to_the_shared_four_rule_cursor_block: Test succeeded with choicepoint
% [255/345] expansion_order:s..vel_rule_is_refused .. passed (0.000 sec)
% [256/345] expansion_order:c..reads_the_bare_atom ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:2994:
Warning: [Thread main]     test expansion_order:coalesce_level_arm_reads_the_bare_atom: Test succeeded with choicepoint
% [257/345] expansion_order:c..stead_of_triggering ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3009:
Warning: [Thread main]     test expansion_order:coalesce_edge_arm_samples_instead_of_triggering: Test succeeded with choicepoint
% [258/345] expansion_order:c..on_spine_is_refused .. passed (0.000 sec)
% [259/345] expansion_order:l..s_target_membership ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3032:
Warning: [Thread main]     test expansion_order:level_relation_value_adds_target_membership: Test succeeded with choicepoint
% [260/345] expansion_order:e..s_target_membership ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3041:
Warning: [Thread main]     test expansion_order:edge_relation_value_samples_target_membership: Test succeeded with choicepoint
% [261/345] expansion_order:e..p_is_not_duplicated ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3051:
Warning: [Thread main]     test expansion_order:existing_target_membership_is_not_duplicated: Test succeeded with choicepoint
% [262/345] expansion_order:e..rves_expanded_terms ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3063:
Warning: [Thread main]     test expansion_order:enum_first_preserves_expanded_terms: Test succeeded with choicepoint
% [263/345] expansion_order:e..nonexhaustive_match .. passed (0.000 sec)
% [264/345] expansion_order:c..erated_variant_refs ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3078:
Warning: [Thread main]     test expansion_order:context_carries_generated_variant_refs: Test succeeded with choicepoint
% [265/345] expansion_order:p..m_has_empty_context ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3085:
Warning: [Thread main]     test expansion_order:program_without_enum_has_empty_context: Test succeeded with choicepoint
% [266/345] expansion_order:d.._and_match_together ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3093:
Warning: [Thread main]     test expansion_order:driver_expands_enum_and_match_together: Test succeeded with choicepoint
% [267/345] expression_invent..y_the_expected_rows .. passed (0.000 sec)
% [268/345] expression_invent..s_with_surface_rows .. passed (0.000 sec)
% [269/345] expression_invent..recognizer_is_total .. passed (0.000 sec)
% [270/345] expression_invent..c_row_lowers_to_sql .. passed (0.000 sec)
% [271/345] expression_invent..wers_sign_corrected .. passed (0.000 sec)
% [272/345] expression_invent..ii_character_filter .. passed (0.000 sec)
% [273/345] expression_invent..ses_integer_operand .. passed (0.000 sec)
% [274/345] expression_invent.._is_a_guard_surface ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3185:
Warning: [Thread main]     test expression_inventory:regexp_is_a_guard_surface: Test succeeded with choicepoint
% [275/345] expression_invent..owers_to_sql_regexp .. passed (0.001 sec)
% [276/345] expression_invent..agrees_across_doors .. passed (0.000 sec)
% [277/345] expression_invent..agrees_across_doors .. passed (0.000 sec)
% [278/345] expression_invent..to_its_sql_operator .. passed (0.000 sec)
% [279/345] expression_invent..omes_from_the_table .. passed (0.000 sec)
% [280/345] phase5_value_plan..ut_surface_wrappers ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3234:
Warning: [Thread main]     test phase5_value_plane:parser_and_printer_round_trip_bool_and_float_without_surface_wrappers: Test succeeded with choicepoint
% [281/345] phase5_value_plan..ool_literal_witness .. passed (0.000 sec)
% [282/345] phase5_value_plan..nstraints_are_exact .. passed (0.001 sec)
% [283/345] phase5_value_plan..ite_real_operations .. passed (0.001 sec)
% [284/345] phase5_value_plan.._scan_state_numeric ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3281:
Warning: [Thread main]     test phase5_value_plane:arithmetic_operator_constraint_keeps_unwitnessed_scan_state_numeric: Test succeeded with choicepoint
% [285/345] oracle_aggregate_..an_oracle_aggregate .. passed (0.000 sec)
% [286/345] oracle_aggregate_..ered_aggregate_rows .. passed (0.000 sec)
% [287/345] oracle_aggregate_..hree_distinct_roles ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3364:
Warning: [Thread main]     test oracle_aggregate_classification:aggregate_axis_carries_three_distinct_roles: Test succeeded with choicepoint
% [288/345] oracle_aggregate_.._live_in_the_oracle .. passed (0.000 sec)
% [289/345] oracle_aggregate_..is_not_an_aggregate .. passed (0.000 sec)
% [290/345] oracle_aggregate_..rgument_stays_plain .. passed (0.000 sec)
% [291/345] relation_depth_lo..negation_is_refused .. passed (0.000 sec)
% [292/345] relation_depth_lo..dge_rule_is_refused .. passed (0.000 sec)
% [293/345] relation_depth_lo.._one_join_per_level .. passed (0.001 sec)
% [294/345] relation_depth_lo.._no_dictionary_join ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3519:
Warning: [Thread main]     test relation_depth_lowering:head_value_that_is_a_body_atom_needs_no_dictionary_join: Test succeeded with choicepoint
% [295/345] json_grammar:unqu..entifier_keys_parse .. passed (0.000 sec)
% [296/345] json_grammar:trai.._comma_is_not_taken .. passed (0.000 sec)
% [297/345] json_grammar:both..s_give_the_same_key .. passed (0.000 sec)
% [298/345] json_grammar:quot.._is_a_literal_label .. passed (0.000 sec)
% [299/345] json_grammar:dollar_marks_a_key_hole .... passed (0.000 sec)
% [300/345] json_grammar:doll..the_bare_identifier .. passed (0.000 sec)
% [301/345] json_grammar:desc.._to_the_quoted_atom .. passed (0.000 sec)
% [302/345] json_grammar:gh_cache_flagship_parses ... passed (0.000 sec)
% [303/345] json_grammar:empt..the_arity_zero_atom .. passed (0.000 sec)
% [304/345] json_grammar:empt..y_is_the_empty_list .. passed (0.000 sec)
% [305/345] json_grammar:tagg..ith_a_named_refusal .. passed (0.000 sec)
% [306/345] json_grammar:unde..ith_a_named_refusal .. passed (0.000 sec)
% [307/345] json_grammar:ever..duction_round_trips .. passed (0.002 sec)
% [308/345] json_grammar:non_..r_key_prints_quoted .. passed (0.000 sec)
% [309/345] json_grammar:type..s_to_a_nested_colon .. passed (0.000 sec)
% [310/345] json_grammar:typed_capture_round_trips .. passed (0.001 sec)
% [311/345] json_grammar:unty..till_prints_untyped .. passed (0.000 sec)
% [312/345] json_grammar:capt.._agree_across_doors .. passed (0.000 sec)
% [313/345] json_grammar:unkn..sed_by_the_compiler .. passed (0.000 sec)
% [314/345] json_grammar:unkn..fused_by_the_oracle .. passed (0.000 sec)
% [315/345] json_grammar:orac..ir_json_type_answer .. passed (0.000 sec)
% [316-1/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-2/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-3/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-4/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-5/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-6/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-7/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-8/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-9/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-10/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-11/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-12/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-13/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-14/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-15/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-16/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-17/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-18/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-19/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-20/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-21/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-22/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-23/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-24/345] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [316-25/345] parse_error_posit..l_position_is_exact .. passed (0.003 sec)
% [317/345] parse_error_posit.._with_a_prefix_walk .. passed (0.000 sec)
% [318/345] dot_member_access..s_to_a_dot_get_nest .. passed (0.000 sec)
% [319/345] dot_member_access..rses_to_one_dot_get .. passed (0.000 sec)
% [320/345] dot_member_access..es_to_the_same_nest .. passed (0.000 sec)
% [321/345] dot_member_access..ot_still_terminates .. passed (0.000 sec)
% [322/345] dot_member_access..tatement_terminator .. passed (0.000 sec)
% [323/345] dot_member_access.._terminator_reading .. passed (0.000 sec)
% [324/345] dot_member_access..tays_a_syntax_error .. passed (0.000 sec)
% [325/345] dot_member_access..rals_are_unaffected .. passed (0.000 sec)
% [326/345] dot_member_access..through_the_printer .. passed (0.000 sec)
% [327/345] dot_member_access..through_the_printer .. passed (0.000 sec)
% [328/345] dot_member_access.._to_a_nested_decode ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3978:
Warning: [Thread main]     test dot_member_access:bound_head_member_desugars_to_a_nested_decode: Test succeeded with choicepoint
% [329/345] dot_member_access..e_author_could_type ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3986:
Warning: [Thread main]     test dot_member_access:head_dot_expands_to_the_brace_body_the_author_could_type: Test succeeded with choicepoint
% [330/345] dot_member_access..e_brace_decode_goal ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:3995:
Warning: [Thread main]     test dot_member_access:whole_rhs_bind_expands_to_the_brace_decode_goal: Test succeeded with choicepoint
% [331/345] dot_member_access..des_after_that_atom ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:4004:
Warning: [Thread main]     test dot_member_access:member_inside_a_relation_atom_decodes_after_that_atom: Test succeeded with choicepoint
% [332/345] dot_member_access..des_before_the_bind ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:4009:
Warning: [Thread main]     test dot_member_access:member_inside_a_bind_expression_decodes_before_the_bind: Test succeeded with choicepoint
% [333/345] dot_member_access..goal_still_resolves ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:4019:
Warning: [Thread main]     test dot_member_access:receiver_bound_by_a_later_goal_still_resolves: Test succeeded with choicepoint
% [334/345] dot_member_access..it_and_never_writes ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:4030:
Warning: [Thread main]     test dot_member_access:dot_on_the_bind_left_side_reads_it_and_never_writes: Test succeeded with choicepoint
% [335/345] dot_member_access.._returned_unchanged ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-codex-cstnative/v6/prolog/compile/test/plunit_tests.pl:4037:
Warning: [Thread main]     test dot_member_access:a_rule_without_a_dot_is_returned_unchanged: Test succeeded with choicepoint
% [336/345] dot_member_access..ind_refuses_by_name .. passed (0.000 sec)
% [337/345] dot_member_access..ead_refuses_by_name .. passed (0.000 sec)
% [338/345] dot_member_access..with_the_whole_path .. passed (0.000 sec)
% [339/345] dot_member_access..on_is_a_parse_error .. passed (0.000 sec)
% [340/345] dot_member_access..oal_refuses_by_name .. passed (0.000 sec)
% [341/345] fact_seeding:dl6_fact_seeds_initial ..... passed (0.003 sec)
% [342/345] fact_seeding:dl6_..t_nonground_refuses .. passed (0.001 sec)
% [343/345] fact_seeding:dl6_fact_derives ........... passed (0.003 sec)
% [344/345] fact_seeding:dl6_..eds_with_query_form .. passed (0.003 sec)
% [345/345] fact_seeding:rege..int_column_compiles .. passed (0.003 sec)
ERROR: [Thread main] 1 test failed
error: recipe `plunit` failed on line 53 with exit code 1
```
