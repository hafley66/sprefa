# LANE catrel REPORT

Base commit: 94524991c1b1c21f56c80f59fd304c5db6dbe680 (branch lane/catrel)

## Deviations

- `sweep.sh` exits 1 on a trailing "working-tree manifest: duplicate fixture
  name enum_decl_variant_rows_round_trip_through_tag_view" error. Pre-existing:
  reproduced identically on the base commit 94524991 with my changes stashed.
  The RUN/FINAL receipt itself is unchanged: RUN total=420 identical=418
  wrong=0 final_wrong=0. Not caused by this lane.
- Commit 1 includes `v6/INDEX.md` (20 deletions). The repo's pre-commit hook
  (sprefa/.githooks/pre-commit) runs gen-index.sh and stages the regenerated
  index on every commit; I did not edit it directly. Contents are the removal of
  stale `out/` entries, unrelated to the catalog rename.
- tsv2 required dependencies to be installed before `pnpm test` could run:
  `pnpm install` in both `v6/tsv2` and `v6/sprefa-store/js` (the node_modules
  tree was absent). Also required building the release `extract` binary
  (`cargo build --release --features cli --bin extract` in v6/sprefa-extract),
  which the pre-commit comment-budget rail needs to exist. These are
  environment setup, not source edits.


## Commits (in order)

| # | sha | subject |
|---|---|---|
| 1 | 3a4a628ef562332b366b74dca671c979b938a762 | catalog: rename __catalog_rel to __rel |
| 2 | 1a43106a70465c3ae5d6a28a0b6ff7fd8f4e267b | catalog: arity column, and refuse two arities of one rel name |

## Grep receipt

`grep -rn "__catalog_rel" v6/prolog v6/tsv2 | grep -v ARCH.pl` prints NOTHING (empty). The single remaining repo-wide occurrence is in v6/prolog/ARCH.pl (1), excluded by design.

## Validation stdout

Rail verdicts: plunit 352/352, conformance 302/0, TEXT_DOOR 420/420, prolog-lint findings=1 baseline=1, tsv2 149/1/0, sweep wrong=0 final_wrong=0.

### 1. plunit (swipl -q -l test/plunit_tests.pl -g run_tests -g halt)

```
Warning: /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/2_subscribe.plt:81:
Warning:    Singleton variables: [Id]
Warning: /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3293:
Warning:    Singleton variables: [Pattern]
% [1/352] clock_checker:reg.._roles_are_complete .... passed (0.002 sec)
% [2/352] clock_checker:edg.._labels_and_offsets .... passed (0.000 sec)
% [3/352] clock_checker:sin.._fact_are_queryable .... passed (0.000 sec)
% [4/352] clock_checker:pip..atch_observed_ticks .... passed (0.001 sec)
% [5/352] clock_checker:equ..rade_diamond_passes .... passed (0.000 sec)
% [6/352] clock_checker:une..ade_diamond_refuses .... passed (0.000 sec)
% [7/352] clock_checker:pos..scc_is_constructive .... passed (0.000 sec)
% [8/352] clock_checker:pos..y_scc_is_productive .... passed (0.000 sec)
% [9/352] clock_checker:clo..ear_in_chain_length .... passed (0.004 sec)
% [10/352] clock_checker:clo.._in_parallel_routes ... passed (0.003 sec)
% [11/352] clock_checker:zer..egative_scc_refuses ... passed (0.000 sec)
% [12/352] clock_checker:com.._runs_clock_checker ... passed (0.000 sec)
% [13/352] clock_checker:ora.._runs_clock_checker ... passed (0.000 sec)
% [14/352] clock_checker:fiv..classes_are_derived ... passed (0.000 sec)
% [15/352] clock_checker:his..ed_ids_and_programs ... passed (0.000 sec)
% [16/352] clock_checker:his.._partition_is_exact ... passed (0.000 sec)
% [17/352] clock_checker:his.._partition_is_exact ... passed (0.000 sec)
% [18/352] clock_checker:a2_..t_provable_boundary ... passed (0.000 sec)
% [19/352] clock_checker:sin..gger_batch_boundary ... passed (0.000 sec)
% [20/352] clock_checker:a4_..no_rule_clock_claim ... passed (0.000 sec)
% [21/352] clock_checker:a5_..arity_refs_distinct ... passed (0.000 sec)
% [22/352] clock_checker:a6_..untime_crosschecked ... passed (0.001 sec)
% [23/352] clock_checker:a7_..no_rule_clock_claim ... passed (0.000 sec)
% [24/352] clock_checker:a8_..ing_partition_clock ... passed (0.000 sec)
% [25/352] clock_checker:a9_..no_rule_clock_claim ... passed (0.000 sec)
% [26/352] clock_checker:a11..ggregate_dependency ... passed (0.000 sec)
% [27/352] clock_checker:a4_..g_b_and_stops_there ... passed (0.000 sec)
% [28/352] clock_checker:a12..eplay_from_sampling ... passed (0.001 sec)
% [29/352] clock_checker:c2_.._with_observed_tick ... passed (0.001 sec)
% [30/352] clock_checker:d1_..e_edge_headed_plane ... passed (0.002 sec)
% [31/352] clock_checker:liv..abelled_not_refused ... passed (0.000 sec)
% [32/352] clock_checker:two..d_is_a_race_not_one ... passed (0.001 sec)
% [33-1/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-2/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-3/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-4/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-5/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-6/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-7/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-8/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-9/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-10/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [33-11/352] graph_module:comp..tch_warshall_oracle .. passed (0.000 sec)
% [34-1/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-2/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-3/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-4/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-5/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-6/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-7/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-8/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-9/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-10/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [34-11/352] graph_module:comp..tition_the_vertices .. passed (0.000 sec)
% [35/352] graph_module:cycl.._acyclic_singletons ... passed (0.000 sec)
% [36/352] graph_module:self.._a_cyclic_component ... passed (0.000 sec)
% [37/352] graph_module:acyc.._a_cyclic_component ... passed (0.000 sec)
% [38/352] graph_module:comp.._by_smallest_member ... passed (0.000 sec)
% [39/352] graph_module:comp..ontaining_component ... passed (0.000 sec)
% [40/352] graph_module:clos..ict_positive_length ... passed (0.000 sec)
% [41/352] graph_module:clos..s_a_node_on_a_cycle ... passed (0.000 sec)
% [42/352] graph_module:topo..ical_order_on_a_dag ... passed (0.000 sec)
% [43/352] graph_module:topo..er_fails_on_a_cycle ... passed (0.000 sec)
% [44/352] graph_module:topo..ails_on_a_self_loop ... passed (0.000 sec)
% [45/352] graph_module:isol..urvive_construction ... passed (0.000 sec)
% [46/352] graph_module:duplicate_edges_collapse .... passed (0.000 sec)
% [47/352] graph_module:comp..e_with_connectivity ... passed (0.000 sec)
% [48/352] diag_channel:one_based_to_zero_based ..... passed (0.000 sec)
% [49-1/352] diag_channel:json..e_equals_human_line .. passed (0.004 sec)
% [50/352] diag_channel:reco..round_trips_as_json ... passed (0.000 sec)
% [51/352] diag_channel:refu..not_earlier_mention ... passed (0.000 sec)
% [52/352] diag_channel:refu.._statement_position ... passed (0.000 sec)
% [53/352] diag_channel:pars.._is_exact_in_record ... passed (0.000 sec)
% [54/352] diag_channel:uri_..encoded_file_scheme ... passed (0.000 sec)
% [55/352] subscribe_cone:ze.._subscribes_nothing ... passed (0.000 sec)
% [56/352] subscribe_cone:hand_computed_cone ........ passed (0.000 sec)
% [57/352] subscribe_cone:sampler_included .......... passed (0.000 sec)
% [58/352] subscribe_cone:negation_included ......... passed (0.000 sec)
% [59/352] subscribe_cone:de..the_real_decl_forms ... passed (0.000 sec)
% [60/352] subscribe_cone:de..to_a_queryless_cone ... passed (0.000 sec)
% [61/352] subscribe_cone:ed..chain_including_pre ... passed (0.000 sec)
% [62/352] subscribe_cone:ne..and_combine_spliced ... passed (0.000 sec)
% [63/352] subscribe_cone:re..ides_what_a_read_is ... passed (0.000 sec)
% [64/352] subscribe_cone:go..lex_cone_invariants ... passed (0.287 sec)
% [65/352] subscribe_cone:em..ath_behind_the_flag ... passed (0.003 sec)
% [66/352] subscribe_cone:em..ents_name_their_rel ... passed (0.002 sec)
% [67/352] subscribe_cone:em.._hand_computed_cone ... passed (0.002 sec)
% [68/352] subscribe_cone:ze.._subscribes_nothing ... passed (0.002 sec)
% [69/352] stratum_order:swi..d_replace_one_group ... passed (0.001 sec)
% [70/352] stratum_order:dem.._laziness_one_group ... passed (0.001 sec)
% [71/352] stratum_order:swi.._replace_rule_order ... passed (0.001 sec)
% [72/352] stratum_order:dem..laziness_rule_order ... passed (0.001 sec)
% [73/352] stratum_order:sel..remains_in_p2_order ... passed (0.000 sec)
% [74/352] column_naming:swi..yed_replace_columns ... passed (0.001 sec)
% [75/352] column_naming:demand_laziness_columns .... passed (0.001 sec)
% [76/352] sql_text_snapshot..ed_replace_edge_sql ... passed (0.002 sec)
% [77/352] sql_text_snapshot..eplace_ddl_pk_shape ... passed (0.002 sec)
% [78/352] sql_text_snapshot..straint_and_replace ... passed (0.001 sec)
% [79/352] sql_text_snapshot..eplace_frontier_ddl ... passed (0.002 sec)
% [80/352] sql_text_snapshot..dered_snapshot_read ... passed (0.001 sec)
% [81/352] sql_text_snapshot.._mirrors_each_write ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:297:
Warning: [Thread main]     test sql_text_snapshots:ordered_pre_snapshots_once_then_mirrors_each_write: Test succeeded with choicepoint
% [82/352] sql_text_snapshot..d_replace_level_sql ... passed (0.002 sec)
% [83/352] sql_text_snapshot..iness_no_edge_rules ... passed (0.001 sec)
% [84/352] sql_text_snapshot..one_batch_statement ... passed (0.001 sec)
% [85/352] sql_text_snapshot.._laziness_level_sql ... passed (0.001 sec)
% [86/352] sql_text_snapshot..s_promoted_frontier ... passed (0.001 sec)
% [87/352] sql_text_snapshot.._same_tick_frontier ... passed (0.001 sec)
% [88/352] sql_text_snapshot..l_column_expr_shape ... passed (0.000 sec)
% [89/352] sql_text_snapshot..elta_sql_open_scope ... passed (0.002 sec)
% [90/352] sql_text_snapshot..ql_route_change_log ... passed (0.002 sec)
% [91/352] sql_text_snapshot..n_both_sql_families ... passed (0.001 sec)
% [92/352] sql_text_snapshot.._departure_frontier ... passed (0.001 sec)
% [93/352] sql_text_snapshot..with_key_predicates ... passed (0.001 sec)
% [94/352] incremental_mode:..gram_is_incremental ... passed (0.002 sec)
% [95/352] incremental_mode:..cremental_reconcile ... passed (0.003 sec)
% [96/352] incremental_mode:..remental_carry_path ... passed (0.001 sec)
% [97/352] incremental_mode:..e_referee_available ... passed (0.002 sec)
% [98/352] incremental_mode:..tements_are_emitted ... passed (0.001 sec)
% [99/352] incremental_mode:..ecursive_cte_reseed ... passed (0.000 sec)
% [100/352] incremental_mode:..son_batch_statement .. passed (0.001 sec)
% [101/352] catalog_g1:catalog_absent_by_default .... passed (0.001 sec)
% [102/352] catalog_g1:catalog_table_shape .......... passed (0.001 sec)
% [103/352] catalog_g1:catalo..r_an_arrival_target .. passed (0.000 sec)
% [104/352] catalog_g1:catalog_gate_is_arity_exact .. passed (0.001 sec)
% [105/352] catalog_g1:catalo..s_are_one_statement .. passed (0.001 sec)
% [106/352] catalog_g1:catalog_ids_are_positional ... passed (0.001 sec)
% [107/352] catalog_g1:refuse..ies_of_one_rel_name .. passed (0.000 sec)
% [108/352] supported_subset_..ount_aggregate_head ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:669:
Warning: [Thread main]     test supported_subset_gate:accepts_count_aggregate_head: Test succeeded with choicepoint
% [109/352] supported_subset_.._variable_separator .. passed (0.000 sec)
% [110/352] supported_subset_..ate_non_int_ordinal .. passed (0.001 sec)
% [111/352] supported_subset_..gregate_wrong_arity .. passed (0.000 sec)
% [112/352] supported_subset_..rray_aggregate_head .. passed (0.000 sec)
% [113/352] supported_subset_..ject_aggregate_head .. passed (0.000 sec)
% [114/352] supported_subset_..ding_aggregate_head .. passed (0.000 sec)
% [115/352] supported_subset_..tern_on_arrival_rel .. passed (0.000 sec)
% [116/352] supported_subset_..tern_on_derived_rel ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:761:
Warning: [Thread main]     test supported_subset_gate:accepts_compound_pattern_on_derived_rel: Test succeeded with choicepoint
% [117/352] supported_subset_..eps_its_own_refusal .. passed (0.001 sec)
% [118/352] supported_subset_..tern_on_arrival_rel ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:795:
Warning: [Thread main]     test supported_subset_gate:accepts_bool_literal_pattern_on_arrival_rel: Test succeeded with choicepoint
% [119/352] supported_subset_..uard_under_negation .. passed (0.000 sec)
% [120/352] supported_subset_..d_atom_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:824:
Warning: [Thread main]     test supported_subset_gate:accepts_negated_atom_in_edge_body: Test succeeded with choicepoint
% [121/352] supported_subset_..nction_in_edge_body .. passed (0.000 sec)
% [122/352] supported_subset_..d_bind_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:840:
Warning: [Thread main]     test supported_subset_gate:accepts_comparison_and_bind_in_edge_body: Test succeeded with choicepoint
% [123/352] supported_subset_..e_atom_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:847:
Warning: [Thread main]     test supported_subset_gate:accepts_plain_pre_atom_in_edge_body: Test succeeded with choicepoint
% [124/352] supported_subset_..ts_now_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:864:
Warning: [Thread main]     test supported_subset_gate:accepts_now_in_edge_body: Test succeeded with choicepoint
% [125/352] supported_subset_..n_variable_argument .. passed (0.000 sec)
% [126/352] supported_subset_..s_now_in_level_rule .. passed (0.000 sec)
% [127/352] supported_subset_..typed_from_its_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:896:
Warning: [Thread main]     test supported_subset_gate:accepts_edge_head_column_typed_from_its_body: Test succeeded with choicepoint
% [128/352] supported_subset_.._still_gets_a_table ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:907:
Warning: [Thread main]     test supported_subset_gate:initial_only_ref_still_gets_a_table: Test succeeded with choicepoint
% [129/352] supported_subset_..rival_fed_level_rel ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:926:
Warning: [Thread main]     test supported_subset_gate:accepts_edge_join_against_an_arrival_fed_level_rel: Test succeeded with choicepoint
% [130/352] supported_subset_.._plane_before_edges .. passed (0.004 sec)
% [131/352] supported_subset_.._edge_fed_level_rel ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:961:
Warning: [Thread main]     test supported_subset_gate:accepts_edge_join_against_an_edge_fed_level_rel: Test succeeded with choicepoint
% [132/352] supported_subset_.._is_integer_storage ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:976:
Warning: [Thread main]     test supported_subset_gate:now_bound_head_column_is_integer_storage: Test succeeded with choicepoint
% [133/352] supported_subset_..sample_in_edge_body ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:997:
Warning: [Thread main]     test supported_subset_gate:accepts_latest_plain_rel_sample_in_edge_body: Test succeeded with choicepoint
% [134/352] supported_subset_..nction_in_edge_body .. passed (0.000 sec)
% [135/352] supported_subset_..on_level_headed_rel .. passed (0.000 sec)
% [136/352] supported_subset_..atest_in_level_rule .. passed (0.000 sec)
% [137/352] supported_subset_..s_pre_in_level_rule .. passed (0.000 sec)
% [138/352] supported_subset_..keep_on_non_log_rel .. passed (0.000 sec)
% [139/352] supported_subset_..erived_edge_trigger ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1062:
Warning: [Thread main]     test supported_subset_gate:accepts_level_derived_edge_trigger: Test succeeded with choicepoint
% [140/352] supported_subset_..erived_edge_trigger ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1069:
Warning: [Thread main]     test supported_subset_gate:accepts_edge_derived_edge_trigger: Test succeeded with choicepoint
% [141/352] expression_miscom..t_not_text_collapse .. passed (0.001 sec)
% [142/352] expression_miscom..t_not_text_collapse .. passed (0.001 sec)
% [143/352] expression_miscom..t_column_stays_text .. passed (0.000 sec)
% [144/352] expression_miscom..eat_column_affinity .. passed (0.001 sec)
% [145/352] expression_miscom..mparison_is_refused .. passed (0.001 sec)
% [146/352] expression_miscom..ype_join_is_refused .. passed (0.001 sec)
% [147/352] expression_miscom..ype_reaches_the_ddl .. passed (0.001 sec)
% [148/352] expression_miscom.._floored_correction .. passed (0.001 sec)
% [149/352] enum_decl_expansi..semicolon_enum_decl ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1221:
Warning: [Thread main]     test enum_decl_expansion:parser_retains_semicolon_enum_decl: Test succeeded with choicepoint
% [150/352] enum_decl_expansi.._rels_and_tag_union .. passed (0.000 sec)
% [151/352] enum_decl_expansi..iant_name_collision .. passed (0.000 sec)
% [152/352] enum_decl_expansi..ger_keyed_edge_head ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1262:
Warning: [Thread main]     test enum_decl_expansion:enum_tag_view_can_trigger_keyed_edge_head: Test succeeded with choicepoint
% [153/352] match_block:share..dinary_rule_per_arm .. passed (0.000 sec)
% [154/352] match_block:enum_..uires_every_variant .. passed (0.000 sec)
% [155/352] match_block:keyed..med_compile_refusal .. passed (0.000 sec)
% [156/352] match_block:key_p..med_compile_refusal .. passed (0.000 sec)
% [157/352] match_block:key_p..med_compile_refusal .. passed (0.000 sec)
% [158/352] match_block:dupli..med_compile_refusal .. passed (0.000 sec)
% [159/352] match_block:keyed..d_remains_supported ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1337:
Warning: [Thread main]     test match_block:keyed_edge_head_remains_supported: Test succeeded with choicepoint
% [160/352] match_block:match.._left_to_right_arms ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1347:
Warning: [Thread main]     test match_block:match_surface_round_trips_with_prefix_semicolon_and_left_to_right_arms: Test succeeded with choicepoint
% [161/352] match_block:match..ut_prefix_semicolon ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1367:
Warning: [Thread main]     test match_block:match_surface_allows_first_arm_without_prefix_semicolon: Test succeeded with choicepoint
% [162/352] match_block:seq_s.._parser_and_printer ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1378:
Warning: [Thread main]     test match_block:seq_surface_round_trips_through_parser_and_printer: Test succeeded with choicepoint
% [163/352] match_block:match.._first_arm_spelling .. passed (0.000 sec)
% [164/352] match_block:sugar..er_to_identical_sql .. passed (0.003 sec)
% [165/352] match_block:reten..ed_delete_statement .. passed (0.000 sec)
% [166/352] hosts_wiring:sele..surface_round_trips ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1417:
Warning: [Thread main]     test hosts_wiring:selected_surface_round_trips: Test succeeded with choicepoint
% [167/352] hosts_wiring:host..ull_type_vocabulary ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1463:
Warning: [Thread main]     test hosts_wiring:host_input_and_bind_columns_read_the_full_type_vocabulary: Test succeeded with choicepoint
% [168/352] hosts_wiring:host..ill_a_named_refusal ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1478:
Warning: [Thread main]     test hosts_wiring:host_input_column_wrapper_is_still_a_named_refusal: Test succeeded with choicepoint
% [169/352] hosts_wiring:rhs_.._marker_is_rejected .. passed (0.000 sec)
% [170/352] hosts_wiring:rhs_.._marker_is_rejected .. passed (0.000 sec)
% [171/352] hosts_wiring:plai..n_order_independent ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1500:
Warning: [Thread main]     test hosts_wiring:plain_host_resolution_is_declaration_order_independent: Test succeeded with choicepoint
% [172/352] hosts_wiring:remo..surface_is_rejected .. passed (0.000 sec)
% [173/352] hosts_wiring:plai..mains_relation_atom ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1524:
Warning: [Thread main]     test hosts_wiring:plain_non_host_rhs_remains_relation_atom: Test succeeded with choicepoint
% [174/352] hosts_wiring:plai..sting_named_refusal .. passed (0.000 sec)
% [175/352] hosts_wiring:remo..keyword_is_rejected .. passed (0.000 sec)
% [176/352] hosts_wiring:refe..arks_reference_edge ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1543:
Warning: [Thread main]     test hosts_wiring:referenced_rel_remains_queryable_and_marks_reference_edge: Test succeeded with choicepoint
% [177/352] hosts_wiring:name..omissions_are_fresh ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1555:
Warning: [Thread main]     test hosts_wiring:named_body_omissions_are_fresh: Test succeeded with choicepoint
% [178/352] hosts_wiring:name..ial_head_is_refused ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1567:
Warning: [Thread main]     test hosts_wiring:named_partial_head_is_refused: Test succeeded with choicepoint
% [179/352] hosts_wiring:host..enced_input_refusal .. passed (0.000 sec)
% [180/352] hosts_wiring:host..bsent_from_template .. passed (0.000 sec)
% [181/352] hosts_wiring:host..ypes_not_name_alone .. passed (0.000 sec)
% [182/352] hosts_wiring:host..t_reference_refusal .. passed (0.000 sec)
% [183/352] hosts_wiring:host..nown_column_refusal .. passed (0.000 sec)
% [184/352] hosts_wiring:host..ame_is_not_a_column .. passed (0.000 sec)
% [185/352] hosts_wiring:extr..iler_known_executor .. passed (0.000 sec)
% [186/352] hosts_wiring:name..e_selected_executor .. passed (0.000 sec)
% [187/352] hosts_wiring:extr..uses_non_path_input .. passed (0.000 sec)
% [188/352] hosts_wiring:host_overlap_refusal ....... passed (0.000 sec)
% [189/352] hosts_wiring:host..cate_column_refusal .. passed (0.000 sec)
% [190/352] hosts_wiring:host..s_and_lowers_as_ref .. passed (0.005 sec)
% [191/352] hosts_wiring:host..efuses_by_type_name .. passed (0.001 sec)
% [192/352] hosts_wiring:probe_arity_refusal ........ passed (0.000 sec)
% [193/352] hosts_wiring:bind..d_rule_head_refusal .. passed (0.000 sec)
% [194/352] hosts_wiring:nati..ts_query_exact_text .. passed (0.000 sec)
% [195/352] hosts_wiring:nati.._parses_to_ts_query ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1762:
Warning: [Thread main]     test hosts_wiring:native_cst_block_parses_to_ts_query: Test succeeded with choicepoint
% [196/352] hosts_wiring:nati..round_trips_fixture ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1779:
Warning: [Thread main]     test hosts_wiring:native_cst_query_round_trips_fixture: Test succeeded with choicepoint
% [197/352] hosts_wiring:native_cst_capture_unused .. passed (0.000 sec)
% [198/352] hosts_wiring:nati..variable_uncaptured .. passed (0.000 sec)
% [199/352] hosts_wiring:nati.._uses_regexp_subset .. passed (0.000 sec)
% [200/352] hosts_wiring:emit..lans_and_demand_sql .. passed (0.008 sec)
% [201/352] hosts_wiring:quer..and_bound_positions .. passed (0.002 sec)
% [202/352] hosts_wiring:back..low_the_stated_rule ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1892:
Warning: [Thread main]     test hosts_wiring:backslash_escapes_follow_the_stated_rule: Test succeeded with choicepoint
% [203/352] hosts_wiring:back..s_print_and_reparse ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:1914:
Warning: [Thread main]     test hosts_wiring:backslash_survives_print_and_reparse: Test succeeded with choicepoint
% [204/352] hosts_wiring:rese.._the_generated_ones .. passed (0.000 sec)
% [205/352] hosts_wiring:ever..umn_refuses_by_name .. passed (0.000 sec)
% [206/352] hosts_wiring:unpr..lares_its_relations .. passed (0.000 sec)
% [207/352] body_walk_charact..ation:body_ref_uses .. passed (0.000 sec)
% [208/352] body_walk_charact..n:conjunction_goals .. passed (0.000 sec)
% [209/352] body_walk_charact..ation:trigger_items .. passed (0.000 sec)
% [210/352] body_walk_charact..ngine_finalize_refs .. passed (0.000 sec)
% [211/352] body_walk_charact..:engine_latest_refs .. passed (0.000 sec)
% [212/352] body_walk_charact..ion:engine_pre_refs .. passed (0.000 sec)
% [213/352] body_walk_charact..analyze_latest_refs .. passed (0.000 sec)
% [214/352] body_walk_charact..on:analyze_pre_refs .. passed (0.000 sec)
% [215/352] body_walk_charact..ation:goal_rel_refs .. passed (0.000 sec)
% [216/352] body_walk_characterization:body_atoms ... passed (0.000 sec)
% [217/352] body_walk_charact..reserved_constructs .. passed (0.000 sec)
% [218/352] body_walk_charact..ion:forbidden_goals .. passed (0.000 sec)
% [219/352] body_walk_charact..ion:host_body_goals .. passed (0.000 sec)
% [220/352] body_walk_charact.._latest_scans_agree .. passed (0.000 sec)
% [221/352] body_walk_charact..ler_pre_scans_agree .. passed (0.000 sec)
% [222/352] body_walk_charact.._agree_across_doors .. passed (0.000 sec)
% [223/352] body_walk_charact..atches_the_registry .. passed (0.000 sec)
% [224/352] cross_plane_check..of_range_both_doors .. passed (0.000 sec)
% [225/352] cross_plane_check..uplicate_both_doors .. passed (0.000 sec)
% [226/352] cross_plane_check..vel_head_both_doors .. passed (0.000 sec)
% [227/352] cross_plane_check..ds_differ_by_design .. passed (0.000 sec)
% [228/352] cross_plane_check..aded_rel_both_doors .. passed (0.000 sec)
% [229/352] cross_plane_check.._log_rel_both_doors .. passed (0.000 sec)
% [230/352] cross_plane_check..ict_risk_both_doors .. passed (0.000 sec)
% [231/352] cross_plane_check..epted_at_both_doors .. passed (0.000 sec)
% [232/352] cross_plane_check..fuses_at_both_doors .. passed (0.000 sec)
% [233/352] cross_plane_check..rd_refusal_payloads .. passed (0.000 sec)
% [234/352] cross_plane_check..fuses_at_both_doors .. passed (0.001 sec)
% [235/352] cross_plane_check..vel_rule_both_doors .. passed (0.000 sec)
% [236/352] cross_plane_check..vel_rule_both_doors .. passed (0.000 sec)
% [237/352] cross_plane_check..agrees_across_doors .. passed (0.000 sec)
% [238/352] cross_plane_check..d_not_latest_parity .. passed (0.000 sec)
% [239/352] cross_plane_check..sted_not_pre_parity .. passed (0.000 sec)
% [240/352] cross_plane_check..fused_by_both_doors .. passed (0.000 sec)
% [241/352] cross_plane_check..g_without_retention .. passed (0.000 sec)
% [242/352] cross_plane_check..regate_in_edge_head .. passed (0.000 sec)
% [243/352] cross_plane_check.._at_the_oracle_door .. passed (0.000 sec)
% [244/352] cross_plane_check..program_at_lowering .. passed (0.001 sec)
% [245/352] cross_plane_check.._engine_value_guard .. passed (0.000 sec)
% [246/352] cross_plane_check..epted_at_both_doors .. passed (0.001 sec)
% [247/352] cross_plane_check..lumn_stays_accepted .. passed (0.000 sec)
% [248/352] cross_plane_check..epted_by_both_doors .. passed (0.000 sec)
% [249/352] refusal_messages:..al_renders_one_line .. passed (0.002 sec)
% [250/352] declaration_query..agrees_across_doors .. passed (0.000 sec)
% [251/352] declaration_query..agrees_across_doors .. passed (0.000 sec)
% [252/352] expansion_order:declared_phase_order .... passed (0.000 sec)
% [253/352] expansion_order:s..es_are_placeholders .. passed (0.000 sec)
% [254/352] expansion_order:a..d_rewrites_the_rule .. passed (0.000 sec)
% [255/352] expansion_order:a.._language_and_query .. passed (0.000 sec)
% [256/352] expansion_order:a..ry_must_be_a_string .. passed (0.000 sec)
% [257/352] expansion_order:a..guage_is_restricted .. passed (0.000 sec)
% [258/352] expansion_order:a..ust_be_a_known_atom .. passed (0.000 sec)
% [259/352] expansion_order:a..ejects_single_quote .. passed (0.000 sec)
% [260/352] expansion_order:a..res_a_named_capture .. passed (0.000 sec)
% [261/352] expansion_order:s..r_rule_cursor_block ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3055:
Warning: [Thread main]     test expansion_order:seq_expands_to_the_shared_four_rule_cursor_block: Test succeeded with choicepoint
% [262/352] expansion_order:s..vel_rule_is_refused .. passed (0.000 sec)
% [263/352] expansion_order:c..reads_the_bare_atom ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3091:
Warning: [Thread main]     test expansion_order:coalesce_level_arm_reads_the_bare_atom: Test succeeded with choicepoint
% [264/352] expansion_order:c..stead_of_triggering ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3106:
Warning: [Thread main]     test expansion_order:coalesce_edge_arm_samples_instead_of_triggering: Test succeeded with choicepoint
% [265/352] expansion_order:c..on_spine_is_refused .. passed (0.000 sec)
% [266/352] expansion_order:l..s_target_membership ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3129:
Warning: [Thread main]     test expansion_order:level_relation_value_adds_target_membership: Test succeeded with choicepoint
% [267/352] expansion_order:e..s_target_membership ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3138:
Warning: [Thread main]     test expansion_order:edge_relation_value_samples_target_membership: Test succeeded with choicepoint
% [268/352] expansion_order:e..p_is_not_duplicated ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3148:
Warning: [Thread main]     test expansion_order:existing_target_membership_is_not_duplicated: Test succeeded with choicepoint
% [269/352] expansion_order:e..rves_expanded_terms ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3160:
Warning: [Thread main]     test expansion_order:enum_first_preserves_expanded_terms: Test succeeded with choicepoint
% [270/352] expansion_order:e..nonexhaustive_match .. passed (0.000 sec)
% [271/352] expansion_order:c..erated_variant_refs ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3175:
Warning: [Thread main]     test expansion_order:context_carries_generated_variant_refs: Test succeeded with choicepoint
% [272/352] expansion_order:p..m_has_empty_context ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3182:
Warning: [Thread main]     test expansion_order:program_without_enum_has_empty_context: Test succeeded with choicepoint
% [273/352] expansion_order:d.._and_match_together ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3190:
Warning: [Thread main]     test expansion_order:driver_expands_enum_and_match_together: Test succeeded with choicepoint
% [274/352] expression_invent..y_the_expected_rows .. passed (0.000 sec)
% [275/352] expression_invent..s_with_surface_rows .. passed (0.000 sec)
% [276/352] expression_invent..recognizer_is_total .. passed (0.000 sec)
% [277/352] expression_invent..c_row_lowers_to_sql .. passed (0.000 sec)
% [278/352] expression_invent..wers_sign_corrected .. passed (0.000 sec)
% [279/352] expression_invent..ii_character_filter .. passed (0.000 sec)
% [280/352] expression_invent..ses_integer_operand .. passed (0.000 sec)
% [281/352] expression_invent.._is_a_guard_surface ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3282:
Warning: [Thread main]     test expression_inventory:regexp_is_a_guard_surface: Test succeeded with choicepoint
% [282/352] expression_invent..owers_to_sql_regexp .. passed (0.001 sec)
% [283/352] expression_invent..agrees_across_doors .. passed (0.000 sec)
% [284/352] expression_invent..agrees_across_doors .. passed (0.000 sec)
% [285/352] expression_invent..to_its_sql_operator .. passed (0.000 sec)
% [286/352] expression_invent..omes_from_the_table .. passed (0.000 sec)
% [287/352] phase5_value_plan..ut_surface_wrappers ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3331:
Warning: [Thread main]     test phase5_value_plane:parser_and_printer_round_trip_bool_and_float_without_surface_wrappers: Test succeeded with choicepoint
% [288/352] phase5_value_plan..ool_literal_witness .. passed (0.000 sec)
% [289/352] phase5_value_plan..nstraints_are_exact .. passed (0.001 sec)
% [290/352] phase5_value_plan..ite_real_operations .. passed (0.001 sec)
% [291/352] phase5_value_plan.._scan_state_numeric ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3378:
Warning: [Thread main]     test phase5_value_plane:arithmetic_operator_constraint_keeps_unwitnessed_scan_state_numeric: Test succeeded with choicepoint
% [292/352] oracle_aggregate_..an_oracle_aggregate .. passed (0.000 sec)
% [293/352] oracle_aggregate_..ered_aggregate_rows .. passed (0.000 sec)
% [294/352] oracle_aggregate_..hree_distinct_roles ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3461:
Warning: [Thread main]     test oracle_aggregate_classification:aggregate_axis_carries_three_distinct_roles: Test succeeded with choicepoint
% [295/352] oracle_aggregate_.._live_in_the_oracle .. passed (0.000 sec)
% [296/352] oracle_aggregate_..is_not_an_aggregate .. passed (0.000 sec)
% [297/352] oracle_aggregate_..rgument_stays_plain .. passed (0.000 sec)
% [298/352] relation_depth_lo..negation_is_refused .. passed (0.000 sec)
% [299/352] relation_depth_lo..dge_rule_is_refused .. passed (0.000 sec)
% [300/352] relation_depth_lo.._one_join_per_level .. passed (0.001 sec)
% [301/352] relation_depth_lo.._no_dictionary_join ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:3616:
Warning: [Thread main]     test relation_depth_lowering:head_value_that_is_a_body_atom_needs_no_dictionary_join: Test succeeded with choicepoint
% [302/352] json_grammar:unqu..entifier_keys_parse .. passed (0.000 sec)
% [303/352] json_grammar:trai.._comma_is_not_taken .. passed (0.000 sec)
% [304/352] json_grammar:both..s_give_the_same_key .. passed (0.000 sec)
% [305/352] json_grammar:quot.._is_a_literal_label .. passed (0.000 sec)
% [306/352] json_grammar:dollar_marks_a_key_hole .... passed (0.000 sec)
% [307/352] json_grammar:doll..the_bare_identifier .. passed (0.000 sec)
% [308/352] json_grammar:desc.._to_the_quoted_atom .. passed (0.000 sec)
% [309/352] json_grammar:gh_cache_flagship_parses ... passed (0.000 sec)
% [310/352] json_grammar:empt..the_arity_zero_atom .. passed (0.000 sec)
% [311/352] json_grammar:empt..y_is_the_empty_list .. passed (0.000 sec)
% [312/352] json_grammar:tagg..ith_a_named_refusal .. passed (0.000 sec)
% [313/352] json_grammar:unde..ith_a_named_refusal .. passed (0.000 sec)
% [314/352] json_grammar:ever..duction_round_trips .. passed (0.002 sec)
% [315/352] json_grammar:non_..r_key_prints_quoted .. passed (0.000 sec)
% [316/352] json_grammar:type..s_to_a_nested_colon .. passed (0.000 sec)
% [317/352] json_grammar:typed_capture_round_trips .. passed (0.001 sec)
% [318/352] json_grammar:unty..till_prints_untyped .. passed (0.000 sec)
% [319/352] json_grammar:capt.._agree_across_doors .. passed (0.000 sec)
% [320/352] json_grammar:unkn..sed_by_the_compiler .. passed (0.000 sec)
% [321/352] json_grammar:unkn..fused_by_the_oracle .. passed (0.000 sec)
% [322/352] json_grammar:orac..ir_json_type_answer .. passed (0.000 sec)
% [323-1/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-2/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-3/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-4/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-5/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-6/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-7/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-8/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-9/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-10/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-11/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-12/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-13/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-14/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-15/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-16/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-17/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-18/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-19/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-20/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-21/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-22/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-23/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-24/352] parse_error_posit..l_position_is_exact .. passed (0.000 sec)
% [323-25/352] parse_error_posit..l_position_is_exact .. passed (0.003 sec)
% [324/352] parse_error_posit.._with_a_prefix_walk .. passed (0.000 sec)
% [325/352] dot_member_access..s_to_a_dot_get_nest .. passed (0.000 sec)
% [326/352] dot_member_access..rses_to_one_dot_get .. passed (0.000 sec)
% [327/352] dot_member_access..es_to_the_same_nest .. passed (0.000 sec)
% [328/352] dot_member_access..ot_still_terminates .. passed (0.000 sec)
% [329/352] dot_member_access..tatement_terminator .. passed (0.000 sec)
% [330/352] dot_member_access.._terminator_reading .. passed (0.000 sec)
% [331/352] dot_member_access..tays_a_syntax_error .. passed (0.000 sec)
% [332/352] dot_member_access..rals_are_unaffected .. passed (0.000 sec)
% [333/352] dot_member_access..through_the_printer .. passed (0.000 sec)
% [334/352] dot_member_access..through_the_printer .. passed (0.000 sec)
% [335/352] dot_member_access.._to_a_nested_decode ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:4075:
Warning: [Thread main]     test dot_member_access:bound_head_member_desugars_to_a_nested_decode: Test succeeded with choicepoint
% [336/352] dot_member_access..e_author_could_type ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:4083:
Warning: [Thread main]     test dot_member_access:head_dot_expands_to_the_brace_body_the_author_could_type: Test succeeded with choicepoint
% [337/352] dot_member_access..e_brace_decode_goal ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:4092:
Warning: [Thread main]     test dot_member_access:whole_rhs_bind_expands_to_the_brace_decode_goal: Test succeeded with choicepoint
% [338/352] dot_member_access..des_after_that_atom ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:4101:
Warning: [Thread main]     test dot_member_access:member_inside_a_relation_atom_decodes_after_that_atom: Test succeeded with choicepoint
% [339/352] dot_member_access..des_before_the_bind ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:4106:
Warning: [Thread main]     test dot_member_access:member_inside_a_bind_expression_decodes_before_the_bind: Test succeeded with choicepoint
% [340/352] dot_member_access..goal_still_resolves ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:4116:
Warning: [Thread main]     test dot_member_access:receiver_bound_by_a_later_goal_still_resolves: Test succeeded with choicepoint
% [341/352] dot_member_access..it_and_never_writes ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:4127:
Warning: [Thread main]     test dot_member_access:dot_on_the_bind_left_side_reads_it_and_never_writes: Test succeeded with choicepoint
% [342/352] dot_member_access.._returned_unchanged ..
Warning: [Thread main] /Users/chrishafley/projects/sprefa-lanes/catrel/v6/prolog/compile/test/plunit_tests.pl:4134:
Warning: [Thread main]     test dot_member_access:a_rule_without_a_dot_is_returned_unchanged: Test succeeded with choicepoint
% [343/352] dot_member_access..ind_refuses_by_name .. passed (0.000 sec)
% [344/352] dot_member_access..ead_refuses_by_name .. passed (0.000 sec)
% [345/352] dot_member_access..with_the_whole_path .. passed (0.000 sec)
% [346/352] dot_member_access..on_is_a_parse_error .. passed (0.000 sec)
% [347/352] dot_member_access..oal_refuses_by_name .. passed (0.000 sec)
% [348/352] fact_seeding:dl6_fact_seeds_initial ..... passed (0.003 sec)
% [349/352] fact_seeding:dl6_..t_nonground_refuses .. passed (0.001 sec)
% [350/352] fact_seeding:dl6_fact_derives ........... passed (0.003 sec)
% [351/352] fact_seeding:dl6_..eds_with_query_form .. passed (0.002 sec)
% [352/352] fact_seeding:rege..int_column_compiles .. passed (0.003 sec)

```

### 2. conformance (swipl -q -l go.pl -g go -g halt)

```
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
PASS  flagship_flow_reach_over_batched_resolved_edges
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

### 3. TEXT_DOOR (compile/scripts/text_door_receipt.sh)

```
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=0/0 plan=4/45445 lower=0/2612 boot=0/194 emit=2/11092 write=0/92 total=6/59435
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=0/0 plan=0/7712 lower=1/2608 boot=0/194 emit=1/11087 write=1/91 total=3/21692
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=1/4911 plan=0/7712 lower=1/2608 boot=0/194 emit=1/11087 write=0/91 total=3/26603
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=0/0 plan=0/7712 lower=1/2608 boot=0/194 emit=1/11087 write=0/91 total=2/21692
COMPILE-TRACE program=enum_decl_variant_rows_round_trip_through_tag_view parse=0/972 plan=1/7712 lower=0/2608 boot=0/194 emit=1/11087 write=0/91 total=2/22664
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/0 plan=0/7914 lower=1/2609 boot=0/194 emit=2/12237 write=0/91 total=3/23045
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/0 plan=0/7712 lower=1/2609 boot=0/194 emit=1/12237 write=0/91 total=2/22843
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/982 plan=1/7712 lower=0/2609 boot=0/194 emit=2/12237 write=0/91 total=3/23825
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/0 plan=1/7712 lower=0/2609 boot=0/194 emit=2/12237 write=0/91 total=3/22843
COMPILE-TRACE program=enum_decl_two_variants_union_in_tag_view parse=0/982 plan=1/7712 lower=0/2609 boot=0/194 emit=2/12237 write=0/91 total=3/23825
COMPILE-TRACE program=match_classify_response parse=0/0 plan=0/15381 lower=1/4945 boot=0/227 emit=3/23544 write=0/91 total=4/44188
COMPILE-TRACE program=match_classify_response parse=0/0 plan=1/14755 lower=1/4945 boot=0/227 emit=3/23544 write=0/91 total=5/43562
COMPILE-TRACE program=match_classify_response parse=1/6688 plan=1/14755 lower=1/4945 boot=0/227 emit=2/23544 write=1/91 total=6/50250
COMPILE-TRACE program=match_classify_response parse=0/0 plan=1/14755 lower=1/4945 boot=0/227 emit=3/23544 write=0/91 total=5/43562
COMPILE-TRACE program=match_classify_response parse=1/6687 plan=0/14755 lower=1/4945 boot=0/227 emit=3/23544 write=0/91 total=5/50249
COMPILE-TRACE program=match_classify_response_desugared parse=0/0 plan=1/15293 lower=1/4945 boot=0/227 emit=2/23544 write=1/91 total=5/44100
COMPILE-TRACE program=match_classify_response_desugared parse=0/0 plan=1/14825 lower=0/4945 boot=0/227 emit=3/23544 write=0/91 total=4/43632
COMPILE-TRACE program=match_classify_response_desugared parse=1/15665 plan=1/14825 lower=1/4945 boot=0/227 emit=2/23544 write=1/91 total=6/59297
COMPILE-TRACE program=match_classify_response_desugared parse=0/0 plan=0/14825 lower=1/4945 boot=0/227 emit=3/23544 write=0/91 total=4/43632
COMPILE-TRACE program=match_classify_response_desugared parse=1/15665 plan=1/14825 lower=1/4945 boot=0/227 emit=2/23544 write=1/91 total=6/59297
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=0/0 plan=1/4909 lower=0/1091 boot=0/158 emit=1/8316 write=1/91 total=3/14565
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=0/0 plan=0/4932 lower=1/1097 boot=0/160 emit=1/8369 write=0/91 total=2/14649
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=0/3000 plan=1/4932 lower=0/1097 boot=0/160 emit=1/8369 write=0/91 total=2/17649
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=0/0 plan=1/4932 lower=0/1097 boot=0/160 emit=1/8369 write=0/91 total=2/14649
COMPILE-TRACE program=match_edge_arm_keeps_edge_semantics parse=1/3000 plan=0/4932 lower=0/1097 boot=0/160 emit=1/8369 write=1/91 total=3/17649
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=0/0 plan=0/4862 lower=0/1091 boot=0/158 emit=1/8624 write=1/91 total=2/14826
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=0/0 plan=1/4885 lower=0/1097 boot=0/160 emit=1/8678 write=0/91 total=2/14911
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=0/4256 plan=0/4885 lower=0/1097 boot=0/160 emit=1/8678 write=1/91 total=2/19167
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=0/0 plan=0/4885 lower=0/1097 boot=0/160 emit=1/8678 write=0/91 total=1/14911
COMPILE-TRACE program=keyed_edge_head_still_replaces parse=0/4256 plan=0/4885 lower=0/1097 boot=0/160 emit=2/8678 write=0/91 total=2/19167
COMPILE-TRACE program=extraction_fork_callgraph parse=0/0 plan=2/43401 lower=3/16897 boot=0/393 emit=6/56332 write=1/91 total=12/117114
COMPILE-TRACE program=extraction_fork_callgraph parse=0/0 plan=3/42638 lower=2/16906 boot=0/283 emit=6/56216 write=1/91 total=12/116134
COMPILE-TRACE program=extraction_fork_callgraph parse=1/22431 plan=2/42638 lower=3/16906 boot=0/283 emit=6/56216 write=0/91 total=12/138565
COMPILE-TRACE program=extraction_fork_callgraph parse=0/0 plan=2/42638 lower=2/16906 boot=0/283 emit=5/56216 write=1/91 total=10/116134
COMPILE-TRACE program=extraction_fork_callgraph parse=2/22431 plan=2/42638 lower=2/16906 boot=1/283 emit=5/56216 write=1/91 total=13/138565
COMPILE-TRACE program=extraction_fork_span_line parse=0/0 plan=1/23379 lower=2/8728 boot=0/354 emit=4/39770 write=1/91 total=8/72322
COMPILE-TRACE program=extraction_fork_span_line parse=0/0 plan=2/22845 lower=1/8737 boot=0/250 emit=4/39651 write=1/91 total=8/71574
COMPILE-TRACE program=extraction_fork_span_line parse=0/10962 plan=2/22845 lower=1/8737 boot=0/250 emit=5/39651 write=0/91 total=8/82536
COMPILE-TRACE program=extraction_fork_span_line parse=0/0 plan=1/22845 lower=1/8737 boot=0/250 emit=4/39651 write=1/91 total=7/71574
COMPILE-TRACE program=extraction_fork_span_line parse=0/10962 plan=2/22845 lower=1/8737 boot=0/250 emit=4/39651 write=1/91 total=8/82536
COMPILE-TRACE program=native_ts_query_term parse=0/0 plan=2/26417 lower=2/9652 boot=0/472 emit=4/43356 write=1/91 total=9/79988
COMPILE-TRACE program=native_ts_query_term parse=0/0 plan=1/26043 lower=2/9658 boot=0/301 emit=4/43130 write=1/91 total=8/79223
COMPILE-TRACE program=native_ts_query_term parse=1/19101 plan=2/26043 lower=2/9658 boot=0/301 emit=4/43130 write=1/91 total=10/98324
COMPILE-TRACE program=native_ts_query_term parse=0/0 plan=2/26043 lower=2/9658 boot=0/301 emit=4/43130 write=1/91 total=9/79223
COMPILE-TRACE program=native_ts_query_term parse=1/19101 plan=2/26043 lower=1/9658 boot=0/301 emit=5/43130 write=0/91 total=9/98324
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=0/0 plan=1/14984 lower=1/5507 boot=0/224 emit=2/23818 write=1/91 total=5/44624
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=0/0 plan=1/13840 lower=1/5507 boot=0/224 emit=2/23818 write=1/91 total=5/43480
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=1/15223 plan=1/13840 lower=1/5507 boot=0/224 emit=2/23818 write=1/91 total=6/58703
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=0/0 plan=0/13840 lower=1/5507 boot=0/224 emit=3/23818 write=0/91 total=4/43480
COMPILE-TRACE program=callgraph_derivation_over_extraction parse=1/15223 plan=1/13840 lower=1/5507 boot=0/224 emit=2/23818 write=1/91 total=6/58703
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=0/0 plan=1/12491 lower=0/3400 boot=0/223 emit=3/21584 write=0/91 total=4/37789
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=0/0 plan=1/11264 lower=0/3400 boot=0/223 emit=2/21584 write=0/91 total=3/36562
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=1/11411 plan=1/11264 lower=0/3400 boot=0/223 emit=3/21584 write=0/91 total=5/47973
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=0/0 plan=1/11264 lower=0/3400 boot=0/223 emit=3/21584 write=0/91 total=4/36562
COMPILE-TRACE program=callgraph_unused_inverts_with_the_call_set parse=1/11411 plan=0/11264 lower=1/3400 boot=0/223 emit=2/21584 write=1/91 total=5/47973
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=0/0 plan=0/13932 lower=1/4870 boot=0/206 emit=3/28358 write=0/91 total=4/47457
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=0/0 plan=0/13568 lower=2/4870 boot=0/206 emit=2/28358 write=1/91 total=5/47093
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=1/20551 plan=1/13568 lower=1/4870 boot=0/206 emit=3/28358 write=0/91 total=6/67644
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=0/0 plan=1/13568 lower=1/4870 boot=0/206 emit=2/28358 write=1/91 total=5/47093
COMPILE-TRACE program=flagship_flow_reach_over_resolved_edges parse=1/20551 plan=1/13568 lower=1/4870 boot=0/206 emit=3/28358 write=0/91 total=6/67644
COMPILE-TRACE program=flagship_flow_reach_over_batched_resolved_edges parse=0/0 plan=1/13906 lower=1/4870 boot=0/206 emit=2/28358 write=1/91 total=5/47431
COMPILE-TRACE program=flagship_flow_reach_over_batched_resolved_edges parse=0/0 plan=1/13568 lower=1/4870 boot=0/206 emit=3/28358 write=0/91 total=5/47093
COMPILE-TRACE program=flagship_flow_reach_over_batched_resolved_edges parse=1/20551 plan=1/13568 lower=1/4870 boot=0/206 emit=3/28358 write=0/91 total=6/67644
COMPILE-TRACE program=flagship_flow_reach_over_batched_resolved_edges parse=0/0 plan=1/13568 lower=1/4870 boot=0/206 emit=3/28358 write=0/91 total=5/47093
COMPILE-TRACE program=flagship_flow_reach_over_batched_resolved_edges parse=1/20551 plan=1/13568 lower=1/4870 boot=0/206 emit=3/28358 write=0/91 total=6/67644
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=0/0 plan=0/2603 lower=0/897 boot=0/187 emit=1/6926 write=1/91 total=2/10704
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=0/0 plan=0/2464 lower=0/739 boot=0/187 emit=1/6926 write=0/91 total=1/10407
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=1/1397 plan=0/2464 lower=0/739 boot=0/187 emit=1/6926 write=0/91 total=2/11804
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=0/0 plan=0/2464 lower=0/739 boot=0/187 emit=1/6926 write=1/91 total=2/10407
COMPILE-TRACE program=struct_arrival_key_order_canonicalized parse=0/1397 plan=0/2464 lower=0/739 boot=0/187 emit=1/6926 write=0/91 total=1/11804
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/0 plan=1/5888 lower=0/1830 boot=0/217 emit=2/10712 write=0/91 total=3/18738
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/0 plan=1/5595 lower=0/1830 boot=0/217 emit=2/10712 write=0/91 total=3/18445
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/4258 plan=1/5595 lower=0/1830 boot=0/217 emit=1/10712 write=1/91 total=3/22703
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/0 plan=0/5595 lower=0/1830 boot=0/217 emit=2/10712 write=0/91 total=2/18445
COMPILE-TRACE program=struct_column_renders_canonical_json parse=0/4258 plan=1/5595 lower=0/1830 boot=0/217 emit=1/10712 write=1/91 total=3/22703
COMPILE-TRACE program=struct_intern_order_a parse=0/0 plan=1/2535 lower=0/704 boot=0/186 emit=1/5602 write=0/91 total=2/9118
COMPILE-TRACE program=struct_intern_order_a parse=0/0 plan=0/2309 lower=0/704 boot=0/186 emit=0/5602 write=1/91 total=1/8892
COMPILE-TRACE program=struct_intern_order_a parse=0/1253 plan=0/2309 lower=0/704 boot=0/186 emit=1/5602 write=0/91 total=1/10145
COMPILE-TRACE program=struct_intern_order_a parse=0/0 plan=0/2309 lower=0/704 boot=0/186 emit=1/5602 write=0/91 total=1/8892
COMPILE-TRACE program=struct_intern_order_a parse=0/1253 plan=0/2309 lower=1/704 boot=0/186 emit=1/5602 write=0/91 total=2/10145
COMPILE-TRACE program=struct_intern_order_b parse=0/0 plan=1/2535 lower=0/704 boot=0/186 emit=0/5602 write=1/91 total=2/9118
COMPILE-TRACE program=struct_intern_order_b parse=0/0 plan=0/2309 lower=0/704 boot=0/186 emit=1/5602 write=0/91 total=1/8892
COMPILE-TRACE program=struct_intern_order_b parse=0/1253 plan=1/2309 lower=0/704 boot=0/186 emit=1/5602 write=0/91 total=2/10145
COMPILE-TRACE program=struct_intern_order_b parse=0/0 plan=1/2309 lower=0/704 boot=0/186 emit=1/5602 write=0/91 total=2/8892
COMPILE-TRACE program=struct_intern_order_b parse=0/1253 plan=0/2309 lower=0/704 boot=0/186 emit=1/5602 write=0/91 total=1/10145
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=0/0 plan=1/8367 lower=0/2841 boot=0/268 emit=2/15278 write=0/91 total=3/26845
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=0/0 plan=1/8156 lower=0/2840 boot=0/268 emit=2/15278 write=0/91 total=3/26633
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=1/5932 plan=0/8156 lower=1/2840 boot=0/268 emit=2/15278 write=0/91 total=4/32565
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=0/0 plan=1/8156 lower=0/2840 boot=0/268 emit=2/15278 write=0/91 total=3/26633
COMPILE-TRACE program=struct_nested_value_renders_whole_tree parse=1/5932 plan=0/8156 lower=1/2840 boot=0/268 emit=2/15278 write=0/91 total=4/32565
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/0 plan=0/7353 lower=1/2574 boot=0/218 emit=1/13199 write=1/91 total=3/23435
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/0 plan=1/7052 lower=0/2574 boot=0/218 emit=2/13199 write=0/91 total=3/23134
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/5735 plan=1/7052 lower=0/2574 boot=0/218 emit=2/13199 write=0/91 total=3/28869
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/0 plan=1/7052 lower=0/2574 boot=0/218 emit=2/13199 write=0/91 total=3/23134
COMPILE-TRACE program=struct_ghcacher_stars_normalization parse=0/5735 plan=1/7052 lower=0/2574 boot=0/218 emit=2/13199 write=0/91 total=3/28869
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=0/0 plan=0/8048 lower=1/2615 boot=0/221 emit=1/12640 write=1/91 total=3/23615
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=0/0 plan=1/7684 lower=0/2615 boot=0/221 emit=2/12640 write=0/91 total=3/23251
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=0/6593 plan=0/7684 lower=0/2615 boot=0/221 emit=2/12640 write=0/91 total=2/29844
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=0/0 plan=1/7684 lower=0/2615 boot=0/221 emit=2/12640 write=0/91 total=3/23251
COMPILE-TRACE program=struct_span_columns_are_int_after_decode parse=1/6593 plan=0/7684 lower=1/2615 boot=0/221 emit=1/12640 write=0/91 total=3/29844
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=0/0 plan=2/23028 lower=1/7749 boot=0/359 emit=4/33190 write=0/91 total=7/64417
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=0/0 plan=1/22670 lower=2/7749 boot=0/308 emit=3/33093 write=1/91 total=7/63911
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=1/11274 plan=1/22670 lower=1/7749 boot=0/308 emit=4/33093 write=0/91 total=7/75185
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=0/0 plan=1/22670 lower=1/7749 boot=0/308 emit=4/33093 write=0/91 total=6/63911
COMPILE-TRACE program=struct_host_output_schedule_answer_interned parse=1/11274 plan=1/22670 lower=1/7749 boot=0/308 emit=4/33093 write=0/91 total=7/75185
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/0 plan=0/3005 lower=0/739 boot=0/187 emit=1/6801 write=0/91 total=1/10823
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/0 plan=0/2464 lower=0/739 boot=0/187 emit=1/6801 write=0/91 total=1/10282
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/1387 plan=1/2464 lower=0/739 boot=0/187 emit=1/6801 write=0/91 total=2/11669
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/0 plan=0/2464 lower=0/739 boot=0/187 emit=1/6801 write=0/91 total=1/10282
COMPILE-TRACE program=struct_shared_child_survives_one_release parse=0/1387 plan=0/2464 lower=1/739 boot=0/187 emit=1/6801 write=0/91 total=2/11669
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=0/0 plan=0/5650 lower=0/1871 boot=0/217 emit=1/9283 write=1/91 total=2/17112
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=0/0 plan=1/5523 lower=0/1871 boot=0/217 emit=1/9283 write=0/91 total=2/16985
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=1/4440 plan=0/5523 lower=1/1871 boot=0/217 emit=1/9283 write=0/91 total=3/21425
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=0/0 plan=0/5523 lower=1/1871 boot=0/217 emit=1/9283 write=0/91 total=2/16985
COMPILE-TRACE program=relation_reference_target_and_parent_share_tick parse=0/4440 plan=1/5523 lower=0/1871 boot=0/217 emit=1/9283 write=1/91 total=3/21425
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/0 plan=1/4642 lower=0/1521 boot=0/167 emit=1/8286 write=0/91 total=2/14707
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/0 plan=0/4507 lower=0/1521 boot=0/167 emit=1/8286 write=0/91 total=1/14572
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/3443 plan=0/4507 lower=0/1521 boot=0/167 emit=1/8286 write=1/91 total=2/18015
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/0 plan=1/4507 lower=0/1521 boot=0/167 emit=1/8286 write=0/91 total=2/14572
COMPILE-TRACE program=groupby_two_bare_integer_literals parse=0/3443 plan=1/4507 lower=0/1521 boot=0/167 emit=1/8286 write=0/91 total=2/18015
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=0/0 plan=0/4707 lower=0/1682 boot=0/167 emit=1/6807 write=1/91 total=2/13454
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=0/0 plan=0/4572 lower=1/1682 boot=0/167 emit=1/6807 write=0/91 total=2/13319
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=0/3639 plan=0/4572 lower=1/1682 boot=0/167 emit=1/6807 write=0/91 total=2/16958
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=0/0 plan=0/4572 lower=1/1682 boot=0/167 emit=0/6807 write=1/91 total=2/13319
COMPILE-TRACE program=groupby_aggregate_two_bare_integer_literals parse=0/3639 plan=0/4572 lower=1/1682 boot=0/167 emit=0/6807 write=1/91 total=2/16958
COMPILE-TRACE program=probe_output_comparison_guard parse=0/0 plan=1/14754 lower=1/5614 boot=0/270 emit=3/24273 write=0/91 total=5/45002
COMPILE-TRACE program=probe_output_comparison_guard parse=0/0 plan=1/14306 lower=1/5614 boot=0/225 emit=2/24191 write=1/91 total=5/44427
COMPILE-TRACE program=probe_output_comparison_guard parse=0/6533 plan=1/14306 lower=1/5614 boot=0/225 emit=3/24191 write=0/91 total=5/50960
COMPILE-TRACE program=probe_output_comparison_guard parse=0/0 plan=1/14306 lower=0/5614 boot=0/225 emit=2/24191 write=0/91 total=3/44427
COMPILE-TRACE program=probe_output_comparison_guard parse=1/6533 plan=1/14306 lower=1/5614 boot=0/225 emit=2/24191 write=0/91 total=5/50960
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=0/0 plan=0/4845 lower=1/1722 boot=0/165 emit=1/7729 write=0/91 total=2/14552
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=0/0 plan=1/4744 lower=0/1722 boot=0/165 emit=1/7729 write=0/91 total=2/14451
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=1/3678 plan=0/4744 lower=0/1722 boot=0/165 emit=1/7729 write=0/91 total=2/18129
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=0/0 plan=1/4744 lower=0/1722 boot=0/165 emit=1/7729 write=0/91 total=2/14451
COMPILE-TRACE program=backslash_in_string_literal_survives_both_doors parse=0/3678 plan=1/4744 lower=0/1722 boot=0/165 emit=1/7729 write=0/91 total=2/18129
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=0/0 plan=0/9473 lower=1/3348 boot=0/283 emit=2/14049 write=0/91 total=3/27244
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=0/0 plan=0/9283 lower=1/3348 boot=0/227 emit=2/13942 write=0/91 total=3/26891
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=0/9258 plan=1/9283 lower=1/3348 boot=0/227 emit=1/13942 write=1/91 total=4/36149
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=0/0 plan=1/9283 lower=0/3348 boot=0/227 emit=2/13942 write=0/91 total=3/26891
COMPILE-TRACE program=host_free_query_leaves_a_derived_rel_unsubscribed parse=0/9258 plan=1/9283 lower=1/3348 boot=0/227 emit=1/13942 write=0/91 total=3/36149
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=0/0 plan=1/15475 lower=1/4687 boot=0/222 emit=3/25374 write=0/91 total=5/45849
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=0/0 plan=1/13893 lower=1/4687 boot=0/222 emit=3/25374 write=0/91 total=5/44267
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=1/16495 plan=1/13893 lower=1/4687 boot=0/222 emit=3/25374 write=0/91 total=6/60762
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=0/0 plan=1/13893 lower=1/4687 boot=0/222 emit=2/25374 write=0/91 total=4/44267
COMPILE-TRACE program=flow_arg_param_hop_is_positional_and_site_pinned parse=0/16495 plan=1/13893 lower=1/4687 boot=0/222 emit=3/25374 write=0/91 total=5/60762
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=0/0 plan=2/28578 lower=2/10116 boot=0/279 emit=4/39629 write=0/91 total=8/78693
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=0/0 plan=1/27123 lower=2/10116 boot=0/279 emit=4/39629 write=0/91 total=7/77238
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=2/30308 plan=2/27123 lower=1/10116 boot=0/279 emit=4/39629 write=1/91 total=10/107546
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=0/0 plan=1/27123 lower=2/10116 boot=0/279 emit=4/39629 write=0/91 total=7/77238
COMPILE-TRACE program=flow_sig_owner_join_types_the_resolved_callee parse=2/30308 plan=2/27123 lower=1/10116 boot=0/279 emit=4/39629 write=1/91 total=10/107546
COMPILE-TRACE program=bool_literals_round_trip parse=0/0 plan=0/1704 lower=0/439 boot=0/183 emit=1/3927 write=0/91 total=1/6344
COMPILE-TRACE program=bool_literals_round_trip parse=0/0 plan=1/1557 lower=0/439 boot=0/136 emit=0/3826 write=0/91 total=1/6049
COMPILE-TRACE program=bool_literals_round_trip parse=0/811 plan=0/1557 lower=0/439 boot=0/136 emit=0/3826 write=1/91 total=1/6860
COMPILE-TRACE program=bool_literals_round_trip parse=0/0 plan=0/1557 lower=0/439 boot=0/136 emit=1/3826 write=0/91 total=1/6049
COMPILE-TRACE program=bool_literals_round_trip parse=0/811 plan=0/1557 lower=1/439 boot=0/136 emit=0/3826 write=0/91 total=1/6860
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/0 plan=0/5255 lower=0/1772 boot=0/266 emit=1/8240 write=1/91 total=2/15624
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/0 plan=1/5104 lower=0/1772 boot=0/166 emit=1/8039 write=0/91 total=2/15172
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/4453 plan=1/5104 lower=0/1772 boot=0/166 emit=1/8039 write=0/91 total=2/19625
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/0 plan=0/5104 lower=1/1772 boot=0/166 emit=1/8039 write=0/91 total=2/15172
COMPILE-TRACE program=bool_identity_comparison_filters parse=0/4453 plan=1/5104 lower=0/1772 boot=0/166 emit=1/8039 write=0/91 total=2/19625
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=0/0 plan=0/7011 lower=1/2041 boot=0/378 emit=1/11079 write=0/91 total=2/20600
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=0/0 plan=0/6735 lower=1/2041 boot=0/188 emit=1/10699 write=0/91 total=2/19754
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=1/5264 plan=0/6735 lower=0/2041 boot=0/188 emit=1/10699 write=0/91 total=2/25018
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=0/0 plan=1/6735 lower=0/2041 boot=0/188 emit=1/10699 write=1/91 total=3/19754
COMPILE-TRACE program=bool_relation_negation_is_two_valued parse=0/5264 plan=1/6735 lower=0/2041 boot=0/188 emit=1/10699 write=0/91 total=2/25018
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=0/0 plan=0/4607 lower=1/1606 boot=0/217 emit=1/9130 write=0/91 total=2/15651
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=0/0 plan=0/4525 lower=0/1605 boot=0/167 emit=1/9029 write=0/91 total=1/15417
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=1/4151 plan=0/4525 lower=1/1605 boot=0/167 emit=1/9029 write=0/91 total=3/19568
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=0/0 plan=1/4525 lower=0/1605 boot=0/167 emit=1/9029 write=0/91 total=2/15417
COMPILE-TRACE program=float_arithmetic_is_binary64 parse=1/4150 plan=0/4525 lower=0/1605 boot=0/167 emit=1/9029 write=1/91 total=3/19567
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=0/0 plan=1/4960 lower=0/1659 boot=0/229 emit=1/9577 write=0/91 total=2/16516
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=0/0 plan=0/4850 lower=1/1659 boot=0/168 emit=1/9454 write=0/91 total=2/16222
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=1/5014 plan=0/4850 lower=0/1659 boot=0/168 emit=1/9454 write=1/91 total=3/21236
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=0/0 plan=0/4850 lower=0/1659 boot=0/168 emit=1/9454 write=1/91 total=2/16222
COMPILE-TRACE program=int_float_arithmetic_keeps_real_result parse=0/5014 plan=1/4850 lower=0/1659 boot=0/168 emit=1/9454 write=0/91 total=2/21236
COMPILE-TRACE program=float_avg_is_grouped parse=0/0 plan=0/4786 lower=1/1837 boot=0/321 emit=1/10091 write=0/91 total=2/17126
COMPILE-TRACE program=float_avg_is_grouped parse=0/0 plan=0/4541 lower=0/1837 boot=0/171 emit=1/9797 write=1/91 total=2/16437
COMPILE-TRACE program=float_avg_is_grouped parse=0/4221 plan=0/4541 lower=1/1837 boot=0/171 emit=1/9797 write=0/91 total=2/20658
COMPILE-TRACE program=float_avg_is_grouped parse=0/0 plan=1/4541 lower=0/1837 boot=0/171 emit=1/9797 write=0/91 total=2/16437
COMPILE-TRACE program=float_avg_is_grouped parse=1/4221 plan=0/4541 lower=0/1837 boot=0/171 emit=2/9797 write=0/91 total=3/20658
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=0/0 plan=0/5231 lower=0/1784 boot=0/266 emit=1/8274 write=1/91 total=2/15646
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=0/0 plan=0/5076 lower=1/1784 boot=0/166 emit=1/8072 write=0/91 total=2/15189
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=0/4478 plan=1/5076 lower=0/1784 boot=0/166 emit=1/8072 write=0/91 total=2/19667
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=0/0 plan=1/5076 lower=0/1784 boot=0/166 emit=1/8072 write=0/91 total=2/15189
COMPILE-TRACE program=float_exact_comparison_has_no_epsilon parse=0/4478 plan=1/5076 lower=0/1784 boot=0/166 emit=1/8072 write=0/91 total=2/19667
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=0/0 plan=0/6856 lower=1/2394 boot=0/401 emit=1/12066 write=0/91 total=2/21808
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=0/0 plan=1/6514 lower=0/2394 boot=0/189 emit=2/11668 write=0/91 total=3/20856
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=0/6159 plan=1/6514 lower=0/2394 boot=0/189 emit=2/11668 write=0/91 total=3/27015
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=0/0 plan=0/6514 lower=0/2394 boot=0/189 emit=1/11668 write=1/91 total=2/20856
COMPILE-TRACE program=float_exact_join_has_no_epsilon parse=0/6159 plan=0/6514 lower=1/2394 boot=0/189 emit=1/11668 write=1/91 total=3/27015
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=0/0 plan=0/1486 lower=1/404 boot=0/171 emit=0/3009 write=0/91 total=1/5161
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=0/2931 write=1/91 total=1/4997
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=0/698 plan=0/1436 lower=0/404 boot=0/135 emit=0/2931 write=1/91 total=1/5695
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=0/0 plan=0/1436 lower=1/404 boot=0/135 emit=0/2931 write=0/91 total=1/4997
COMPILE-TRACE program=float_negative_zero_canonical_boundary parse=0/698 plan=1/1436 lower=0/404 boot=0/135 emit=0/2931 write=0/91 total=1/5695
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/0 plan=0/1537 lower=0/404 boot=0/171 emit=1/3009 write=0/91 total=1/5212
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=1/2931 write=0/91 total=1/4997
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/698 plan=0/1436 lower=1/404 boot=0/135 emit=0/2931 write=0/91 total=1/5695
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=1/2931 write=0/91 total=1/4997
COMPILE-TRACE program=float_integral_value_keeps_real_storage parse=0/698 plan=0/1436 lower=0/404 boot=0/135 emit=1/2931 write=0/91 total=1/5695
COMPILE-TRACE program=float_shortest_round_trip_wire parse=0/0 plan=0/1588 lower=0/404 boot=0/171 emit=1/3051 write=0/91 total=1/5305
COMPILE-TRACE program=float_shortest_round_trip_wire parse=0/0 plan=1/1436 lower=0/404 boot=0/135 emit=0/2971 write=0/91 total=1/5037
COMPILE-TRACE program=float_shortest_round_trip_wire parse=1/702 plan=0/1436 lower=0/404 boot=0/135 emit=0/2971 write=0/91 total=1/5739
COMPILE-TRACE program=float_shortest_round_trip_wire parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=1/2971 write=0/91 total=1/5037
COMPILE-TRACE program=float_shortest_round_trip_wire parse=0/702 plan=0/1436 lower=1/404 boot=0/135 emit=0/2971 write=0/91 total=1/5739
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/0 plan=0/5054 lower=1/1837 boot=0/321 emit=1/10091 write=0/91 total=2/17394
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/0 plan=0/4541 lower=1/1837 boot=0/171 emit=1/9797 write=0/91 total=2/16437
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/4221 plan=1/4541 lower=0/1837 boot=0/171 emit=1/9797 write=1/91 total=3/20658
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/0 plan=0/4541 lower=0/1837 boot=0/171 emit=2/9797 write=0/91 total=2/16437
COMPILE-TRACE program=float_avg_retracts_to_empty_group parse=0/4221 plan=0/4541 lower=1/1837 boot=0/171 emit=1/9797 write=0/91 total=2/20658
COMPILE-TRACE program=int_accepts_integral_float parse=0/0 plan=0/1488 lower=1/404 boot=0/171 emit=0/2683 write=0/91 total=1/4837
COMPILE-TRACE program=int_accepts_integral_float parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=0/2601 write=1/91 total=1/4667
COMPILE-TRACE program=int_accepts_integral_float parse=0/694 plan=0/1436 lower=0/404 boot=0/135 emit=0/2601 write=1/91 total=1/5361
COMPILE-TRACE program=int_accepts_integral_float parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=1/2601 write=0/91 total=1/4667
COMPILE-TRACE program=int_accepts_integral_float parse=0/694 plan=0/1436 lower=0/404 boot=0/135 emit=1/2601 write=0/91 total=1/5361
COMPILE-TRACE program=float_widens_integer_ingress parse=0/0 plan=0/1487 lower=0/404 boot=0/135 emit=0/2931 write=0/91 total=0/5048
COMPILE-TRACE program=float_widens_integer_ingress parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=0/2931 write=1/91 total=1/4997
COMPILE-TRACE program=float_widens_integer_ingress parse=0/698 plan=0/1436 lower=0/404 boot=0/135 emit=0/2931 write=1/91 total=1/5695
COMPILE-TRACE program=float_widens_integer_ingress parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=1/2931 write=0/91 total=1/4997
COMPILE-TRACE program=float_widens_integer_ingress parse=0/698 plan=0/1436 lower=0/404 boot=0/135 emit=1/2931 write=0/91 total=1/5695
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/0 plan=0/1503 lower=0/404 boot=0/171 emit=0/3009 write=1/91 total=1/5178
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=1/2931 write=0/91 total=1/4997
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/698 plan=0/1436 lower=0/404 boot=0/135 emit=1/2931 write=0/91 total=1/5695
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=0/2931 write=1/91 total=1/4997
COMPILE-TRACE program=float_widens_wide_integer_ingress parse=0/698 plan=0/1436 lower=0/404 boot=0/135 emit=0/2931 write=1/91 total=1/5695
COMPILE-TRACE program=head_column_int_widens_into_float parse=0/0 plan=0/3947 lower=0/1337 boot=0/204 emit=1/6045 write=0/91 total=1/11624
COMPILE-TRACE program=head_column_int_widens_into_float parse=0/0 plan=0/3891 lower=0/1337 boot=0/165 emit=1/5965 write=0/91 total=1/11449
COMPILE-TRACE program=head_column_int_widens_into_float parse=1/2907 plan=0/3891 lower=0/1337 boot=0/165 emit=1/5965 write=0/91 total=2/14356
COMPILE-TRACE program=head_column_int_widens_into_float parse=0/0 plan=0/3891 lower=1/1337 boot=0/165 emit=0/5965 write=1/91 total=2/11449
COMPILE-TRACE program=head_column_int_widens_into_float parse=0/2907 plan=0/3891 lower=0/1337 boot=0/165 emit=1/5965 write=0/91 total=1/14356
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=0/0 plan=0/3944 lower=0/1337 boot=0/244 emit=0/5778 write=0/91 total=0/11394
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=0/0 plan=0/3890 lower=0/1337 boot=0/165 emit=1/5685 write=0/91 total=1/11168
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=1/2961 plan=0/3890 lower=0/1337 boot=0/165 emit=1/5685 write=0/91 total=2/14129
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=0/0 plan=0/3890 lower=0/1337 boot=0/165 emit=1/5685 write=0/91 total=1/11168
COMPILE-TRACE program=head_column_list_and_json_share_storage parse=0/2961 plan=1/3890 lower=0/1337 boot=0/165 emit=1/5685 write=0/91 total=2/14129
COMPILE-TRACE program=relation_depth2_construct_and_read parse=0/0 plan=2/29201 lower=1/13045 boot=0/369 emit=4/30777 write=0/91 total=7/73483
COMPILE-TRACE program=relation_depth2_construct_and_read parse=0/0 plan=1/28992 lower=2/13045 boot=0/369 emit=3/30777 write=1/91 total=7/73274
COMPILE-TRACE program=relation_depth2_construct_and_read parse=2/25564 plan=1/28992 lower=2/13045 boot=0/369 emit=4/30777 write=0/91 total=9/98838
COMPILE-TRACE program=relation_depth2_construct_and_read parse=0/0 plan=2/28992 lower=1/13045 boot=0/369 emit=4/30777 write=0/91 total=7/73274
COMPILE-TRACE program=relation_depth2_construct_and_read parse=2/25564 plan=2/28992 lower=2/13045 boot=0/369 emit=3/30777 write=0/91 total=9/98838
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=0/0 plan=2/31213 lower=2/15404 boot=0/397 emit=3/32082 write=1/91 total=8/79187
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=0/0 plan=2/31011 lower=2/15404 boot=0/397 emit=4/32082 write=0/91 total=8/78985
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=2/29346 plan=2/31011 lower=2/15404 boot=0/397 emit=3/32082 write=1/91 total=10/108331
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=0/0 plan=2/31011 lower=1/15404 boot=0/397 emit=4/32082 write=1/91 total=8/78985
COMPILE-TRACE program=relation_depth2_literal_leaf_selects_zero_and_one parse=2/29346 plan=1/31011 lower=2/15404 boot=0/397 emit=4/32082 write=1/91 total=10/108331
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=0/0 plan=1/29567 lower=2/13045 boot=0/369 emit=4/30777 write=0/91 total=7/73849
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=0/0 plan=2/28992 lower=2/13045 boot=0/369 emit=3/30777 write=1/91 total=8/73274
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=1/25564 plan=2/28992 lower=2/13045 boot=0/369 emit=4/30777 write=0/91 total=9/98838
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=0/0 plan=1/28992 lower=2/13045 boot=0/369 emit=4/30777 write=0/91 total=7/73274
COMPILE-TRACE program=relation_depth2_many_rows_share_one_leaf parse=2/25564 plan=2/28992 lower=1/13045 boot=0/369 emit=4/30777 write=0/91 total=9/98838
COMPILE-TRACE program=relation_depth2_chained_decode parse=0/0 plan=1/31908 lower=2/13245 boot=0/369 emit=4/30968 write=0/91 total=7/76581
COMPILE-TRACE program=relation_depth2_chained_decode parse=0/0 plan=2/31699 lower=2/13245 boot=0/369 emit=3/30968 write=1/91 total=8/76372
COMPILE-TRACE program=relation_depth2_chained_decode parse=2/25747 plan=2/31699 lower=1/13245 boot=0/369 emit=4/30968 write=0/91 total=9/102119
COMPILE-TRACE program=relation_depth2_chained_decode parse=0/0 plan=1/31699 lower=2/13245 boot=0/369 emit=4/30968 write=0/91 total=7/76372
COMPILE-TRACE program=relation_depth2_chained_decode parse=2/25747 plan=2/31699 lower=2/13245 boot=0/369 emit=3/30968 write=1/91 total=10/102119
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=0/0 plan=2/30636 lower=1/13085 boot=0/369 emit=4/30895 write=0/91 total=7/75076
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=0/0 plan=2/30397 lower=1/13085 boot=0/369 emit=4/30895 write=0/91 total=7/74837
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=2/25748 plan=2/30397 lower=2/13085 boot=0/369 emit=3/30895 write=1/91 total=10/100585
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=0/0 plan=2/30397 lower=1/13085 boot=0/369 emit=4/30895 write=0/91 total=7/74837
COMPILE-TRACE program=relation_depth2_nested_decode_pattern parse=2/25748 plan=2/30397 lower=1/13085 boot=0/369 emit=3/30895 write=0/91 total=8/100585
COMPILE-TRACE program=relation_depth2_dot_read parse=0/0 plan=1/30634 lower=2/13085 boot=0/369 emit=4/30895 write=0/91 total=7/75074
COMPILE-TRACE program=relation_depth2_dot_read parse=0/0 plan=2/30395 lower=2/13085 boot=0/369 emit=3/30895 write=0/91 total=7/74835
COMPILE-TRACE program=relation_depth2_dot_read parse=1/23785 plan=2/30395 lower=2/13085 boot=0/369 emit=3/30895 write=1/91 total=9/98620
COMPILE-TRACE program=relation_depth2_dot_read parse=0/0 plan=2/30395 lower=1/13085 boot=0/369 emit=3/30895 write=1/91 total=7/74835
COMPILE-TRACE program=relation_depth2_dot_read parse=1/23785 plan=2/30395 lower=2/13085 boot=0/369 emit=3/30895 write=0/91 total=8/98620
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=0/0 plan=2/30745 lower=2/13085 boot=0/369 emit=3/30895 write=1/91 total=8/75185
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=0/0 plan=2/30506 lower=2/13085 boot=0/369 emit=3/30895 write=0/91 total=7/74946
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=2/24409 plan=2/30506 lower=2/13085 boot=0/369 emit=3/30895 write=1/91 total=10/99355
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=0/0 plan=1/30506 lower=2/13085 boot=0/369 emit=4/30895 write=0/91 total=7/74946
COMPILE-TRACE program=relation_depth2_member_dot_pattern parse=2/24409 plan=2/30506 lower=2/13085 boot=0/369 emit=3/30895 write=1/91 total=10/99355
COMPILE-TRACE program=relation_depth3_construct_and_read parse=0/0 plan=2/37899 lower=2/19175 boot=0/432 emit=5/40347 write=0/91 total=9/97944
COMPILE-TRACE program=relation_depth3_construct_and_read parse=0/0 plan=2/37656 lower=2/19175 boot=0/432 emit=5/40347 write=0/91 total=9/97701
COMPILE-TRACE program=relation_depth3_construct_and_read parse=3/35299 plan=2/37656 lower=2/19175 boot=0/432 emit=4/40347 write=0/91 total=11/133000
COMPILE-TRACE program=relation_depth3_construct_and_read parse=0/0 plan=2/37656 lower=2/19175 boot=0/432 emit=5/40347 write=0/91 total=9/97701
COMPILE-TRACE program=relation_depth3_construct_and_read parse=2/35299 plan=2/37656 lower=3/19175 boot=0/432 emit=4/40347 write=1/91 total=12/133000
COMPILE-TRACE program=relation_depth3_chained_decode parse=0/0 plan=3/49675 lower=3/22898 boot=0/463 emit=5/46710 write=0/91 total=11/119837
COMPILE-TRACE program=relation_depth3_chained_decode parse=0/0 plan=2/49418 lower=3/22898 boot=0/463 emit=5/46710 write=1/91 total=11/119580
COMPILE-TRACE program=relation_depth3_chained_decode parse=3/42102 plan=3/49418 lower=2/22898 boot=0/463 emit=6/46710 write=0/91 total=14/161682
COMPILE-TRACE program=relation_depth3_chained_decode parse=0/0 plan=2/49418 lower=3/22898 boot=0/463 emit=5/46710 write=0/91 total=10/119580
COMPILE-TRACE program=relation_depth3_chained_decode parse=4/42102 plan=2/49418 lower=3/22898 boot=0/463 emit=5/46710 write=0/91 total=14/161682
COMPILE-TRACE program=relation_depth3_many_rows parse=0/0 plan=2/38327 lower=2/19175 boot=0/432 emit=5/40347 write=0/91 total=9/98372
COMPILE-TRACE program=relation_depth3_many_rows parse=0/0 plan=2/37656 lower=2/19175 boot=0/432 emit=4/40347 write=1/91 total=9/97701
COMPILE-TRACE program=relation_depth3_many_rows parse=2/35299 plan=2/37656 lower=3/19175 boot=0/432 emit=4/40347 write=0/91 total=11/133000
COMPILE-TRACE program=relation_depth3_many_rows parse=0/0 plan=2/37656 lower=2/19175 boot=0/432 emit=5/40347 write=0/91 total=9/97701
COMPILE-TRACE program=relation_depth3_many_rows parse=3/35299 plan=2/37656 lower=2/19175 boot=0/432 emit=4/40347 write=1/91 total=12/133000
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=0/0 plan=1/12386 lower=0/4795 boot=0/251 emit=2/14801 write=0/91 total=3/32324
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=0/0 plan=0/12279 lower=1/4795 boot=0/251 emit=2/14801 write=0/91 total=3/32217
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=1/9648 plan=1/12279 lower=0/4795 boot=0/251 emit=2/14801 write=0/91 total=4/41865
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=0/0 plan=1/12279 lower=0/4795 boot=0/251 emit=2/14801 write=0/91 total=3/32217
COMPILE-TRACE program=relation_ref_column_fed_by_ref_variable_accepted parse=1/9648 plan=1/12279 lower=0/4795 boot=0/251 emit=2/14801 write=0/91 total=4/41865
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=0/0 plan=1/10976 lower=0/4006 boot=0/328 emit=2/14910 write=0/91 total=3/30311
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=0/0 plan=1/10771 lower=0/4006 boot=0/191 emit=2/14616 write=0/91 total=3/29675
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=0/4727 plan=1/10771 lower=1/4006 boot=0/191 emit=1/14616 write=0/91 total=3/34402
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=0/0 plan=0/10771 lower=1/4006 boot=0/191 emit=2/14616 write=0/91 total=3/29675
COMPILE-TRACE program=coalesce_defaults_the_absent_row parse=0/4727 plan=1/10771 lower=1/4006 boot=0/191 emit=1/14616 write=1/91 total=4/34402
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=0/0 plan=1/11016 lower=0/4006 boot=1/233 emit=1/14700 write=0/91 total=3/30046
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=0/0 plan=1/10771 lower=1/4006 boot=0/191 emit=1/14616 write=0/91 total=3/29675
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=1/4727 plan=1/10771 lower=0/4006 boot=0/191 emit=2/14616 write=0/91 total=4/34402
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=0/0 plan=1/10771 lower=1/4006 boot=0/191 emit=1/14616 write=1/91 total=4/29675
COMPILE-TRACE program=coalesce_default_returns_when_source_retracts parse=0/4727 plan=1/10771 lower=0/4006 boot=0/191 emit=2/14616 write=0/91 total=3/34402
COMPILE-TRACE program=coalesce_over_derived_source parse=0/0 plan=1/15445 lower=1/5463 boot=0/469 emit=2/13694 write=0/91 total=4/35162
COMPILE-TRACE program=coalesce_over_derived_source parse=0/0 plan=1/15013 lower=1/5463 boot=0/222 emit=1/13274 write=0/91 total=3/34063
COMPILE-TRACE program=coalesce_over_derived_source parse=1/8711 plan=1/15013 lower=1/5463 boot=0/222 emit=1/13274 write=1/91 total=5/42774
COMPILE-TRACE program=coalesce_over_derived_source parse=0/0 plan=1/15013 lower=1/5463 boot=0/222 emit=1/13274 write=0/91 total=3/34063
COMPILE-TRACE program=coalesce_over_derived_source parse=1/8711 plan=1/15013 lower=1/5463 boot=0/222 emit=1/13274 write=0/91 total=4/42774
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/0 plan=1/12859 lower=0/2821 boot=0/238 emit=2/9307 write=0/91 total=3/25316
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/0 plan=1/12421 lower=0/2821 boot=0/185 emit=1/9207 write=1/91 total=3/24725
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/4947 plan=1/12421 lower=0/2821 boot=0/185 emit=1/9207 write=1/91 total=3/29672
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/0 plan=1/12421 lower=0/2821 boot=0/185 emit=1/9207 write=0/91 total=2/24725
COMPILE-TRACE program=coalesce_in_edge_body_samples parse=0/4947 plan=1/12421 lower=0/2821 boot=0/185 emit=1/9207 write=0/91 total=2/29672
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/0 plan=1/4172 lower=0/1339 boot=0/165 emit=1/6952 write=0/91 total=2/12719
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/0 plan=1/3879 lower=0/1339 boot=0/165 emit=1/6952 write=0/91 total=2/12426
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/2846 plan=1/3879 lower=0/1339 boot=0/165 emit=1/6952 write=0/91 total=2/15272
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/0 plan=0/3879 lower=0/1339 boot=0/165 emit=1/6952 write=1/91 total=2/12426
COMPILE-TRACE program=json_string_control_escapes_are_valid_json parse=0/2846 plan=0/3879 lower=0/1339 boot=0/165 emit=1/6952 write=0/91 total=1/15272
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=0/0 plan=0/3969 lower=1/1337 boot=0/497 emit=0/5828 write=0/91 total=1/11722
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=0/0 plan=0/3875 lower=1/1337 boot=0/165 emit=1/5680 write=0/91 total=2/11148
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=0/2873 plan=0/3875 lower=1/1337 boot=0/165 emit=0/5680 write=1/91 total=2/14021
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=0/0 plan=1/3875 lower=0/1337 boot=0/165 emit=1/5680 write=0/91 total=2/11148
COMPILE-TRACE program=json_control_escapes_inside_a_document parse=0/2873 plan=1/3875 lower=0/1337 boot=0/165 emit=1/5680 write=0/91 total=2/14021
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=0/0 plan=1/3980 lower=0/1337 boot=0/314 emit=1/5790 write=0/91 total=2/11512
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=0/0 plan=0/3875 lower=0/1337 boot=0/165 emit=1/5680 write=0/91 total=1/11148
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=0/2873 plan=1/3875 lower=0/1337 boot=0/165 emit=1/5680 write=0/91 total=2/14021
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=0/0 plan=0/3875 lower=0/1337 boot=0/165 emit=1/5680 write=0/91 total=1/11148
COMPILE-TRACE program=json_non_ascii_keys_sort_by_code_point parse=1/2873 plan=0/3875 lower=0/1337 boot=0/165 emit=1/5680 write=0/91 total=2/14021
COMPILE-TRACE program=json_nfc_and_nfd_keys_stay_distinct parse=0/0 plan=0/4865 lower=0/1787 boot=1/303 emit=0/7156 write=1/91 total=2/14202
COMPILE-TRACE program=json_nfc_and_nfd_keys_stay_distinct parse=0/0 plan=1/4786 lower=0/1787 boot=0/165 emit=1/7051 write=0/91 total=2/13880
COMPILE-TRACE program=json_nfc_and_nfd_keys_stay_distinct parse=0/3808 plan=1/4786 lower=0/1787 boot=0/165 emit=1/7051 write=0/91 total=2/17688
COMPILE-TRACE program=json_nfc_and_nfd_keys_stay_distinct parse=0/0 plan=0/4786 lower=1/1787 boot=0/165 emit=1/7051 write=0/91 total=2/13880
COMPILE-TRACE program=json_nfc_and_nfd_keys_stay_distinct parse=0/3808 plan=0/4786 lower=1/1787 boot=0/165 emit=1/7051 write=0/91 total=2/17688
COMPILE-TRACE program=json_empty_string_key_round_trips parse=0/0 plan=1/5192 lower=0/1854 boot=0/264 emit=1/7451 write=0/91 total=2/14852
COMPILE-TRACE program=json_empty_string_key_round_trips parse=0/0 plan=0/5109 lower=0/1854 boot=0/166 emit=1/7354 write=0/91 total=1/14574
COMPILE-TRACE program=json_empty_string_key_round_trips parse=1/4227 plan=0/5109 lower=0/1854 boot=0/166 emit=1/7354 write=1/91 total=3/18801
COMPILE-TRACE program=json_empty_string_key_round_trips parse=0/0 plan=1/5109 lower=0/1854 boot=0/166 emit=1/7354 write=0/91 total=2/14574
COMPILE-TRACE program=json_empty_string_key_round_trips parse=0/4227 plan=0/5109 lower=0/1854 boot=0/166 emit=1/7354 write=1/91 total=2/18801
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=0/0 plan=1/5205 lower=0/1854 boot=0/332 emit=1/7466 write=0/91 total=2/14948
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=0/0 plan=0/5109 lower=0/1854 boot=0/166 emit=1/7354 write=0/91 total=1/14574
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=1/4227 plan=0/5109 lower=0/1854 boot=0/166 emit=1/7354 write=1/91 total=3/18801
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=0/0 plan=0/5109 lower=0/1854 boot=0/166 emit=1/7354 write=0/91 total=1/14574
COMPILE-TRACE program=json_marker_shaped_keys_are_ordinary_data parse=1/4227 plan=0/5109 lower=0/1854 boot=0/166 emit=1/7354 write=0/91 total=2/18801
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=0/0 plan=0/4733 lower=1/1465 boot=0/167 emit=1/8101 write=0/91 total=2/14557
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=0/0 plan=1/4473 lower=0/1465 boot=0/167 emit=1/8101 write=0/91 total=2/14297
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=0/3989 plan=1/4473 lower=0/1465 boot=0/167 emit=1/8101 write=1/91 total=3/18286
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=0/0 plan=0/4473 lower=0/1465 boot=0/167 emit=1/8101 write=1/91 total=2/14297
COMPILE-TRACE program=json_safe_integer_boundary_survives_both_doors parse=0/3989 plan=0/4473 lower=0/1465 boot=1/167 emit=1/8101 write=0/91 total=2/18286
COMPILE-TRACE program=json_empty_containers_nest parse=0/0 plan=0/4003 lower=0/1337 boot=0/502 emit=1/5821 write=0/91 total=1/11754
COMPILE-TRACE program=json_empty_containers_nest parse=0/0 plan=0/3875 lower=0/1337 boot=0/165 emit=1/5680 write=0/91 total=1/11148
COMPILE-TRACE program=json_empty_containers_nest parse=0/2873 plan=1/3875 lower=0/1337 boot=0/165 emit=1/5680 write=0/91 total=2/14021
COMPILE-TRACE program=json_empty_containers_nest parse=0/0 plan=0/3875 lower=0/1337 boot=0/165 emit=1/5680 write=0/91 total=1/11148
COMPILE-TRACE program=json_empty_containers_nest parse=0/2873 plan=0/3875 lower=1/1337 boot=0/165 emit=1/5680 write=0/91 total=2/14021
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=0/0 plan=1/5234 lower=0/2462 boot=0/518 emit=1/8610 write=0/91 total=2/16915
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=0/0 plan=0/5070 lower=1/2462 boot=0/165 emit=1/8476 write=0/91 total=2/16264
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=4/118918 plan=1/5070 lower=0/2462 boot=0/165 emit=1/8476 write=0/91 total=6/135182
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=0/0 plan=1/5070 lower=0/2462 boot=0/165 emit=1/8476 write=0/91 total=2/16264
COMPILE-TRACE program=json_deep_exact_key_chain_binds parse=4/118918 plan=1/5070 lower=0/2462 boot=0/165 emit=1/8476 write=1/91 total=7/135182
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=0/0 plan=1/8955 lower=1/2735 boot=0/221 emit=1/17071 write=1/91 total=4/29073
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=0/0 plan=1/8337 lower=0/2735 boot=0/221 emit=2/17071 write=1/91 total=4/28455
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=0/7668 plan=1/8337 lower=0/2735 boot=0/221 emit=2/17071 write=1/91 total=4/36123
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=0/0 plan=1/8337 lower=0/2735 boot=0/221 emit=2/17071 write=0/91 total=3/28455
COMPILE-TRACE program=json_top_level_scalar_document_is_a_value parse=1/7668 plan=0/8337 lower=1/2735 boot=0/221 emit=2/17071 write=0/91 total=4/36123
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=0/0 plan=0/7847 lower=1/3068 boot=0/168 emit=1/9908 write=0/91 total=2/21082
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=0/0 plan=1/7774 lower=0/3068 boot=0/168 emit=1/9908 write=0/91 total=2/21009
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=1/7135 plan=0/7774 lower=1/3068 boot=0/168 emit=1/9908 write=0/91 total=3/28144
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=0/0 plan=1/7774 lower=0/3068 boot=0/168 emit=1/9908 write=1/91 total=3/21009
COMPILE-TRACE program=json_absent_key_yields_no_row_under_arrivals parse=0/7135 plan=0/7774 lower=1/3068 boot=0/168 emit=1/9908 write=0/91 total=2/28144
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=0/0 plan=1/7839 lower=0/3192 boot=0/576 emit=2/10322 write=0/91 total=3/22020
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=0/0 plan=0/7652 lower=1/3192 boot=0/167 emit=1/10173 write=0/91 total=2/21275
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=0/9434 plan=0/7652 lower=0/3192 boot=0/167 emit=1/10173 write=0/91 total=1/30709
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=0/0 plan=0/7652 lower=1/3192 boot=0/167 emit=1/10173 write=0/91 total=2/21275
COMPILE-TRACE program=json_spread_and_capture_and_descent_multiply parse=1/9434 plan=0/7652 lower=1/3192 boot=0/167 emit=1/10173 write=0/91 total=3/30709
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=0/0 plan=1/15594 lower=1/4374 boot=0/190 emit=1/13272 write=1/91 total=4/33521
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=0/0 plan=1/15278 lower=0/4374 boot=0/190 emit=2/13272 write=0/91 total=3/33205
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=1/10364 plan=1/15278 lower=0/4374 boot=0/190 emit=2/13272 write=1/91 total=5/43569
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=0/0 plan=1/15278 lower=1/4374 boot=0/190 emit=2/13272 write=0/91 total=4/33205
COMPILE-TRACE program=json_typed_capture_folds_into_a_keyed_int_total parse=0/10364 plan=1/15278 lower=1/4374 boot=0/190 emit=2/13272 write=0/91 total=4/43569
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=0/0 plan=0/5554 lower=1/1964 boot=0/168 emit=1/7905 write=0/91 total=2/15682
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=0/0 plan=1/5363 lower=0/1964 boot=0/168 emit=1/7905 write=0/91 total=2/15491
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=1/4860 plan=0/5363 lower=0/1964 boot=0/168 emit=1/7905 write=1/91 total=3/20351
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=0/0 plan=0/5363 lower=0/1964 boot=0/168 emit=1/7905 write=0/91 total=1/15491
COMPILE-TRACE program=json_typed_capture_filters_a_wrong_typed_value parse=0/4860 plan=0/5363 lower=1/1964 boot=0/168 emit=1/7905 write=0/91 total=2/20351
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/0 plan=0/5048 lower=0/1749 boot=0/167 emit=1/6891 write=0/91 total=1/13946
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/0 plan=0/4942 lower=0/1749 boot=0/167 emit=1/6891 write=1/91 total=2/13840
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/4092 plan=0/4942 lower=1/1749 boot=0/167 emit=0/6891 write=1/91 total=2/17932
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/0 plan=0/4942 lower=0/1749 boot=0/167 emit=1/6891 write=0/91 total=1/13840
COMPILE-TRACE program=json_untyped_capture_binds_without_a_type parse=0/4092 plan=1/4942 lower=0/1749 boot=0/167 emit=1/6891 write=0/91 total=2/17932
COMPILE-TRACE program=ordered_json_group_array_value parse=0/0 plan=0/4604 lower=0/1449 boot=0/313 emit=1/9205 write=1/91 total=2/15662
COMPILE-TRACE program=ordered_json_group_array_value parse=0/0 plan=1/4592 lower=0/1455 boot=0/165 emit=1/8930 write=0/91 total=2/15233
COMPILE-TRACE program=ordered_json_group_array_value parse=0/3732 plan=1/4592 lower=0/1455 boot=0/165 emit=1/8930 write=0/91 total=2/18965
COMPILE-TRACE program=ordered_json_group_array_value parse=0/0 plan=0/4592 lower=1/1455 boot=0/165 emit=1/8930 write=0/91 total=2/15233
COMPILE-TRACE program=ordered_json_group_array_value parse=0/3732 plan=1/4592 lower=0/1455 boot=0/165 emit=1/8930 write=0/91 total=2/18965
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=0/0 plan=0/4555 lower=1/1448 boot=0/263 emit=1/8275 write=0/91 total=2/14632
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=0/0 plan=1/4592 lower=0/1454 boot=0/165 emit=1/8128 write=0/91 total=2/14430
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=0/3733 plan=1/4592 lower=0/1454 boot=0/165 emit=1/8128 write=0/91 total=2/18163
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=0/0 plan=0/4592 lower=1/1454 boot=0/165 emit=1/8128 write=0/91 total=2/14430
COMPILE-TRACE program=ordered_json_group_array_integer_values parse=0/3733 plan=0/4592 lower=1/1454 boot=0/165 emit=1/8128 write=0/91 total=2/18163
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/0 plan=1/4916 lower=0/1536 boot=0/346 emit=1/9714 write=0/91 total=2/16603
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/0 plan=0/4930 lower=1/1545 boot=0/166 emit=1/9410 write=0/91 total=2/16142
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/4749 plan=1/4930 lower=0/1545 boot=0/166 emit=1/9410 write=0/91 total=2/20891
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/0 plan=1/4930 lower=0/1545 boot=0/166 emit=1/9410 write=1/91 total=3/16142
COMPILE-TRACE program=ordered_json_group_array_ordinal parse=0/4749 plan=0/4930 lower=1/1545 boot=0/166 emit=1/9410 write=0/91 total=2/20891
COMPILE-TRACE program=ordered_group_concat_value parse=0/0 plan=1/4626 lower=0/1488 boot=0/313 emit=1/9989 write=0/91 total=2/16507
COMPILE-TRACE program=ordered_group_concat_value parse=0/0 plan=0/4614 lower=1/1494 boot=0/165 emit=1/9716 write=0/91 total=2/16080
COMPILE-TRACE program=ordered_group_concat_value parse=0/3855 plan=1/4614 lower=0/1494 boot=0/165 emit=1/9716 write=0/91 total=2/19935
COMPILE-TRACE program=ordered_group_concat_value parse=0/0 plan=0/4614 lower=1/1494 boot=0/165 emit=1/9716 write=0/91 total=2/16080
COMPILE-TRACE program=ordered_group_concat_value parse=0/3855 plan=0/4614 lower=1/1494 boot=0/165 emit=1/9716 write=0/91 total=2/19935
COMPILE-TRACE program=ordered_group_concat_ordinal parse=0/0 plan=0/4938 lower=0/1575 boot=0/346 emit=1/10500 write=1/91 total=2/17450
COMPILE-TRACE program=ordered_group_concat_ordinal parse=0/0 plan=1/4952 lower=0/1584 boot=0/166 emit=1/10196 write=0/91 total=2/16989
COMPILE-TRACE program=ordered_group_concat_ordinal parse=1/4872 plan=0/4952 lower=0/1584 boot=0/166 emit=1/10196 write=1/91 total=3/21861
COMPILE-TRACE program=ordered_group_concat_ordinal parse=0/0 plan=0/4952 lower=0/1584 boot=0/166 emit=1/10196 write=1/91 total=2/16989
COMPILE-TRACE program=ordered_group_concat_ordinal parse=0/4872 plan=0/4952 lower=1/1584 boot=0/166 emit=1/10196 write=0/91 total=2/21861
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/0 plan=0/5229 lower=0/1536 boot=0/346 emit=1/9714 write=1/91 total=2/16916
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/0 plan=1/4930 lower=0/1545 boot=0/166 emit=1/9410 write=0/91 total=2/16142
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/4749 plan=1/4930 lower=0/1545 boot=0/166 emit=1/9410 write=0/91 total=2/20891
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/0 plan=1/4930 lower=0/1545 boot=0/166 emit=1/9410 write=0/91 total=2/16142
COMPILE-TRACE program=ordered_aggregate_retraction_rebuild parse=0/4749 plan=1/4930 lower=0/1545 boot=0/166 emit=1/9410 write=0/91 total=2/20891
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=0/0 plan=0/4772 lower=1/1456 boot=0/359 emit=1/8222 write=0/91 total=2/14900
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=0/0 plan=1/4602 lower=0/1456 boot=0/165 emit=1/7980 write=0/91 total=2/14294
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=0/3803 plan=1/4602 lower=0/1456 boot=0/165 emit=1/7980 write=0/91 total=2/18097
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=0/0 plan=1/4602 lower=0/1456 boot=0/165 emit=1/7980 write=0/91 total=2/14294
COMPILE-TRACE program=ordered_json_group_array_nested_json parse=0/3803 plan=1/4602 lower=0/1456 boot=0/165 emit=1/7980 write=0/91 total=2/18097
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=0/0 plan=0/4944 lower=0/1575 boot=0/285 emit=2/11368 write=0/91 total=2/18263
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=0/0 plan=0/5030 lower=0/1584 boot=0/166 emit=1/11138 write=1/91 total=2/18009
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=0/5232 plan=0/5030 lower=1/1584 boot=0/166 emit=1/11138 write=0/91 total=2/23241
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=0/0 plan=1/5030 lower=0/1584 boot=0/166 emit=1/11138 write=0/91 total=2/18009
COMPILE-TRACE program=ordered_mermaid_line_assembly parse=1/5232 plan=0/5030 lower=0/1584 boot=0/166 emit=2/11138 write=0/91 total=3/23241
COMPILE-TRACE program=ordered_fragment_line_assembly parse=0/0 plan=0/4992 lower=0/1575 boot=0/285 emit=2/11874 write=0/91 total=2/18817
COMPILE-TRACE program=ordered_fragment_line_assembly parse=0/0 plan=1/5078 lower=0/1584 boot=0/166 emit=1/11613 write=1/91 total=3/18532
COMPILE-TRACE program=ordered_fragment_line_assembly parse=0/5366 plan=0/5078 lower=1/1584 boot=0/166 emit=1/11613 write=0/91 total=2/23898
COMPILE-TRACE program=ordered_fragment_line_assembly parse=0/0 plan=0/5078 lower=1/1584 boot=0/166 emit=1/11613 write=0/91 total=2/18532
COMPILE-TRACE program=ordered_fragment_line_assembly parse=1/5366 plan=0/5078 lower=0/1584 boot=0/166 emit=2/11613 write=0/91 total=3/23898
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=0/0 plan=1/5364 lower=0/1576 boot=0/379 emit=2/13827 write=0/91 total=3/21237
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=0/0 plan=0/5424 lower=0/1588 boot=0/167 emit=2/13320 write=0/91 total=2/20590
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=0/6032 plan=1/5424 lower=0/1588 boot=0/167 emit=2/13320 write=0/91 total=3/26622
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=0/0 plan=0/5424 lower=1/1588 boot=0/167 emit=1/13320 write=0/91 total=2/20590
COMPILE-TRACE program=ordered_group_rels_v5_collect parse=1/6032 plan=0/5424 lower=0/1588 boot=0/167 emit=2/13320 write=0/91 total=3/26622
COMPILE-TRACE program=ordered_group_rels_json_head parse=0/0 plan=0/5364 lower=0/1576 boot=0/379 emit=2/14069 write=0/91 total=2/21479
COMPILE-TRACE program=ordered_group_rels_json_head parse=0/0 plan=0/5424 lower=1/1588 boot=0/167 emit=1/13562 write=0/91 total=2/20832
COMPILE-TRACE program=ordered_group_rels_json_head parse=1/6043 plan=0/5424 lower=0/1588 boot=0/167 emit=2/13562 write=0/91 total=3/26875
COMPILE-TRACE program=ordered_group_rels_json_head parse=0/0 plan=1/5424 lower=0/1588 boot=0/167 emit=1/13562 write=1/91 total=3/20832
COMPILE-TRACE program=ordered_group_rels_json_head parse=0/6043 plan=1/5424 lower=0/1588 boot=0/167 emit=1/13562 write=1/91 total=3/26875
COMPILE-TRACE program=regexp_positive_match parse=0/0 plan=1/5139 lower=0/1662 boot=0/281 emit=1/7646 write=0/91 total=2/14819
COMPILE-TRACE program=regexp_positive_match parse=0/0 plan=0/4991 lower=0/1662 boot=0/164 emit=1/7388 write=1/91 total=2/14296
COMPILE-TRACE program=regexp_positive_match parse=0/2980 plan=0/4991 lower=0/1662 boot=0/164 emit=1/7388 write=0/91 total=1/17276
COMPILE-TRACE program=regexp_positive_match parse=0/0 plan=1/4991 lower=0/1662 boot=0/164 emit=1/7388 write=0/91 total=2/14296
COMPILE-TRACE program=regexp_positive_match parse=0/2980 plan=1/4991 lower=0/1662 boot=0/164 emit=1/7388 write=0/91 total=2/17276
COMPILE-TRACE program=regexp_non_match parse=0/0 plan=0/5091 lower=0/1662 boot=0/242 emit=1/7555 write=0/91 total=1/14641
COMPILE-TRACE program=regexp_non_match parse=0/0 plan=0/4993 lower=0/1662 boot=0/164 emit=0/7383 write=1/91 total=1/14293
COMPILE-TRACE program=regexp_non_match parse=0/2977 plan=0/4993 lower=0/1662 boot=1/164 emit=0/7383 write=1/91 total=2/17270
COMPILE-TRACE program=regexp_non_match parse=0/0 plan=1/4993 lower=0/1662 boot=0/164 emit=1/7383 write=0/91 total=2/14293
COMPILE-TRACE program=regexp_non_match parse=0/2977 plan=1/4993 lower=0/1662 boot=0/164 emit=1/7383 write=0/91 total=2/17270
COMPILE-TRACE program=regexp_retraction_flip parse=0/0 plan=0/5144 lower=0/1662 boot=0/203 emit=1/7474 write=0/91 total=1/14574
COMPILE-TRACE program=regexp_retraction_flip parse=0/0 plan=0/4991 lower=0/1662 boot=0/164 emit=0/7388 write=1/91 total=1/14296
COMPILE-TRACE program=regexp_retraction_flip parse=0/2980 plan=0/4991 lower=0/1662 boot=0/164 emit=1/7388 write=1/91 total=2/17276
COMPILE-TRACE program=regexp_retraction_flip parse=0/0 plan=1/4991 lower=0/1662 boot=0/164 emit=1/7388 write=0/91 total=2/14296
COMPILE-TRACE program=regexp_retraction_flip parse=0/2980 plan=1/4991 lower=0/1662 boot=0/164 emit=1/7388 write=0/91 total=2/17276
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/0 plan=1/4168 lower=0/1310 boot=0/165 emit=1/5766 write=0/91 total=2/11500
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/0 plan=1/4201 lower=0/1313 boot=0/166 emit=1/5798 write=0/91 total=2/11569
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/2788 plan=0/4201 lower=0/1313 boot=0/166 emit=1/5798 write=0/91 total=1/14357
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/0 plan=0/4201 lower=1/1313 boot=0/166 emit=0/5798 write=1/91 total=2/11569
COMPILE-TRACE program=arrival_affinity_rewrite_keeps_delta parse=0/2788 plan=0/4201 lower=0/1313 boot=0/166 emit=1/5798 write=0/91 total=1/14357
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/0 plan=0/4342 lower=0/1336 boot=0/164 emit=1/7435 write=0/91 total=1/13368
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/0 plan=0/4049 lower=0/1336 boot=0/164 emit=1/7435 write=0/91 total=1/13075
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/2529 plan=0/4049 lower=0/1336 boot=0/164 emit=1/7435 write=0/91 total=1/15604
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/0 plan=0/4049 lower=1/1336 boot=0/164 emit=0/7435 write=0/91 total=1/13075
COMPILE-TRACE program=arrival_dup_batch_partial_ignore parse=0/2529 plan=0/4049 lower=0/1336 boot=0/164 emit=1/7435 write=1/91 total=2/15604
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/0 plan=1/6985 lower=0/2557 boot=0/184 emit=1/7924 write=0/91 total=2/17741
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/0 plan=0/7042 lower=1/2563 boot=0/186 emit=1/7990 write=0/91 total=2/17872
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/2489 plan=1/7042 lower=0/2563 boot=0/186 emit=1/7990 write=0/91 total=2/20361
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/0 plan=1/7042 lower=0/2563 boot=0/186 emit=1/7990 write=0/91 total=2/17872
COMPILE-TRACE program=combine_level_is_the_conjunction_spelling parse=0/2489 plan=1/7042 lower=0/2563 boot=0/186 emit=1/7990 write=0/91 total=2/20361
COMPILE-TRACE program=conjunction_level_control_for_combine parse=0/0 plan=0/6214 lower=1/2276 boot=0/184 emit=0/7851 write=0/91 total=1/16616
COMPILE-TRACE program=conjunction_level_control_for_combine parse=0/0 plan=1/6271 lower=0/2282 boot=0/186 emit=1/7917 write=0/91 total=2/16747
COMPILE-TRACE program=conjunction_level_control_for_combine parse=1/4544 plan=0/6271 lower=0/2282 boot=0/186 emit=1/7917 write=0/91 total=2/21291
COMPILE-TRACE program=conjunction_level_control_for_combine parse=0/0 plan=0/6271 lower=0/2282 boot=0/186 emit=1/7917 write=1/91 total=2/16747
COMPILE-TRACE program=conjunction_level_control_for_combine parse=0/4544 plan=0/6271 lower=1/2282 boot=0/186 emit=1/7917 write=0/91 total=2/21291
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=0/0 plan=1/7510 lower=0/1914 boot=0/177 emit=1/7084 write=1/91 total=3/16776
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=0/0 plan=1/7567 lower=0/1920 boot=0/179 emit=1/7150 write=0/91 total=2/16907
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=1/3060 plan=0/7567 lower=0/1920 boot=0/179 emit=1/7150 write=1/91 total=3/19967
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=0/0 plan=0/7567 lower=1/1920 boot=0/179 emit=0/7150 write=1/91 total=2/16907
COMPILE-TRACE program=combine_edge_is_the_conjunction_spelling parse=0/3060 plan=0/7567 lower=1/1920 boot=0/179 emit=1/7150 write=0/91 total=2/19967
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/0 plan=0/6616 lower=0/1830 boot=0/177 emit=1/7095 write=1/91 total=2/15809
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/0 plan=1/6673 lower=0/1836 boot=0/179 emit=1/7161 write=0/91 total=2/15940
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/5117 plan=1/6673 lower=0/1836 boot=0/179 emit=1/7161 write=1/91 total=3/21057
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/0 plan=0/6673 lower=0/1836 boot=0/179 emit=1/7161 write=1/91 total=2/15940
COMPILE-TRACE program=conjunction_edge_control_for_combine parse=0/5117 plan=0/6673 lower=1/1836 boot=0/179 emit=1/7161 write=0/91 total=2/21057
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/0 plan=0/4723 lower=0/1561 boot=0/163 emit=1/5361 write=0/91 total=1/11899
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/0 plan=0/4757 lower=0/1564 boot=0/164 emit=1/5393 write=0/91 total=1/11969
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/1484 plan=1/4757 lower=0/1564 boot=0/164 emit=1/5393 write=0/91 total=2/13453
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/0 plan=0/4757 lower=0/1564 boot=0/164 emit=1/5393 write=0/91 total=1/11969
COMPILE-TRACE program=next_level_is_the_bare_atom_spelling parse=0/1484 plan=1/4757 lower=0/1564 boot=0/164 emit=1/5393 write=0/91 total=2/13453
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/0 plan=1/5033 lower=0/1123 boot=0/156 emit=1/4321 write=0/91 total=2/10724
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/0 plan=1/5067 lower=0/1126 boot=0/157 emit=0/4353 write=1/91 total=2/10794
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/1985 plan=0/5067 lower=0/1126 boot=0/157 emit=1/4353 write=0/91 total=1/12779
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/0 plan=1/5067 lower=0/1126 boot=0/157 emit=0/4353 write=1/91 total=2/10794
COMPILE-TRACE program=next_edge_is_the_bare_atom_spelling parse=0/1985 plan=0/5067 lower=0/1126 boot=0/157 emit=1/4353 write=0/91 total=1/12779
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=0/0 plan=3/57190 lower=2/13113 boot=0/346 emit=5/47015 write=0/91 total=10/117755
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=0/0 plan=3/54435 lower=2/13134 boot=0/353 emit=5/47244 write=0/91 total=10/115257
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=3/37909 plan=3/54435 lower=2/13134 boot=0/353 emit=5/47244 write=0/91 total=13/153166
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=0/0 plan=3/54435 lower=2/13134 boot=0/353 emit=5/47244 write=0/91 total=10/115257
COMPILE-TRACE program=diag_scenario_seven_ticks_end_to_end parse=3/37909 plan=3/54435 lower=2/13134 boot=0/353 emit=5/47244 write=0/91 total=13/153166
COMPILE-TRACE program=clock_rel_join_storms parse=0/0 plan=1/25845 lower=1/5019 boot=0/252 emit=3/26606 write=0/91 total=5/57813
COMPILE-TRACE program=clock_rel_join_storms parse=0/0 plan=1/22523 lower=1/5037 boot=0/258 emit=3/26789 write=0/91 total=5/54698
COMPILE-TRACE program=clock_rel_join_storms parse=1/18245 plan=2/22523 lower=1/5037 boot=0/258 emit=3/26789 write=0/91 total=7/72943
COMPILE-TRACE program=clock_rel_join_storms parse=0/0 plan=1/22523 lower=1/5037 boot=0/258 emit=3/26789 write=0/91 total=5/54698
COMPILE-TRACE program=clock_rel_join_storms parse=1/18245 plan=2/22523 lower=1/5037 boot=0/258 emit=3/26789 write=0/91 total=7/72943
COMPILE-TRACE program=retention_count_prunes_oldest parse=0/0 plan=0/1617 lower=0/396 boot=0/138 emit=1/3300 write=0/91 total=1/5542
COMPILE-TRACE program=retention_count_prunes_oldest parse=0/0 plan=0/1567 lower=0/399 boot=0/139 emit=0/3332 write=1/91 total=1/5528
COMPILE-TRACE program=retention_count_prunes_oldest parse=0/876 plan=0/1567 lower=0/399 boot=0/139 emit=1/3332 write=0/91 total=1/6404
COMPILE-TRACE program=retention_count_prunes_oldest parse=0/0 plan=1/1567 lower=0/399 boot=0/139 emit=0/3332 write=0/91 total=1/5528
COMPILE-TRACE program=retention_count_prunes_oldest parse=1/876 plan=0/1567 lower=0/399 boot=0/139 emit=0/3332 write=1/91 total=2/6404
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=0/0 plan=0/1617 lower=0/396 boot=0/138 emit=1/3300 write=0/91 total=1/5542
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=0/0 plan=0/1567 lower=1/399 boot=0/139 emit=0/3332 write=0/91 total=1/5528
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=1/876 plan=0/1567 lower=0/399 boot=0/139 emit=0/3332 write=1/91 total=2/6404
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=0/0 plan=0/1567 lower=0/399 boot=0/139 emit=1/3332 write=0/91 total=1/5528
COMPILE-TRACE program=retention_prune_is_a_visible_minus parse=0/876 plan=0/1567 lower=1/399 boot=0/139 emit=0/3332 write=0/91 total=1/6404
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/0 plan=0/5421 lower=1/1057 boot=0/161 emit=1/6962 write=0/91 total=2/13692
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/0 plan=1/5137 lower=0/1063 boot=0/163 emit=1/7015 write=0/91 total=2/13469
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/2870 plan=1/5137 lower=0/1063 boot=0/163 emit=1/7015 write=0/91 total=2/16339
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/0 plan=0/5137 lower=1/1063 boot=0/163 emit=1/7015 write=0/91 total=2/13469
COMPILE-TRACE program=finalize_over_log_fires_on_retention_prune parse=0/2870 plan=1/5137 lower=0/1063 boot=0/163 emit=1/7015 write=0/91 total=2/16339
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=0/0 plan=1/13527 lower=0/3217 boot=0/158 emit=2/10189 write=0/91 total=3/27182
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=0/0 plan=1/13437 lower=0/3223 boot=0/160 emit=2/10242 write=0/91 total=3/27153
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=1/13281 plan=1/13437 lower=0/3223 boot=0/160 emit=2/10242 write=0/91 total=4/40434
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=0/0 plan=0/13437 lower=0/3223 boot=0/160 emit=1/10242 write=1/91 total=2/27153
COMPILE-TRACE program=created_at_pinned_updated_at_advances parse=1/13281 plan=1/13437 lower=0/3223 boot=0/160 emit=2/10242 write=0/91 total=4/40434
COMPILE-TRACE program=log_retraction_rejected parse=0/0 plan=0/1565 lower=0/381 boot=0/136 emit=1/3145 write=0/91 total=1/5318
COMPILE-TRACE program=log_retraction_rejected parse=0/0 plan=0/1554 lower=0/384 boot=0/137 emit=0/3177 write=1/91 total=1/5343
COMPILE-TRACE program=log_retraction_rejected parse=0/810 plan=0/1554 lower=0/384 boot=0/137 emit=1/3177 write=0/91 total=1/6153
COMPILE-TRACE program=log_retraction_rejected parse=0/0 plan=0/1554 lower=0/384 boot=0/137 emit=0/3177 write=1/91 total=1/5343
COMPILE-TRACE program=log_retraction_rejected parse=0/810 plan=0/1554 lower=0/384 boot=0/137 emit=1/3177 write=0/91 total=1/6153
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/0 plan=1/1621 lower=0/483 boot=0/135 emit=0/3981 write=1/91 total=2/6311
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/0 plan=0/1639 lower=0/489 boot=0/137 emit=1/4032 write=0/91 total=1/6388
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/953 plan=0/1639 lower=1/489 boot=0/137 emit=0/4032 write=0/91 total=1/7341
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/0 plan=0/1639 lower=0/489 boot=0/137 emit=1/4032 write=0/91 total=1/6388
COMPILE-TRACE program=world_fed_keyed_arrival_replaces parse=0/953 plan=1/1639 lower=0/489 boot=0/137 emit=0/4032 write=1/91 total=2/7341
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/0 plan=0/4518 lower=0/976 boot=0/161 emit=1/6119 write=1/91 total=2/11865
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/0 plan=0/4462 lower=1/979 boot=0/162 emit=0/6152 write=1/91 total=2/11846
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/3213 plan=0/4462 lower=0/979 boot=0/162 emit=1/6152 write=0/91 total=1/15059
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/0 plan=1/4462 lower=0/979 boot=0/162 emit=1/6152 write=0/91 total=2/11846
COMPILE-TRACE program=retention_single_arm_still_prunes parse=0/3213 plan=1/4462 lower=0/979 boot=0/162 emit=1/6152 write=0/91 total=2/15059
COMPILE-TRACE program=now_reads_the_tick parse=0/0 plan=1/6039 lower=0/1319 boot=0/159 emit=1/6249 write=0/91 total=2/13857
COMPILE-TRACE program=now_reads_the_tick parse=0/0 plan=0/6017 lower=0/1322 boot=0/160 emit=1/6282 write=0/91 total=1/13872
COMPILE-TRACE program=now_reads_the_tick parse=0/3723 plan=0/6017 lower=0/1322 boot=0/160 emit=1/6282 write=1/91 total=2/17595
COMPILE-TRACE program=now_reads_the_tick parse=0/0 plan=0/6017 lower=0/1322 boot=0/160 emit=1/6282 write=0/91 total=1/13872
COMPILE-TRACE program=now_reads_the_tick parse=1/3723 plan=0/6017 lower=0/1322 boot=0/160 emit=1/6282 write=1/91 total=3/17595
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=0/0 plan=1/7802 lower=0/1547 boot=0/182 emit=1/8726 write=0/91 total=2/18348
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=0/0 plan=1/7839 lower=0/1550 boot=0/183 emit=1/8760 write=0/91 total=2/18423
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=1/5481 plan=0/7839 lower=1/1550 boot=0/183 emit=1/8760 write=0/91 total=3/23904
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=0/0 plan=1/7839 lower=0/1550 boot=0/183 emit=1/8760 write=0/91 total=2/18423
COMPILE-TRACE program=edge_chain_hops_tick_per_stage parse=1/5481 plan=0/7839 lower=1/1550 boot=0/183 emit=1/8760 write=0/91 total=3/23904
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/0 plan=0/7255 lower=1/1545 boot=0/182 emit=1/9669 write=0/91 total=2/18742
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/0 plan=0/7246 lower=1/1551 boot=0/184 emit=1/9737 write=0/91 total=2/18809
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/4447 plan=1/7246 lower=0/1551 boot=0/184 emit=1/9737 write=1/91 total=3/23256
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/0 plan=0/7246 lower=0/1551 boot=0/184 emit=2/9737 write=0/91 total=2/18809
COMPILE-TRACE program=marker_stops_backlog_replay parse=0/4447 plan=1/7246 lower=0/1551 boot=0/184 emit=1/9737 write=0/91 total=2/23256
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=0/0 plan=1/7029 lower=0/1708 boot=0/182 emit=1/10192 write=1/91 total=3/19202
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=0/0 plan=1/7020 lower=0/1714 boot=0/184 emit=1/10260 write=1/91 total=3/19269
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=0/5493 plan=0/7020 lower=1/1714 boot=0/184 emit=1/10260 write=0/91 total=2/24762
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=0/0 plan=0/7020 lower=1/1714 boot=0/184 emit=1/10260 write=0/91 total=2/19269
COMPILE-TRACE program=unmarked_edge_replays_backlog parse=1/5493 plan=0/7020 lower=0/1714 boot=0/184 emit=2/10260 write=0/91 total=3/24762
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/0 plan=0/4062 lower=0/1333 boot=0/163 emit=1/7270 write=1/91 total=2/12919
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/0 plan=0/3989 lower=1/1336 boot=0/164 emit=1/7303 write=0/91 total=2/12883
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/2511 plan=0/3989 lower=0/1336 boot=0/164 emit=0/7303 write=0/91 total=0/15394
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/0 plan=0/3989 lower=0/1336 boot=0/164 emit=1/7303 write=0/91 total=1/12883
COMPILE-TRACE program=retraction_only_tick_retracts_level_view parse=0/2511 plan=1/3989 lower=0/1336 boot=0/164 emit=1/7303 write=0/91 total=2/15394
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/0 plan=0/8741 lower=1/2156 boot=0/186 emit=1/10859 write=0/91 total=2/22033
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/0 plan=1/8721 lower=0/2159 boot=0/187 emit=2/10893 write=0/91 total=3/22051
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/4447 plan=1/8721 lower=0/2159 boot=0/187 emit=2/10893 write=0/91 total=3/26498
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/0 plan=1/8721 lower=0/2159 boot=0/187 emit=2/10893 write=0/91 total=3/22051
COMPILE-TRACE program=departed_fires_next_tick_on_retraction parse=0/4447 plan=1/8721 lower=0/2159 boot=0/187 emit=2/10893 write=0/91 total=3/26498
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=0/0 plan=0/8705 lower=1/1727 boot=0/181 emit=2/12809 write=0/91 total=3/23513
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=0/0 plan=0/8712 lower=1/1733 boot=0/183 emit=1/12865 write=0/91 total=2/23584
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=1/6106 plan=1/8712 lower=0/1733 boot=0/183 emit=2/12865 write=0/91 total=4/29690
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=0/0 plan=1/8712 lower=0/1733 boot=0/183 emit=2/12865 write=0/91 total=3/23584
COMPILE-TRACE program=keyed_replace_departs_the_old_row parse=1/6106 plan=0/8712 lower=0/1733 boot=1/183 emit=1/12865 write=0/91 total=3/29690
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=0/0 plan=1/6843 lower=0/1437 boot=0/158 emit=1/7847 write=1/91 total=3/16376
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=0/0 plan=1/6771 lower=0/1443 boot=0/160 emit=1/7900 write=0/91 total=2/16365
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=1/5255 plan=0/6771 lower=1/1443 boot=0/160 emit=1/7900 write=0/91 total=3/21620
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=0/0 plan=1/6771 lower=0/1443 boot=0/160 emit=1/7900 write=0/91 total=2/16365
COMPILE-TRACE program=pairwise_reads_state_at_the_departure_tick parse=0/5255 plan=1/6771 lower=0/1443 boot=0/160 emit=1/7900 write=1/91 total=3/21620
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=0/0 plan=1/6855 lower=0/1437 boot=0/158 emit=1/7847 write=0/91 total=2/16388
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=0/0 plan=1/6771 lower=0/1443 boot=0/160 emit=1/7900 write=0/91 total=2/16365
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=1/5255 plan=0/6771 lower=0/1443 boot=0/160 emit=2/7900 write=0/91 total=3/21620
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=0/0 plan=0/6771 lower=1/1443 boot=0/160 emit=1/7900 write=0/91 total=2/16365
COMPILE-TRACE program=pairwise_pairs_adjacent_values_when_the_source_idles parse=0/5255 plan=1/6771 lower=0/1443 boot=0/160 emit=1/7900 write=0/91 total=2/21620
COMPILE-TRACE program=set_dedups_log_stacks parse=0/0 plan=1/7916 lower=0/1719 boot=0/203 emit=2/11061 write=0/91 total=3/20990
COMPILE-TRACE program=set_dedups_log_stacks parse=0/0 plan=0/7886 lower=1/1725 boot=0/205 emit=1/11131 write=0/91 total=2/21038
COMPILE-TRACE program=set_dedups_log_stacks parse=1/5768 plan=0/7886 lower=1/1725 boot=0/205 emit=1/11131 write=0/91 total=3/26806
COMPILE-TRACE program=set_dedups_log_stacks parse=0/0 plan=0/7886 lower=0/1725 boot=1/205 emit=1/11131 write=0/91 total=2/21038
COMPILE-TRACE program=set_dedups_log_stacks parse=1/5768 plan=0/7886 lower=1/1725 boot=0/205 emit=1/11131 write=1/91 total=4/26806
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=0/0 plan=1/9539 lower=1/3996 boot=0/471 emit=2/16323 write=0/91 total=4/30420
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=0/0 plan=0/9575 lower=1/4011 boot=0/189 emit=2/15798 write=0/91 total=3/29664
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=1/9937 plan=0/9575 lower=1/4011 boot=0/189 emit=1/15798 write=1/91 total=4/39601
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=0/0 plan=1/9575 lower=0/4011 boot=0/189 emit=2/15798 write=0/91 total=3/29664
COMPILE-TRACE program=head_expression_evaluates_derived_column parse=1/9937 plan=0/9575 lower=1/4011 boot=0/189 emit=2/15798 write=0/91 total=4/39601
COMPILE-TRACE program=comparison_filters_rows parse=0/0 plan=1/17687 lower=1/7552 boot=0/515 emit=2/23335 write=1/91 total=5/49180
COMPILE-TRACE program=comparison_filters_rows parse=0/0 plan=1/17689 lower=1/7567 boot=0/218 emit=2/22815 write=1/91 total=5/48380
COMPILE-TRACE program=comparison_filters_rows parse=1/18632 plan=1/17689 lower=1/7567 boot=0/218 emit=2/22815 write=1/91 total=6/67012
COMPILE-TRACE program=comparison_filters_rows parse=0/0 plan=0/17689 lower=2/7567 boot=0/218 emit=2/22815 write=0/91 total=4/48380
COMPILE-TRACE program=comparison_filters_rows parse=1/18632 plan=1/17689 lower=1/7567 boot=0/218 emit=3/22815 write=0/91 total=6/67012
COMPILE-TRACE program=range_join_over_arithmetic parse=0/0 plan=0/9397 lower=1/3534 boot=0/343 emit=1/14110 write=1/91 total=3/27475
COMPILE-TRACE program=range_join_over_arithmetic parse=0/0 plan=1/9528 lower=1/3546 boot=0/188 emit=1/13829 write=0/91 total=3/27182
COMPILE-TRACE program=range_join_over_arithmetic parse=1/8396 plan=0/9528 lower=1/3546 boot=0/188 emit=1/13829 write=1/91 total=4/35578
COMPILE-TRACE program=range_join_over_arithmetic parse=0/0 plan=0/9528 lower=1/3546 boot=0/188 emit=1/13829 write=1/91 total=3/27182
COMPILE-TRACE program=range_join_over_arithmetic parse=0/8396 plan=1/9528 lower=0/3546 boot=0/188 emit=2/13829 write=0/91 total=3/35578
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=0/0 plan=0/9836 lower=1/3603 boot=0/418 emit=1/13211 write=1/91 total=3/27159
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=0/0 plan=1/9954 lower=1/3618 boot=0/189 emit=1/12903 write=0/91 total=3/26755
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=1/8104 plan=1/9954 lower=0/3618 boot=0/189 emit=2/12903 write=0/91 total=4/34859
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=0/0 plan=1/9954 lower=1/3618 boot=0/189 emit=1/12903 write=0/91 total=3/26755
COMPILE-TRACE program=bind_computes_derived_value_then_comparison_filters parse=1/8104 plan=0/9954 lower=1/3618 boot=0/189 emit=1/12903 write=1/91 total=4/34859
COMPILE-TRACE program=interpolation_desugars_to_concat parse=0/0 plan=1/5885 lower=0/2042 boot=0/213 emit=1/10483 write=1/91 total=3/18714
COMPILE-TRACE program=interpolation_desugars_to_concat parse=0/0 plan=1/5977 lower=0/2048 boot=0/165 emit=1/10411 write=1/91 total=3/18692
COMPILE-TRACE program=interpolation_desugars_to_concat parse=0/5457 plan=0/5977 lower=1/2048 boot=0/165 emit=1/10411 write=0/91 total=2/24149
COMPILE-TRACE program=interpolation_desugars_to_concat parse=0/0 plan=1/5977 lower=0/2048 boot=0/165 emit=1/10411 write=0/91 total=2/18692
COMPILE-TRACE program=interpolation_desugars_to_concat parse=0/5457 plan=0/5977 lower=1/2048 boot=0/165 emit=1/10411 write=0/91 total=2/24149
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=0/0 plan=0/7661 lower=1/2501 boot=0/407 emit=1/10862 write=0/91 total=2/21522
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=0/0 plan=1/7579 lower=0/2510 boot=0/166 emit=1/10332 write=0/91 total=2/20678
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=1/6410 plan=0/7579 lower=1/2510 boot=0/166 emit=1/10332 write=0/91 total=3/27088
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=0/0 plan=1/7579 lower=0/2510 boot=0/166 emit=1/10332 write=1/91 total=3/20678
COMPILE-TRACE program=division_truncates_toward_zero_mod_follows_divisor_sign parse=0/6410 plan=0/7579 lower=1/2510 boot=0/166 emit=1/10332 write=0/91 total=2/27088
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/0 plan=1/1436 lower=0/404 boot=0/135 emit=0/2777 write=1/91 total=2/4843
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/0 plan=0/1436 lower=0/404 boot=0/135 emit=0/2777 write=1/91 total=1/4843
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/712 plan=0/1436 lower=0/404 boot=0/135 emit=0/2777 write=0/91 total=0/5555
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/0 plan=1/1436 lower=0/404 boot=0/135 emit=0/2777 write=0/91 total=1/4843
COMPILE-TRACE program=typed_int_without_literal_witness parse=0/712 plan=1/1436 lower=0/404 boot=0/135 emit=0/2777 write=1/91 total=2/5555
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=0/0 plan=0/5775 lower=1/2230 boot=0/755 emit=1/10924 write=0/91 total=2/19775
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=0/0 plan=0/5607 lower=1/2230 boot=0/167 emit=1/10736 write=0/91 total=2/18831
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=1/6411 plan=0/5607 lower=0/2230 boot=0/167 emit=2/10736 write=0/91 total=3/25242
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=0/0 plan=1/5607 lower=0/2230 boot=0/167 emit=1/10736 write=1/91 total=3/18831
COMPILE-TRACE program=json_array_spread_fans_out_correlated_siblings parse=0/6411 plan=1/5607 lower=0/2230 boot=0/167 emit=1/10736 write=1/91 total=3/25242
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=0/0 plan=1/4916 lower=0/1818 boot=0/459 emit=1/6642 write=0/91 total=2/13926
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=0/0 plan=0/4805 lower=0/1818 boot=0/165 emit=1/6514 write=1/91 total=2/13393
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=0/4091 plan=0/4805 lower=0/1818 boot=0/165 emit=0/6514 write=1/91 total=1/17484
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=0/0 plan=0/4805 lower=0/1818 boot=0/165 emit=1/6514 write=0/91 total=1/13393
COMPILE-TRACE program=json_array_spread_skips_non_matching_elements parse=1/4091 plan=0/4805 lower=0/1818 boot=0/165 emit=1/6514 write=1/91 total=3/17484
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/0 plan=0/5186 lower=0/1855 boot=0/353 emit=1/8354 write=1/91 total=2/15839
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/0 plan=0/5109 lower=1/1855 boot=0/166 emit=1/8240 write=0/91 total=2/15461
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/4230 plan=1/5109 lower=0/1855 boot=0/166 emit=1/8240 write=0/91 total=2/19691
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/0 plan=1/5109 lower=0/1855 boot=0/166 emit=1/8240 write=0/91 total=2/15461
COMPILE-TRACE program=json_key_capture_binds_key_and_value parse=0/4230 plan=1/5109 lower=0/1855 boot=0/166 emit=1/8240 write=0/91 total=2/19691
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=0/0 plan=0/5474 lower=1/2211 boot=0/945 emit=1/10856 write=0/91 total=2/19577
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=0/0 plan=0/5302 lower=1/2211 boot=0/164 emit=1/10639 write=0/91 total=2/18407
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=1/10495 plan=0/5302 lower=1/2211 boot=0/164 emit=1/10639 write=0/91 total=3/28902
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=0/0 plan=1/5302 lower=0/2211 boot=0/164 emit=1/10639 write=1/91 total=3/18407
COMPILE-TRACE program=json_key_capture_nests_and_fans_out parse=0/10495 plan=0/5302 lower=1/2211 boot=0/164 emit=1/10639 write=0/91 total=2/28902
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=0/0 plan=0/5246 lower=1/2092 boot=0/846 emit=1/9847 write=0/91 total=2/18122
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=0/0 plan=0/5081 lower=1/2092 boot=0/164 emit=1/9645 write=0/91 total=2/17073
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=1/7658 plan=0/5081 lower=0/2092 boot=0/164 emit=1/9645 write=1/91 total=3/24731
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=0/0 plan=0/5081 lower=0/2092 boot=1/164 emit=1/9645 write=0/91 total=2/17073
COMPILE-TRACE program=json_descent_matches_at_any_depth parse=0/7658 plan=1/5081 lower=0/2092 boot=0/164 emit=1/9645 write=1/91 total=3/24731
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=0/0 plan=1/4866 lower=0/1840 boot=0/440 emit=1/7255 write=0/91 total=2/14492
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=0/0 plan=1/4736 lower=0/1840 boot=0/164 emit=1/7130 write=0/91 total=2/13961
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=1/4322 plan=0/4736 lower=0/1840 boot=0/164 emit=1/7130 write=0/91 total=2/18283
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=0/0 plan=1/4736 lower=0/1840 boot=0/164 emit=1/7130 write=0/91 total=2/13961
COMPILE-TRACE program=json_descent_into_scalars_is_silent parse=0/4322 plan=1/4736 lower=0/1840 boot=0/164 emit=1/7130 write=0/91 total=2/18283
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/0 plan=0/5514 lower=0/1747 boot=1/382 emit=1/8285 write=0/91 total=2/16019
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/0 plan=1/5273 lower=0/1747 boot=0/165 emit=1/7955 write=0/91 total=2/15231
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/3729 plan=1/5273 lower=0/1747 boot=0/165 emit=1/7955 write=0/91 total=2/18960
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/0 plan=1/5273 lower=0/1747 boot=0/165 emit=1/7955 write=0/91 total=2/15231
COMPILE-TRACE program=json_empty_object_pattern_matches_any_object parse=0/3729 plan=1/5273 lower=0/1747 boot=0/165 emit=1/7955 write=0/91 total=2/18960
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/0 plan=0/5568 lower=1/1880 boot=0/331 emit=1/9623 write=0/91 total=2/17493
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/0 plan=0/5410 lower=0/1880 boot=0/167 emit=1/9406 write=0/91 total=1/16954
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/4782 plan=0/5410 lower=1/1880 boot=0/167 emit=1/9406 write=0/91 total=2/21736
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/0 plan=0/5410 lower=1/1880 boot=0/167 emit=1/9406 write=0/91 total=2/16954
COMPILE-TRACE program=list_column_fans_out_through_spread parse=0/4782 plan=1/5410 lower=0/1880 boot=0/167 emit=1/9406 write=1/91 total=3/21736
COMPILE-TRACE program=count_is_bag_of_derivations parse=0/0 plan=1/4848 lower=0/1549 boot=0/346 emit=1/7827 write=0/91 total=2/14661
COMPILE-TRACE program=count_is_bag_of_derivations parse=0/0 plan=0/4853 lower=0/1558 boot=0/166 emit=1/7565 write=1/91 total=2/14233
COMPILE-TRACE program=count_is_bag_of_derivations parse=0/4224 plan=0/4853 lower=1/1558 boot=0/166 emit=1/7565 write=0/91 total=2/18457
COMPILE-TRACE program=count_is_bag_of_derivations parse=0/0 plan=0/4853 lower=0/1558 boot=0/166 emit=1/7565 write=0/91 total=1/14233
COMPILE-TRACE program=count_is_bag_of_derivations parse=1/4224 plan=0/4853 lower=0/1558 boot=0/166 emit=1/7565 write=0/91 total=2/18457
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=0/0 plan=0/5234 lower=1/1772 boot=0/313 emit=1/8444 write=0/91 total=2/15854
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=0/0 plan=0/5195 lower=0/1778 boot=0/165 emit=1/8180 write=0/91 total=1/15409
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=0/4571 plan=0/5195 lower=0/1778 boot=0/165 emit=1/8180 write=0/91 total=1/19980
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=0/0 plan=0/5195 lower=0/1778 boot=0/165 emit=1/8180 write=0/91 total=1/15409
COMPILE-TRACE program=sum_min_max_group_by_plain_columns parse=1/4571 plan=0/5195 lower=0/1778 boot=0/165 emit=1/8180 write=1/91 total=3/19980
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=0/0 plan=1/5383 lower=0/1760 boot=0/163 emit=1/8099 write=0/91 total=2/15496
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=0/0 plan=0/5171 lower=1/1766 boot=0/165 emit=1/8152 write=0/91 total=2/15345
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=0/4583 plan=0/5171 lower=1/1766 boot=0/165 emit=1/8152 write=0/91 total=2/19928
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=0/0 plan=0/5171 lower=1/1766 boot=0/165 emit=0/8152 write=1/91 total=2/15345
COMPILE-TRACE program=aggregate_count_min_max_track_arrivals_and_retraction parse=0/4583 plan=0/5171 lower=1/1766 boot=0/165 emit=1/8152 write=0/91 total=2/19928
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=0/0 plan=0/5502 lower=0/1760 boot=0/363 emit=1/8520 write=0/91 total=1/16236
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=0/0 plan=0/5171 lower=1/1766 boot=0/165 emit=1/8152 write=0/91 total=2/15345
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=0/4583 plan=1/5171 lower=0/1766 boot=0/165 emit=1/8152 write=0/91 total=2/19928
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=0/0 plan=0/5171 lower=1/1766 boot=0/165 emit=1/8152 write=0/91 total=2/15345
COMPILE-TRACE program=aggregate_min_recomputes_when_the_minimum_is_retracted parse=0/4583 plan=1/5171 lower=0/1766 boot=0/165 emit=1/8152 write=0/91 total=2/19928
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=0/0 plan=0/4996 lower=0/1562 boot=1/285 emit=1/8852 write=0/91 total=2/15786
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=0/0 plan=1/4917 lower=0/1571 boot=0/166 emit=1/8683 write=0/91 total=2/15428
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=1/4424 plan=0/4917 lower=0/1571 boot=0/166 emit=1/8683 write=0/91 total=2/19852
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=0/0 plan=1/4917 lower=0/1571 boot=0/166 emit=1/8683 write=0/91 total=2/15428
COMPILE-TRACE program=aggregate_sum_tracks_a_growing_and_shrinking_group parse=1/4424 plan=0/4917 lower=0/1571 boot=0/166 emit=1/8683 write=0/91 total=2/19852
COMPILE-TRACE program=merge_batches_per_tick parse=0/0 plan=1/7313 lower=0/1570 boot=0/182 emit=1/8571 write=0/91 total=2/17727
COMPILE-TRACE program=merge_batches_per_tick parse=0/0 plan=0/7276 lower=1/1576 boot=0/184 emit=1/8639 write=0/91 total=2/17766
COMPILE-TRACE program=merge_batches_per_tick parse=0/5381 plan=1/7276 lower=0/1576 boot=0/184 emit=1/8639 write=1/91 total=3/23147
COMPILE-TRACE program=merge_batches_per_tick parse=0/0 plan=0/7276 lower=0/1576 boot=0/184 emit=1/8639 write=1/91 total=2/17766
COMPILE-TRACE program=merge_batches_per_tick parse=0/5381 plan=0/7276 lower=1/1576 boot=0/184 emit=1/8639 write=0/91 total=2/23147
COMPILE-TRACE program=merge_never_retracts parse=0/0 plan=0/7213 lower=0/1570 boot=1/182 emit=1/8571 write=0/91 total=2/17627
COMPILE-TRACE program=merge_never_retracts parse=0/0 plan=1/7276 lower=0/1576 boot=0/184 emit=1/8639 write=1/91 total=3/17766
COMPILE-TRACE program=merge_never_retracts parse=0/5381 plan=0/7276 lower=1/1576 boot=0/184 emit=1/8639 write=6/91 total=8/23147
COMPILE-TRACE program=merge_never_retracts parse=0/0 plan=1/7276 lower=0/1576 boot=0/184 emit=1/8639 write=0/91 total=2/17766
COMPILE-TRACE program=merge_never_retracts parse=1/5381 plan=0/7276 lower=1/1576 boot=0/184 emit=1/8639 write=0/91 total=3/23147
COMPILE-TRACE program=key_last_write_wins parse=0/0 plan=0/8218 lower=1/1792 boot=0/181 emit=1/12440 write=1/91 total=3/22722
COMPILE-TRACE program=key_last_write_wins parse=0/0 plan=1/8309 lower=0/1804 boot=0/185 emit=2/12552 write=0/91 total=3/22941
COMPILE-TRACE program=key_last_write_wins parse=1/7523 plan=0/8309 lower=0/1804 boot=0/185 emit=2/12552 write=0/91 total=3/30464
COMPILE-TRACE program=key_last_write_wins parse=0/0 plan=1/8309 lower=0/1804 boot=0/185 emit=2/12552 write=0/91 total=3/22941
COMPILE-TRACE program=key_last_write_wins parse=1/7523 plan=0/8309 lower=1/1804 boot=0/185 emit=1/12552 write=0/91 total=3/30464
COMPILE-TRACE program=key_identical_write_is_silent parse=0/0 plan=0/8136 lower=1/1792 boot=0/181 emit=1/12440 write=1/91 total=3/22640
COMPILE-TRACE program=key_identical_write_is_silent parse=0/0 plan=1/8309 lower=0/1804 boot=0/185 emit=2/12552 write=0/91 total=3/22941
COMPILE-TRACE program=key_identical_write_is_silent parse=1/7523 plan=0/8309 lower=1/1804 boot=0/185 emit=1/12552 write=0/91 total=3/30464
COMPILE-TRACE program=key_identical_write_is_silent parse=0/0 plan=0/8309 lower=1/1804 boot=0/185 emit=1/12552 write=1/91 total=3/22941
COMPILE-TRACE program=key_identical_write_is_silent parse=0/7523 plan=0/8309 lower=0/1804 boot=0/185 emit=2/12552 write=0/91 total=2/30464
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=0/0 plan=0/8122 lower=1/1792 boot=0/181 emit=1/12440 write=0/91 total=2/22626
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=0/0 plan=1/8309 lower=0/1804 boot=0/185 emit=2/12552 write=0/91 total=3/22941
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=1/7523 plan=0/8309 lower=1/1804 boot=0/185 emit=1/12552 write=1/91 total=4/30464
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=0/0 plan=0/8309 lower=1/1804 boot=0/185 emit=1/12552 write=1/91 total=3/22941
COMPILE-TRACE program=key_same_tick_ordered_not_conflict parse=0/7523 plan=0/8309 lower=0/1804 boot=0/185 emit=1/12552 write=1/91 total=2/30464
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=0/0 plan=1/18061 lower=1/4644 boot=0/266 emit=2/17257 write=1/91 total=5/40319
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=0/0 plan=1/19107 lower=1/4656 boot=0/214 emit=2/17268 write=0/91 total=4/41336
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=1/13148 plan=1/19107 lower=1/4656 boot=0/214 emit=2/17268 write=1/91 total=6/54484
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=0/0 plan=1/19107 lower=1/4656 boot=0/214 emit=2/17268 write=0/91 total=4/41336
COMPILE-TRACE program=counter_fold_matches_hand_computation parse=1/13148 plan=1/19107 lower=1/4656 boot=0/214 emit=2/17268 write=0/91 total=5/54484
COMPILE-TRACE program=batched_increments_both_count parse=0/0 plan=0/7041 lower=0/1839 boot=1/208 emit=1/8703 write=0/91 total=2/17882
COMPILE-TRACE program=batched_increments_both_count parse=0/0 plan=0/7455 lower=0/1845 boot=0/160 emit=1/8652 write=0/91 total=1/18203
COMPILE-TRACE program=batched_increments_both_count parse=0/5414 plan=1/7455 lower=0/1845 boot=0/160 emit=1/8652 write=0/91 total=2/23617
COMPILE-TRACE program=batched_increments_both_count parse=0/0 plan=1/7455 lower=0/1845 boot=0/160 emit=1/8652 write=1/91 total=3/18203
COMPILE-TRACE program=batched_increments_both_count parse=0/5414 plan=1/7455 lower=0/1845 boot=0/160 emit=1/8652 write=1/91 total=3/23617
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=0/0 plan=0/12879 lower=1/3263 boot=0/234 emit=1/13515 write=1/91 total=3/29982
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=0/0 plan=1/13856 lower=0/3275 boot=0/185 emit=2/13522 write=0/91 total=3/30929
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=1/9967 plan=0/13856 lower=0/3275 boot=0/185 emit=2/13522 write=0/91 total=3/40896
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=0/0 plan=1/13856 lower=0/3275 boot=0/185 emit=2/13522 write=1/91 total=4/30929
COMPILE-TRACE program=increment_decrement_same_tick_nets_zero parse=0/9967 plan=1/13856 lower=1/3275 boot=0/185 emit=2/13522 write=0/91 total=4/40896
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/0 plan=0/6708 lower=0/1794 boot=0/208 emit=1/7572 write=1/91 total=2/16373
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/0 plan=1/7061 lower=0/1797 boot=0/159 emit=1/7500 write=1/91 total=3/16608
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/4767 plan=1/7061 lower=0/1797 boot=0/159 emit=1/7500 write=0/91 total=2/21375
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/0 plan=0/7061 lower=0/1797 boot=0/159 emit=2/7500 write=0/91 total=2/16608
COMPILE-TRACE program=log_driver_fold_needs_no_id_column parse=0/4767 plan=1/7061 lower=0/1797 boot=0/159 emit=1/7500 write=0/91 total=2/21375
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=0/0 plan=1/6751 lower=0/1794 boot=0/208 emit=1/7572 write=1/91 total=3/16416
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=0/0 plan=1/7061 lower=0/1797 boot=0/159 emit=1/7500 write=1/91 total=3/16608
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=0/4767 plan=0/7061 lower=1/1797 boot=0/159 emit=1/7500 write=0/91 total=2/21375
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=0/0 plan=0/7061 lower=0/1797 boot=0/159 emit=2/7500 write=0/91 total=2/16608
COMPILE-TRACE program=identical_increments_stack_as_log_deltas parse=0/4767 plan=1/7061 lower=0/1797 boot=0/159 emit=1/7500 write=0/91 total=2/21375
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/0 plan=1/5944 lower=0/1525 boot=0/208 emit=1/9460 write=0/91 total=2/17228
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/0 plan=0/6259 lower=1/1531 boot=0/160 emit=1/9408 write=1/91 total=3/17449
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/4893 plan=1/6259 lower=0/1531 boot=0/160 emit=1/9408 write=1/91 total=3/22342
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/0 plan=0/6259 lower=1/1531 boot=0/160 emit=1/9408 write=6/91 total=8/17449
COMPILE-TRACE program=lww_fold_follows_arrival_order parse=0/4893 plan=0/6259 lower=1/1531 boot=0/160 emit=1/9408 write=4/91 total=6/22342
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=0/0 plan=0/7129 lower=1/1837 boot=0/208 emit=1/10054 write=0/91 total=2/19319
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=0/0 plan=1/7533 lower=0/1843 boot=0/160 emit=2/9996 write=0/91 total=3/19623
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=0/6140 plan=1/7533 lower=0/1843 boot=0/160 emit=1/9996 write=1/91 total=3/25763
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=0/0 plan=0/7533 lower=1/1843 boot=0/160 emit=1/9996 write=0/91 total=2/19623
COMPILE-TRACE program=concat_fold_follows_arrival_order parse=1/6140 plan=0/7533 lower=1/1843 boot=0/160 emit=1/9996 write=0/91 total=3/25763
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=0/0 plan=1/7129 lower=0/1837 boot=0/208 emit=1/10054 write=0/91 total=2/19319
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=0/0 plan=0/7533 lower=0/1843 boot=0/160 emit=1/9996 write=0/91 total=1/19623
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=0/6140 plan=1/7533 lower=0/1843 boot=0/160 emit=1/9996 write=1/91 total=3/25763
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=0/0 plan=1/7533 lower=0/1843 boot=0/160 emit=1/9996 write=1/91 total=3/19623
COMPILE-TRACE program=concat_fold_reversed_arrival_reverses_result parse=0/6140 plan=1/7533 lower=0/1843 boot=0/160 emit=1/9996 write=1/91 total=3/25763
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=0/0 plan=1/4756 lower=0/1414 boot=0/165 emit=1/8158 write=0/91 total=2/14584
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=0/0 plan=0/4769 lower=1/1423 boot=0/168 emit=1/8232 write=0/91 total=2/14683
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=0/4243 plan=1/4769 lower=0/1423 boot=0/168 emit=1/8232 write=0/91 total=2/18926
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=0/0 plan=1/4769 lower=0/1423 boot=0/168 emit=1/8232 write=0/91 total=2/14683
COMPILE-TRACE program=log_deltas_follow_arrival_order parse=0/4243 plan=1/4769 lower=0/1423 boot=0/168 emit=1/8232 write=0/91 total=2/18926
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/0 plan=0/4756 lower=0/1414 boot=0/165 emit=1/8158 write=0/91 total=1/14584
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/0 plan=0/4769 lower=1/1423 boot=0/168 emit=1/8232 write=0/91 total=2/14683
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/4243 plan=1/4769 lower=0/1423 boot=0/168 emit=1/8232 write=0/91 total=2/18926
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/0 plan=1/4769 lower=0/1423 boot=0/168 emit=1/8232 write=0/91 total=2/14683
COMPILE-TRACE program=shuffled_arrival_reorders_log_deltas parse=0/4243 plan=1/4769 lower=0/1423 boot=0/168 emit=1/8232 write=0/91 total=2/18926
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/0 plan=0/4695 lower=1/1414 boot=0/165 emit=1/8158 write=0/91 total=2/14523
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/0 plan=1/4769 lower=0/1423 boot=0/168 emit=1/8232 write=0/91 total=2/14683
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/4243 plan=1/4769 lower=0/1423 boot=0/168 emit=1/8232 write=0/91 total=2/18926
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/0 plan=1/4769 lower=0/1423 boot=0/168 emit=1/8232 write=0/91 total=2/14683
COMPILE-TRACE program=level_view_reads_set_projection_not_occurrences parse=0/4243 plan=0/4769 lower=1/1423 boot=0/168 emit=1/8232 write=0/91 total=2/18926
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=0/0 plan=0/7321 lower=1/1904 boot=0/188 emit=1/10423 write=0/91 total=2/19927
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=0/0 plan=1/7306 lower=0/1907 boot=0/189 emit=1/10457 write=1/91 total=3/19950
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=0/5128 plan=1/7306 lower=0/1907 boot=0/189 emit=1/10457 write=1/91 total=3/25078
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=0/0 plan=0/7306 lower=1/1907 boot=0/189 emit=1/10457 write=0/91 total=2/19950
COMPILE-TRACE program=demand_view_fires_its_consumer_once parse=1/5128 plan=0/7306 lower=1/1907 boot=0/189 emit=1/10457 write=0/91 total=3/25078
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=0/0 plan=0/4458 lower=1/961 boot=0/159 emit=0/5923 write=1/91 total=2/11592
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=0/0 plan=1/4408 lower=0/964 boot=0/160 emit=1/5956 write=0/91 total=2/11579
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=0/3078 plan=1/4408 lower=0/964 boot=0/160 emit=1/5956 write=0/91 total=2/14657
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=0/0 plan=0/4408 lower=0/964 boot=0/160 emit=1/5956 write=0/91 total=1/11579
COMPILE-TRACE program=log_stacks_within_tick_and_across_ticks parse=1/3078 plan=0/4408 lower=0/964 boot=0/160 emit=1/5956 write=0/91 total=2/14657
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=0/0 plan=0/4274 lower=1/982 boot=0/157 emit=0/5789 write=1/91 total=2/11293
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=0/0 plan=0/4273 lower=1/985 boot=0/158 emit=1/5822 write=0/91 total=2/11329
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=0/2875 plan=1/4273 lower=0/985 boot=0/158 emit=1/5822 write=0/91 total=2/14204
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=0/0 plan=0/4273 lower=0/985 boot=0/158 emit=1/5822 write=0/91 total=1/11329
COMPILE-TRACE program=set_rel_identical_arrival_is_one_occurrence parse=1/2875 plan=0/4273 lower=0/985 boot=0/158 emit=1/5822 write=0/91 total=2/14204
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/0 plan=0/4412 lower=0/961 boot=0/159 emit=1/5598 write=0/91 total=1/11221
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/0 plan=0/4408 lower=0/964 boot=0/160 emit=1/5631 write=1/91 total=2/11254
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/3004 plan=0/4408 lower=0/964 boot=0/160 emit=1/5631 write=0/91 total=1/14258
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/0 plan=1/4408 lower=0/964 boot=0/160 emit=1/5631 write=0/91 total=2/11254
COMPILE-TRACE program=log_rel_identical_arrival_is_two_occurrences parse=0/3004 plan=0/4408 lower=0/964 boot=0/160 emit=0/5631 write=1/91 total=1/14258
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=0/0 plan=1/7466 lower=0/1681 boot=0/178 emit=1/8725 write=1/91 total=3/18141
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=0/0 plan=1/7523 lower=0/1687 boot=0/180 emit=1/8791 write=0/91 total=2/18272
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=1/6069 plan=0/7523 lower=1/1687 boot=0/180 emit=1/8791 write=0/91 total=3/24341
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=0/0 plan=1/7523 lower=0/1687 boot=0/180 emit=1/8791 write=0/91 total=2/18272
COMPILE-TRACE program=any_two_tagged_arms_land_on_one_tick parse=1/6069 plan=0/7523 lower=0/1687 boot=1/180 emit=1/8791 write=0/91 total=3/24341
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=0/0 plan=0/7424 lower=0/1765 boot=0/177 emit=2/9020 write=0/91 total=2/18477
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=0/0 plan=1/7481 lower=0/1771 boot=0/179 emit=2/9086 write=0/91 total=3/18608
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=0/6085 plan=1/7481 lower=0/1771 boot=0/179 emit=2/9086 write=0/91 total=3/24693
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=0/0 plan=0/7481 lower=0/1771 boot=0/179 emit=1/9086 write=0/91 total=1/18608
COMPILE-TRACE program=one_attempt_keyed_head_loses_the_first_arm_silently parse=1/6085 plan=0/7481 lower=0/1771 boot=1/179 emit=1/9086 write=0/91 total=3/24693
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=0/0 plan=0/11900 lower=1/2543 boot=0/178 emit=1/10555 write=1/91 total=3/25267
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=0/0 plan=1/11957 lower=0/2549 boot=0/180 emit=1/10621 write=1/91 total=3/25398
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=0/10824 plan=1/11957 lower=1/2549 boot=0/180 emit=1/10621 write=0/91 total=3/36222
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=0/0 plan=1/11957 lower=0/2549 boot=0/180 emit=2/10621 write=0/91 total=3/25398
COMPILE-TRACE program=one_attempt_guard_by_negation_lands_one_unnamed_winner parse=1/10824 plan=1/11957 lower=0/2549 boot=0/180 emit=2/10621 write=0/91 total=4/36222
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=0/0 plan=1/11900 lower=0/2543 boot=0/178 emit=2/10555 write=0/91 total=3/25267
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=0/0 plan=1/11957 lower=0/2549 boot=0/180 emit=1/10621 write=1/91 total=3/25398
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=0/10824 plan=1/11957 lower=0/2549 boot=0/180 emit=1/10621 write=0/91 total=2/36222
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=0/0 plan=1/11957 lower=0/2549 boot=0/180 emit=2/10621 write=0/91 total=3/25398
COMPILE-TRACE program=one_attempt_guard_by_negation_arrival_order_beats_arm_order parse=1/10824 plan=0/11957 lower=1/2549 boot=0/180 emit=1/10621 write=0/91 total=3/36222
COMPILE-TRACE program=filter_map_is_a_level_rule parse=0/0 plan=1/6746 lower=0/2300 boot=0/263 emit=1/8437 write=1/91 total=3/17837
COMPILE-TRACE program=filter_map_is_a_level_rule parse=0/0 plan=1/6651 lower=0/2306 boot=0/165 emit=1/8283 write=0/91 total=2/17496
COMPILE-TRACE program=filter_map_is_a_level_rule parse=1/4908 plan=0/6651 lower=0/2306 boot=0/165 emit=1/8283 write=0/91 total=2/22404
COMPILE-TRACE program=filter_map_is_a_level_rule parse=0/0 plan=0/6651 lower=0/2306 boot=0/165 emit=1/8283 write=1/91 total=2/17496
COMPILE-TRACE program=filter_map_is_a_level_rule parse=0/4908 plan=0/6651 lower=1/2306 boot=0/165 emit=1/8283 write=0/91 total=2/22404
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=0/0 plan=1/8367 lower=0/2056 boot=0/159 emit=1/5417 write=0/91 total=2/16090
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=0/0 plan=0/8406 lower=1/2059 boot=0/160 emit=1/5450 write=0/91 total=2/16166
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=0/6101 plan=1/8406 lower=0/2059 boot=0/160 emit=1/5450 write=0/91 total=2/22267
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=0/0 plan=0/8406 lower=1/2059 boot=0/160 emit=1/5450 write=0/91 total=2/16166
COMPILE-TRACE program=repeat_is_a_self_carry_chain parse=0/6101 plan=1/8406 lower=0/2059 boot=0/160 emit=1/5450 write=0/91 total=2/22267
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=0/0 plan=1/6264 lower=0/2280 boot=0/184 emit=2/12071 write=0/91 total=3/20890
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=0/0 plan=0/6307 lower=0/2286 boot=0/186 emit=2/12139 write=0/91 total=2/21009
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=0/4656 plan=1/6307 lower=0/2286 boot=0/186 emit=2/12139 write=0/91 total=3/25665
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=0/0 plan=0/6307 lower=1/2286 boot=0/186 emit=1/12139 write=0/91 total=2/21009
COMPILE-TRACE program=fork_join_is_a_conjunctive_body parse=0/4656 plan=1/6307 lower=0/2286 boot=0/186 emit=2/12139 write=0/91 total=3/25665
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=0/0 plan=2/37800 lower=1/9018 boot=0/234 emit=3/23846 write=0/91 total=6/70989
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=0/0 plan=1/37670 lower=2/9030 boot=0/238 emit=2/23950 write=1/91 total=6/70979
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=1/17009 plan=2/37670 lower=1/9030 boot=0/238 emit=3/23950 write=0/91 total=7/87988
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=0/0 plan=2/37670 lower=1/9030 boot=0/238 emit=3/23950 write=1/91 total=7/70979
COMPILE-TRACE program=ordered_program_level_fold_reaches_three_links parse=1/17009 plan=2/37670 lower=1/9030 boot=0/238 emit=2/23950 write=1/91 total=7/87988
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=0/0 plan=1/10379 lower=0/4056 boot=0/165 emit=1/10246 write=1/91 total=3/24937
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=0/0 plan=1/10331 lower=1/4068 boot=0/169 emit=1/10338 write=0/91 total=3/24997
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=1/13481 plan=0/10331 lower=1/4068 boot=0/169 emit=1/10338 write=0/91 total=3/38478
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=0/0 plan=0/10331 lower=1/4068 boot=0/169 emit=1/10338 write=0/91 total=2/24997
COMPILE-TRACE program=unordered_program_level_fold_reaches_three_links parse=1/13481 plan=1/10331 lower=0/4068 boot=0/169 emit=1/10338 write=0/91 total=3/38478
COMPILE-TRACE program=switch_as_keyed_replace parse=0/0 plan=0/14798 lower=1/4313 boot=0/355 emit=3/25744 write=0/91 total=4/45301
COMPILE-TRACE program=switch_as_keyed_replace parse=0/0 plan=1/14793 lower=1/4325 boot=0/241 emit=3/25602 write=0/91 total=5/45052
COMPILE-TRACE program=switch_as_keyed_replace parse=1/13505 plan=1/14793 lower=1/4325 boot=0/241 emit=2/25602 write=1/91 total=6/58557
COMPILE-TRACE program=switch_as_keyed_replace parse=0/0 plan=1/14793 lower=0/4325 boot=0/241 emit=3/25602 write=0/91 total=4/45052
COMPILE-TRACE program=switch_as_keyed_replace parse=1/13505 plan=1/14793 lower=1/4325 boot=0/241 emit=3/25602 write=0/91 total=6/58557
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=0/0 plan=0/1719 lower=0/484 boot=0/135 emit=1/4973 write=0/91 total=1/7402
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=0/0 plan=0/1639 lower=1/490 boot=0/137 emit=0/5025 write=0/91 total=1/7382
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=1/968 plan=0/1639 lower=0/490 boot=0/137 emit=0/5025 write=1/91 total=2/8350
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=0/0 plan=1/1639 lower=0/490 boot=0/137 emit=0/5025 write=1/91 total=2/7382
COMPILE-TRACE program=stale_keyed_retraction_keeps_replacement parse=0/968 plan=0/1639 lower=0/490 boot=0/137 emit=1/5025 write=0/91 total=1/8350
COMPILE-TRACE program=merge_policy parse=0/0 plan=2/29138 lower=1/6889 boot=0/448 emit=4/39411 write=1/91 total=8/75977
COMPILE-TRACE program=merge_policy parse=0/0 plan=1/29151 lower=2/6907 boot=0/318 emit=4/39377 write=0/91 total=7/75844
COMPILE-TRACE program=merge_policy parse=2/22352 plan=1/29151 lower=2/6907 boot=0/318 emit=4/39377 write=0/91 total=9/98196
COMPILE-TRACE program=merge_policy parse=0/0 plan=2/29151 lower=1/6907 boot=0/318 emit=4/39377 write=1/91 total=8/75844
COMPILE-TRACE program=merge_policy parse=1/22352 plan=2/29151 lower=1/6907 boot=0/318 emit=4/39377 write=0/91 total=8/98196
COMPILE-TRACE program=exhaust_policy parse=0/0 plan=1/32969 lower=2/7320 boot=0/448 emit=4/39635 write=0/91 total=7/80463
COMPILE-TRACE program=exhaust_policy parse=0/0 plan=1/32804 lower=1/7338 boot=1/318 emit=4/39601 write=0/91 total=7/80152
COMPILE-TRACE program=exhaust_policy parse=2/24365 plan=1/32804 lower=2/7338 boot=0/318 emit=4/39601 write=0/91 total=9/104517
COMPILE-TRACE program=exhaust_policy parse=0/0 plan=2/32804 lower=1/7338 boot=0/318 emit=5/39601 write=0/91 total=8/80152
COMPILE-TRACE program=exhaust_policy parse=1/24365 plan=1/32804 lower=2/7338 boot=0/318 emit=4/39601 write=0/91 total=8/104517
COMPILE-TRACE program=concat_program_queue parse=0/0 plan=4/94348 lower=3/17031 boot=0/769 emit=7/68611 write=1/91 total=15/180850
COMPILE-TRACE program=concat_program_queue parse=0/0 plan=5/93349 lower=2/17049 boot=0/443 emit=8/68369 write=1/91 total=16/179301
COMPILE-TRACE program=concat_program_queue parse=5/51331 plan=4/93349 lower=3/17049 boot=0/443 emit=8/68369 write=0/91 total=20/230632
COMPILE-TRACE program=concat_program_queue parse=0/0 plan=4/93349 lower=3/17049 boot=0/443 emit=8/68369 write=0/91 total=15/179301
COMPILE-TRACE program=concat_program_queue parse=5/51331 plan=5/93349 lower=2/17049 boot=0/443 emit=8/68369 write=1/91 total=21/230632
COMPILE-TRACE program=completion_propagation_lattice_tick parse=0/0 plan=2/31547 lower=1/8342 boot=0/592 emit=4/37293 write=1/91 total=8/77865
COMPILE-TRACE program=completion_propagation_lattice_tick parse=0/0 plan=2/31214 lower=1/8360 boot=0/336 emit=4/37055 write=0/91 total=7/77056
COMPILE-TRACE program=completion_propagation_lattice_tick parse=2/22214 plan=2/31214 lower=1/8360 boot=0/336 emit=4/37055 write=1/91 total=10/99270
COMPILE-TRACE program=completion_propagation_lattice_tick parse=0/0 plan=1/31214 lower=2/8360 boot=0/336 emit=4/37055 write=0/91 total=7/77056
COMPILE-TRACE program=completion_propagation_lattice_tick parse=2/22214 plan=1/31214 lower=2/8360 boot=0/336 emit=4/37055 write=0/91 total=9/99270
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=0/0 plan=2/34745 lower=1/7828 boot=0/355 emit=4/32431 write=1/91 total=8/75450
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=0/0 plan=2/34660 lower=1/7837 boot=0/293 emit=4/32418 write=1/91 total=8/75299
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=1/20634 plan=2/34660 lower=1/7837 boot=0/293 emit=4/32418 write=1/91 total=9/95933
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=0/0 plan=2/34660 lower=1/7837 boot=0/293 emit=4/32418 write=1/91 total=8/75299
COMPILE-TRACE program=take_until_keyed_replace_negated_done parse=1/20634 plan=2/34660 lower=1/7837 boot=0/293 emit=4/32418 write=0/91 total=8/95933
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=0/0 plan=2/32954 lower=1/7828 boot=0/409 emit=4/32538 write=0/91 total=7/73820
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=0/0 plan=2/34660 lower=1/7837 boot=0/293 emit=4/32418 write=1/91 total=8/75299
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=1/20634 plan=2/34660 lower=1/7837 boot=0/293 emit=4/32418 write=1/91 total=9/95933
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=0/0 plan=2/34660 lower=1/7837 boot=0/293 emit=3/32418 write=1/91 total=7/75299
COMPILE-TRACE program=state_flap_nets_to_zero_scope_churn parse=1/20634 plan=2/34660 lower=1/7837 boot=0/293 emit=4/32418 write=1/91 total=9/95933
COMPILE-TRACE program=fill_as_cache_update_swr parse=0/0 plan=1/14519 lower=1/4253 boot=0/238 emit=3/24298 write=0/91 total=5/43399
COMPILE-TRACE program=fill_as_cache_update_swr parse=0/0 plan=1/14468 lower=1/4265 boot=0/242 emit=2/24418 write=1/91 total=5/43484
COMPILE-TRACE program=fill_as_cache_update_swr parse=1/12485 plan=0/14468 lower=1/4265 boot=0/242 emit=3/24418 write=0/91 total=5/55969
COMPILE-TRACE program=fill_as_cache_update_swr parse=0/0 plan=1/14468 lower=1/4265 boot=0/242 emit=2/24418 write=1/91 total=5/43484
COMPILE-TRACE program=fill_as_cache_update_swr parse=0/12485 plan=1/14468 lower=1/4265 boot=0/242 emit=3/24418 write=0/91 total=5/55969
COMPILE-TRACE program=demand_laziness_effect_rows parse=0/0 plan=0/7842 lower=1/2482 boot=0/193 emit=1/14824 write=1/91 total=3/25432
COMPILE-TRACE program=demand_laziness_effect_rows parse=0/0 plan=1/7600 lower=0/2488 boot=0/195 emit=2/14880 write=0/91 total=3/25254
COMPILE-TRACE program=demand_laziness_effect_rows parse=1/6123 plan=0/7600 lower=1/2488 boot=0/195 emit=1/14880 write=1/91 total=4/31377
COMPILE-TRACE program=demand_laziness_effect_rows parse=0/0 plan=0/7600 lower=1/2488 boot=0/195 emit=2/14880 write=0/91 total=3/25254
COMPILE-TRACE program=demand_laziness_effect_rows parse=0/6123 plan=1/7600 lower=0/2488 boot=0/195 emit=2/14880 write=0/91 total=3/31377
COMPILE-TRACE program=shared_demand_refcount parse=0/0 plan=1/7703 lower=0/2482 boot=0/193 emit=2/14824 write=0/91 total=3/25293
COMPILE-TRACE program=shared_demand_refcount parse=0/0 plan=0/7600 lower=1/2488 boot=0/195 emit=1/14880 write=1/91 total=3/25254
COMPILE-TRACE program=shared_demand_refcount parse=0/6123 plan=1/7600 lower=0/2488 boot=0/195 emit=2/14880 write=0/91 total=3/31377
COMPILE-TRACE program=shared_demand_refcount parse=0/0 plan=1/7600 lower=0/2488 boot=0/195 emit=2/14880 write=0/91 total=3/25254
COMPILE-TRACE program=shared_demand_refcount parse=1/6123 plan=0/7600 lower=1/2488 boot=0/195 emit=1/14880 write=1/91 total=4/31377
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=0/0 plan=1/15683 lower=1/5104 boot=0/265 emit=3/30889 write=0/91 total=5/52032
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=0/0 plan=0/15678 lower=1/5122 boot=0/271 emit=4/31075 write=0/91 total=5/52237
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=1/13314 plan=1/15678 lower=1/5122 boot=0/271 emit=3/31075 write=1/91 total=7/65551
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=0/0 plan=0/15678 lower=1/5122 boot=0/271 emit=3/31075 write=0/91 total=4/52237
COMPILE-TRACE program=zombie_scope_negative_case_a2b parse=1/13314 plan=1/15678 lower=1/5122 boot=0/271 emit=3/31075 write=0/91 total=6/65551
COMPILE-TRACE program=seq_wire_surface parse=0/0 plan=1/20505 lower=1/5057 boot=0/184 emit=2/14134 write=0/91 total=4/39971
COMPILE-TRACE program=seq_wire_surface parse=0/0 plan=1/20272 lower=0/5057 boot=0/184 emit=1/14134 write=1/91 total=3/39738
COMPILE-TRACE program=seq_wire_surface parse=0/4210 plan=1/20272 lower=1/5057 boot=0/184 emit=2/14134 write=0/91 total=4/43948
COMPILE-TRACE program=seq_wire_surface parse=0/0 plan=2/20272 lower=0/5057 boot=0/184 emit=2/14134 write=1/91 total=5/39738
COMPILE-TRACE program=seq_wire_surface parse=0/4210 plan=1/20272 lower=1/5057 boot=0/184 emit=2/14134 write=0/91 total=4/43948
COMPILE-TRACE program=seq_wire_hand parse=0/0 plan=1/20645 lower=1/5057 boot=0/184 emit=2/14134 write=1/91 total=5/40111
COMPILE-TRACE program=seq_wire_hand parse=0/0 plan=1/20412 lower=1/5057 boot=0/184 emit=2/14134 write=0/91 total=4/39878
COMPILE-TRACE program=seq_wire_hand parse=1/16593 plan=1/20412 lower=1/5057 boot=0/184 emit=2/14134 write=0/91 total=5/56471
COMPILE-TRACE program=seq_wire_hand parse=0/0 plan=2/20412 lower=0/5057 boot=0/184 emit=2/14134 write=1/91 total=5/39878
COMPILE-TRACE program=seq_wire_hand parse=1/16593 plan=1/20412 lower=1/5057 boot=0/184 emit=2/14134 write=0/91 total=5/56471
COMPILE-TRACE program=identical_demand_dedups parse=0/0 plan=1/12319 lower=0/2471 boot=0/204 emit=3/19252 write=0/91 total=4/34337
COMPILE-TRACE program=identical_demand_dedups parse=0/0 plan=1/12517 lower=0/2489 boot=0/210 emit=2/19414 write=1/91 total=4/34721
COMPILE-TRACE program=identical_demand_dedups parse=0/10545 plan=1/12517 lower=1/2489 boot=0/210 emit=2/19414 write=0/91 total=4/45266
COMPILE-TRACE program=identical_demand_dedups parse=0/0 plan=1/12517 lower=0/2489 boot=0/210 emit=2/19414 write=1/91 total=4/34721
COMPILE-TRACE program=identical_demand_dedups parse=0/10545 plan=1/12517 lower=1/2489 boot=0/210 emit=2/19414 write=0/91 total=4/45266
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=0/0 plan=0/5183 lower=0/1133 boot=0/158 emit=1/9535 write=0/91 total=1/16100
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=0/0 plan=1/5167 lower=0/1142 boot=0/161 emit=1/9610 write=1/91 total=3/16171
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=0/4921 plan=0/5167 lower=1/1142 boot=0/161 emit=1/9610 write=0/91 total=2/21092
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=0/0 plan=1/5167 lower=0/1142 boot=0/161 emit=1/9610 write=0/91 total=2/16171
COMPILE-TRACE program=new_salt_refires_fresh_stream parse=0/4921 plan=1/5167 lower=0/1142 boot=0/161 emit=1/9610 write=0/91 total=2/21092
COMPILE-TRACE program=terminal_is_terminal parse=0/0 plan=1/10496 lower=0/2992 boot=0/190 emit=2/14887 write=0/91 total=3/28656
COMPILE-TRACE program=terminal_is_terminal parse=0/0 plan=1/10633 lower=0/3007 boot=0/195 emit=2/15020 write=0/91 total=3/28946
COMPILE-TRACE program=terminal_is_terminal parse=0/9360 plan=1/10633 lower=1/3007 boot=0/195 emit=1/15020 write=1/91 total=4/38306
COMPILE-TRACE program=terminal_is_terminal parse=0/0 plan=1/10633 lower=0/3007 boot=0/195 emit=2/15020 write=0/91 total=3/28946
COMPILE-TRACE program=terminal_is_terminal parse=0/9360 plan=1/10633 lower=1/3007 boot=0/195 emit=1/15020 write=0/91 total=3/38306
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/0 plan=0/2184 lower=1/606 boot=0/159 emit=1/8308 write=0/91 total=2/11348
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/0 plan=0/2310 lower=1/621 boot=0/164 emit=1/8436 write=0/91 total=2/11622
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/1757 plan=0/2310 lower=0/621 boot=0/164 emit=1/8436 write=0/91 total=1/13379
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/0 plan=0/2310 lower=1/621 boot=0/164 emit=0/8436 write=1/91 total=2/11622
COMPILE-TRACE program=live_nonzero_exit_keeps_rows parse=0/1757 plan=0/2310 lower=0/621 boot=0/164 emit=1/8436 write=0/91 total=1/13379
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=0/0 plan=1/8551 lower=0/2195 boot=0/187 emit=2/15083 write=1/91 total=4/26107
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=0/0 plan=1/8508 lower=0/2204 boot=0/190 emit=2/15161 write=0/91 total=3/26154
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=1/8182 plan=0/8508 lower=1/2204 boot=0/190 emit=1/15161 write=0/91 total=3/34336
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=0/0 plan=1/8508 lower=0/2204 boot=0/190 emit=2/15161 write=0/91 total=3/26154
COMPILE-TRACE program=worktree_edit_replaces_digest_and_flips_kind_view parse=0/8182 plan=1/8508 lower=0/2204 boot=0/190 emit=2/15161 write=1/91 total=4/34336
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=0/0 plan=1/5381 lower=0/1183 boot=0/158 emit=1/11071 write=1/91 total=3/17884
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=0/0 plan=1/5438 lower=0/1192 boot=0/161 emit=1/11146 write=1/91 total=3/18028
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=0/5380 plan=0/5438 lower=1/1192 boot=0/161 emit=1/11146 write=0/91 total=2/23408
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=0/0 plan=0/5438 lower=1/1192 boot=0/161 emit=1/11146 write=0/91 total=2/18028
COMPILE-TRACE program=worktree_edit_identical_resave_is_silent parse=1/5380 plan=0/5438 lower=0/1192 boot=0/161 emit=2/11146 write=0/91 total=3/23408
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=0/0 plan=1/14892 lower=1/5117 boot=0/419 emit=3/20839 write=0/91 total=5/41358
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=0/0 plan=1/15183 lower=1/5138 boot=0/238 emit=2/20660 write=1/91 total=5/41310
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=0/14254 plan=0/15183 lower=1/5138 boot=0/238 emit=2/20660 write=1/91 total=4/55564
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=0/0 plan=0/15183 lower=1/5138 boot=0/238 emit=3/20660 write=0/91 total=4/41310
COMPILE-TRACE program=dirty_derives_from_digest_mismatch parse=1/14254 plan=1/15183 lower=0/5138 boot=0/238 emit=3/20660 write=0/91 total=5/55564
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=0/0 plan=1/22885 lower=1/6507 boot=0/483 emit=3/26521 write=1/91 total=6/56487
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=0/0 plan=2/24127 lower=1/6528 boot=0/284 emit=3/26368 write=0/91 total=6/57398
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=1/21899 plan=2/24127 lower=1/6528 boot=0/284 emit=3/26368 write=0/91 total=7/79297
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=0/0 plan=2/24127 lower=1/6528 boot=0/284 emit=3/26368 write=0/91 total=6/57398
COMPILE-TRACE program=dirty_retracts_on_matching_commit parse=2/21899 plan=1/24127 lower=1/6528 boot=0/284 emit=3/26368 write=1/91 total=8/79297
COMPILE-TRACE program=head_move_replaces_key parse=0/0 plan=0/4750 lower=0/1087 boot=0/208 emit=1/4977 write=0/91 total=1/11113
COMPILE-TRACE program=head_move_replaces_key parse=0/0 plan=0/4921 lower=0/1093 boot=0/160 emit=1/4935 write=0/91 total=1/11200
COMPILE-TRACE program=head_move_replaces_key parse=0/4264 plan=1/4921 lower=0/1093 boot=0/160 emit=1/4935 write=0/91 total=2/15464
COMPILE-TRACE program=head_move_replaces_key parse=0/0 plan=0/4921 lower=0/1093 boot=0/160 emit=1/4935 write=0/91 total=1/11200
COMPILE-TRACE program=head_move_replaces_key parse=1/4264 plan=0/4921 lower=0/1093 boot=0/160 emit=1/4935 write=0/91 total=2/15464
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=0/0 plan=0/11209 lower=1/3247 boot=0/600 emit=2/15956 write=0/91 total=3/31103
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=0/0 plan=1/11014 lower=0/3262 boot=0/214 emit=2/15303 write=0/91 total=3/29884
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=1/10522 plan=1/11014 lower=0/3262 boot=0/214 emit=2/15303 write=0/91 total=4/40406
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=0/0 plan=0/11014 lower=1/3262 boot=0/214 emit=2/15303 write=0/91 total=3/29884
COMPILE-TRACE program=head_move_flips_current_tree_in_one_tick parse=1/10522 plan=0/11014 lower=1/3262 boot=0/214 emit=2/15303 write=0/91 total=4/40406
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=0/0 plan=1/12857 lower=0/3131 boot=0/253 emit=2/17972 write=1/91 total=4/34304
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=0/0 plan=0/12977 lower=0/3149 boot=0/214 emit=2/18037 write=0/91 total=2/34468
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=0/14222 plan=1/12977 lower=1/3149 boot=0/214 emit=2/18037 write=0/91 total=4/48690
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=0/0 plan=1/12977 lower=0/3149 boot=0/214 emit=2/18037 write=0/91 total=3/34468
COMPILE-TRACE program=pin_to_unknown_repo_derives_repo_candidate parse=1/14222 plan=1/12977 lower=1/3149 boot=0/214 emit=2/18037 write=0/91 total=5/48690
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=0/0 plan=0/11973 lower=1/2460 boot=0/331 emit=2/17863 write=0/91 total=3/32718
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=0/0 plan=1/12183 lower=1/2484 boot=0/232 emit=2/17904 write=0/91 total=4/32894
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=1/12535 plan=1/12183 lower=0/2484 boot=0/232 emit=2/17904 write=1/91 total=5/45429
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=0/0 plan=1/12183 lower=0/2484 boot=0/232 emit=2/17904 write=1/91 total=4/32894
COMPILE-TRACE program=xref_rev_is_pin_data_not_live_head parse=0/12535 plan=1/12183 lower=1/2484 boot=0/232 emit=2/17904 write=0/91 total=4/45429
COMPILE-TRACE program=changed_since_spans_two_turns parse=0/0 plan=1/19675 lower=1/4772 boot=0/234 emit=2/20965 write=1/91 total=5/45737
COMPILE-TRACE program=changed_since_spans_two_turns parse=0/0 plan=1/19434 lower=1/4781 boot=0/237 emit=2/21061 write=0/91 total=4/45604
COMPILE-TRACE program=changed_since_spans_two_turns parse=1/14896 plan=1/19434 lower=1/4781 boot=0/237 emit=2/21061 write=1/91 total=6/60500
COMPILE-TRACE program=changed_since_spans_two_turns parse=0/0 plan=1/19434 lower=1/4781 boot=0/237 emit=2/21061 write=0/91 total=4/45604
COMPILE-TRACE program=changed_since_spans_two_turns parse=1/14896 plan=1/19434 lower=1/4781 boot=0/237 emit=2/21061 write=1/91 total=6/60500
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=0/0 plan=1/19463 lower=1/4772 boot=0/304 emit=2/21099 write=0/91 total=4/45729
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=0/0 plan=1/19434 lower=0/4781 boot=0/237 emit=3/21061 write=0/91 total=4/45604
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=1/14896 plan=1/19434 lower=1/4781 boot=0/237 emit=2/21061 write=1/91 total=6/60500
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=0/0 plan=1/19434 lower=0/4781 boot=1/237 emit=2/21061 write=0/91 total=4/45604
COMPILE-TRACE program=changed_since_ignores_events_before_turn parse=1/14896 plan=1/19434 lower=1/4781 boot=0/237 emit=2/21061 write=0/91 total=5/60500
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=0/0 plan=2/19066 lower=0/4145 boot=0/232 emit=3/20046 write=0/91 total=5/43580
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=0/0 plan=1/19274 lower=0/4166 boot=0/239 emit=3/20233 write=0/91 total=4/44003
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=1/17046 plan=1/19274 lower=1/4166 boot=0/239 emit=2/20233 write=0/91 total=5/61049
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=0/0 plan=1/19274 lower=1/4166 boot=0/239 emit=2/20233 write=1/91 total=5/44003
COMPILE-TRACE program=two_pins_dedup_to_one_demand_row parse=1/17046 plan=1/19274 lower=1/4166 boot=0/239 emit=2/20233 write=0/91 total=5/61049
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=0/0 plan=1/18944 lower=1/4145 boot=0/232 emit=2/20046 write=0/91 total=4/43458
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=0/0 plan=1/19274 lower=1/4166 boot=0/239 emit=2/20233 write=0/91 total=4/44003
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=1/17046 plan=1/19274 lower=0/4166 boot=0/239 emit=3/20233 write=0/91 total=5/61049
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=0/0 plan=1/19274 lower=1/4166 boot=0/239 emit=2/20233 write=1/91 total=5/44003
COMPILE-TRACE program=rev_fill_not_behind_keeps_stale_pin_empty parse=1/17046 plan=1/19274 lower=0/4166 boot=0/239 emit=2/20233 write=0/91 total=4/61049
COMPILE-TRACE program=clean_state_no_diags parse=0/0 plan=4/81620 lower=3/19921 boot=0/1674 emit=7/67957 write=0/91 total=14/171263
COMPILE-TRACE program=clean_state_no_diags parse=0/0 plan=4/80644 lower=3/19954 boot=0/430 emit=7/66307 write=0/91 total=14/167426
COMPILE-TRACE program=clean_state_no_diags parse=6/52309 plan=4/80644 lower=3/19954 boot=0/430 emit=7/66307 write=1/91 total=21/219735
COMPILE-TRACE program=clean_state_no_diags parse=0/0 plan=4/80644 lower=3/19954 boot=0/430 emit=7/66307 write=1/91 total=15/167426
COMPILE-TRACE program=clean_state_no_diags parse=6/52309 plan=4/80644 lower=3/19954 boot=0/430 emit=7/66307 write=1/91 total=21/219735
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=0/0 plan=8/158761 lower=4/35724 boot=1/3267 emit=11/103194 write=0/91 total=24/301037
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=0/0 plan=8/156578 lower=5/35778 boot=0/641 emit=11/100684 write=1/91 total=25/293772
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=10/84321 plan=8/156578 lower=5/35778 boot=0/641 emit=11/100684 write=1/91 total=35/378093
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=0/0 plan=8/156578 lower=5/35778 boot=0/641 emit=10/100684 write=1/91 total=24/293772
COMPILE-TRACE program=clean_state_gate_and_exit_zero parse=10/84321 plan=8/156578 lower=5/35778 boot=0/641 emit=10/100684 write=1/91 total=34/378093
COMPILE-TRACE program=waiver_range_join_exact_rows parse=0/0 plan=2/28196 lower=1/8352 boot=0/749 emit=4/34681 write=1/91 total=8/72069
COMPILE-TRACE program=waiver_range_join_exact_rows parse=0/0 plan=2/27943 lower=1/8364 boot=0/298 emit=4/33880 write=0/91 total=7/70576
COMPILE-TRACE program=waiver_range_join_exact_rows parse=1/22038 plan=2/27943 lower=1/8364 boot=0/298 emit=4/33880 write=1/91 total=9/92614
COMPILE-TRACE program=waiver_range_join_exact_rows parse=0/0 plan=2/27943 lower=1/8364 boot=0/298 emit=4/33880 write=0/91 total=7/70576
COMPILE-TRACE program=waiver_range_join_exact_rows parse=2/22038 plan=1/27943 lower=2/8364 boot=0/298 emit=3/33880 write=1/91 total=9/92614
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=0/0 plan=3/52942 lower=2/13467 boot=0/1198 emit=5/53678 write=1/91 total=11/121376
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=0/0 plan=3/52127 lower=2/13485 boot=0/352 emit=5/52263 write=1/91 total=11/118318
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=3/35802 plan=3/52127 lower=2/13485 boot=0/352 emit=5/52263 write=1/91 total=14/154120
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=0/0 plan=2/52127 lower=3/13485 boot=0/352 emit=5/52263 write=0/91 total=10/118318
COMPILE-TRACE program=over_baseline_diag_exact_rows parse=4/35802 plan=2/52127 lower=3/13485 boot=0/352 emit=5/52263 write=0/91 total=14/154120
COMPILE-TRACE program=over_baseline_count_row parse=0/0 plan=1/28284 lower=2/8352 boot=0/814 emit=3/34813 write=1/91 total=7/72354
COMPILE-TRACE program=over_baseline_count_row parse=0/0 plan=2/27943 lower=1/8364 boot=0/298 emit=3/33880 write=1/91 total=7/70576
COMPILE-TRACE program=over_baseline_count_row parse=1/22038 plan=2/27943 lower=1/8364 boot=0/298 emit=4/33880 write=0/91 total=8/92614
COMPILE-TRACE program=over_baseline_count_row parse=0/0 plan=1/27943 lower=2/8364 boot=0/298 emit=3/33880 write=0/91 total=6/70576
COMPILE-TRACE program=over_baseline_count_row parse=2/22038 plan=1/27943 lower=1/8364 boot=0/298 emit=4/33880 write=0/91 total=8/92614
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=0/0 plan=6/121223 lower=4/28418 boot=0/2344 emit=9/88492 write=1/91 total=20/240568
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=0/0 plan=6/119799 lower=4/28457 boot=0/563 emit=9/86559 write=0/91 total=19/235469
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=8/67729 plan=5/119799 lower=4/28457 boot=0/563 emit=9/86559 write=1/91 total=27/303198
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=0/0 plan=6/119799 lower=4/28457 boot=0/563 emit=9/86559 write=0/91 total=19/235469
COMPILE-TRACE program=over_baseline_gate_blocks_commit_only parse=7/67729 plan=5/119799 lower=4/28457 boot=0/563 emit=9/86559 write=1/91 total=26/303198
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=0/0 plan=6/121960 lower=4/28417 boot=0/2629 emit=8/87895 write=1/91 total=19/240992
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=0/0 plan=6/120326 lower=4/28462 boot=0/565 emit=8/85623 write=1/91 total=19/235067
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=7/68374 plan=6/120326 lower=4/28462 boot=0/565 emit=9/85623 write=0/91 total=26/303441
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=0/0 plan=6/120326 lower=4/28462 boot=0/565 emit=8/85623 write=1/91 total=19/235067
COMPILE-TRACE program=fix_by_waiver_returns_to_clean parse=7/68374 plan=6/120326 lower=4/28462 boot=0/565 emit=8/85623 write=1/91 total=26/303441
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=0/0 plan=3/53057 lower=2/13467 boot=0/1269 emit=5/53804 write=1/91 total=11/121688
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=0/0 plan=2/52127 lower=2/13485 boot=0/352 emit=6/52263 write=0/91 total=10/118318
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=4/35802 plan=2/52127 lower=3/13485 boot=0/352 emit=5/52263 write=0/91 total=14/154120
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=0/0 plan=2/52127 lower=3/13485 boot=0/352 emit=5/52263 write=0/91 total=10/118318
COMPILE-TRACE program=new_file_diag_at_hit_line_exact_rows parse=3/35802 plan=3/52127 lower=3/13485 boot=0/352 emit=5/52263 write=1/91 total=15/154120
COMPILE-TRACE program=new_file_no_exceeded_diag parse=0/0 plan=3/53057 lower=2/13467 boot=0/1269 emit=6/53804 write=0/91 total=11/121688
COMPILE-TRACE program=new_file_no_exceeded_diag parse=0/0 plan=2/52127 lower=3/13485 boot=0/352 emit=5/52263 write=1/91 total=11/118318
COMPILE-TRACE program=new_file_no_exceeded_diag parse=3/35802 plan=2/52127 lower=2/13485 boot=0/352 emit=6/52263 write=0/91 total=13/154120
COMPILE-TRACE program=new_file_no_exceeded_diag parse=0/0 plan=3/52127 lower=2/13485 boot=0/352 emit=5/52263 write=1/91 total=11/118318
COMPILE-TRACE program=new_file_no_exceeded_diag parse=3/35802 plan=3/52127 lower=3/13485 boot=0/352 emit=5/52263 write=0/91 total=14/154120
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=0/0 plan=1/19590 lower=1/6403 boot=0/1194 emit=3/24653 write=0/91 total=5/51931
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=0/0 plan=1/18543 lower=1/6418 boot=0/218 emit=2/22869 write=0/91 total=4/48139
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=1/16825 plan=1/18543 lower=1/6418 boot=0/218 emit=2/22869 write=0/91 total=5/64964
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=0/0 plan=1/18543 lower=1/6418 boot=0/218 emit=2/22869 write=0/91 total=4/48139
COMPILE-TRACE program=unwrap_aggregate_and_interpolation parse=1/16825 plan=1/18543 lower=1/6418 boot=0/218 emit=3/22869 write=0/91 total=6/64964
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=0/0 plan=1/20982 lower=1/6403 boot=1/2130 emit=2/26417 write=1/91 total=6/56023
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=0/0 plan=1/18543 lower=1/6418 boot=0/218 emit=3/22869 write=0/91 total=5/48139
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=1/16825 plan=1/18543 lower=1/6418 boot=0/218 emit=3/22869 write=0/91 total=6/64964
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=0/0 plan=1/18543 lower=1/6418 boot=0/218 emit=3/22869 write=0/91 total=5/48139
COMPILE-TRACE program=unwrap_unchanged_file_silent parse=1/16825 plan=1/18543 lower=1/6418 boot=0/218 emit=2/22869 write=1/91 total=6/64964
COMPILE-TRACE program=unwrap_below_budget_silent parse=0/0 plan=1/18546 lower=2/6403 boot=0/492 emit=2/23294 write=1/91 total=6/48826
COMPILE-TRACE program=unwrap_below_budget_silent parse=0/0 plan=1/18543 lower=1/6418 boot=0/218 emit=3/22869 write=0/91 total=5/48139
COMPILE-TRACE program=unwrap_below_budget_silent parse=1/16825 plan=1/18543 lower=1/6418 boot=0/218 emit=3/22869 write=0/91 total=6/64964
COMPILE-TRACE program=unwrap_below_budget_silent parse=0/0 plan=1/18543 lower=1/6418 boot=0/218 emit=2/22869 write=1/91 total=5/48139
COMPILE-TRACE program=unwrap_below_budget_silent parse=1/16825 plan=1/18543 lower=1/6418 boot=0/218 emit=2/22869 write=1/91 total=6/64964
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=0/0 plan=4/76443 lower=3/18463 boot=0/1376 emit=6/65539 write=1/91 total=14/161912
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=0/0 plan=4/75630 lower=3/18484 boot=0/434 emit=7/64217 write=0/91 total=14/158856
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=4/47702 plan=4/75630 lower=3/18484 boot=0/434 emit=7/64217 write=0/91 total=18/206558
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=0/0 plan=4/75630 lower=2/18484 boot=0/434 emit=7/64217 write=0/91 total=13/158856
COMPILE-TRACE program=tightened_baseline_catches_regrowth parse=5/47702 plan=4/75630 lower=3/18484 boot=0/434 emit=7/64217 write=0/91 total=19/206558
TEXT_DOOR compiled=420 byte_identical=420 failures=0

```

### 4. prolog-lint (tools/prolog-lint.sh)

```
── advisory: unused-export candidates and counts ──
files_cross_referenced	111
unused_export_candidate	0_ast_expand.pl-expand_ast_in_context/3
unused_export_candidate	0_body_walk.pl-event_relation_atom/2
unused_export_candidate	0_coalesce_expand.pl-expand_coalesce_in_context/3
unused_export_candidate	0_dot_expand.pl-expand_dot_in_context/3
unused_export_candidate	0_enum_expand.pl-expand_enum_in_context/3
unused_export_candidate	0_match_expand.pl-expand_match_program_in_context/3
unused_export_candidate	0_program_check.pl-program_violation/3
unused_export_candidate	0_refusal_messages.pl-refusal_inventory_forget/0
unused_export_candidate	0_refusal_messages.pl-refusal_renderer_counts/2
unused_export_candidate	0_relation_edge_expand.pl-expand_relation_edges_in_context/3
unused_export_candidate	0_seq_expand.pl-expand_seq_in_context/3
unused_export_candidate	0_type_plane.pl-json_object_value/2
unused_export_candidate	0_type_plane.pl-type_ref_columns/3
unused_export_candidate	0_type_plane.pl-type_shape_error/4
unused_export_candidate	6_profile.pl-compile_dl6_profiled/2
unused_export_candidate	6_profile.pl-execution_profile_dl6/2
unused_export_candidate	analyze.pl-arrival_target_refs/2
unused_export_candidate	analyze.pl-decl_keep/3
unused_export_candidate	analyze.pl-rel_column_types/5
unused_export_candidate	analyze.pl-rel_column_types/7
unused_export_candidate	analyze.pl-rel_columns/4
unused_export_candidate	analyze.pl-snake_name/2
unused_export_candidate	compile.pl-compile_fixture/3
unused_export_candidate	compile.pl-compile_fixture/4
unused_export_candidate	compile/1_emit_registry_docs.pl-emit_dl6_grammar/0
unused_export_candidate	compile/1_emit_registry_docs.pl-emit_registry_docs/0
unused_export_candidate	compile/2_emit_cli_inventory.pl-cli_inventory_text/1
unused_export_candidate	compile/2_emit_cli_inventory.pl-emit_cli_inventory/0
unused_export_candidate	compile/3_emit_trace_schema.pl-emit_trace_schema/0
unused_export_candidate	compile/3_emit_trace_schema.pl-trace_schema_text/1
unused_export_candidate	compile/parse_dl.pl-statement_location_for_reference/4
unused_export_candidate	compile/registry.pl-host_executor/2
unused_export_candidate	compile/scripts/bop_check.pl-bop_check/1
unused_export_candidate	compile/scripts/bop_check.pl-bop_check_env/0
unused_export_candidate	compile/scripts/text_door_receipt.pl-run/0
unused_export_candidate	compile/test/run_sql_check.pl-check/1
unused_export_candidate	compile/test/run_sql_check.pl-check_all/0
unused_export_candidate	conformance/engine.pl-fixture_expectations_hold/2
unused_export_candidate	conformance/engine.pl-run_fixture_checks/2
unused_export_candidate	diag.pl-diag_stream_open/0
unused_export_candidate	diag.pl-dl6_span/6
unused_export_candidate	diag.pl-emit_diag/2
unused_export_candidate	diag.pl-emit_diag_term/1
unused_export_candidate	diag.pl-set_diag_file/1
unused_export_candidate	labs/generic_scan_instantiation/0_receipts.pl-go/0
unused_export_candidate	labs/generic_scan_instantiation/0_receipts.pl-scan_plan_fact/3
unused_export_candidate	labs/generic_scan_instantiation/0_receipts.pl-scan_refusal/2
unused_export_candidate	labs/generic_scan_instantiation/0_receipts.pl-scan_spec/6
unused_export_candidate	labs/generic_scan_instantiation/0_receipts.pl-specialize_scan/3
unused_export_candidate	labs/json_interop/0_receipts.pl-go/0
unused_export_candidate	labs/json_syntax/0_receipts.pl-go/0
unused_export_candidate	labs/json_syntax/1_grammar.pl-example/4
unused_export_candidate	labs/json_syntax/1_grammar.pl-parse_literal/3
unused_export_candidate	labs/json_syntax/1_grammar.pl-pattern_is_literal/1
unused_export_candidate	labs/json_syntax/1_grammar.pl-pattern_to_literal/2
unused_export_candidate	labs/json_syntax/2_lowering.pl-pattern_sql/4
unused_export_candidate	labs/json_syntax/3_lists.pl-grade/4
unused_export_candidate	labs/json_syntax/3_lists.pl-proto_column_storage/3
unused_export_candidate	labs/json_syntax/4_cards.pl-card_answered/4
unused_export_candidate	labs/json_syntax/4_cards.pl-card_open/3
unused_export_candidate	labs/json_syntax/4_cards.pl-directive/2
unused_export_candidate	labs/json_syntax/4_cards.pl-spelling/4
unused_export_candidate	labs/json_syntax/4_cards.pl-spelling_free/2
unused_export_candidate	labs/openapi_codegen/emit_openapi.pl-emit_openapi/0
unused_export_candidate	labs/openapi_codegen/emit_openapi.pl-openapi_document/1
unused_export_candidate	labs/openapi_codegen/emit_openapi.pl-openapi_json_text/1
unused_export_candidate	labs/openapi_codegen/emit_openapi.pl-spec_operations/1
unused_export_candidate	labs/rel_as_stream/0_receipts.pl-go/0
unused_export_candidate	lower.pl-dictionary_render_expr/3
unused_export_candidate	lower.pl-dictionary_table_name/2
unused_export_candidate	print_dl.pl-print_dl_program_with_edb_types/7
unused_export_candidate	print_dl.pl-print_dl_to_file/3
unused_export_candidate	src/checks.pl-covers_enum/2
unused_export_candidate	src/checks.pl-has_subscribe_arm/1
unused_export_candidate	src/checks.pl-no_self_union/1
unused_export_candidate	src/checks.pl-no_twin_names/1
unused_export_candidate	src/checks.pl-surface_grounded/0
unused_export_candidate	src/emit_ts.pl-decl_ts/2
unused_export_candidate	src/emit_ts.pl-emit/3
unused_export_candidate	src/emit_ts.pl-go/0
unused_export_candidate	src/emit_ts.pl-rule_ts/3
unused_export_candidate	src/emit_ts.pl-used_helpers/2
unused_export_candidate	src/kernel.pl-grounds/1
unused_export_candidate	sweep.pl-sweep/0
unused_export_candidate	tools/prolog_lint.pl-lint_loaded/1
unused_export_candidate	tools/prolog_lint.pl-lint_sources/0
unused_export_candidate	tools/self_map_facts.pl-emit_for/1
unused_export_candidate	tools/self_map_facts.pl-main/0

PROLOG_LINT findings=1 baseline=1 OK

```

### 5. tsv2 (pnpm test)

```
$ node --test --experimental-transform-types --test-concurrency=6
(node:72848) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ arrival delta carries the text value returned by SQLite (13.494042ms)
(node:72850) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ file-watch scale: duplicate events, edits, identical saves, and deletes have exact receipts (206.075375ms)
(node:72851) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ callgraph, diagnostics, and flow obey one extractor process per path and digest (10.685291ms)
✔ extractor batching is frontier-local, digest-separated, and ignores demand retractions (1.981041ms)
✔ generic shell demands remain one process per witness (0.987625ms)
✔ a cached projection is omitted while its unanswered sibling still runs (1.002ms)
(node:72852) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ bool and float storage rejects invalid values and decodes canonical values (10.545666ms)
✔ bool filters use an indexed SQLite search (0.467166ms)
✔ tick boundary emits booleans and shortest finite float JSON (0.153583ms)
(node:72853) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ ordered aggregate maintenance statement count is flat in group count (42.628792ms)
✔ ordered aggregate scoped INSERT uses SEARCH on the source group key (0.81625ms)
(node:72854) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ aggregate scope seed selects only the groups this tick touched (32.629583ms)
✔ scoped delete and recompute SEARCH by group key, never SCAN (21.230583ms)
✔ scoped delete and recompute touch one group out of 5000, and min moves (21.443708ms)
(node:72862) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ driver_binds_a_js_number_as_real (the hazard, measured not assumed) (2.518292ms)
✔ boot_runner_preserves_an_integer_into_a_text_column (2.119666ms)
✔ boot_runner_leaves_an_integer_column_an_integer (1.218417ms)
(node:72863) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ check: a program with zero findings that compiles clean exits 0, silently (408.813041ms)
✔ check: a program that hits a named compiler refusal exits 2 and names it on stderr (807.612541ms)
✔ check: a file that does not exist exits 1, broken (311.849958ms)
✔ check: a located refusal names file and line, the same location compile_dl6.sh prints (725.641667ms)
✔ check: a file that does not parse at all exits 1, broken (365.125709ms)
(node:72864) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ registry.pl cli_command/3 and cli/bop.ts's commander verbs name the same set (16.146459ms)
✔ generated CLI and HTTP inventory is current with canonical Prolog facts (15.693791ms)
(node:72865) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ load then q: a program POSTed by `bop load` is readable by `bop q` on the same running server (922.78ms)
✔ q: nothing listening on the port exits 1 with a clear message, never a stack trace (221.482ms)
✔ load: nothing listening on the port exits 1 with a clear message (223.792542ms)
✔ q: a running server with no program loaded exits 1 (404 'no program loaded') (217.146458ms)
✔ load: a program that hits a named compiler refusal over http exits 2, not 1 (780.353834ms)
✔ stats: bop reads the existing GET /stats route after load (634.351667ms)
(node:72866) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ run: a program with no binds/hosts quiesces at zero ticks and exits 0 (767.820166ms)
✔ run --ticks 1: a live interval bind produces exactly one tick line, then a clean exit (1405.124167ms)
(node:72887) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ catalog rows land in the program database (2.419ms)
✔ a column is a child row of its rel (8.492833ms)
✔ replaying the DDL mints no duplicate rows (0.928417ms)
✔ the parent index is used, never a scan (0.357209ms)
(node:72907) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ coalesce statements per tick are flat in the source rel's size (38.483375ms)
✔ the coalesce default arm SEARCHes the source rel, never SCANs it (6.809625ms)
(node:72908) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ the org crawl is ONE program: repos on a clock -> repo_files_at -> repo_extract, graded against the oracle (1746.255833ms)
(node:72921) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ count: a program with no finalize emits no departure table anywhere (1.357875ms)
✔ count: staging is one clear, plus one insert only when something departed (3.142ms)
✔ plan: the departure arm reads only its own departure table (0.78975ms)
✔ endurance: a staged departure is exactly as durable as a staged addition (2.793167ms)
✔ count: a departure fires exactly one tick after the row left (6.625917ms)
(node:72934) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ multisetDiff: plain set-style add and del (0.714417ms)
✔ multisetDiff: unchanged rows produce no delta (0.081709ms)
✔ multisetDiff: duplicate row values are counted, not deduped (Log append) (0.072917ms)
(node:72998) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ edge-body negation SEARCHes the negated rel by key, never SCANs it (11.299167ms)
✔ edge-body negation admits exactly the arrivals its guard lets through (12.576958ms)
✔ edge-body negation plan does not change shape as the negated rel grows (8.455666ms)
✔ edge-body comparison and bind filter and compute inside the arm (1.439209ms)
(node:73028) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ a submit with no ticks$ reader still ticks, and a late reader sees the state it left (14.779084ms)
✔ tick numbering does not restart when the readers leave and come back (11.021541ms)
(node:73105) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ a tick fault answers its own submitter and the lane keeps turning (147.701625ms)
✔ the app graph survives a tick fault (7.704625ms)
✔ a program swap after a tick fault still loads and ticks (239.465417ms)
(node:73107) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ a schedule ending on a retraction tick mints no extra empty drain tick (11.249792ms)
✔ the full schedule still matches the checked-in oracle log line for line (8.053333ms)
(node:73143) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ golden-flex served: the live host runs, and the served tick log matches the oracle replayed on the served schedule (1068.67625ms)
(node:73187) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ sh host: a JSON object stream projects by NAME, and a record missing a declared column is no row (291.612959ms)
✔ sh host: a grid answer is one row per line at every cardinality, 0 through 3 (168.697458ms)
✔ sh host: one value per line still wins when the lines are not a grid (142.595916ms)
✔ compiler-known extract host uses the registered process executor (3.000291ms)
(node:73317) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ a {col} splice cannot execute anything, in any quoting context (333.049542ms)
(node:73320) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ count: a drain tick pays nothing for the mid-tick level freeze (2.864792ms)
✔ count: an arrival tick with no retraction is the retraction guard alone (1.516708ms)
✔ count: a staged retraction reconciles exactly the plain level statements (1.6785ms)
✔ plan: the retraction guard SEARCHes each delta table by its _sign index (1.295333ms)
(node:73321) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ norm: emitted SQLite keeps V5 ASCII alphanumerics and lowercases them (136.122792ms)
(node:73327) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ the ordered/pre family costs 13 + 2n statements per tick, against the incremental family's flat 31 (28.413ms)
✔ the ordered/pre snapshot copies the whole relation every tick, arrivals or not (1.242542ms)
✔ ordered frontier staging carries only the supplied boundary additions (0.443875ms)
✔ ordered frontier staging retains sequence across relation groups (0.46525ms)
(node:73342) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ depth 2 construction joins one dictionary per level, all indexed (26.057209ms)
✔ depth 2 destructure reads two hops, both keyed on __id (8.160083ms)
✔ depth 3 destructure reads three hops, all keyed on __id (4.417833ms)
✔ depth 3 construction interns each level once (4.094167ms)
(node:73356) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ keep(count) lowers to one set-based retention statement (0.692333ms)
✔ keep(count) statement count is flat and the oldest rows are pruned (10.369666ms)
(node:73358) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ a tick publishes one record per emitted statement that ran (10.314833ms)
✔ two arms of one head are two different rules in the log (2.446084ms)
✔ the records account for the rows the tick actually derived (1.735167ms)
(node:73360) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ a row that is not an array is a 400 naming what is wrong (151.104875ms)
✔ an object in a text column is a 400 naming what is wrong (116.126833ms)
✔ an array in a text column is a 400 naming what is wrong (106.732416ms)
✔ a null in a struct column is a 400 naming what is wrong (110.630792ms)
✔ a batch that is not an array is a 400 naming what is wrong (122.062ms)
✔ a body that is not JSON is a 400 naming what is wrong (119.042083ms)
✔ a null arrival is a 400 naming what is wrong (112.313834ms)
✔ a value of the wrong declared type is a 400 naming what is wrong (112.802667ms)
✔ a rejected row is never stored, and never printed as an empty delta (122.865792ms)
✔ well-formed arrivals are untouched by the new checks (122.4815ms)
(node:73373) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ compile budget: a compile that outruns its budget is a NAMED compile_timeout, not a hang (68.367ms)
✔ compile budget: the timed-out compiler's process group is dead, and the server still loads programs (693.637083ms)
(node:73374) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ receipt (a): door-handwritten served over http matches the oracle tick log byte for byte (265.544625ms)
✔ receipt (a) guard: a batch naming a rel that is not an arrival target is a 400, not a tick (114.215458ms)
✔ receipt (a) refusal: a program the compiler refuses is a 400 and leaves the running program alone (662.140458ms)
(node:73382) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ a carrying program served one POST at a time matches the oracle replayed on its own consumed schedule (260.881583ms)
✔ a batch already queued behind a carrying tick takes the tick, not a drain (7.127541ms)
(node:73383) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ receipt (b): live interval bind + live sh host, served, matches the oracle fed the same answers (288.564916ms)
✔ receipt (b) teardown: a program swap stops the previous program's interval (240.399542ms)
✔ declared-struct live host output interns once and tick logs render the canonical value (154.404958ms)
(node:73452) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
﹣ receipt (c): 20 program-swap cycles leave no handle, timer, or subscription behind (0.301625ms) # set DL_PERF_LOG (scripts/leak-soak.sh)
(node:73506) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ the program outlives close() while a request is still being served, and is dropped only after (271.929458ms)
✔ stop() resolves only after the port is actually released, so the next server can take it (137.2085ms)
✔ servers started with no port asked for never collide (1.526583ms)
✔ reservePort hands back an address nothing is listening on (0.7985ms)
(node:73521) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ GET /stats before any program is loaded is a 404, same convention as /idb/:rel (7.365667ms)
✔ GET /stats reports process memory always, and dbstat page bytes for requested tables (131.133959ms)
(node:73620) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ watch bind: real file bytes become (glob, path, digest) rows, one batch per coalesce window (495.426958ms)
✔ watch bind: the watched subtree is the glob's literal prefix, not the whole root (0.195792ms)
(node:73622) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ edge 1: two build orders render identically while their dense ids differ (8.824125ms)
✔ edge 2: the nested target and parent are public arrivals in one tick (2.714209ms)
✔ count: resolving is three statements per target relation, flat in the number of values (1.594875ms)
✔ count: a tick carrying no nested relation value runs zero normalization statements (0.411125ms)
✔ key: equal key and equal row reuse one target id (0.389459ms)
✔ key: an existing key with different non-key fields refuses before insertion (0.712292ms)
✔ key: an UPSERT replacement preserves the target id held by parents (0.375333ms)
✔ key: two different rows with one key in the same batch refuse before SQL (0.275291ms)
✔ plan: the boundary render of a ref column SEARCHes the target view by rowid (0.522417ms)
✔ crash: standalone resolution replay follows ordinary duplicate-arrival semantics (6.442125ms)
✔ canonicalText is sorted-keys-no-whitespace, the ruled cross-target encoding (0.544833ms)
(node:73623) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ flag off is identity on every list, by reference and not by value (0.442625ms)
✔ the default environment is flag off (0.077209ms)
✔ flag off + query-bearing: the fixture's own expectations, unpruned (19.565667ms)
✔ flag on + query-bearing: subscribed rels identical, the host edge held (9.929792ms)
✔ flag off + host-free: the off-cone chain derives, cone or no cone (7.431ms)
✔ flag on + host-free: the off-cone chain costs zero statements (7.427209ms)
✔ flag off + zero-query: the level view derives, cone or no cone (4.780708ms)
✔ flag on + zero-query: nothing derives, ingestion still lands (5.156417ms)
(node:73653) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ the tick counter survives a re-run of the program DDL (3.751416ms)
✔ the tick advance is one statement per tick, flat in arrivals (0.186959ms)
✔ now() reads the counter as a scalar subquery, never a joined row (1.092292ms)
(node:73654) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ demand_laziness_effect_rows matches the oracle tick log (5.691334ms)
✔ switch_as_keyed_replace matches the oracle tick log, including the drain tick (2.466083ms)
✔ demand_laziness_effect_rows PERTURBED schedule matches the oracle's perturbed log (proves real computation, not replay) (2.089417ms)
(node:73655) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ the declared half of the trace matches the pinned golden (323.598958ms)
(node:73656) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ the generated schema is not stale against registry.pl's trace_event rows (29.214959ms)
✔ every wire key is lower snake case and every elapsed value ends _ms (0.22775ms)
✔ the tick and effect records the runtime publishes carry exactly the schema's keys, in order (0.51025ms)
(node:73657) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ watch boot: stored rows and tracked disk become one difference batch, then seed a later delete (68.789917ms)
✔ watch boot: identical tracked rows seed state without submitting a tick (49.590583ms)
✔ watch boot: notifications arriving during the row read queue behind the boot batch (44.343625ms)
(node:73679) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ count: one coalesce window is ONE tick, and one file is ONE row however many events it fired (391.751666ms)
✔ count: statements per tick are FLAT from a 5-file burst to a 50-file burst (369.217208ms)
(node:73694) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ watch boot: `src/**/*.rs` admits direct children of src/, not only nested ones (72.675125ms)
✔ watch boot: `**/*.md` admits repo-root files (47.16925ms)
✔ watch boot: a brace glob boots rows rather than a silent zero (43.601542ms)
✔ watch: boot and live accept identical path sets across every census glob shape (261.727791ms)
(node:73723) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
✔ watch bind: successive saves to ONE file stay audible on the real fs backend (354.8895ms)
ℹ tests 150
ℹ suites 0
ℹ pass 149
ℹ fail 0
ℹ cancelled 0
ℹ skipped 1
ℹ todo 0
ℹ duration_ms 5674.088459

```

### 6. sweep (scripts/sweep.sh)

```
=== stage 1: compile sweep ===
SWEEP total=512 compiled=420 unsupported=92 crash=0
  UNSUPPORTED enum_decl_variant_name_collision_is_refused enum_variant_name_collision(page)
  UNSUPPORTED match_enum_nonexhaustive_is_refused match_nonexhaustive(body,redirect)
  UNSUPPORTED keyed_level_head_is_refused keyed_level_head(current_value/2)
  UNSUPPORTED ghcacher_json_normalization level_body_goal(pull_request(_593316,_593318,_593320,_593322,_593324),json_each(_593328,_593330))
  UNSUPPORTED ghcacher_host_program_term level_body_goal(pull_request(_593276,_593278,_593280,_593282,_593284),json_each(_593288,_593290))
  UNSUPPORTED host_output_column_shadows_runtime_ordinal host_column_shadows_runtime(look,output,ordinal)
  UNSUPPORTED host_input_column_shadows_runtime_witness host_column_shadows_runtime(peek,input,witness_digest)
  UNSUPPORTED duplicate_host_name_is_refused duplicate_host_decl(look)
  UNSUPPORTED repo_on_bind_watch_is_refused bind_repo_column(watch)
  UNSUPPORTED struct_type_cycle_rejected type_cycle([node])
  UNSUPPORTED struct_type_mutual_cycle_rejected type_cycle([left,right])
  UNSUPPORTED struct_column_type_unknown_rejected column_type_unknown(spann)
  UNSUPPORTED struct_host_output_type_unknown_rejected column_type_unknown(spann)
  UNSUPPORTED struct_arrival_missing_key_rejected type_arrival_shape_mismatch(finding/2,at,span,missing_key(span,end))
  UNSUPPORTED struct_arrival_field_type_rejected type_arrival_shape_mismatch(finding/2,at,span,field_not_int(span,end,nine))
  UNSUPPORTED struct_arrival_unknown_key_rejected type_arrival_shape_mismatch(finding/2,at,span,unknown_key(span,extra))
  UNSUPPORTED struct_arrival_functor_term_rejected type_arrival_shape_mismatch(finding/2,at,span,not_an_object(span,span(3,9)))
  UNSUPPORTED struct_decode_field_unknown_rejected decode_field_unknown(span,beginning)
  UNSUPPORTED higher_order_call_goal_rejected dynamic_relation_name(call/3)
  UNSUPPORTED higher_order_call_over_atom_rejected dynamic_relation_name(call/1)
  UNSUPPORTED int_out_of_range_is_named_refusal int_out_of_range(measure/1,value,9007199254740993)
  UNSUPPORTED bool_rejects_text_ingress type_arrival_shape_mismatch(flag/1,value,bool,field_not_bool(true))
  UNSUPPORTED float_rejects_non_float_ingress type_arrival_shape_mismatch(score/1,value,float,field_not_finite_float(not_a_number))
  UNSUPPORTED text_rejects_number_ingress type_arrival_shape_mismatch(label/1,value,text,field_not_text(4))
  UNSUPPORTED int_rejects_fractional_ingress type_arrival_shape_mismatch(measure/1,value,int,field_not_int(1.5))
  UNSUPPORTED wide_int_refused_at_undeclared_column int_out_of_range(untyped/1,1,9007199254740993)
  UNSUPPORTED wide_int_refused_inside_json_document int_out_of_range(payload/1,document,9007199254740993)
  UNSUPPORTED head_column_type_conflict_is_refused head_column_type_conflict(target/1,total,int,source/1,name,text)
  UNSUPPORTED relation_pattern_text_literal_in_ref_column_rejected relation_pattern_not_a_relation_value(span/3,file,file,'src/a.rs')
  UNSUPPORTED relation_pattern_wrong_target_rejected relation_pattern_not_a_relation_value(span/3,file,file,fpath('a.rs'))
  UNSUPPORTED relation_pattern_target_arity_rejected relation_pattern_not_a_relation_value(span/3,file,file,file(repo(acme)))
  UNSUPPORTED relation_ref_column_fed_by_text_variable_rejected relation_column_type_conflict(span/3,file,file,raw3/3,file,text)
  UNSUPPORTED relation_value_under_negation_rejected relation_value_under_negation(span/3,file,file,file(repo(acme),fpath('missing.rs')))
  UNSUPPORTED relation_value_in_edge_rule_rejected relation_value_in_edge_rule(span/3,file,file,file(repo(acme),fpath('src/a.rs')))
  UNSUPPORTED coalesce_without_an_output_is_refused coalesce_no_output(archived/1)
  UNSUPPORTED coalesce_with_two_outputs_is_refused coalesce_multiple_outputs(commit_by/3,2)
  UNSUPPORTED coalesce_with_a_variable_default_is_refused coalesce_default_not_literal(latest_commit/2,variable)
  UNSUPPORTED coalesce_under_negation_is_refused coalesce_not_top_level(latest_commit/2)
  UNSUPPORTED json_capture_type_bool_is_refused json_capture_type_unknown(bool)
  UNSUPPORTED json_capture_type_typo_is_refused json_capture_type_unknown(itn)
  UNSUPPORTED regexp_pattern_not_literal regexp_pattern_not_literal
  UNSUPPORTED regexp_operand_not_text regexp_operand_not_text(source/1,value,int)
  UNSUPPORTED regexp_pattern_outside_subset regexp_pattern_outside_subset("a(?=b)")
  UNSUPPORTED regexp_pattern_invalid regexp_pattern_invalid("[","Syntax error: missing terminating ] for character class")
  UNSUPPORTED zip_is_a_named_refusal zip
  UNSUPPORTED subscribe_is_a_named_refusal lifecycle_arm(subscribe)
  UNSUPPORTED unsubscribe_is_a_named_refusal lifecycle_arm(unsubscribe)
  UNSUPPORTED complete_is_a_named_refusal lifecycle_arm(complete)
  UNSUPPORTED error_is_a_named_refusal lifecycle_arm(error)
  UNSUPPORTED scan_is_a_named_refusal removed_word(scan)
  UNSUPPORTED scan_is_a_named_refusal_at_five_arguments removed_word(scan)
  UNSUPPORTED edge_trigger_literal_filters_on_the_oracle_door trigger_arg_not_var(200)
  UNSUPPORTED log_without_retention_rejected missing_retention(event/1)
  UNSUPPORTED aggregate_in_edge_head_rejected aggregate_in_edge_head(total/1)
  UNSUPPORTED unimplemented_aggregate_head_rejected aggregate_not_implemented(roster/1,group_concat/1,[avg,count,group_concat,json_group_array,max,min,sum])
  UNSUPPORTED keep_on_non_log_rel_rejected keep_on_non_log_rel(state/1)
  UNSUPPORTED keyed_log_rejected keyed_log_rel(latest/2,[1])
  UNSUPPORTED edge_into_unkeyed_set_rejected edge_into_unkeyed_set(sink/1)
  UNSUPPORTED log_on_level_headed_rel_rejected log_on_level_headed_rel(derived_event/1)
  UNSUPPORTED retention_head_conflict_risk_rejected retention_head_conflict_risk(journal/1,count(1))
  UNSUPPORTED latest_in_level_rule_rejected latest_in_level_rule(source_item/1)
  UNSUPPORTED pre_in_level_rule_rejected pre_in_level_rule(source_item/1)
  UNSUPPORTED arithmetic_rejects_non_int_operand_at_runtime arith_operand_not_number(_587772+1,_587772,text)
  UNSUPPORTED text_one_and_numeric_one_never_join join_column_type_mismatch('b1."value"',int,'b0."value"',text)
  UNSUPPORTED text_one_and_numeric_one_are_not_equal comparison_type_mismatch(_587722==_587724,text,int)
  UNSUPPORTED typed_int_contradicts_text_witness type_arrival_shape_mismatch(typed_conflict/1,value,int,field_not_int(text_value))
  UNSUPPORTED braces_literal_canonicalizes json_value_expression({stars:4,name:_587626})
  UNSUPPORTED braces_in_head_position json_value_expression({repo:_587584})
  UNSUPPORTED decode_open_pattern_binds_nested decode_source_not_struct(decode(_587542,{name:_587554}))
  UNSUPPORTED decode_missing_key_fails_quietly decode_source_not_struct(decode(_587506,{absent_key:_587518}))
  UNSUPPORTED json_each_fans_out level_body_goal(repo_lang(_587476),json_each(_587480,_587476))
  UNSUPPORTED json_array_keeps_bag_duplicates aggregate_head(json_array(_587130))
  UNSUPPORTED json_array_groups_and_nests aggregate_head(json_array(_587106))
  UNSUPPORTED json_object_builds_document aggregate_head(json_object(_587080,_587082))
  UNSUPPORTED json_object_dup_key_rejected aggregate_head(json_object(_587054,_587056))
  UNSUPPORTED json_round_trip_decode_to_document level_body_goal(repo_lang(_586926,_586928),json_each(_586932,_586928))
  UNSUPPORTED seed_and_transition_are_disjoint edge_body_with_negation((increment(_586692,_586694),not(pre(counter(_586692,_586708)))))
  UNSUPPORTED one_occurrence_two_rows_still_conflicts edge_head_conflict_risk(latest/2,[ping/1])
  UNSUPPORTED one_attempt_bounded_log_two_arms_refused retention_head_conflict_risk(dispatch_first/2,count(1))
  UNSUPPORTED fork_join_error_arm_is_a_value compound_pattern_on_arrival_rel(outcome_a/1,1,ok(_585918))
  UNSUPPORTED scope_done_three_spellings trigger_arg_not_var(done)
  UNSUPPORTED async_state_machine_with_pattern_scan trigger_arg_not_var(fresh(_584838,_584840))
  UNSUPPORTED same_tick_error_then_fresh_chains_arms trigger_arg_not_var(error(_584814))
  UNSUPPORTED desugared_trace_equals_hand_written edge_body_needs_json_destructure((demand_row(_584764,_584766),decode(_584766,fresh(_584782,_584784)),stars_of(_584784,_584790)))
  UNSUPPORTED trigger_marker_is_what_stops_backlog_replay edge_body_needs_json_destructure((demand_row(_584708,_584710),decode(_584710,fresh(_584726,_584728)),stars_of(_584728,_584734)))
  UNSUPPORTED unmarked_chain_replays_to_late_subscriber edge_body_needs_json_destructure((demand_row(_584652,_584654),decode(_584654,fresh(_584670,_584672)),stars_of(_584672,_584678)))
  UNSUPPORTED unmarked_first_stage_refires_on_late_watch edge_body_needs_json_destructure((demand_row(_584596,_584598),decode(_584598,fresh(_584614,_584616)),stars_of(_584616,_584622)))
  UNSUPPORTED pipe_stage_costs_one_tick edge_body_needs_json_destructure((demand_row(_584540,_584542),decode(_584542,fresh(_584558,_584560)),stars_of(_584560,_584566)))
  UNSUPPORTED chain_into_keyed_head_replaces edge_body_needs_json_destructure((demand_row(_584496,_584498),decode(_584498,fresh(_584508,_584510))))
  UNSUPPORTED guard_stage_fires_on_negation_and_comparison edge_body_needs_json_destructure((demand_row(_584414,_584416),decode(_584416,fresh(_584432,_584434)),stars_of(_584434,_584446),_584446>100,not(muted(_584414))))
  UNSUPPORTED guard_stage_silent_when_muted edge_body_needs_json_destructure((demand_row(_584332,_584334),decode(_584334,fresh(_584350,_584352)),stars_of(_584352,_584364),_584364>100,not(muted(_584332))))
  UNSUPPORTED guard_stage_silent_below_threshold edge_body_needs_json_destructure((demand_row(_584250,_584252),decode(_584252,fresh(_584268,_584270)),stars_of(_584270,_584282),_584282>100,not(muted(_584250))))

=== stage 2: oracle dump ===
ORACLE_OK enum_decl_variant_rows_round_trip_through_tag_view
ORACLE_OK enum_decl_two_variants_union_in_tag_view
ORACLE_THROW enum_decl_variant_name_collision_is_refused unsupported_construct(enum_variant_name_collision(page))
ORACLE_OK match_classify_response
ORACLE_OK match_classify_response_desugared
ORACLE_OK match_edge_arm_keeps_edge_semantics
ORACLE_THROW match_enum_nonexhaustive_is_refused unsupported_construct(match_nonexhaustive(body,redirect))
ORACLE_THROW keyed_level_head_is_refused keyed_level_head(current_value/2)
ORACLE_OK keyed_edge_head_still_replaces
ORACLE_OK ghcacher_json_normalization
ORACLE_OK ghcacher_host_program_term
ORACLE_OK extraction_fork_callgraph
ORACLE_OK extraction_fork_span_line
ORACLE_OK native_ts_query_term
ORACLE_THROW host_output_column_shadows_runtime_ordinal host_column_shadows_runtime(look,output,ordinal)
ORACLE_THROW host_input_column_shadows_runtime_witness host_column_shadows_runtime(peek,input,witness_digest)
ORACLE_THROW duplicate_host_name_is_refused duplicate_host_decl(look)
ORACLE_THROW repo_on_bind_watch_is_refused bind_repo_column(watch)
ORACLE_OK callgraph_derivation_over_extraction
ORACLE_OK callgraph_unused_inverts_with_the_call_set
ORACLE_OK flagship_flow_reach_over_resolved_edges
ORACLE_OK flagship_flow_reach_over_batched_resolved_edges
ORACLE_THROW struct_type_cycle_rejected type_cycle([node])
ORACLE_THROW struct_type_mutual_cycle_rejected type_cycle([left,right])
ORACLE_THROW struct_column_type_unknown_rejected column_type_unknown(spann)
ORACLE_THROW struct_host_output_type_unknown_rejected column_type_unknown(spann)
ORACLE_THROW struct_arrival_missing_key_rejected type_arrival_shape_mismatch(finding/2,at,span,missing_key(span,end))
ORACLE_THROW struct_arrival_field_type_rejected type_arrival_shape_mismatch(finding/2,at,span,field_not_int(span,end,nine))
ORACLE_THROW struct_arrival_unknown_key_rejected type_arrival_shape_mismatch(finding/2,at,span,unknown_key(span,extra))
ORACLE_OK struct_arrival_key_order_canonicalized
ORACLE_THROW struct_arrival_functor_term_rejected type_arrival_shape_mismatch(finding/2,at,span,not_an_object(span,span(3,9)))
ORACLE_OK struct_column_renders_canonical_json
ORACLE_OK struct_intern_order_a
ORACLE_OK struct_intern_order_b
ORACLE_OK struct_nested_value_renders_whole_tree
ORACLE_OK struct_ghcacher_stars_normalization
ORACLE_OK struct_decode_field_unknown_rejected
ORACLE_OK struct_span_columns_are_int_after_decode
ORACLE_OK struct_host_output_schedule_answer_interned
ORACLE_OK struct_shared_child_survives_one_release
ORACLE_OK relation_reference_target_and_parent_share_tick
ORACLE_OK groupby_two_bare_integer_literals
ORACLE_OK groupby_aggregate_two_bare_integer_literals
ORACLE_OK probe_output_comparison_guard
ORACLE_THROW higher_order_call_goal_rejected dynamic_relation_name(call/3)
ORACLE_THROW higher_order_call_over_atom_rejected dynamic_relation_name(call/1)
ORACLE_OK backslash_in_string_literal_survives_both_doors
ORACLE_OK host_free_query_leaves_a_derived_rel_unsubscribed
ORACLE_OK flow_arg_param_hop_is_positional_and_site_pinned
ORACLE_OK flow_sig_owner_join_types_the_resolved_callee
ORACLE_OK bool_literals_round_trip
ORACLE_OK bool_identity_comparison_filters
ORACLE_OK bool_relation_negation_is_two_valued
ORACLE_OK float_arithmetic_is_binary64
ORACLE_OK int_float_arithmetic_keeps_real_result
ORACLE_OK float_avg_is_grouped
ORACLE_OK float_exact_comparison_has_no_epsilon
ORACLE_OK float_exact_join_has_no_epsilon
ORACLE_OK float_negative_zero_canonical_boundary
ORACLE_OK float_integral_value_keeps_real_storage
ORACLE_OK float_shortest_round_trip_wire
ORACLE_OK float_avg_retracts_to_empty_group
ORACLE_THROW int_out_of_range_is_named_refusal int_out_of_range(measure/1,value,9007199254740993)
ORACLE_THROW bool_rejects_text_ingress type_arrival_shape_mismatch(flag/1,value,bool,field_not_bool(true))
ORACLE_THROW float_rejects_non_float_ingress type_arrival_shape_mismatch(score/1,value,float,field_not_finite_float(not_a_number))
ORACLE_THROW text_rejects_number_ingress type_arrival_shape_mismatch(label/1,value,text,field_not_text(4))
ORACLE_THROW int_rejects_fractional_ingress type_arrival_shape_mismatch(measure/1,value,int,field_not_int(1.5))
ORACLE_OK int_accepts_integral_float
ORACLE_OK float_widens_integer_ingress
ORACLE_THROW wide_int_refused_at_undeclared_column int_out_of_range(untyped/1,1,9007199254740993)
ORACLE_THROW wide_int_refused_inside_json_document int_out_of_range(payload/1,document,9007199254740993)
ORACLE_OK float_widens_wide_integer_ingress
ORACLE_THROW head_column_type_conflict_is_refused head_column_type_conflict(target/1,total,int,source/1,name,text)
ORACLE_OK head_column_int_widens_into_float
ORACLE_OK head_column_list_and_json_share_storage
ORACLE_OK relation_depth2_construct_and_read
ORACLE_OK relation_depth2_literal_leaf_selects_zero_and_one
ORACLE_OK relation_depth2_many_rows_share_one_leaf
ORACLE_OK relation_depth2_chained_decode
ORACLE_OK relation_depth2_nested_decode_pattern
ORACLE_OK relation_depth2_dot_read
ORACLE_OK relation_depth2_member_dot_pattern
ORACLE_OK relation_depth3_construct_and_read
ORACLE_OK relation_depth3_chained_decode
ORACLE_OK relation_depth3_many_rows
ORACLE_THROW relation_pattern_text_literal_in_ref_column_rejected relation_pattern_not_a_relation_value(span/3,file,file,'src/a.rs')
ORACLE_THROW relation_pattern_wrong_target_rejected relation_pattern_not_a_relation_value(span/3,file,file,fpath('a.rs'))
ORACLE_THROW relation_pattern_target_arity_rejected relation_pattern_not_a_relation_value(span/3,file,file,file(repo(acme)))
ORACLE_THROW relation_ref_column_fed_by_text_variable_rejected relation_column_type_conflict(span/3,file,file,raw3/3,file,text)
ORACLE_OK relation_ref_column_fed_by_ref_variable_accepted
ORACLE_THROW relation_value_under_negation_rejected relation_value_under_negation(span/3,file,file,file(repo(acme),fpath('missing.rs')))
ORACLE_THROW relation_value_in_edge_rule_rejected relation_value_in_edge_rule(span/3,file,file,file(repo(acme),fpath('src/a.rs')))
ORACLE_OK coalesce_defaults_the_absent_row
ORACLE_OK coalesce_default_returns_when_source_retracts
ORACLE_OK coalesce_over_derived_source
ORACLE_OK coalesce_in_edge_body_samples
ORACLE_THROW coalesce_without_an_output_is_refused unsupported_construct(coalesce_no_output(archived/1))
ORACLE_THROW coalesce_with_two_outputs_is_refused unsupported_construct(coalesce_multiple_outputs(commit_by/3,2))
ORACLE_THROW coalesce_with_a_variable_default_is_refused unsupported_construct(coalesce_default_not_literal(latest_commit/2,variable))
ORACLE_THROW coalesce_under_negation_is_refused unsupported_construct(coalesce_not_top_level(latest_commit/2))
ORACLE_OK json_string_control_escapes_are_valid_json
ORACLE_OK json_control_escapes_inside_a_document
ORACLE_OK json_non_ascii_keys_sort_by_code_point
ORACLE_OK json_nfc_and_nfd_keys_stay_distinct
ORACLE_OK json_empty_string_key_round_trips
ORACLE_OK json_marker_shaped_keys_are_ordinary_data
ORACLE_OK json_safe_integer_boundary_survives_both_doors
ORACLE_OK json_empty_containers_nest
ORACLE_OK json_deep_exact_key_chain_binds
ORACLE_OK json_top_level_scalar_document_is_a_value
ORACLE_OK json_absent_key_yields_no_row_under_arrivals
ORACLE_OK json_spread_and_capture_and_descent_multiply
ORACLE_OK json_typed_capture_folds_into_a_keyed_int_total
ORACLE_OK json_typed_capture_filters_a_wrong_typed_value
ORACLE_OK json_untyped_capture_binds_without_a_type
ORACLE_THROW json_capture_type_bool_is_refused json_capture_type_unknown(bool)
ORACLE_THROW json_capture_type_typo_is_refused json_capture_type_unknown(itn)
ORACLE_OK ordered_json_group_array_value
ORACLE_OK ordered_json_group_array_integer_values
ORACLE_OK ordered_json_group_array_ordinal
ORACLE_OK ordered_group_concat_value
ORACLE_OK ordered_group_concat_ordinal
ORACLE_OK ordered_aggregate_retraction_rebuild
ORACLE_OK ordered_json_group_array_nested_json
ORACLE_OK ordered_mermaid_line_assembly
ORACLE_OK ordered_fragment_line_assembly
ORACLE_OK ordered_group_rels_v5_collect
ORACLE_OK ordered_group_rels_json_head
ORACLE_OK regexp_positive_match
ORACLE_OK regexp_non_match
ORACLE_OK regexp_retraction_flip
ORACLE_THROW regexp_pattern_not_literal regexp_pattern_not_literal
ORACLE_THROW regexp_operand_not_text regexp_operand_not_text(source/1,value,int)
ORACLE_THROW regexp_pattern_outside_subset regexp_pattern_outside_subset("a(?=b)")
ORACLE_THROW regexp_pattern_invalid regexp_pattern_invalid("[","Syntax error: missing terminating ] for character class")
ORACLE_OK arrival_affinity_rewrite_keeps_delta
ORACLE_OK arrival_dup_batch_partial_ignore
ORACLE_OK combine_level_is_the_conjunction_spelling
ORACLE_OK conjunction_level_control_for_combine
ORACLE_OK combine_edge_is_the_conjunction_spelling
ORACLE_OK conjunction_edge_control_for_combine
ORACLE_OK next_level_is_the_bare_atom_spelling
ORACLE_OK next_edge_is_the_bare_atom_spelling
ORACLE_THROW zip_is_a_named_refusal reserved_body_word(zip/2)
ORACLE_THROW subscribe_is_a_named_refusal reserved_body_word(subscribe/1)
ORACLE_THROW unsubscribe_is_a_named_refusal reserved_body_word(unsubscribe/1)
ORACLE_THROW complete_is_a_named_refusal reserved_body_word(complete/1)
ORACLE_THROW error_is_a_named_refusal reserved_body_word(error/1)
ORACLE_THROW scan_is_a_named_refusal reserved_body_word(scan/4)
ORACLE_THROW scan_is_a_named_refusal_at_five_arguments reserved_body_word(scan/5)
ORACLE_OK diag_scenario_seven_ticks_end_to_end
ORACLE_OK clock_rel_join_storms
ORACLE_OK edge_trigger_literal_filters_on_the_oracle_door
ORACLE_OK retention_count_prunes_oldest
ORACLE_OK retention_prune_is_a_visible_minus
ORACLE_OK finalize_over_log_fires_on_retention_prune
ORACLE_OK created_at_pinned_updated_at_advances
ORACLE_THROW log_without_retention_rejected missing_retention(event/1)
ORACLE_THROW aggregate_in_edge_head_rejected aggregate_in_edge_head
ORACLE_THROW unimplemented_aggregate_head_rejected aggregate_not_implemented(roster/1,group_concat/1,[avg,count,group_concat,json_group_array,max,min,sum])
ORACLE_THROW keep_on_non_log_rel_rejected keep_on_non_log_rel(state/1)
ORACLE_THROW keyed_log_rejected keyed_log_rel(latest/2)
ORACLE_THROW edge_into_unkeyed_set_rejected edge_into_unkeyed_set(sink/1)
ORACLE_THROW log_retraction_rejected retract_from_log(event/1)
ORACLE_OK world_fed_keyed_arrival_replaces
ORACLE_THROW log_on_level_headed_rel_rejected log_on_level_headed_rel(derived_event/1)
ORACLE_THROW retention_head_conflict_risk_rejected retention_head_conflict_risk(journal/1,count(1))
ORACLE_OK retention_single_arm_still_prunes
ORACLE_THROW latest_in_level_rule_rejected latest_in_level_rule(source_item/1)
ORACLE_THROW pre_in_level_rule_rejected pre_in_level_rule(source_item/1)
ORACLE_OK now_reads_the_tick
ORACLE_OK edge_chain_hops_tick_per_stage
ORACLE_OK marker_stops_backlog_replay
ORACLE_OK unmarked_edge_replays_backlog
ORACLE_OK retraction_only_tick_retracts_level_view
ORACLE_OK departed_fires_next_tick_on_retraction
ORACLE_OK keyed_replace_departs_the_old_row
ORACLE_OK pairwise_reads_state_at_the_departure_tick
ORACLE_OK pairwise_pairs_adjacent_values_when_the_source_idles
ORACLE_OK set_dedups_log_stacks
ORACLE_OK head_expression_evaluates_derived_column
ORACLE_OK comparison_filters_rows
ORACLE_OK range_join_over_arithmetic
ORACLE_OK bind_computes_derived_value_then_comparison_filters
ORACLE_OK interpolation_desugars_to_concat
ORACLE_OK division_truncates_toward_zero_mod_follows_divisor_sign
ORACLE_THROW arithmetic_rejects_non_int_operand_at_runtime arith_on_non_int(not_a_number,1)
ORACLE_OK text_one_and_numeric_one_never_join
ORACLE_OK text_one_and_numeric_one_are_not_equal
ORACLE_OK typed_int_without_literal_witness
ORACLE_THROW typed_int_contradicts_text_witness type_arrival_shape_mismatch(typed_conflict/1,value,int,field_not_int(text_value))
ORACLE_OK braces_literal_canonicalizes
ORACLE_OK braces_in_head_position
ORACLE_OK decode_open_pattern_binds_nested
ORACLE_OK decode_missing_key_fails_quietly
ORACLE_OK json_each_fans_out
ORACLE_OK json_array_spread_fans_out_correlated_siblings
ORACLE_OK json_array_spread_skips_non_matching_elements
ORACLE_OK json_key_capture_binds_key_and_value
ORACLE_OK json_key_capture_nests_and_fans_out
ORACLE_OK json_descent_matches_at_any_depth
ORACLE_OK json_descent_into_scalars_is_silent
ORACLE_OK json_empty_object_pattern_matches_any_object
ORACLE_OK list_column_fans_out_through_spread
ORACLE_OK count_is_bag_of_derivations
ORACLE_OK sum_min_max_group_by_plain_columns
ORACLE_OK json_array_keeps_bag_duplicates
ORACLE_OK json_array_groups_and_nests
ORACLE_OK json_object_builds_document
ORACLE_THROW json_object_dup_key_rejected json_object_dup_key([name,name])
ORACLE_OK aggregate_count_min_max_track_arrivals_and_retraction
ORACLE_OK aggregate_min_recomputes_when_the_minimum_is_retracted
ORACLE_OK aggregate_sum_tracks_a_growing_and_shrinking_group
ORACLE_OK json_round_trip_decode_to_document
ORACLE_OK merge_batches_per_tick
ORACLE_OK merge_never_retracts
ORACLE_OK key_last_write_wins
ORACLE_OK key_identical_write_is_silent
ORACLE_OK key_same_tick_ordered_not_conflict
ORACLE_OK counter_fold_matches_hand_computation
ORACLE_OK seed_and_transition_are_disjoint
ORACLE_OK batched_increments_both_count
ORACLE_OK increment_decrement_same_tick_nets_zero
ORACLE_THROW one_occurrence_two_rows_still_conflicts keyed_conflict(latest/2,[cli],[latest(cli,a),latest(cli,b)])
ORACLE_OK log_driver_fold_needs_no_id_column
ORACLE_OK identical_increments_stack_as_log_deltas
ORACLE_OK lww_fold_follows_arrival_order
ORACLE_OK concat_fold_follows_arrival_order
ORACLE_OK concat_fold_reversed_arrival_reverses_result
ORACLE_OK log_deltas_follow_arrival_order
ORACLE_OK shuffled_arrival_reorders_log_deltas
ORACLE_OK level_view_reads_set_projection_not_occurrences
ORACLE_OK demand_view_fires_its_consumer_once
ORACLE_OK log_stacks_within_tick_and_across_ticks
ORACLE_OK set_rel_identical_arrival_is_one_occurrence
ORACLE_OK log_rel_identical_arrival_is_two_occurrences
ORACLE_OK any_two_tagged_arms_land_on_one_tick
ORACLE_OK one_attempt_keyed_head_loses_the_first_arm_silently
ORACLE_THROW one_attempt_bounded_log_two_arms_refused retention_head_conflict_risk(dispatch_first/2,count(1))
ORACLE_OK one_attempt_guard_by_negation_lands_one_unnamed_winner
ORACLE_OK one_attempt_guard_by_negation_arrival_order_beats_arm_order
ORACLE_OK filter_map_is_a_level_rule
ORACLE_OK repeat_is_a_self_carry_chain
ORACLE_OK fork_join_is_a_conjunctive_body
ORACLE_OK fork_join_error_arm_is_a_value
ORACLE_OK ordered_program_level_fold_reaches_three_links
ORACLE_OK unordered_program_level_fold_reaches_three_links
ORACLE_OK switch_as_keyed_replace
ORACLE_OK stale_keyed_retraction_keeps_replacement
ORACLE_OK merge_policy
ORACLE_OK exhaust_policy
ORACLE_OK concat_program_queue
ORACLE_OK scope_done_three_spellings
ORACLE_OK completion_propagation_lattice_tick
ORACLE_OK take_until_keyed_replace_negated_done
ORACLE_OK state_flap_nets_to_zero_scope_churn
ORACLE_OK fill_as_cache_update_swr
ORACLE_OK demand_laziness_effect_rows
ORACLE_OK shared_demand_refcount
ORACLE_OK zombie_scope_negative_case_a2b
ORACLE_OK seq_wire_surface
ORACLE_OK seq_wire_hand
ORACLE_OK identical_demand_dedups
ORACLE_OK new_salt_refires_fresh_stream
ORACLE_OK terminal_is_terminal
ORACLE_OK live_nonzero_exit_keeps_rows
ORACLE_OK worktree_edit_replaces_digest_and_flips_kind_view
ORACLE_OK worktree_edit_identical_resave_is_silent
ORACLE_OK dirty_derives_from_digest_mismatch
ORACLE_OK dirty_retracts_on_matching_commit
ORACLE_OK head_move_replaces_key
ORACLE_OK head_move_flips_current_tree_in_one_tick
ORACLE_OK pin_to_unknown_repo_derives_repo_candidate
ORACLE_OK xref_rev_is_pin_data_not_live_head
ORACLE_OK changed_since_spans_two_turns
ORACLE_OK changed_since_ignores_events_before_turn
ORACLE_OK two_pins_dedup_to_one_demand_row
ORACLE_OK rev_fill_not_behind_keeps_stale_pin_empty
ORACLE_OK async_state_machine_with_pattern_scan
ORACLE_OK same_tick_error_then_fresh_chains_arms
ORACLE_OK desugared_trace_equals_hand_written
ORACLE_OK trigger_marker_is_what_stops_backlog_replay
ORACLE_OK unmarked_chain_replays_to_late_subscriber
ORACLE_OK unmarked_first_stage_refires_on_late_watch
ORACLE_OK pipe_stage_costs_one_tick
ORACLE_OK chain_into_keyed_head_replaces
ORACLE_OK guard_stage_fires_on_negation_and_comparison
ORACLE_OK guard_stage_silent_when_muted
ORACLE_OK guard_stage_silent_below_threshold
ORACLE_OK clean_state_no_diags
ORACLE_OK clean_state_gate_and_exit_zero
ORACLE_OK waiver_range_join_exact_rows
ORACLE_OK over_baseline_diag_exact_rows
ORACLE_OK over_baseline_count_row
ORACLE_OK over_baseline_gate_blocks_commit_only
ORACLE_OK fix_by_waiver_returns_to_clean
ORACLE_OK new_file_diag_at_hit_line_exact_rows
ORACLE_OK new_file_no_exceeded_diag
ORACLE_OK unwrap_aggregate_and_interpolation
ORACLE_OK unwrap_unchanged_file_silent
ORACLE_OK unwrap_below_budget_silent
ORACLE_OK tightened_baseline_catches_regrowth

=== stage 3: copy compiled modules into gen_emitted/, run the diff ===
(node:75314) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
RUN total=420 identical=418 wrong=0 emitted_crash=0 rejection=2 no_oracle_log=0
  REJECTION log_retraction_rejected retract from log rel 'event'
  REJECTION log_retraction_rejected retract from log rel 'event'
FINAL total=420 final_identical=418 final_wrong=0 no_oracle_final=2
  NO_ORACLE_FINAL log_retraction_rejected oracle threw on this schedule too; no final state to diff
  NO_ORACLE_FINAL log_retraction_rejected oracle threw on this schedule too; no final state to diff

=== stage 4: refusal-reason diff vs HEAD (informational) ===
(node:75370) ExperimentalWarning: Transform Types is an experimental feature and might change at any time
(Use `node --trace-warnings ...` to show where the warning was created)
/Users/chrishafley/projects/sprefa-lanes/catrel/v6/tsv2/scripts/manifest-reason-diff.ts:136
      if (byName.has(row.name)) throw new Error(`${origin}: duplicate fixture name ${row.name}`);
                                      ^

Error: working-tree manifest: duplicate fixture name enum_decl_variant_rows_round_trip_through_tag_view
    at Object.index (/Users/chrishafley/projects/sprefa-lanes/catrel/v6/tsv2/scripts/manifest-reason-diff.ts:136:39)
    at <anonymous> (/Users/chrishafley/projects/sprefa-lanes/catrel/v6/tsv2/scripts/manifest-reason-diff.ts:205:33)
    at ModuleJob.run (node:internal/modules/esm/module_job:437:25)
    at process.processTicksAndRejections (node:internal/process/task_queues:104:5)
    at async node:internal/modules/esm/loader:639:26
    at async asyncRunEntryPointWithESMLoader (node:internal/modules/run_main:101:5)

Node.js v24.15.0

```


