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
