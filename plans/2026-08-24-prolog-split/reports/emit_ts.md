# v6/prolog/emit_ts.pl -> v6/prolog/emit_ts/

module head keeps lines 1..37 (37 lines): 15 directives, 2 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_text_helpers.pl` | 141 | 38-178 | 37 | 21 |
| `1_header_and_imports.pl` | 113 | 179-291 | 7 | 4 |
| `2_value_plane.pl` | 190 | 292-481 | 49 | 30 |
| `3_local_types.pl` | 181 | 482-662 | 14 | 14 |
| `4_bind_and_query.pl` | 91 | 663-753 | 9 | 8 |
| `5_arrival_gate.pl` | 152 | 754-905 | 3 | 3 |
| `6_catalog.pl` | 188 | 906-1093 | 36 | 19 |
| `7_snapshot.pl` | 127 | 1094-1220 | 21 | 14 |
| `8_arrivals.pl` | 50 | 1221-1270 | 4 | 3 |
| `9_incremental_plans.pl` | 482 | 1271-1752 | 78 | 48 |
| `10_ordered_loop.pl` | 350 | 1753-2102 | 26 | 17 |
| `11_level_recompute.pl` | 114 | 2103-2216 | 6 | 4 |
| `12_deltas_and_tick.pl` | 171 | 2217-2387 | 16 | 10 |
| `13_prune.pl` | 248 | 2388-2635 | 24 | 16 |
| `14_top_level.pl` | 152 | 2636-2787 | 4 | 4 |
| **total** | **2750** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

none

## cross-part call edges

| from | to | callees |
|---|---|---|
| `2_value_plane.pl` | `0_text_helpers.pl` | `js_string/2`, `js_template/2`, `pairs_to_dict/2`, `ref_name/2` |
| `2_value_plane.pl` | `9_incremental_plans.pl` | `quote_ident_local/2` |
| `3_local_types.pl` | `0_text_helpers.pl` | `js_string/2` |
| `3_local_types.pl` | `4_bind_and_query.pl` | `bind_read_literals/4`, `host_columns_json/2` |
| `4_bind_and_query.pl` | `0_text_helpers.pl` | `js_string/2`, `param_text/2` |
| `4_bind_and_query.pl` | `3_local_types.pl` | `host_type_json_text/2` |
| `6_catalog.pl` | `0_text_helpers.pl` | `js_object_key/2`, `js_string/2`, `js_template/2`, `quoted_string_array_text/2`, `ref_name/2` |
| `7_snapshot.pl` | `0_text_helpers.pl` | `js_object_key/2`, `js_string/2`, `js_template/2`, `params_array_text/2`, `ref_name/2` |
| `8_arrivals.pl` | `0_text_helpers.pl` | `js_object_key/2`, `js_template/2`, `ref_name/2` |
| `9_incremental_plans.pl` | `0_text_helpers.pl` | `js_string/2`, `js_template/2`, `quoted_string_array_text/2`, `ref_name/2` |
| `10_ordered_loop.pl` | `0_text_helpers.pl` | `js_template/2`, `quoted_string_array_text/2`, `ref_name/2` |
| `10_ordered_loop.pl` | `9_incremental_plans.pl` | `intern_sql_field/2`, `key_indices/3` |
| `11_level_recompute.pl` | `0_text_helpers.pl` | `js_template/2` |
| `11_level_recompute.pl` | `9_incremental_plans.pl` | `quote_ident_local/2` |
| `12_deltas_and_tick.pl` | `0_text_helpers.pl` | `ref_name/2` |
| `12_deltas_and_tick.pl` | `13_prune.pl` | `snapshot_advance_tick_line/2` |
| `12_deltas_and_tick.pl` | `2_value_plane.pl` | `snapshot_reference_normalize_lines/3`, `snapshot_text_intern_lines/2` |
| `12_deltas_and_tick.pl` | `7_snapshot.pl` | `ordered_after_read_lines/2`, `ordered_mid_read_line/2`, `tick_head_read_line/2`, `tick_stored_before/2` |
| `13_prune.pl` | `0_text_helpers.pl` | `ir_version/1`, `js_string/2`, `ref_name/2` |
| `13_prune.pl` | `11_level_recompute.pl` | `recursive_level_refs/2` |
| `13_prune.pl` | `12_deltas_and_tick.pl` | `departure_stage_incremental_lines/2` |
| `13_prune.pl` | `2_value_plane.pl` | `incremental_reference_normalize_lines/3`, `incremental_text_intern_lines/2` |
| `14_top_level.pl` | `10_ordered_loop.pl` | `edge_statements_intern/2`, `ordered_carry_lines/4`, `ordered_occurrence_lines/6`, `ordered_pre_lines/5`, `ordered_program/1`, `plan_pre_refs/2` |
| `14_top_level.pl` | `11_level_recompute.pl` | `recompute_levels_fn_lines/4`, `recursive_level_refs/2` |
| `14_top_level.pl` | `12_deltas_and_tick.pl` | `build_deltas_fn_lines/5`, `incremental_mode_lines/2`, `run_ordered_tick_fn_lines/8`, `snapshot_retention_fn_lines/2` |
| `14_top_level.pl` | `13_prune.pl` | `advance_tick_fn_lines/2`, `derived_edge_carry_required/3`, `incremental_plan_export_lines/3`, `program_export_lines/3`, `reconcile_every_tick/2`, `retraction_guard/2`, `run_incremental_tick_fn_lines/9`, `subscribe_prune_lines/4` |
| `14_top_level.pl` | `1_header_and_imports.pl` | `header_lines/2`, `imports_lines/8` |
| `14_top_level.pl` | `2_value_plane.pl` | `enum_identity_ddls/2`, `enum_plane_lines/4`, `enum_ref_columns_map/3`, `enum_type_plans/4`, `struct_plane_lines/4`, `struct_tick_wrapper_lines/3`, `text_intern_plan_lines/3` |
| `14_top_level.pl` | `3_local_types.pl` | `local_types_lines/2`, `world_plan_lines/2` |
| `14_top_level.pl` | `5_arrival_gate.pl` | `arrival_value_guard_lines/1`, `bind_args_helper_lines/1`, `trigger_occurrences_helper_lines/1` |
| `14_top_level.pl` | `6_catalog.pl` | `arrival_targets_lines/2`, `ddl_lines/2`, `program_catalog_rows/10`, `rel_catalog_lines/2`, `rel_column_types_lines/2`, `rel_columns_lines/2`, `rel_declared_column_types_lines/2`, `rel_physical_names_lines/2`, +1 more |
| `14_top_level.pl` | `7_snapshot.pl` | `boot_lines/2`, `final_select_lines/2`, `read_snapshot_fn_lines/2`, `read_stored_snapshot_fn_lines/4`, `snapshot_type_lines/2` |
| `14_top_level.pl` | `8_arrivals.pl` | `arrival_statement_fn_lines/2`, `arrival_statements_lines/2` |
| `14_top_level.pl` | `9_incremental_plans.pl` | `incremental_edge_statement_lines/4`, `incremental_level_statement_lines/5`, `incremental_relation_lines/6`, `incremental_retention_statement_lines/2` |

34 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_text_helpers.pl` | the IR version and the js template, string, identifier and case text helpers |
| `1_header_and_imports.pl` | the emitted file's header comment and every import line |
| `2_value_plane.pl` | the declared value plane: struct and enum type plans, ref column maps, identity tables and the normalize lines |
| `3_local_types.pl` | local helper types, the world plan and the host plan json |
| `4_bind_and_query.pl` | bind config literals and the query plan json |
| `5_arrival_gate.pl` | the bind args helper, the arrival value type gate and the trigger occurrence helper |
| `6_catalog.pl` | ddl entries, rel columns, physical names, raw and declared column types, and the catalog rows |
| `7_snapshot.pl` | boot entries, the snapshot type and its two readers, and final_select |
| `8_arrivals.pl` | arrival statements and the function that runs them |
| `9_incremental_plans.pl` | incremental relation plans: edge and level statements, retention, refCount and dred sql, and the fixpoint IR text |
| `10_ordered_loop.pl` | the ordered pre-occurrence loop with its carry, arm and departure lines |
| `11_level_recompute.pl` | level recompute and the row-count sql it reads |
| `12_deltas_and_tick.pl` | build_deltas, snapshot retention, the ordered tick function and the incremental mode lines |
| `13_prune.pl` | subscribe-cone pruning, plan export, advance_tick and the incremental tick dispatch |
| `14_top_level.pl` | emit_program/5 and the two statement classifiers |
