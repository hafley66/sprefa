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
