# v6/prolog/0_program_check.pl -> v6/prolog/0_program_check/

module head keeps lines 1..38 (38 lines): 9 directives, 0 stray clauses

| part | lines | span | clauses | predicates |
|---|---:|---|---:|---:|
| `0_lookups.pl` | 62 | 39-100 | 15 | 9 |
| `1_violations_decls.pl` | 312 | 101-412 | 51 | 13 |
| `2_violations_rules.pl` | 359 | 413-771 | 13 | 2 |
| `3_aggregates_and_types.pl` | 104 | 772-875 | 20 | 14 |
| `4_column_variables.pl` | 111 | 876-986 | 16 | 9 |
| **total** | **948** | | | |

parts over 700 lines: none

## clauses of one predicate landing in two parts

| predicate | parts |
|---|---|
| `program_violation/3` | 1_violations_decls.pl, 2_violations_rules.pl |

## directives sitting below the first anchor

none

## cross-part call edges

| from | to | callees |
|---|---|---|
| `0_lookups.pl` | `1_violations_decls.pl` | `program_violation/3` |
| `0_lookups.pl` | `2_violations_rules.pl` | `program_violation/3` |
| `1_violations_decls.pl` | `0_lookups.pl` | `declared_kind/3`, `level_headed/2`, `relation_kind/3` |
| `1_violations_decls.pl` | `2_violations_rules.pl` | `declared_ref/2` |
| `1_violations_decls.pl` | `3_aggregates_and_types.pl` | `declared_column_type_use/2`, `rule_body_goal/2`, `rule_relation_atom/2` |
| `1_violations_decls.pl` | `4_column_variables.pl` | `declared_column_table/4`, `relation_argument_violation/6`, `rule_body_column_variable/6` |
| `2_violations_rules.pl` | `0_lookups.pl` | `aggregate_head_ref/2`, `head_ref/2`, `ordered_aggregate_name/1`, `relation_kind/3` |
| `2_violations_rules.pl` | `3_aggregates_and_types.pl` | `declared_relation/2`, `headed_relation/2`, `implemented_aggregates/1`, `number_column_type/2`, `numeric_aggregate_operand/3`, `relation_value_in_ref_column/7`, `rule_body/2`, `rule_body_goal/2`, +2 more |
| `2_violations_rules.pl` | `4_column_variables.pl` | `column_type_assignable/3`, `declared_column_table/4`, `rule_body_column_variable/6`, `rule_column_variable/7`, `rule_head_column_variable/6` |
| `3_aggregates_and_types.pl` | `0_lookups.pl` | `head_ref/2` |
| `4_column_variables.pl` | `3_aggregates_and_types.pl` | `body_relation_atom/2`, `rule_body/2`, `rule_relation_atom/2` |

11 directed part pairs

## what each part owns

| part | owns |
|---|---|
| `0_lookups.pl` | first_violation/3 and the small decl readers the violation clauses all call |
| `1_violations_decls.pl` | the violation clauses about declarations and patterns, with the cst regexp and ast capture helpers only they use |
| `2_violations_rules.pl` | the violation clauses about rules, reserved carriers and column type conflicts |
| `3_aggregates_and_types.pl` | numeric aggregate operands, the implemented aggregate roster, declared column type uses and the rule atom readers |
| `4_column_variables.pl` | the declared column table, head and body column variables, storage assignability and relation argument violations |
