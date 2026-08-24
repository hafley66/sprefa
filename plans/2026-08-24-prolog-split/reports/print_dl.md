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
