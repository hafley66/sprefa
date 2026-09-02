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
