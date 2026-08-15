# lower.pl arm + throw-site coverage census

One question: which `lower.pl` `unsupported_construct` throw sites and multi-clause
predicate arms does NO corpus program reach? Systematizes the gap the dd arc
caught by hand (ARCH.pl:950 — `mutual_recursion` fired on zero fixtures, which
became the PR #266 silent-wrong).

- Tool: `v6/prolog/compile/scripts/arm_census.pl` + `arm_census.sh`.
- Method: static throw enumeration cross-checked against
  `compile/out/manifest.json` `reason` functors; dynamic per-clause arm reach via
  SWI's own `library(prolog_coverage)` (build-vs-buy — no bespoke instrumentation).
- Corpus: all 448 conformance fixtures compiled through the sweep path.

## Counts

| census | total | reached | unreached |
|---|---|---|---|
| throw sites (`unsupported_construct`) | 50 | 13 | 37 |
| multi-clause predicates | 177 | — | 86 arms (below) |
| single-clause predicates | — | — | 17 fully dead |

Reproducible (two runs, identical):

```
RUN A  total throw sites: 50   reached: 13   unreached: 37
       multi-clause predicates: 177   unreached arms: 86   fully-dead predicates: 17
RUN B  total throw sites: 50   reached: 13   unreached: 37
       multi-clause predicates: 177   unreached arms: 86   fully-dead predicates: 17
```

Conformance battery after instrumentation: **448 PASS / 0 FAIL** unchanged.

## Throw census

A throw is REACHED if its construct functor appears as a `reason` in the
committed manifest (a corpus program threw it during the sweep). Otherwise it is
a hypothesis — a refusal no corpus program exercises, per the repo law that a
refusal is a hypothesis, never an edict.

### Reached (13)

| site | construct |
|---|---|
| lower.pl:346 | join_column_type_mismatch |
| lower.pl:849 | arith_operand_not_number |
| lower.pl:2264 | regexp_operand_not_text |
| lower.pl:2268 | regexp_pattern_not_literal |
| lower.pl:2318 | comparison_type_mismatch |
| lower.pl:3100 | relation_value_in_edge_rule |
| lower.pl:3376 | decode_source_not_struct |
| lower.pl:3379 | column_type_unknown |
| lower.pl:3407 | decode_field_unknown |
| lower.pl:3651 | edge_into_unkeyed_set |
| lower.pl:3831 | trigger_arg_not_var |
| lower.pl:5459 | decode_source_not_struct |
| lower.pl:5601 | json_capture_type_unknown |

### Unreached (37) — each a hypothesis

| site | construct | candidate fixture sketch |
|---|---|---|
| lower.pl:268 | non_finite_float_literal | a float literal `1e400` / `inf` / `nan` |
| lower.pl:310 | pattern_arg | a compound term as a level-rule head pattern arg |
| lower.pl:420 | pre_seed_type_mismatch | a `pre` seed whose value type disagrees with the column |
| lower.pl:436 | pre_seed_no_value | a `pre` seed with no value at all |
| lower.pl:543 | unbound_head_var | a head position left unbound (not bound/check) |
| lower.pl:584 | aggregate_in_expression_position | an aggregate call nested inside a plain expression |
| lower.pl:594 | head_expr | a non-atom/non-var head expression |
| lower.pl:624 | text_operand_not_text | `text` operand fed a non-text column |
| lower.pl:671 | typed_operand_not_int | typed scalar operand fed a non-int column |
| lower.pl:725 | json_operand_not_json | `json` operand fed a non-json column |
| lower.pl:766 | json_value_expression | a JSON value expression in a position that is not lowered |
| lower.pl:797 | json_document_ref_operand | a `ref`-typed operand passed to a json document builder |
| lower.pl:839 | arith_operand_not_int | arithmetic operand fed a non-int column |
| lower.pl:881 | concat_not_a_list | `concat` fed a non-list |
| lower.pl:890 | concat_non_display_piece | `concat` piece that is not a display string |
| lower.pl:2253 | guard_goal_shape | a guard goal with an unexpected shape |
| lower.pl:2300 | unknown_comparison_operator | a comparison operator outside the known set |
| lower.pl:2305 | comparison_operand_not_int | int comparison over a non-int column |
| lower.pl:2312 | comparison_operand_not_number | numeric comparison over a non-number column |
| lower.pl:3276 | relation_pattern_not_lowerable (via `Residue` rethrow) | an unlowerable relation pattern residue |
| lower.pl:3401 | decode_pattern_not_object | a `decode` pattern that is not a `{...}` object |
| lower.pl:3563 | edge_trigger_not_log | an edge rule triggered off a non-log rel |
| lower.pl:3891 | aggregate_head_mixed_with_plain_clause | an aggregate head mixed with a plain clause |
| lower.pl:4312 | aggregate_group_not_delta_local | an aggregate group that is not delta-local |
| lower.pl:4955 | mixed_text_encoding | mixed text encodings in one distinct set |
| lower.pl:5205 | recursive_cte_multiple_self_reads | a recursive head read more than once in its own body |
| lower.pl:5260 | built_text_in_recursive_head | a text built directly in a recursive head |
| lower.pl:5264 | built_list_in_recursive_head | a list built directly in a recursive head |
| lower.pl:5346 | level_rule_no_positive_body | a level rule with no positive body atom |
| lower.pl:5455 | decode_source_not_bound | a `decode` whose source is not bound |
| lower.pl:5495 | json_key_contains_quote | a JSON key literal containing a quote |
| lower.pl:5585 | json_pattern_shape | a JSON pattern of unexpected shape |
| lower.pl:5664 | json_key_shape | a JSON key of unexpected shape |
| lower.pl:5839 | aggregate_kind_not_lowered | an aggregate kind with no lowering |
| lower.pl:5851 | aggregate_ordinal_not_int | an aggregate ordinal that is not an int |
| lower.pl:5857 | aggregate_separator_not_constant | an aggregate separator that is not constant |
| lower.pl:5864 | aggregate_operand_not_number | an aggregate operand that is not numeric |

## Arm census

177 multi-clause predicates in `lower.pl`; 86 of their arms are never entered by
the corpus. Grouped by lowering family with a one-line fixture sketch. `(N arms
total)` = clauses in that predicate; the listed line is one never-entered clause.

### Recursion refusal helpers (ties into the mutual_recursion arc)

| site | predicate | sketch |
|---|---|---|
| lower.pl:5259 | recursive_arm_builds_no_string/2 | direct recursive `text` build — the "direct spelling refused" path (ARCH.pl:952 names `built_text_in_recursive_head`) |
| lower.pl:5263 | recursive_arm_builds_no_list/2 | direct recursive `list` build |

These two guards back the `built_text_in_recursive_head` / `built_list_in_recursive_head`
throws (unreached above). The recursion refusal surface is entirely untested —
the same class of blind spot the mutual-recursion work already tripped on.

### Aggregate / average lowering

| site | predicate | sketch |
|---|---|---|
| lower.pl:4036 | avg_accumulator_seed_sql/4 | an `avg` aggregate over an empty group |
| lower.pl:4114 | avg_body_projection/3 | `avg` body projection arm |
| lower.pl:4130 | avg_body_where_sql/2 | `avg` body where arm |
| lower.pl:4135 | avg_accumulator_scope_predicate/4 | `avg` scope predicate arm |
| lower.pl:4151 | avg_scope_equalities/5 | `avg` scope equality arm |
| lower.pl:4152 | avg_scope_equalities/5 | `avg` scope equality arm |
| lower.pl:4170 | avg_body_matches_accumulator/2 | `avg` accumulator match arm |
| lower.pl:4238 | avg_scope_from/4 | `avg` scope FROM arm |
| lower.pl:4242 | avg_scope_from/4 | `avg` scope FROM arm |
| lower.pl:4249 | avg_join_equalities/3 | `avg` join equality arm |
| lower.pl:4250 | avg_join_equalities/3 | `avg` join equality arm |
| lower.pl:4402 | avg_accumulator_columns/5 | `avg` accumulator column arm |
| lower.pl:5838 | aggregate_select_expr/5 | an aggregate select-expression variant |
| lower.pl:5843 | json_group_array_value_sql/3 | `json_array` aggregate value arm |

### Fixpoint IR / differential re-derivation

| site | predicate | sketch |
|---|---|---|
| lower.pl:4786 | dred_seed_from_part/9 | dred seed part arm |
| lower.pl:4793 | dred_seed_from_part/9 | dred seed part arm |
| lower.pl:4934 | ir_source_ref/2 | fixpoint IR source-ref arm |
| lower.pl:4967 | ir_column_storage/5 | IR column storage class arm |
| lower.pl:4968 | ir_column_storage/5 | IR column storage class arm |
| lower.pl:4970 | ir_column_storage/5 | IR column storage class arm |
| lower.pl:4971 | ir_column_storage/5 | IR column storage class arm |
| lower.pl:4972 | ir_column_storage/5 | IR column storage class arm |
| lower.pl:4975 | ir_column_storage/5 | IR column storage class arm |
| lower.pl:4982 | ir_column_storage/5 | IR column storage class arm |
| lower.pl:5017 | ir_seed_source/6 | IR seed source arm |
| lower.pl:5019 | ir_seed_source/6 | IR seed source arm |
| lower.pl:5118 | ir_literal/2 | IR literal arm |
| lower.pl:5120 | ir_literal/2 | IR literal arm |
| lower.pl:5121 | ir_literal/2 | IR literal arm |
| lower.pl:5154 | ir_arith_operand_type/2 | IR arithmetic operand type arm |
| lower.pl:6167 | dred_wave_ddl/5 | dred wave DDL arm |

### Catalog / metadata emission

| site | predicate | sketch |
|---|---|---|
| lower.pl:1595 | semantic_surface_type/3 | semantic surface type arm |
| lower.pl:1596 | semantic_surface_type/3 | semantic surface type arm |
| lower.pl:1597 | semantic_surface_type/3 | semantic surface type arm |
| lower.pl:1599 | semantic_surface_type/3 | semantic surface type arm |
| lower.pl:1653 | catalog_semantic_id/4 | catalog semantic id arm |
| lower.pl:1665 | semantic_owner_id/4 | semantic owner id arm |
| lower.pl:1711 | metadata_parameter_parts/3 | metadata parameter arm |
| lower.pl:1751 | metadata_generic_type_id/6 | metadata generic type id arm |
| lower.pl:1779 | catalog_source_type_id/4 | catalog source type id arm |
| lower.pl:1781 | catalog_source_type_id/4 | catalog source type id arm |
| lower.pl:1793 | metadata_implementation_rows/6 | metadata implementation rows arm |
| lower.pl:1818 | module_rel_columns/3 | module rel columns arm |
| lower.pl:1826 | take_module_rel_decls/4 | module rel decl arm |
| lower.pl:1830 | take_module_rel_decls/4 | module rel decl arm |
| lower.pl:1855 | catalog_declared_column/2 | a declared column type (only 1 of 8 armed) |
| lower.pl:1856 | catalog_declared_column/2 | a declared column type |
| lower.pl:1857 | catalog_declared_column/2 | a declared column type |
| lower.pl:1858 | catalog_declared_column/2 | a declared column type |
| lower.pl:1859 | catalog_declared_column/2 | a declared column type |
| lower.pl:1860 | catalog_declared_column/2 | a declared column type |
| lower.pl:1862 | catalog_declared_column/2 | a declared column type |
| lower.pl:1864 | catalog_declared_column/2 | a declared column type |
| lower.pl:1869 | catalog_rel_module_ids/3 | catalog rel module id arm |
| lower.pl:1885 | spliced_module_rows/5 | spliced module row arm |
| lower.pl:1904 | first_per_key/3 | first-per-key arm |
| lower.pl:1931 | module_edge_rows/5 | module edge rows arm |
| lower.pl:2066 | catalog_rel_module/5 | catalog rel module arm |

### List / text interning

| site | predicate | sketch |
|---|---|---|
| lower.pl:2370 | intern_write_arm/4 | intern write arm |
| lower.pl:2412 | list_intern_from/2 | list intern FROM arm |
| lower.pl:2418 | member_intern_from/3 | member intern FROM arm |
| lower.pl:2440 | intern_ddl/2 | intern DDL arm |
| lower.pl:2867 | list_element_render/4 | list element render arm |
| lower.pl:2868 | list_element_render/4 | list element render arm |
| lower.pl:2875 | list_element_render/4 | list element render arm |

### JSON lowering

| site | predicate | sketch |
|---|---|---|
| lower.pl:5579 | json_pattern_sql/8 | json pattern SQL arm |
| lower.pl:5584 | json_pattern_sql/8 | json pattern SQL arm |
| lower.pl:5598 | json_capture_json_type/2 | json capture type arm |
| lower.pl:5663 | json_member_sql/9 | json member SQL arm |

### Expression / scalar rendering

| site | predicate | sketch |
|---|---|---|
| lower.pl:272 | sql_literal/2 | a `false` bool literal |
| lower.pl:435 | seeded_pre_args/10 | a `pre` seed with no args |
| lower.pl:639 | text_scalar_rendering/4 | a text scalar render variant |
| lower.pl:796 | json_document_operand_sql/3 | a `ref` json document operand |
| lower.pl:799 | json_document_operand_sql/3 | a `bool` json document operand |
| lower.pl:864 | arithmetic_rendering/6 | `numeric_division` rendering |
| lower.pl:2299 | comparison_operator_sql/5 | a comparison operator SQL arm |
| lower.pl:2302 | check_comparison_types/4 | a comparison type-check arm |

### Relation pattern / decode expansion

| site | predicate | sketch |
|---|---|---|
| lower.pl:3104 | check_edge_rule_relation_values/3 | relation value in an edge rule |
| lower.pl:3122 | expand_relation_pattern_rule/4 | relation pattern expansion arm |
| lower.pl:3235 | rewrite_relation_arguments/6 | relation argument rewrite arm |
| lower.pl:3329 | expand_decode_rule/4 | decode expansion arm |
| lower.pl:3435 | braces_pattern_pairs/2 | braces pattern pairs arm |
| lower.pl:3761 | member_same_term/2 | same-term membership arm |
| lower.pl:3762 | member_same_term/2 | same-term membership arm |

## Contradictions

None found in the strict sense. No unreached throw construct appears as actual
usage in a corpus fixture: the four source-string hits
(`unbound_head_var`, `relation_pattern_not_lowerable`, `head_expr`,
`unknown_comparison_operator`) are all comments/headers, not programs that use
the construct. So no refusal claims "unsupported" for a construct the manifest
shows compiling.

## Real finding (not yet a contradiction)

The **recursion refusal surface is entirely unreached**. Three throw sites —
`recursive_cte_multiple_self_reads` (5205), `built_text_in_recursive_head` (5260),
`built_list_in_recursive_head` (5264) — plus their two guard arms
(`recursive_arm_builds_no_string`, `recursive_arm_builds_no_list`) are never
exercised by any corpus program. ARCH.pl:952 already documents that the "direct
spelling" is refused loudly while the "two-rel spelling" silently under-derives
(the PR #266 silent-wrong). This census shows the direct-spelling refusal itself
has zero coverage, so nothing guards against the refusal regressing into silent
wrongness either. Candidate follow-up: a fixture that spells direct recursive
text/list construction and asserts the named refusal.
