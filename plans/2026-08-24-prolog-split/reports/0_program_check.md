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
