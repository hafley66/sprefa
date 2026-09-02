# v6/prolog/0_dot_expand.pl -> v6/prolog/0_dot_expand/

module head keeps lines 1..76 (76 lines): 9 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_qualified_types.pl` | 147 | 77-223 | 26 | 13 |
| `1_rel_paths.pl` | 140 | 224-363 | 28 | 15 |
| `2_nested_captures.pl` | 150 | 364-513 | 24 | 16 |
| `3_capture_body.pl` | 74 | 514-587 | 12 | 6 |
| `4_dot_rules.pl` | 168 | 588-755 | 25 | 15 |
| `5_body_vars.pl` | 81 | 756-836 | 24 | 9 |
| **total** | **760** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

none

## cross-part call edges

| from | to | callees |
|---|---|---|
| `0_qualified_types.pl` | `1_rel_paths.pl` | `contains_rel_path/1`, `decl_scope_tree/2`, `resolve_path/3` |
| `0_qualified_types.pl` | `2_nested_captures.pl` | `expand_nested_parent_refs/4` |
| `2_nested_captures.pl` | `1_rel_paths.pl` | `decl_scope_tree/2`, `descend/3` |
| `2_nested_captures.pl` | `3_capture_body.pl` | `body_parent_term/5`, `capture_body/4` |
| `3_capture_body.pl` | `5_body_vars.pl` | `conjunction_goals/2`, `goals_conjunction/2` |
| `4_dot_rules.pl` | `5_body_vars.pl` | `bound_body_vars/2`, `conjunction_goals/2`, `contains_dot_get/1`, `goals_conjunction/2`, `memberchk_eq/2`, `plain_relation_goal/1` |

6 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_qualified_types.pl` | the entry point, qualified type path resolution, minted type decls and enum arm refs |
| `1_rel_paths.pl` | rel path rewriting, the decl scope tree and the path collision check |
| `2_nested_captures.pl` | nested parent refs, capture shapes, parent column insertion and the capture rule, arrow and head forms |
| `3_capture_body.pl` | parent atoms inside a body and the captured body rewrite |
| `4_dot_rules.pl` | desugaring a dot rule, rewriting head and goals, replacing dot gets and checking the receiver |
| `5_body_vars.pl` | bound body variables, binding positions and conjunction/goal-list conversion |
# v6/prolog/0_program_check.pl -> v6/prolog/0_program_check/

module head keeps lines 1..38 (38 lines): 9 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_lookups.pl` | 62 | 39-100 | 15 | 9 |
| `1_violations.pl` | 586 | 101-771 * | 38 | 1 |
| `2_violation_helpers.pl` | 85 | relocated * | 26 | 13 |
| `3_aggregates_and_types.pl` | 104 | 772-875 | 20 | 14 |
| `4_column_variables.pl` | 111 | 876-986 | 16 | 9 |
| **total** | **948** | | | |

`*` = the span plus or minus a relocation:

| predicate | lines | moves to | lands after |
|---|---|---|---|
| `cst_regexp_pattern/2` | 230-234 | `2_violation_helpers.pl` | first in the helpers part |
| `ast_capture_names/2 .. regexp_pattern_pcre_error/2` | 275-334 | `2_violation_helpers.pl` | cst_regexp_pattern/2, in file order |
| `anonymous_column_type/1 + declared_template_application/2` | 365-375 | `2_violation_helpers.pl` | the ast and regexp group, in file order |
| `declared_ref/2` | 413-421 | `2_violation_helpers.pl` | anonymous_column_type/1, in file order |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

none

## cross-part call edges

| from | to | callees |
|---|---|---|
| `0_lookups.pl` | `1_violations.pl` | `program_violation/3` |
| `1_violations.pl` | `0_lookups.pl` | `aggregate_head_ref/2`, `declared_kind/3`, `head_ref/2`, `level_headed/2`, `ordered_aggregate_name/1`, `relation_kind/3` |
| `1_violations.pl` | `2_violation_helpers.pl` | `anonymous_column_type/1`, `ast_capture_names/2`, `cst_regexp_pattern/2`, `declared_ref/2`, `declared_template_application/2`, `regexp_pattern_outside_subset/1`, `regexp_pattern_pcre_error/2` |
| `1_violations.pl` | `3_aggregates_and_types.pl` | `declared_column_type_use/2`, `declared_relation/2`, `headed_relation/2`, `implemented_aggregates/1`, `number_column_type/2`, `numeric_aggregate_operand/3`, `relation_value_in_ref_column/7`, `rule_body/2`, +3 more |
| `1_violations.pl` | `4_column_variables.pl` | `column_type_assignable/3`, `declared_column_table/4`, `relation_argument_violation/6`, `rule_body_column_variable/6`, `rule_column_variable/7`, `rule_head_column_variable/6` |
| `3_aggregates_and_types.pl` | `0_lookups.pl` | `head_ref/2` |
| `4_column_variables.pl` | `3_aggregates_and_types.pl` | `body_relation_atom/2`, `rule_body/2`, `rule_relation_atom/2` |

7 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_lookups.pl` | first_violation/3 and the small decl readers the violation clauses all call |
| `1_violations.pl` | every program_violation/3 clause, all 38 of them, contiguous once the four helper groups move out |
| `2_violation_helpers.pl` | the cst regexp, ast capture, anonymous column type and declared_ref helpers that the violation clauses call and that used to sit between them |
| `3_aggregates_and_types.pl` | numeric aggregate operands, the implemented aggregate roster, declared column type uses and the rule atom readers |
| `4_column_variables.pl` | the declared column table, head and body column variables, storage assignability and relation argument violations |
# v6/prolog/0_type_plane.pl -> v6/prolog/0_type_plane/

module head keeps lines 1..66 (66 lines): 5 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_definitions.pl` | 129 | 67-195 | 31 | 9 |
| `1_relation_shape.pl` | 122 | 196-317 | 17 | 11 |
| `2_type_order.pl` | 99 | 318-416 | 12 | 7 |
| `3_canonicalize.pl` | 157 | 417-573 | 20 | 13 |
| `4_row_violations.pl` | 181 | 574-754 | 32 | 12 |
| `5_type_json.pl` | 284 | 755-1038 | 47 | 23 |
| **total** | **972** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

none

## cross-part call edges

| from | to | callees |
|---|---|---|
| `1_relation_shape.pl` | `0_definitions.pl` | `declared_type_name/2`, `type_definition/4` |
| `2_type_order.pl` | `0_definitions.pl` | `declared_type_name/2`, `type_definition/4` |
| `2_type_order.pl` | `1_relation_shape.pl` | `type_ref_columns/3` |
| `2_type_order.pl` | `3_canonicalize.pl` | `json_object_value/2` |
| `2_type_order.pl` | `4_row_violations.pl` | `bool_value/1`, `finite_float/1` |
| `3_canonicalize.pl` | `0_definitions.pl` | `declared_type_name/2`, `type_definition/4`, `type_definitions/2` |
| `3_canonicalize.pl` | `1_relation_shape.pl` | `relation_columns_and_types/5`, `relation_value_object/4` |
| `3_canonicalize.pl` | `2_type_order.pl` | `type_topological_order/2` |
| `3_canonicalize.pl` | `4_row_violations.pl` | `bare_row/2` |
| `3_canonicalize.pl` | `5_type_json.pl` | `ref_column_names/4`, `type_field_values/4` |
| `4_row_violations.pl` | `0_definitions.pl` | `declared_type_name/2`, `type_definitions/2` |
| `4_row_violations.pl` | `2_type_order.pl` | `type_shape_error/4` |
| `4_row_violations.pl` | `5_type_json.pl` | `ref_column_names/4` |
| `5_type_json.pl` | `0_definitions.pl` | `declared_type_name/2`, `type_definition/4` |
| `5_type_json.pl` | `3_canonicalize.pl` | `json_object_value/2` |

15 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_definitions.pl` | type definitions, declared type names, column storage, list element types and the wrapper unwrapping |
| `1_relation_shape.pl` | ref columns, relation columns and their types, and a relation value as a term or an object |
| `2_type_order.pl` | the topological order over declared types, the cycle witness, and type and field shape errors |
| `3_canonicalize.pl` | world row canonicalization, reference target normalization, and canonical struct and field values |
| `4_row_violations.pl` | row shape violations, position column names, the wide integer witness and column value shape errors |
| `5_type_json.pl` | ref column names, type field values and the canonical json renderer with its js float formatting |
# v6/prolog/ARCH.pl -> v6/prolog/ARCH/

module head keeps lines 1..149 (149 lines): 1 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_species.pl` | 218 | 150-367 | 84 | 8 |
| `1_constructs.pl` | 94 | 368-461 | 48 | 3 |
| `2_covers.pl` | 156 | 462-617 | 117 | 2 |
| `3_forks.pl` | 66 | 618-683 | 16 | 1 |
| `4_tasks.pl` | 288 | 684-971 | 265 | 1 |
| `5_gate.pl` | 56 | 972-1027 | 13 | 5 |
| **total** | **878** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

| line | directive | part it falls in |
|---|---|---|
| 315 | `:- use_module('src/kernel.pl')` | `0_species.pl` |
| 366 | `:- use_module('conformance/rulings.pl')` | `0_species.pl` |
| 602 | `:- dynamic arch_dir/1` | `2_covers.pl` |
| 603 | `:- prolog_load_context(directory,_970874),asserta(arch_dir(_970874))` | `2_covers.pl` |
| 1007 | `:- use_module('src/grader',[run/1])` | `5_gate.pl` |

Each one moves up into the module head file, above the includes.

## cross-part call edges

| from | to | callees |
|---|---|---|
| `5_gate.pl` | `0_species.pl` | `refines/2` |
| `5_gate.pl` | `4_tasks.pl` | `task/3` |

2 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_species.pl` | the graph, refines, species, algorithm, prior_art, capability, tech and technique rows |
| `1_constructs.pl` | the construct roster with its status and tier vocabularies |
| `2_covers.pl` | which construct each endpoint covers, and the endpoint existence check |
| `3_forks.pl` | the open design fork rows |
| `4_tasks.pl` | the task rows |
| `5_gate.pl` | roadmap, topsort, the check rows and go/0 |
# v6/prolog/analyze.pl -> v6/prolog/analyze/

module head keeps lines 1..52 (52 lines): 14 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_rel_and_rule_shape.pl` | 58 | 53-110 | 15 | 11 |
| `1_body_walk.pl` | 79 | 111-189 | 15 | 10 |
| `2_guard_goals.pl` | 99 | 190-288 | 11 | 9 |
| `3_ref_inventory.pl` | 43 | 289-331 | 4 | 4 |
| `4_column_names.pl` | 133 | 332-464 | 20 | 14 |
| `5_literal_types.pl` | 94 | 465-558 | 12 | 6 |
| `6_program_types.pl` | 109 | 559-667 | 10 | 7 |
| `7_type_fixpoint.pl` | 168 | 668-835 | 26 | 15 |
| `8_expression_types.pl` | 176 | 836-1011 | 62 | 13 |
| `9_edge_shape.pl` | 175 | 1012-1186 | 29 | 10 |
| `10_edge_head_types.pl` | 59 | 1187-1245 | 2 | 2 |
| `11_subset_gate.pl` | 201 | 1246-1446 | 32 | 4 |
| `12_rule_observers.pl` | 138 | 1447-1584 | 17 | 11 |
| `13_shape_checks.pl` | 308 | 1585-1892 | 30 | 17 |
| **total** | **1840** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

| line | directive | part it falls in |
|---|---|---|
| 109 | `:- table body_ref_uses/2` | `0_rel_and_rule_shape.pl` |

Each one moves up into the module head file, above the includes.

## cross-part call edges

| from | to | callees |
|---|---|---|
| `1_body_walk.pl` | `9_edge_shape.pl` | `conjunction_goals/2` |
| `2_guard_goals.pl` | `0_rel_and_rule_shape.pl` | `rule_body/2`, `rule_head/2` |
| `2_guard_goals.pl` | `9_edge_shape.pl` | `conjunction_goals/2` |
| `3_ref_inventory.pl` | `0_rel_and_rule_shape.pl` | `derived_refs/2`, `rule_body/2`, `rule_head_ref/2` |
| `3_ref_inventory.pl` | `1_body_walk.pl` | `body_ref_uses/2` |
| `4_column_names.pl` | `0_rel_and_rule_shape.pl` | `rule_body/2`, `rule_head/2` |
| `4_column_names.pl` | `1_body_walk.pl` | `body_ref_uses/2` |
| `5_literal_types.pl` | `8_expression_types.pl` | `column_source_args/5` |
| `6_program_types.pl` | `0_rel_and_rule_shape.pl` | `rule_body/2`, `rule_head/2`, `rule_head_ref/2` |
| `6_program_types.pl` | `1_body_walk.pl` | `body_ref_uses/2` |
| `6_program_types.pl` | `2_guard_goals.pl` | `tick_goal/2` |
| `6_program_types.pl` | `5_literal_types.pl` | `column_type_at_decl/9`, `literal_witness/1`, `literal_witnesses_type/2` |
| `6_program_types.pl` | `7_type_fixpoint.pl` | `column_type_fixpoint/5` |
| `6_program_types.pl` | `8_expression_types.pl` | `column_source_args/5` |
| `6_program_types.pl` | `9_edge_shape.pl` | `conjunction_goals/2` |
| `7_type_fixpoint.pl` | `0_rel_and_rule_shape.pl` | `rule_body/2`, `rule_head/2`, `rule_head_ref/2` |
| `7_type_fixpoint.pl` | `1_body_walk.pl` | `body_ref_uses/2` |
| `7_type_fixpoint.pl` | `2_guard_goals.pl` | `bind_goal/3`, `body_guard_goals/2` |
| `7_type_fixpoint.pl` | `8_expression_types.pl` | `expression_type/3`, `merge_contribution_lists/3` |
| `7_type_fixpoint.pl` | `9_edge_shape.pl` | `conjunction_goals/2` |
| `8_expression_types.pl` | `4_column_names.pl` | `ref_occurrence_args/3` |
| `8_expression_types.pl` | `7_type_fixpoint.pl` | `environment_lookup/3` |
| `9_edge_shape.pl` | `2_guard_goals.pl` | `guard_or_bind_goal/1`, `tick_goal/2` |
| `10_edge_head_types.pl` | `0_rel_and_rule_shape.pl` | `rule_is_edge/1` |
| `10_edge_head_types.pl` | `9_edge_shape.pl` | `edge_trigger_shape/2` |
| `11_subset_gate.pl` | `0_rel_and_rule_shape.pl` | `rule_is_edge/1`, `rule_is_level/1` |
| `11_subset_gate.pl` | `12_rule_observers.pl` | `check_edge_rule_shape/1` |
| `11_subset_gate.pl` | `13_shape_checks.pl` | `check_level_rule_shape/1`, `check_no_compound_pattern_on_arrival_rel/2`, `check_no_edge_head_conflict_risk/2`, `reserved_construct_name/3` |
| `12_rule_observers.pl` | `0_rel_and_rule_shape.pl` | `rule_body/2`, `rule_head_ref/2`, `rule_is_edge/1`, `rule_is_level/1` |
| `12_rule_observers.pl` | `13_shape_checks.pl` | `head_arithmetic_shape/2`, `rule_is_aggregate/1` |
| `12_rule_observers.pl` | `1_body_walk.pl` | `body_ref_uses/2` |
| `12_rule_observers.pl` | `9_edge_shape.pl` | `edge_trigger_shape/2` |
| `13_shape_checks.pl` | `0_rel_and_rule_shape.pl` | `decl_key/3`, `rule_body/2`, `rule_head_ref/2`, `rule_is_level/1` |
| `13_shape_checks.pl` | `12_rule_observers.pl` | `level_body_pre_ref/2` |
| `13_shape_checks.pl` | `1_body_walk.pl` | `body_ref_uses/2` |
| `13_shape_checks.pl` | `2_guard_goals.pl` | `body_guard_goals/2` |
| `13_shape_checks.pl` | `3_ref_inventory.pl` | `arrival_target_refs/2` |
| `13_shape_checks.pl` | `9_edge_shape.pl` | `conjunction_goals/2`, `edge_trigger_shape/2`, `shape_trigger_refs/2` |

38 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_rel_and_rule_shape.pl` | rel kind, key and keep readers, edge-vs-level rule shape, and the headed/derived ref lists |
| `1_body_walk.pl` | walking a rule body for the refs it uses, the coalesce output resolution and the event use rows |
| `2_guard_goals.pl` | guard and bind goal classification, tick goals, and whether a program mentions tick or the catalog |
| `3_ref_inventory.pl` | the program-wide ref inventory: seeded, declared, arrival-target and all program refs |
| `4_column_names.pl` | column naming from surface variable identity, ref occurrence args and snake-case folding |
| `5_literal_types.pl` | column type read off a declaration, and the type a concrete literal witnesses |
| `6_program_types.pl` | the driver for program-wide column typing and the seed-row contributions it starts from |
| `7_type_fixpoint.pl` | the contribution fixpoint over rule heads, and the body type environment a rule's goals bind |
| `8_expression_types.pl` | typing an expression, arithmetic result types, and merging two contributions into one column type |
| `9_edge_shape.pl` | edge trigger shape: sampled goals, departure goals and the goals an edge body cannot carry |
| `10_edge_head_types.pl` | edge head column-type consistency across the rules writing one rel |
| `11_subset_gate.pl` | the subset gate: every construct the compiler has not built yet, with the reason term each throws |
| `12_rule_observers.pl` | which rules read which rel, self-read one-pass closure, and the edge rule shape check |
| `13_shape_checks.pl` | reserved constructs in a body, head conflict risk, compound patterns on arrival rels, and the level and aggregate rule shape checks |
# v6/prolog/compile.pl -> v6/prolog/compile_pl/

module head keeps lines 1..79 (79 lines): 28 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_fixtures.pl` | 58 | 80-137 | 2 | 2 |
| `1_program_plan.pl` | 219 | 138-356 | 18 | 14 |
| `2_reserved_namespace.pl` | 47 | 357-403 | 6 | 6 |
| `3_fixture_entry.pl` | 39 | 404-442 | 8 | 5 |
| `4_storage_names.pl` | 203 | 443-645 | 37 | 21 |
| `5_dl6_door.pl` | 124 | 646-769 | 17 | 12 |
| `6_program_phases.pl` | 99 | 770-868 | 9 | 7 |
| `7_phase_trace.pl` | 130 | 869-998 | 17 | 14 |
| **total** | **919** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

| line | directive | part it falls in |
|---|---|---|
| 317 | `:- meta_predicate check_step(+,0)` | `1_program_plan.pl` |

Each one moves up into the module head file, above the includes.

## cross-part call edges

| from | to | callees |
|---|---|---|
| `1_program_plan.pl` | `2_reserved_namespace.pl` | `check_reserved_namespace/1` |
| `1_program_plan.pl` | `3_fixture_entry.pl` | `check_single_arity_per_name/1`, `check_world_shapes/3` |
| `1_program_plan.pl` | `4_storage_names.pl` | `relation_shapes/5`, `relation_storage_names/6` |
| `3_fixture_entry.pl` | `0_fixtures.pl` | `read_fixture_term/4` |
| `3_fixture_entry.pl` | `1_program_plan.pl` | `default_intern_mode/1`, `throw_as_compiler_unsupported/1` |
| `3_fixture_entry.pl` | `6_program_phases.pl` | `compile_program/7` |
| `4_storage_names.pl` | `1_program_plan.pl` | `throw_as_compiler_unsupported/1` |
| `4_storage_names.pl` | `2_reserved_namespace.pl` | `compiler_owned_contract/1`, `reserved_namespace_name/1` |
| `5_dl6_door.pl` | `1_program_plan.pl` | `default_intern_mode/1` |
| `5_dl6_door.pl` | `6_program_phases.pl` | `compile_program_phases/8`, `throw_text_door_error/2` |
| `5_dl6_door.pl` | `7_phase_trace.pl` | `parse_debug/2`, `run_compile_phase/4`, `write_compile_trace/2` |
| `6_program_phases.pl` | `1_program_plan.pl` | `default_intern_mode/1`, `program_plan/3` |
| `6_program_phases.pl` | `7_phase_trace.pl` | `boot_debug/2`, `emit_debug/2`, `lower_debug/4`, `run_compile_phase/4`, `with_emit_context/3`, `write_compile_trace/2` |

13 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_fixtures.pl` | reading one fixture term out of a fixture file, and finding a fixture by name |
| `1_program_plan.pl` | program_plan/3, the one term lower.pl and emit_ts.pl both read, plus the compiler-type-rule partition, reference-target materialization and the plan debug dumps |
| `2_reserved_namespace.pl` | the compiler-owned __ namespace: which names are reserved and what a violation reads as |
| `3_fixture_entry.pl` | the compile_fixture entry points, world shape checks and the single-arity-per-name check |
| `4_storage_names.pl` | shape identity and storage naming: shape digests, declaring-module stems, ascii folding and unique suffix allocation |
| `5_dl6_door.pl` | the .dl6 text door: emitter and schedule options, arrival terms, seeded forms and the fact partition |
| `6_program_phases.pl` | compile_program and the phase pipeline that runs parse, lower, boot and emit, and writes the compiled output |
| `7_phase_trace.pl` | phase measurement, the per-phase debug hooks and the compile trace file |
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
# v6/prolog/compile/parse_dl_dcg.pl -> v6/prolog/compile/parse_dl_dcg/

module head keeps lines 1..46 (46 lines): 12 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_cst_shapes.pl` | 62 | 47-108 | 48 | 7 |
| `1_entry.pl` | 271 | 109-379 | 49 | 31 |
| `2_lexer.pl` | 140 | 380-517 * | 40 | 28 |
| `3_use_and_router.pl` | 56 | 518-573 | 8 | 7 |
| `4_rel_decl.pl` | 447 | 574-1020 | 82 | 55 |
| `5_name_resolution.pl` | 115 | 1021-1135 | 17 | 13 |
| `6_host_and_template.pl` | 40 | 1136-1177 * | 12 | 8 |
| `7_query_and_match.pl` | 63 | 1178-1240 | 11 | 9 |
| `8_rule_and_args.pl` | 153 | 1241-1393 | 29 | 19 |
| `9_body.pl` | 200 | 1394-1593 | 48 | 27 |
| `10_expr.pl` | 184 | 1594-1777 | 42 | 28 |
| **total** | **1731** | | | |

`*` = the span plus or minus a relocation:

| predicate | lines | moves to | lands after |
|---|---|---|---|
| `lex_token/2` | 1163-1164 | `2_lexer.pl` | the lex_token/2 clause at :476, keeping the three rows in file order |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

none

## cross-part call edges

| from | to | callees |
|---|---|---|
| `1_entry.pl` | `4_rel_decl.pl` | `module_path_name/2` |
| `1_entry.pl` | `5_name_resolution.pl` | `normalize_relation_value_decls/2`, `resolve_module_path_collisions/2` |
| `1_entry.pl` | `9_body.pl` | `partition_hiv/5`, `split_probe_values/4` |
| `2_lexer.pl` | `1_entry.pl` | `mark/1` |
| `3_use_and_router.pl` | `1_entry.pl` | `remaining_line_column/3` |
| `3_use_and_router.pl` | `9_body.pl` | `annotate_cst_item/3` |
| `4_rel_decl.pl` | `0_cst_shapes.pl` | `record_cols/2`, `record_host_path/2`, `record_host_signature/3`, `unsupported/1` |
| `4_rel_decl.pl` | `1_entry.pl` | `parse_failure/1` |
| `4_rel_decl.pl` | `2_lexer.pl` | `skip_to_eol/2` |
| `4_rel_decl.pl` | `5_name_resolution.pl` | `scalar_column_type/1` |
| `4_rel_decl.pl` | `6_host_and_template.pl` | `specs_to_columns/2` |
| `5_name_resolution.pl` | `0_cst_shapes.pl` | `lookup_column_order/2`, `record_cols/2` |
| `5_name_resolution.pl` | `4_rel_decl.pl` | `tag_rel_name/2`, `tree_leaf/3` |
| `6_host_and_template.pl` | `0_cst_shapes.pl` | `unsupported/1` |
| `7_query_and_match.pl` | `1_entry.pl` | `parse_failure/1` |
| `7_query_and_match.pl` | `4_rel_decl.pl` | `module_path_name/2` |
| `7_query_and_match.pl` | `8_rule_and_args.pl` | `path_atom/4`, `variable_source_name/2` |
| `8_rule_and_args.pl` | `0_cst_shapes.pl` | `lookup_column_order/2`, `unsupported/1` |
| `8_rule_and_args.pl` | `4_rel_decl.pl` | `module_path_name/2` |
| `9_body.pl` | `0_cst_shapes.pl` | `unsupported/1` |
| `9_body.pl` | `1_entry.pl` | `map_tree/4`, `mark/1`, `parse_failure/1` |
| `9_body.pl` | `2_lexer.pl` | `ws/2` |
| `9_body.pl` | `4_rel_decl.pl` | `module_path_name/2` |
| `9_body.pl` | `8_rule_and_args.pl` | `path_atom/4`, `resolve_named_args/4` |
| `10_expr.pl` | `2_lexer.pl` | `get_or_make_var/2`, `hole_var/2`, `ident/3` |
| `10_expr.pl` | `9_body.pl` | `longest_first/2` |

26 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_cst_shapes.pl` | the editor CST shape and origin tables, and the thread-local recorders the passes write into |
| `1_entry.pl` | the four entry points, the two-pass driver, parse marks, line/column reporting for a reason, statement source refs, and host path flattening |
| `2_lexer.pl` | whitespace and comments, the @ ~ # sigil operators, identifiers, int/float/atom/string literals, escape decoding, variable holes, and all three lex_token/2 rows |
| `3_use_and_router.pl` | use/import items and statement//5, the router that picks rel, query, match or rule |
| `4_rel_decl.pl` | the whole rel declaration grammar: nested rels, arrival tails, generic parameters, interfaces, type expressions, enums, keep/key clauses and the decl-b column tail |
| `5_name_resolution.pl` | the post-parse name passes: module path collisions, reserved names, minted names, relation-value decl normalization |
| `6_host_and_template.pl` | the removed sh/bind statements, host output column specs, and template literals |
| `7_query_and_match.pl` | the ? query statement with its order tail, and match statements with their arms |
| `8_rule_and_args.pl` | rule statements, head atoms, and named/positional argument resolution including keyword puns |
| `9_body.pl` | rule bodies: body items, cst query items, balanced-bracket scanning, rel atom terms and infix items |
| `10_expr.pl` | the arithmetic tier expression grammar, json literals, dotted and slash paths, brace terms and list terms |
# v6/prolog/print_dl.pl -> v6/prolog/print_dl/

module head keeps lines 1..47 (47 lines): 10 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_entry.pl` | 124 | 48-171 | 5 | 4 |
| `1_decl_order.pl` | 91 | 172-262 | 21 | 8 |
| `2_decl_line.pl` | 129 | 263-391 | 16 | 7 |
| `3_column_types.pl` | 145 | 392-536 | 38 | 13 |
| `4_rule_and_query.pl` | 75 | 537-611 | 14 | 8 |
| `5_body.pl` | 113 | 612-724 | 25 | 11 |
| `6_term.pl` | 129 | 725-853 | 20 | 15 |
| `7_braces_and_quoting.pl` | 53 | 854-906 | 9 | 5 |
| **total** | **859** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

none

## directives sitting below the first anchor

none

## cross-part call edges

| from | to | callees |
|---|---|---|
| `0_entry.pl` | `1_decl_order.pl` | `decl_ref_order/2` |
| `1_decl_order.pl` | `0_entry.pl` | `print_dl_program/3` |
| `2_decl_line.pl` | `3_column_types.pl` | `decl_is_modifier/2`, `print_column_type/2`, `print_enum_variants/2` |
| `3_column_types.pl` | `6_term.pl` | `print_term/5` |
| `4_rule_and_query.pl` | `5_body.pl` | `print_body/3`, `print_body_item/3` |
| `4_rule_and_query.pl` | `6_term.pl` | `print_term/5` |
| `5_body.pl` | `4_rule_and_query.pl` | `print_goal_term/3`, `relation_atom_of_arity_zero/1` |
| `5_body.pl` | `6_term.pl` | `print_term/5` |
| `6_term.pl` | `7_braces_and_quoting.pl` | `print_brace_pair/3`, `quote_value/3` |
| `7_braces_and_quoting.pl` | `6_term.pl` | `print_term/5`, `print_var/3` |

10 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_entry.pl` | the entry points and the block join that assembles a printed program |
| `1_decl_order.pl` | EDB decl synthesis for the text door and the declaration ordering it feeds |
| `2_decl_line.pl` | the rel decl line: arrow columns, modifiers, type applications and template columns |
| `3_column_types.pl` | printing a column type, annotations, decl columns and enum, product and sum fields |
| `4_rule_and_query.pl` | rule lines, query lines with their order tails, and match arms |
| `5_body.pl` | the body, one goal per indented line, surface wrappers and host input interleaving |
| `6_term.pl` | the general term printer: vars, ints, atoms, dot chains, lists and json |
| `7_braces_and_quoting.pl` | brace pairs and the always-explicit quoting |
