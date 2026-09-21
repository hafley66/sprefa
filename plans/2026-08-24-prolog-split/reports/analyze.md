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
