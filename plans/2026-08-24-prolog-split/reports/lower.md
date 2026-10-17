# v6/prolog/lower.pl -> v6/prolog/lower/

module head keeps lines 1..215 (215 lines): 20 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_storage_context.pl` | 204 | 216-419 | 33 | 27 |
| `1_pattern_args.pl` | 94 | 420-513 | 14 | 7 |
| `2_positive_uses.pl` | 236 | 514-749 | 35 | 18 |
| `3_negative_uses.pl` | 88 | 750-837 | 10 | 5 |
| `4_head_expr.pl` | 415 | 838-1252 | 70 | 41 |
| `5_catalog_ddl.pl` | 182 | 1253-1434 | 22 | 19 |
| `6_catalog_rows.pl` | 77 | 1435-1511 | 7 | 7 |
| `7_catalog_planes.pl` | 347 | 1512-1858 | 29 | 17 |
| `8_catalog_decls.pl` | 72 | 1859-1930 | 2 | 2 |
| `9_semantic_ids.pl` | 275 | 1931-2205 | 58 | 28 |
| `10_module_rels.pl` | 104 | 2206-2309 | 27 | 11 |
| `11_module_map.pl` | 82 | 2310-2391 | 15 | 11 |
| `12_catalog_lists.pl` | 117 | 2392-2508 | 30 | 14 |
| `13_catalog_paths.pl` | 158 | 2509-2666 | 32 | 16 |
| `14_guards_and_comparisons.pl` | 99 | 2667-2765 | 9 | 6 |
| `15_head_select.pl` | 123 | 2766-2888 | 26 | 14 |
| `16_interning.pl` | 210 | 2889-3098 | 30 | 26 |
| `17_ddl.pl` | 124 | 3099-3222 | 13 | 2 |
| `18_dictionaries.pl` | 367 | 3223-3589 | 33 | 23 |
| `19_relation_values.pl` | 371 | 3590-3960 | 39 | 29 |
| `20_arrivals.pl` | 111 | 3961-4071 | 10 | 6 |
| `21_edge_rules.pl` | 353 | 4072-4424 | 31 | 18 |
| `22_level_rules.pl` | 85 | 4425-4509 | 7 | 5 |
| `23_avg_accumulator.pl` | 319 | 4510-4828 | 44 | 30 |
| `24_aggregate_scope.pl` | 189 | 4829-5017 | 15 | 9 |
| `25_ref_counts.pl` | 189 | 5018-5206 | 10 | 8 |
| `26_expand.pl` | 66 | 5207-5272 | 6 | 5 |
| `27_dred.pl` | 266 | 5273-5538 | 23 | 20 |
| `28_fixpoint_ir.pl` | 573 | 5539-6111 | 74 | 48 |
| `29_json_decode.pl` | 258 | 6112-6369 | 31 | 12 |
| `30_aggregate_heads.pl` | 519 | 6370-6888 | 67 | 37 |
| `31_deltas_and_order.pl` | 462 | 6889-7350 | 47 | 32 |
| `32_boot.pl` | 111 | 7351-7461 | 12 | 9 |
| `33_top_level.pl` | 174 | 7462-7635 | 13 | 5 |
| `34_write_verbs.pl` | 161 | 7636-7796 | 21 | 13 |
| **total** | **7581** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

| line | directive | part it falls in |
|---|---|---|
| 230 | `:- thread_local frontier_mode_option/1` | `0_storage_context.pl` |
| 231 | `:- thread_local shared_frontier_relation_id_fact/2` | `0_storage_context.pl` |
| 233 | `:- meta_predicate with_frontier_mode(+,0)` | `0_storage_context.pl` |
| 234 | `:- meta_predicate with_shared_frontier_ids(+,0)` | `0_storage_context.pl` |

Each one moves up into the module head file, above the includes.

## cross-part call edges

| from | to | callees |
|---|---|---|
| `0_storage_context.pl` | `13_catalog_paths.pl` | `sql_text_literal/2` |
| `1_pattern_args.pl` | `0_storage_context.pl` | `sql_literal/2` |
| `1_pattern_args.pl` | `16_interning.pl` | `column_encoding/3`, `column_literal_sql/3`, `interned_id_sql/2` |
| `1_pattern_args.pl` | `4_head_expr.pl` | `demanded_sql/5` |
| `2_positive_uses.pl` | `0_storage_context.pl` | `frontier_table_name/2`, `pre_table_name/2`, `quote_ident/2` |
| `2_positive_uses.pl` | `16_interning.pl` | `column_encoding/3` |
| `2_positive_uses.pl` | `1_pattern_args.pl` | `align_to_encoding/4`, `bound_lookup/3`, `compile_pattern_arg/8`, `join_column_types_agree/4` |
| `2_positive_uses.pl` | `21_edge_rules.pl` | `reference_target_ref/2` |
| `2_positive_uses.pl` | `25_ref_counts.pl` | `qualified_column_list/3` |
| `2_positive_uses.pl` | `28_fixpoint_ir.pl` | `qualified_equalities/4` |
| `2_positive_uses.pl` | `4_head_expr.pl` | `compile_expr/7` |
| `3_negative_uses.pl` | `0_storage_context.pl` | `quote_ident/2`, `table_name/2` |
| `3_negative_uses.pl` | `1_pattern_args.pl` | `compile_pattern_arg/8` |
| `4_head_expr.pl` | `0_storage_context.pl` | `quote_ident/2`, `sql_literal/2`, `table_name/2` |
| `4_head_expr.pl` | `16_interning.pl` | `dictionary_content_sql/2`, `interned_id_sql/2`, `text_literal_sql/5` |
| `4_head_expr.pl` | `1_pattern_args.pl` | `bound_lookup/3` |
| `4_head_expr.pl` | `30_aggregate_heads.pl` | `json_group_array_value_sql/3` |
| `5_catalog_ddl.pl` | `0_storage_context.pl` | `quote_ident/2`, `table_name/2` |
| `5_catalog_ddl.pl` | `21_edge_rules.pl` | `nth1_list/3` |
| `5_catalog_ddl.pl` | `6_catalog_rows.pl` | `catalog_all_rows/10` |
| `6_catalog_rows.pl` | `0_storage_context.pl` | `with_storage_context/2` |
| `6_catalog_rows.pl` | `18_dictionaries.pl` | `dictionary_relplans/2` |
| `6_catalog_rows.pl` | `19_relation_values.pl` | `expand_decode_rules/4`, `expand_relation_pattern_rules/4` |
| `6_catalog_rows.pl` | `22_level_rules.pl` | `level_statement_groups/4` |
| `6_catalog_rows.pl` | `7_catalog_planes.pl` | `catalog_plane_rows/10`, `catalog_storage_rows/7` |
| `6_catalog_rows.pl` | `8_catalog_decls.pl` | `catalog_decl_rows/6` |
| `7_catalog_planes.pl` | `0_storage_context.pl` | `delta_table_name/2`, `departure_frontier_table_name/2`, `frontier_table_name/2`, `next_frontier_table_name/2`, `pre_table_name/2`, `ref_count_table_name/2` |
| `7_catalog_planes.pl` | `11_module_map.pl` | `rel_module/4` |
| `7_catalog_planes.pl` | `12_catalog_lists.pl` | `rel_row_id/3` |
| `7_catalog_planes.pl` | `16_interning.pl` | `any_interned_column/2`, `interned_column/2` |
| `7_catalog_planes.pl` | `23_avg_accumulator.pl` | `aggregate_scope_table_name/2`, `avg_accumulator_table_name/2` |
| `7_catalog_planes.pl` | `25_ref_counts.pl` | `arrival_scratch_table_name/2` |
| `7_catalog_planes.pl` | `26_expand.pl` | `expand_table_name/3` |
| `7_catalog_planes.pl` | `27_dred.pl` | `dred_cone_table_name/2`, `dred_ping_table_name/2`, `dred_pong_table_name/2` |
| `7_catalog_planes.pl` | `5_catalog_ddl.pl` | `rel_h_id/4`, `schema_hash/4` |
| `7_catalog_planes.pl` | `8_catalog_decls.pl` | `catalog_decl_rows/6` |
| `8_catalog_decls.pl` | `10_module_rels.pl` | `catalog_rel_module_ids/3`, `catalog_rel_plans/4` |
| `8_catalog_decls.pl` | `11_module_map.pl` | `catalog_module_edge_rows/5`, `catalog_primitive_rows/2`, `catalog_spliced_module_rows/6`, `module_id_by_hash/2`, `rel_module_map/3` |
| `8_catalog_decls.pl` | `12_catalog_lists.pl` | `catalog_list_id_map/3`, `catalog_list_rows/5`, `catalog_list_types/2`, `catalog_rel_block_end/3`, `catalog_rel_id_map/4`, `catalog_rel_rows/10` |
| `8_catalog_decls.pl` | `13_catalog_paths.pl` | `catalog_path_tree/8` |
| `8_catalog_decls.pl` | `5_catalog_ddl.pl` | `rule_bodies_map/2` |
| `8_catalog_decls.pl` | `9_semantic_ids.pl` | `annotate_catalog_semantic_ids/4`, `metadata_anonymous_rows/5`, `metadata_derived_relation_rows/5`, `metadata_generic_column_rows/8`, `metadata_instance_rows/7`, `metadata_named_rows/7`, `metadata_parameter_rows/6`, `semantic_generic/4`, +2 more |
| `9_semantic_ids.pl` | `10_module_rels.pl` | `catalog_source_type_id/4` |
| `9_semantic_ids.pl` | `12_catalog_lists.pl` | `rel_row_id/3` |
| `10_module_rels.pl` | `12_catalog_lists.pl` | `list_row_id/3`, `rel_row_id/3` |
| `10_module_rels.pl` | `13_catalog_paths.pl` | `catalog_type_id/2` |
| `12_catalog_lists.pl` | `11_module_map.pl` | `rel_module/4` |
| `12_catalog_lists.pl` | `13_catalog_paths.pl` | `catalog_column_rows/11`, `catalog_rel_scope/5`, `catalog_type_id/2` |
| `12_catalog_lists.pl` | `5_catalog_ddl.pl` | `rel_h_id/4`, `rule_hash/3`, `schema_hash/4` |
| `13_catalog_paths.pl` | `12_catalog_lists.pl` | `list_row_id/3`, `rel_row_id/3` |
| `13_catalog_paths.pl` | `16_interning.pl` | `interned_column/2`, `interned_literal_sql/2` |
| `13_catalog_paths.pl` | `5_catalog_ddl.pl` | `rel_h_id/4` |
| `14_guards_and_comparisons.pl` | `0_storage_context.pl` | `sql_literal/2` |
| `14_guards_and_comparisons.pl` | `1_pattern_args.pl` | `aligned_pair/6`, `bound_lookup/3` |
| `14_guards_and_comparisons.pl` | `4_head_expr.pl` | `compile_expr/7`, `tick_column_sql/1` |
| `15_head_select.pl` | `0_storage_context.pl` | `quote_ident/2`, `table_name/2` |
| `15_head_select.pl` | `16_interning.pl` | `column_encoding/3`, `interned_id_sql/2`, `string_dictionary_table/1` |
| `15_head_select.pl` | `18_dictionaries.pl` | `list_member_ref/2` |
| `15_head_select.pl` | `4_head_expr.pl` | `compile_expr/7` |
| `16_interning.pl` | `0_storage_context.pl` | `quote_ident/2`, `sql_literal/2` |
| `17_ddl.pl` | `0_storage_context.pl` | `quote_ident/2`, `table_name/2` |
| `17_ddl.pl` | `16_interning.pl` | `interned_column/2`, `text_view_ddls/6` |
| `17_ddl.pl` | `18_dictionaries.pl` | `dictionary_table_name/2`, `relation_render_expr/5` |
| `17_ddl.pl` | `5_catalog_ddl.pl` | `option_some_index_ddl/2`, `option_some_table/5`, `set_rel_pk_sql/6`, `set_rel_table_ddl/5` |
| `18_dictionaries.pl` | `0_storage_context.pl` | `quote_ident/2`, `sql_literal/2`, `table_name/2` |
| `18_dictionaries.pl` | `16_interning.pl` | `interned_column/2`, `string_dictionary_table/1`, `text_decode_expr/2` |
| `18_dictionaries.pl` | `19_relation_values.pl` | `incremental_json_select_exprs_from/3` |
| `19_relation_values.pl` | `18_dictionaries.pl` | `dictionary_table_name/2` |
| `20_arrivals.pl` | `0_storage_context.pl` | `quote_ident/2`, `table_name/2` |
| `20_arrivals.pl` | `21_edge_rules.pl` | `nth1_list/3` |
| `21_edge_rules.pl` | `0_storage_context.pl` | `departure_frontier_table_name/2`, `frontier_table_name/2`, `quote_ident/2`, `table_name/2`, `trigger_read_mode/3` |
| `21_edge_rules.pl` | `15_head_select.pl` | `head_select_list/8`, `intern_write_statements/4`, `list_intern_statements/4` |
| `21_edge_rules.pl` | `16_interning.pl` | `column_encoding/3` |
| `21_edge_rules.pl` | `19_relation_values.pl` | `json_decode_goal/3` |
| `21_edge_rules.pl` | `20_arrivals.pl` | `placeholders/2` |
| `21_edge_rules.pl` | `29_json_decode.pl` | `compile_json_decodes/7` |
| `21_edge_rules.pl` | `2_positive_uses.pl` | `compile_atom_args/8`, `compile_positive_uses/7`, `from_parts_sql/2` |
| `21_edge_rules.pl` | `3_negative_uses.pl` | `compile_negative_uses/5` |
| `21_edge_rules.pl` | `4_head_expr.pl` | `compile_guard_goals/5` |
| `22_level_rules.pl` | `0_storage_context.pl` | `quote_ident/2`, `table_name/2` |
| `22_level_rules.pl` | `23_avg_accumulator.pl` | `level_aggregate_sql/5`, `level_avg_sql/5` |
| `22_level_rules.pl` | `25_ref_counts.pl` | `level_ref_count_sql/5` |
| `22_level_rules.pl` | `30_aggregate_heads.pl` | `level_delta_insert_sql/6` |
| `23_avg_accumulator.pl` | `0_storage_context.pl` | `delta_table_name/2`, `quote_ident/2`, `table_name/2` |
| `23_avg_accumulator.pl` | `24_aggregate_scope.pl` | `aggregate_delete_scoped_sql/5`, `aggregate_scope_columns/5`, `aggregate_scope_seed_sql/6`, `avg_accumulator_key_columns/3` |
| `23_avg_accumulator.pl` | `28_fixpoint_ir.pl` | `compile_body_guards/6` |
| `23_avg_accumulator.pl` | `2_positive_uses.pl` | `compile_atom_args/8`, `compile_positive_uses/7`, `from_parts_sql/2` |
| `23_avg_accumulator.pl` | `30_aggregate_heads.pl` | `aggregate_group_exprs/4` |
| `23_avg_accumulator.pl` | `3_negative_uses.pl` | `compile_negative_uses/5` |
| `23_avg_accumulator.pl` | `4_head_expr.pl` | `compile_expr/7` |
| `24_aggregate_scope.pl` | `0_storage_context.pl` | `delta_table_name/2`, `quote_ident/2`, `table_name/2` |
| `24_aggregate_scope.pl` | `17_ddl.pl` | `column_def/4` |
| `24_aggregate_scope.pl` | `21_edge_rules.pl` | `nth1_list/3` |
| `24_aggregate_scope.pl` | `23_avg_accumulator.pl` | `aggregate_scope_table_name/2`, `avg_accumulator_table_name/2` |
| `24_aggregate_scope.pl` | `28_fixpoint_ir.pl` | `compile_body_guards/6` |
| `24_aggregate_scope.pl` | `2_positive_uses.pl` | `compile_atom_args/8`, `compile_positive_uses/7`, `from_parts_sql/2` |
| `24_aggregate_scope.pl` | `30_aggregate_heads.pl` | `aggregate_group_exprs/4`, `aggregate_group_positions/2`, `aggregate_select_statement/9` |
| `24_aggregate_scope.pl` | `3_negative_uses.pl` | `compile_negative_uses/5` |
| `25_ref_counts.pl` | `0_storage_context.pl` | `delta_table_name/2`, `frontier_mode/1`, `frontier_table_name/2`, `next_frontier_table_name/2`, `quote_ident/2`, `ref_count_table_name/2`, `shared_frontier_relation_id/2`, `table_name/2` |
| `25_ref_counts.pl` | `26_expand.pl` | `level_expand_plan/5` |
| `25_ref_counts.pl` | `27_dred.pl` | `level_dred_plan/5` |
| `25_ref_counts.pl` | `28_fixpoint_ir.pl` | `empty_recursive_anchor/2`, `level_fixpoint_ir/5`, `level_ref_count_arm/5`, `qualified_equalities/4`, `rules_read_head_recursively/2` |
| `26_expand.pl` | `0_storage_context.pl` | `quote_ident/2`, `ref_count_table_name/2`, `table_name/2` |
| `26_expand.pl` | `25_ref_counts.pl` | `fixpoint_round_cap/1` |
| `26_expand.pl` | `28_fixpoint_ir.pl` | `level_recursive_arm/4`, `qualified_equalities/4` |
| `27_dred.pl` | `0_storage_context.pl` | `delta_table_name/2`, `quote_ident/2`, `table_name/2` |
| `27_dred.pl` | `25_ref_counts.pl` | `arrival_scratch_table_name/2` |
| `27_dred.pl` | `28_fixpoint_ir.pl` | `level_recursive_arm_parts/9`, `qualified_equalities/4` |
| `27_dred.pl` | `2_positive_uses.pl` | `from_parts_sql/2` |
| `27_dred.pl` | `30_aggregate_heads.pl` | `dictionary_use/1`, `is_negative_use/1` |
| `28_fixpoint_ir.pl` | `0_storage_context.pl` | `quote_ident/2`, `table_name/2` |
| `28_fixpoint_ir.pl` | `14_guards_and_comparisons.pl` | `regexp_goal/1` |
| `28_fixpoint_ir.pl` | `15_head_select.pl` | `head_select_list/8`, `intern_write_statements/4`, `list_intern_statements/4` |
| `28_fixpoint_ir.pl` | `16_interning.pl` | `interned_column/2`, `string_dictionary_table/1` |
| `28_fixpoint_ir.pl` | `19_relation_values.pl` | `is_decode_goal/1` |
| `28_fixpoint_ir.pl` | `1_pattern_args.pl` | `bound_lookup/3` |
| `28_fixpoint_ir.pl` | `27_dred.pl` | `dred_plan_admissible/1` |
| `28_fixpoint_ir.pl` | `29_json_decode.pl` | `compile_json_decodes/7` |
| `28_fixpoint_ir.pl` | `2_positive_uses.pl` | `compile_positive_uses/7`, `compile_positive_uses/8`, `from_parts_sql/2` |
| `28_fixpoint_ir.pl` | `30_aggregate_heads.pl` | `aggregate_select_statement/9` |
| `28_fixpoint_ir.pl` | `3_negative_uses.pl` | `compile_coalesce_recount_markers/3`, `compile_negative_uses/5` |
| `28_fixpoint_ir.pl` | `4_head_expr.pl` | `arithmetic_expr/4`, `arithmetic_result_type/4`, `compile_expr/7`, `compile_guard_goals/5` |
| `29_json_decode.pl` | `0_storage_context.pl` | `sql_literal/2` |
| `29_json_decode.pl` | `1_pattern_args.pl` | `aligned_pair/6`, `bound_lookup/3` |
| `30_aggregate_heads.pl` | `0_storage_context.pl` | `delta_table_name/2`, `frontier_table_name/2`, `quote_ident/2`, `sql_literal/2`, `table_name/2` |
| `30_aggregate_heads.pl` | `15_head_select.pl` | `head_select_list/8`, `intern_write_statements/4`, `list_intern_statements/4` |
| `30_aggregate_heads.pl` | `16_interning.pl` | `column_encoding/3`, `interned_id_sql/2`, `string_dictionary_table/1` |
| `30_aggregate_heads.pl` | `21_edge_rules.pl` | `reference_target_ref/2` |
| `30_aggregate_heads.pl` | `28_fixpoint_ir.pl` | `compile_body_guards/6`, `group_expr/4` |
| `30_aggregate_heads.pl` | `2_positive_uses.pl` | `compile_atom_args/8`, `compile_positive_uses/7`, `from_parts_sql/2` |
| `30_aggregate_heads.pl` | `3_negative_uses.pl` | `compile_negative_atom_args/7`, `compile_negative_uses/5` |
| `30_aggregate_heads.pl` | `4_head_expr.pl` | `compile_expr/7` |
| `31_deltas_and_order.pl` | `0_storage_context.pl` | `delta_table_name/2`, `departure_frontier_table_name/2`, `frontier_mode/1`, `frontier_table_name/2`, `next_frontier_table_name/2`, `pre_table_name/2`, `quote_ident/2`, `ref_count_table_name/2`, +3 more |
| `31_deltas_and_order.pl` | `16_interning.pl` | `any_interned_column/2`, `text_read_table/4`, `text_view_ddls/6` |
| `31_deltas_and_order.pl` | `18_dictionaries.pl` | `dictionary_render_expr/3`, `list_column_alias/2`, `list_column_joins/3` |
| `31_deltas_and_order.pl` | `21_edge_rules.pl` | `nth1_list/3` |
| `31_deltas_and_order.pl` | `25_ref_counts.pl` | `arrival_scratch_table_name/2` |
| `31_deltas_and_order.pl` | `26_expand.pl` | `expand_table_name/3` |
| `31_deltas_and_order.pl` | `27_dred.pl` | `dred_cone_table_name/2`, `dred_ping_table_name/2`, `dred_pong_table_name/2` |
| `31_deltas_and_order.pl` | `5_catalog_ddl.pl` | `set_rel_key_positions/6` |
| `32_boot.pl` | `0_storage_context.pl` | `quote_ident/2`, `table_name/2` |
| `32_boot.pl` | `16_interning.pl` | `interned_column/2`, `string_dictionary_table/1` |
| `33_top_level.pl` | `0_storage_context.pl` | `frontier_mode/1`, `shared_frontier_ddl/1`, `with_shared_frontier_ids/2`, `with_storage_context/2` |
| `33_top_level.pl` | `16_interning.pl` | `literal_seed_ddl/3`, `program_intern_ddl/3` |
| `33_top_level.pl` | `18_dictionaries.pl` | `dictionary_relplans/2`, `list_view_ddls/3` |
| `33_top_level.pl` | `19_relation_values.pl` | `expand_decode_rules/4`, `expand_relation_pattern_rules/4` |
| `33_top_level.pl` | `22_level_rules.pl` | `level_statement_groups/4` |
| `33_top_level.pl` | `31_deltas_and_order.pl` | `audit_scan_index_ddls/5`, `query_order_index_ddls/6`, `retention_statements/3` |
| `33_top_level.pl` | `4_head_expr.pl` | `tick_table_ddl/1` |
| `33_top_level.pl` | `5_catalog_ddl.pl` | `acyclic_guard_ddl/3`, `catalog_row_ddl/10`, `catalog_table_ddl/1` |
| `34_write_verbs.pl` | `0_storage_context.pl` | `delta_table_name/2`, `frontier_mode/1`, `frontier_table_name/2`, `next_frontier_table_name/2`, `quote_ident/2`, `shared_frontier_relation_id/3`, `shared_frontier_table/1`, `shared_next_frontier_table/1`, +2 more |
| `34_write_verbs.pl` | `32_boot.pl` | `boot_seed_statement/6` |
| `34_write_verbs.pl` | `33_top_level.pl` | `lower_program/2` |
| `34_write_verbs.pl` | `5_catalog_ddl.pl` | `rule_body_of/2` |

154 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_storage_context.pl` | the thread-local storage context, frontier mode, shared frontier ids and DDL, every table name and the sql quoting |
| `1_pattern_args.pl` | the pattern-argument compiler for level rule bodies |
| `2_positive_uses.pl` | positive body-atom compilation: joins, coalesced uses, FROM parts, old-state reads and seeded pre uses |
| `3_negative_uses.pl` | NOT EXISTS compilation and the coalesce recount markers |
| `4_head_expr.pl` | head expression compilation, shared by both rule kinds, and the expression-lift guard and bind goals |
| `5_catalog_ddl.pl` | the catalog DDL contract, set-rel tables and keys, option-some tables, acyclic guards and the rel and rule hashes |
| `6_catalog_rows.pl` | the catalog row entry points and the type row families |
| `7_catalog_planes.pl` | the plane rows: rel, departure, pre, view, dict, level and port planes, plus the storage rows |
| `8_catalog_decls.pl` | decl rows and type metadata rows |
| `9_semantic_ids.pl` | semantic type ids and every metadata row family they annotate |
| `10_module_rels.pl` | catalog rel plans and the per-module rel column view |
| `11_module_map.pl` | spliced module rows, the rel-to-module map and module edge rows |
| `12_catalog_lists.pl` | list type rows, the list and rel id maps and the rel rows |
| `13_catalog_paths.pl` | rel scope, the path tree and room rows, column rows and the catalog text sql |
| `14_guards_and_comparisons.pl` | one guard goal, regexp goals, and comparison sql with its no-coercions type check |
| `15_head_select.pl` | the head select list and the intern write statements it splits out |
| `16_interning.pl` | intern mode, text constants in the id space, the decode view and the ingest door's intern plan |
| `17_ddl.pl` | the relation DDL |
| `18_dictionaries.pl` | dictionary tables, relation reference projection and decode/2 as a dictionary join |
| `19_relation_values.pl` | relation-value terms lowered as dictionary joins |
| `20_arrivals.pl` | the arrival statement templates |
| `21_edge_rules.pl` | edge rule lowering |
| `22_level_rules.pl` | level rule lowering and its statement groups |
| `23_avg_accumulator.pl` | the incremental avg accumulator: its seed, body and delta rows, scoped inserts and deletes |
| `24_aggregate_scope.pl` | the aggregate scope table: its DDL, seed sql, scoped insert and delete, and the accumulator columns |
| `25_ref_counts.pl` | refCount sql, the refCount plan, frontier staging and the counted and recursive seeds |
| `26_expand.pl` | the level expand plan with its seed, hop and absorb sql |
| `27_dred.pl` | in-place recursive-head maintenance |
| `28_fixpoint_ir.pl` | the backend-neutral fixpoint IR |
| `29_json_decode.pl` | decode/2 over a json column, lowered to json1 sql |
| `30_aggregate_heads.pl` | aggregate heads |
| `31_deltas_and_order.pl` | the per-rel delta statements and the `?` order tails |
| `32_boot.pl` | boot seeding |
| `33_top_level.pl` | lower_program/2 and the plan term it returns |
| `34_write_verbs.pl` | the six write verbs |
