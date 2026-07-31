# Type matrix -- generated, do not hand-edit

Regenerate: `bash v6/prolog/labs/type_matrix/matrix.sh`

Cells: 422 constructible / 0 not run / 0 n-a

## Verdict counts

| verdict / label | cells |
|---|---|
| IDENTICAL/lossless | 81 |
| NAMED_REFUSAL/compiler_only | 81 |
| DIVERGENT/doors_disagree | 69 |
| SILENT_COERCION/value_changed | 54 |
| NAMED_REFUSAL/both | 48 |
| NAMED_REFUSAL/name_mismatch | 33 |
| SILENT_COERCION/row_absent | 21 |
| DIVERGENT/emitter_only_refusal | 14 |
| DIVERGENT/oracle_only_refusal | 12 |
| DIVERGENT/emitter_modes_disagree | 8 |
| DIVERGENT/emitter_run_error | 1 |

## The two `.dl6` oracle doors

dl6_oracle.pl accepted the arrival in 329 cells, golden_oracle.pl carried 0 more that dl6_oracle refused outright, and 0 cells ran on BOTH doors and produced DIFFERENT tick logs.

| dl6_oracle refuses, golden_oracle accepts | cells |
|---|---|

| both doors ran, logs differ | cells |
|---|---|

## Per position

| position | IDENTICAL | SILENT_COERCION | DIVERGENT | NAMED_REFUSAL |
|---|---|---|---|---|
| arrival | 15 | 14 | 26 | 15 |
| level_head | 21 | 10 | 32 | 7 |
| edge_head | 15 | 4 | 0 | 51 |
| json_capture | 6 | 23 | 11 | 30 |
| aggregate_head | 9 | 10 | 9 | 42 |
| join_column | 15 | 14 | 26 | 15 |
| seed | 0 | 0 | 0 | 2 |

## Per declared type

| declared | IDENTICAL | SILENT_COERCION | DIVERGENT | NAMED_REFUSAL |
|---|---|---|---|---|
| int | 7 | 16 | 19 | 19 |
| float | 8 | 18 | 10 | 24 |
| text | 10 | 11 | 24 | 15 |
| bool | 4 | 0 | 12 | 44 |
| json | 16 | 10 | 9 | 25 |
| list_text | 16 | 10 | 9 | 25 |
| undeclared | 20 | 10 | 21 | 9 |
| int_vs_text | 0 | 0 | 0 | 1 |

## Per fed value

| value | IDENTICAL | SILENT_COERCION | DIVERGENT | NAMED_REFUSAL |
|---|---|---|---|---|
| int | 19 | 2 | 11 | 11 |
| float | 17 | 2 | 13 | 10 |
| numeric_text | 10 | 6 | 7 | 19 |
| plain_text | 11 | 2 | 10 | 20 |
| json_object | 10 | 7 | 7 | 18 |
| json_array | 10 | 7 | 7 | 18 |
| bool | 4 | 3 | 17 | 18 |
| wide_int | 0 | 0 | 14 | 28 |
| float_integral | 0 | 23 | 9 | 10 |
| neg_zero | 0 | 23 | 9 | 10 |

## Every cell

| position | declared | value | verdict | label | receipt |
|---|---|---|---|---|---|
| arrival | int | int | IDENTICAL | lossless | 4 |
| arrival | int | float | DIVERGENT | emitter_only_refusal | oracle ran and graded "1.5"; emitter refused with int_out_of_range: Error: int_out_of_range probe_in[0] |
| arrival | int | numeric_text | DIVERGENT | emitter_only_refusal | oracle ran and graded "\"4\""; emitter refused with int_out_of_range: Error: int_out_of_range probe_in[0] |
| arrival | int | plain_text | DIVERGENT | emitter_only_refusal | oracle ran and graded "\"north\""; emitter refused with int_out_of_range: Error: int_out_of_range probe_in[0] |
| arrival | int | json_object | DIVERGENT | emitter_only_refusal | oracle ran and graded "\"{\\\"key\\\":1}\""; emitter refused with int_out_of_range: Error: int_out_of_range probe_in[0] |
| arrival | int | json_array | DIVERGENT | emitter_only_refusal | oracle ran and graded "\"[1,2]\""; emitter refused with int_out_of_range: Error: int_out_of_range probe_in[0] |
| arrival | int | bool | DIVERGENT | emitter_only_refusal | oracle ran and graded "true"; emitter refused with int_out_of_range: Error: int_out_of_range probe_in[0] |
| arrival | int | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| arrival | int | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| arrival | int | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| arrival | float | int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden type_arrival_shape_mismatch); emitter compiled and stored 4 |
| arrival | float | float | IDENTICAL | lossless | 1.5 |
| arrival | float | numeric_text | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_in[0] requires a finite number oracle=type_arrival_shape_mismatch |
| arrival | float | plain_text | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_in[0] requires a finite number oracle=type_arrival_shape_mismatch |
| arrival | float | json_object | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_in[0] requires a finite number oracle=type_arrival_shape_mismatch |
| arrival | float | json_array | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_in[0] requires a finite number oracle=type_arrival_shape_mismatch |
| arrival | float | bool | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_in[0] requires a finite number oracle=type_arrival_shape_mismatch |
| arrival | float | wide_int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden type_arrival_shape_mismatch); emitter compiled and stored 9007199254740992 |
| arrival | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| arrival | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| arrival | text | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "\"4\"" |
| arrival | text | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "\"1.5\"" |
| arrival | text | numeric_text | IDENTICAL | lossless | "4" |
| arrival | text | plain_text | IDENTICAL | lossless | "north" |
| arrival | text | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| arrival | text | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| arrival | text | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "\"1\"" |
| arrival | text | wide_int | DIVERGENT | emitter_modes_disagree | incremental "\"9007199254740992\"" vs naive "\"9.00719925474099e+15\"" vs oracle "9007199254740992" |
| arrival | text | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "\"1\"" |
| arrival | text | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "\"0\"" |
| arrival | bool | int | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_in[0] requires true or false oracle=type_arrival_shape_mismatch |
| arrival | bool | float | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_in[0] requires true or false oracle=type_arrival_shape_mismatch |
| arrival | bool | numeric_text | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_in[0] requires true or false oracle=type_arrival_shape_mismatch |
| arrival | bool | plain_text | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_in[0] requires true or false oracle=type_arrival_shape_mismatch |
| arrival | bool | json_object | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_in[0] requires true or false oracle=type_arrival_shape_mismatch |
| arrival | bool | json_array | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_in[0] requires true or false oracle=type_arrival_shape_mismatch |
| arrival | bool | bool | IDENTICAL | lossless | true |
| arrival | bool | wide_int | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_in[0] requires true or false oracle=type_arrival_shape_mismatch |
| arrival | bool | float_integral | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_in[0] requires true or false oracle=type_arrival_shape_mismatch |
| arrival | bool | neg_zero | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_in[0] requires true or false oracle=type_arrival_shape_mismatch |
| arrival | json | int | IDENTICAL | lossless | 4 |
| arrival | json | float | IDENTICAL | lossless | 1.5 |
| arrival | json | numeric_text | SILENT_COERCION | value_changed | fed "4", graded 4 |
| arrival | json | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 json_arrival, golden ); emitter compiled and stored ABSENT |
| arrival | json | json_object | IDENTICAL | lossless | {"key":1} |
| arrival | json | json_array | IDENTICAL | lossless | [1,2] |
| arrival | json | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 golden_oracle, golden ); emitter compiled and stored 1 |
| arrival | json | wide_int | DIVERGENT | emitter_modes_disagree | incremental "9007199254740992" vs naive "9007199254740990" vs oracle "9007199254740992" |
| arrival | json | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| arrival | json | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| arrival | list_text | int | IDENTICAL | lossless | 4 |
| arrival | list_text | float | IDENTICAL | lossless | 1.5 |
| arrival | list_text | numeric_text | SILENT_COERCION | value_changed | fed "4", graded 4 |
| arrival | list_text | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 json_arrival, golden ); emitter compiled and stored ABSENT |
| arrival | list_text | json_object | IDENTICAL | lossless | {"key":1} |
| arrival | list_text | json_array | IDENTICAL | lossless | [1,2] |
| arrival | list_text | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 golden_oracle, golden ); emitter compiled and stored 1 |
| arrival | list_text | wide_int | DIVERGENT | emitter_modes_disagree | incremental "9007199254740992" vs naive "9007199254740990" vs oracle "9007199254740992" |
| arrival | list_text | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| arrival | list_text | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| arrival | undeclared | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "\"4\"" |
| arrival | undeclared | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "\"1.5\"" |
| arrival | undeclared | numeric_text | IDENTICAL | lossless | "4" |
| arrival | undeclared | plain_text | IDENTICAL | lossless | "north" |
| arrival | undeclared | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| arrival | undeclared | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| arrival | undeclared | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "\"1\"" |
| arrival | undeclared | wide_int | DIVERGENT | emitter_modes_disagree | incremental "\"9007199254740992\"" vs naive "\"9.00719925474099e+15\"" vs oracle "9007199254740992" |
| arrival | undeclared | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "\"1\"" |
| arrival | undeclared | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "\"0\"" |
| level_head | int | int | IDENTICAL | lossless | 4 |
| level_head | int | float | DIVERGENT | emitter_only_refusal | oracle ran and graded "1.5"; emitter refused with int_out_of_range: Error: int_out_of_range |
| level_head | int | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "4" |
| level_head | int | plain_text | IDENTICAL | lossless | "north" |
| level_head | int | json_object | DIVERGENT | doors_disagree | oracle "{\"key\":1}" vs emitter "\"{\\\"key\\\":1}\"" |
| level_head | int | json_array | DIVERGENT | doors_disagree | oracle "[1,2]" vs emitter "\"[1,2]\"" |
| level_head | int | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "1" |
| level_head | int | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| level_head | int | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | int | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| level_head | float | int | IDENTICAL | lossless | 4 |
| level_head | float | float | IDENTICAL | lossless | 1.5 |
| level_head | float | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "4" |
| level_head | float | plain_text | DIVERGENT | doors_disagree | oracle "\"north\"" vs emitter "ABSENT" |
| level_head | float | json_object | DIVERGENT | doors_disagree | oracle "{\"key\":1}" vs emitter "ABSENT" |
| level_head | float | json_array | DIVERGENT | doors_disagree | oracle "[1,2]" vs emitter "ABSENT" |
| level_head | float | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "1" |
| level_head | float | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| level_head | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| level_head | text | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "\"4\"" |
| level_head | text | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "\"1.5\"" |
| level_head | text | numeric_text | IDENTICAL | lossless | "4" |
| level_head | text | plain_text | IDENTICAL | lossless | "north" |
| level_head | text | json_object | DIVERGENT | doors_disagree | oracle "{\"key\":1}" vs emitter "\"{\\\"key\\\":1}\"" |
| level_head | text | json_array | DIVERGENT | doors_disagree | oracle "[1,2]" vs emitter "\"[1,2]\"" |
| level_head | text | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "\"1\"" |
| level_head | text | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| level_head | text | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "\"1.0\"" |
| level_head | text | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "\"0.0\"" |
| level_head | bool | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "ABSENT" |
| level_head | bool | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "ABSENT" |
| level_head | bool | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "ABSENT" |
| level_head | bool | plain_text | DIVERGENT | doors_disagree | oracle "\"north\"" vs emitter "ABSENT" |
| level_head | bool | json_object | DIVERGENT | doors_disagree | oracle "{\"key\":1}" vs emitter "ABSENT" |
| level_head | bool | json_array | DIVERGENT | doors_disagree | oracle "[1,2]" vs emitter "ABSENT" |
| level_head | bool | bool | IDENTICAL | lossless | true |
| level_head | bool | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| level_head | bool | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "true" |
| level_head | bool | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "false" |
| level_head | json | int | IDENTICAL | lossless | 4 |
| level_head | json | float | IDENTICAL | lossless | 1.5 |
| level_head | json | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "4" |
| level_head | json | plain_text | DIVERGENT | doors_disagree | oracle "\"north\"" vs emitter "ABSENT" |
| level_head | json | json_object | IDENTICAL | lossless | {"key":1} |
| level_head | json | json_array | IDENTICAL | lossless | [1,2] |
| level_head | json | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "1" |
| level_head | json | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| level_head | json | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | json | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| level_head | list_text | int | IDENTICAL | lossless | 4 |
| level_head | list_text | float | IDENTICAL | lossless | 1.5 |
| level_head | list_text | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "4" |
| level_head | list_text | plain_text | DIVERGENT | doors_disagree | oracle "\"north\"" vs emitter "ABSENT" |
| level_head | list_text | json_object | IDENTICAL | lossless | {"key":1} |
| level_head | list_text | json_array | IDENTICAL | lossless | [1,2] |
| level_head | list_text | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "1" |
| level_head | list_text | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| level_head | list_text | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | list_text | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| level_head | undeclared | int | IDENTICAL | lossless | 4 |
| level_head | undeclared | float | IDENTICAL | lossless | 1.5 |
| level_head | undeclared | numeric_text | IDENTICAL | lossless | "4" |
| level_head | undeclared | plain_text | IDENTICAL | lossless | "north" |
| level_head | undeclared | json_object | IDENTICAL | lossless | {"key":1} |
| level_head | undeclared | json_array | IDENTICAL | lossless | [1,2] |
| level_head | undeclared | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "\"1\"" |
| level_head | undeclared | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| level_head | undeclared | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | undeclared | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| edge_head | int | int | IDENTICAL | lossless | 4 |
| edge_head | int | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | json_object | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | json_array | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| edge_head | int | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | float | IDENTICAL | lossless | 1.5 |
| edge_head | float | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | json_object | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | json_array | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | wide_int | NAMED_REFUSAL | name_mismatch | compiler=edge_head_column_type_mismatch oracle=int_out_of_range |
| edge_head | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| edge_head | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| edge_head | text | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | numeric_text | IDENTICAL | lossless | "4" |
| edge_head | text | plain_text | IDENTICAL | lossless | "north" |
| edge_head | text | json_object | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | json_array | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | wide_int | NAMED_REFUSAL | name_mismatch | compiler=edge_head_column_type_mismatch oracle=int_out_of_range |
| edge_head | text | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | json_object | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | json_array | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | bool | IDENTICAL | lossless | true |
| edge_head | bool | wide_int | NAMED_REFUSAL | name_mismatch | compiler=edge_head_column_type_mismatch oracle=int_out_of_range |
| edge_head | bool | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | json_object | IDENTICAL | lossless | {"key":1} |
| edge_head | json | json_array | IDENTICAL | lossless | [1,2] |
| edge_head | json | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | wide_int | NAMED_REFUSAL | name_mismatch | compiler=edge_head_column_type_mismatch oracle=int_out_of_range |
| edge_head | json | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | json_object | IDENTICAL | lossless | {"key":1} |
| edge_head | list_text | json_array | IDENTICAL | lossless | [1,2] |
| edge_head | list_text | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | wide_int | NAMED_REFUSAL | name_mismatch | compiler=edge_head_column_type_mismatch oracle=int_out_of_range |
| edge_head | list_text | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | undeclared | int | IDENTICAL | lossless | 4 |
| edge_head | undeclared | float | IDENTICAL | lossless | 1.5 |
| edge_head | undeclared | numeric_text | IDENTICAL | lossless | "4" |
| edge_head | undeclared | plain_text | IDENTICAL | lossless | "north" |
| edge_head | undeclared | json_object | IDENTICAL | lossless | {"key":1} |
| edge_head | undeclared | json_array | IDENTICAL | lossless | [1,2] |
| edge_head | undeclared | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | undeclared | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| edge_head | undeclared | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| edge_head | undeclared | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| json_capture | int | int | IDENTICAL | lossless | 4 |
| json_capture | int | float | SILENT_COERCION | row_absent | fed 1.5, graded ABSENT |
| json_capture | int | numeric_text | SILENT_COERCION | row_absent | fed "4", graded ABSENT |
| json_capture | int | plain_text | SILENT_COERCION | row_absent | fed "north", graded ABSENT |
| json_capture | int | json_object | SILENT_COERCION | row_absent | fed {"key":1}, graded ABSENT |
| json_capture | int | json_array | SILENT_COERCION | row_absent | fed [1,2], graded ABSENT |
| json_capture | int | bool | SILENT_COERCION | row_absent | fed true, graded ABSENT |
| json_capture | int | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| json_capture | int | float_integral | SILENT_COERCION | row_absent | fed 1.0, graded ABSENT |
| json_capture | int | neg_zero | SILENT_COERCION | row_absent | fed -0.0, graded ABSENT |
| json_capture | float | int | SILENT_COERCION | row_absent | fed 4, graded ABSENT |
| json_capture | float | float | IDENTICAL | lossless | 1.5 |
| json_capture | float | numeric_text | SILENT_COERCION | row_absent | fed "4", graded ABSENT |
| json_capture | float | plain_text | SILENT_COERCION | row_absent | fed "north", graded ABSENT |
| json_capture | float | json_object | SILENT_COERCION | row_absent | fed {"key":1}, graded ABSENT |
| json_capture | float | json_array | SILENT_COERCION | row_absent | fed [1,2], graded ABSENT |
| json_capture | float | bool | SILENT_COERCION | row_absent | fed true, graded ABSENT |
| json_capture | float | wide_int | DIVERGENT | doors_disagree | oracle "ABSENT" vs emitter "ABSENT" |
| json_capture | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| json_capture | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| json_capture | text | int | SILENT_COERCION | row_absent | fed 4, graded ABSENT |
| json_capture | text | float | SILENT_COERCION | row_absent | fed 1.5, graded ABSENT |
| json_capture | text | numeric_text | IDENTICAL | lossless | "4" |
| json_capture | text | plain_text | IDENTICAL | lossless | "north" |
| json_capture | text | json_object | SILENT_COERCION | row_absent | fed {"key":1}, graded ABSENT |
| json_capture | text | json_array | SILENT_COERCION | row_absent | fed [1,2], graded ABSENT |
| json_capture | text | bool | SILENT_COERCION | row_absent | fed true, graded ABSENT |
| json_capture | text | wide_int | DIVERGENT | doors_disagree | oracle "ABSENT" vs emitter "ABSENT" |
| json_capture | text | float_integral | SILENT_COERCION | row_absent | fed 1.0, graded ABSENT |
| json_capture | text | neg_zero | SILENT_COERCION | row_absent | fed -0.0, graded ABSENT |
| json_capture | bool | int | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | bool | float | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | bool | numeric_text | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | bool | plain_text | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | bool | json_object | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | bool | json_array | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | bool | bool | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | bool | wide_int | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | bool | float_integral | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | bool | neg_zero | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | int | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | float | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | numeric_text | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | plain_text | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | json_object | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | json_array | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | bool | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | wide_int | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | float_integral | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | json | neg_zero | NAMED_REFUSAL | both | json_capture_type_unknown |
| json_capture | list_text | int | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | list_text | float | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | list_text | numeric_text | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | list_text | plain_text | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | list_text | json_object | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | list_text | json_array | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | list_text | bool | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | list_text | wide_int | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | list_text | float_integral | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | list_text | neg_zero | NAMED_REFUSAL | both | dl_parse_error |
| json_capture | undeclared | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "\"4\"" |
| json_capture | undeclared | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "\"1.5\"" |
| json_capture | undeclared | numeric_text | IDENTICAL | lossless | "4" |
| json_capture | undeclared | plain_text | IDENTICAL | lossless | "north" |
| json_capture | undeclared | json_object | DIVERGENT | doors_disagree | oracle "{\"key\":1}" vs emitter "\"{\\\"key\\\":1}\"" |
| json_capture | undeclared | json_array | DIVERGENT | doors_disagree | oracle "[1,2]" vs emitter "\"[1,2]\"" |
| json_capture | undeclared | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "\"1\"" |
| json_capture | undeclared | wide_int | DIVERGENT | doors_disagree | oracle "9007199254740992" vs emitter "\"9007199254740993\"" |
| json_capture | undeclared | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "\"1.0\"" |
| json_capture | undeclared | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "\"0.0\"" |
| aggregate_head | int | int | IDENTICAL | lossless | 4 |
| aggregate_head | int | float | DIVERGENT | emitter_only_refusal | oracle ran and graded "1.5"; emitter refused with int_out_of_range: Error: int_out_of_range |
| aggregate_head | int | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| aggregate_head | int | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | int | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| aggregate_head | float | int | IDENTICAL | lossless | 4 |
| aggregate_head | float | float | IDENTICAL | lossless | 1.5 |
| aggregate_head | float | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| aggregate_head | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| aggregate_head | text | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "\"4\"" |
| aggregate_head | text | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "\"1.5\"" |
| aggregate_head | text | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| aggregate_head | text | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "\"1.0\"" |
| aggregate_head | text | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "\"0.0\"" |
| aggregate_head | bool | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "ABSENT" |
| aggregate_head | bool | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "ABSENT" |
| aggregate_head | bool | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| aggregate_head | bool | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "true" |
| aggregate_head | bool | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "false" |
| aggregate_head | json | int | IDENTICAL | lossless | 4 |
| aggregate_head | json | float | IDENTICAL | lossless | 1.5 |
| aggregate_head | json | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| aggregate_head | json | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | json | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| aggregate_head | list_text | int | IDENTICAL | lossless | 4 |
| aggregate_head | list_text | float | IDENTICAL | lossless | 1.5 |
| aggregate_head | list_text | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| aggregate_head | list_text | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | list_text | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| aggregate_head | undeclared | int | IDENTICAL | lossless | 4 |
| aggregate_head | undeclared | float | IDENTICAL | lossless | 1.5 |
| aggregate_head | undeclared | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| aggregate_head | undeclared | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | undeclared | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| join_column | int | int | IDENTICAL | lossless | 4 |
| join_column | int | float | DIVERGENT | emitter_only_refusal | oracle ran and graded "1.5"; emitter refused with int_out_of_range: Error: int_out_of_range probe_left[0] |
| join_column | int | numeric_text | DIVERGENT | emitter_only_refusal | oracle ran and graded "\"4\""; emitter refused with int_out_of_range: Error: int_out_of_range probe_left[0] |
| join_column | int | plain_text | DIVERGENT | emitter_only_refusal | oracle ran and graded "\"north\""; emitter refused with int_out_of_range: Error: int_out_of_range probe_left[0] |
| join_column | int | json_object | DIVERGENT | emitter_only_refusal | oracle ran and graded "\"{\\\"key\\\":1}\""; emitter refused with int_out_of_range: Error: int_out_of_range probe_left[0] |
| join_column | int | json_array | DIVERGENT | emitter_only_refusal | oracle ran and graded "\"[1,2]\""; emitter refused with int_out_of_range: Error: int_out_of_range probe_left[0] |
| join_column | int | bool | DIVERGENT | emitter_only_refusal | oracle ran and graded "true"; emitter refused with int_out_of_range: Error: int_out_of_range probe_left[0] |
| join_column | int | wide_int | NAMED_REFUSAL | both | int_out_of_range |
| join_column | int | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| join_column | int | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| join_column | float | int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden type_arrival_shape_mismatch); emitter compiled and stored 4 |
| join_column | float | float | IDENTICAL | lossless | 1.5 |
| join_column | float | numeric_text | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_left[0] requires a finite number oracle=type_arrival_shape_mismatch |
| join_column | float | plain_text | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_left[0] requires a finite number oracle=type_arrival_shape_mismatch |
| join_column | float | json_object | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_left[0] requires a finite number oracle=type_arrival_shape_mismatch |
| join_column | float | json_array | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_left[0] requires a finite number oracle=type_arrival_shape_mismatch |
| join_column | float | bool | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: float arrival probe_left[0] requires a finite number oracle=type_arrival_shape_mismatch |
| join_column | float | wide_int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden type_arrival_shape_mismatch); emitter compiled and stored 9007199254740992 |
| join_column | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| join_column | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| join_column | text | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "\"4\"" |
| join_column | text | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "\"1.5\"" |
| join_column | text | numeric_text | IDENTICAL | lossless | "4" |
| join_column | text | plain_text | IDENTICAL | lossless | "north" |
| join_column | text | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| join_column | text | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| join_column | text | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "\"1\"" |
| join_column | text | wide_int | DIVERGENT | emitter_modes_disagree | incremental "\"9007199254740992\"" vs naive "\"9.00719925474099e+15\"" vs oracle "9007199254740992" |
| join_column | text | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "\"1\"" |
| join_column | text | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "\"0\"" |
| join_column | bool | int | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_left[0] requires true or false oracle=type_arrival_shape_mismatch |
| join_column | bool | float | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_left[0] requires true or false oracle=type_arrival_shape_mismatch |
| join_column | bool | numeric_text | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_left[0] requires true or false oracle=type_arrival_shape_mismatch |
| join_column | bool | plain_text | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_left[0] requires true or false oracle=type_arrival_shape_mismatch |
| join_column | bool | json_object | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_left[0] requires true or false oracle=type_arrival_shape_mismatch |
| join_column | bool | json_array | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_left[0] requires true or false oracle=type_arrival_shape_mismatch |
| join_column | bool | bool | IDENTICAL | lossless | true |
| join_column | bool | wide_int | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_left[0] requires true or false oracle=type_arrival_shape_mismatch |
| join_column | bool | float_integral | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_left[0] requires true or false oracle=type_arrival_shape_mismatch |
| join_column | bool | neg_zero | NAMED_REFUSAL | name_mismatch | emitter=(no functor) Error: bool arrival probe_left[0] requires true or false oracle=type_arrival_shape_mismatch |
| join_column | json | int | IDENTICAL | lossless | 4 |
| join_column | json | float | IDENTICAL | lossless | 1.5 |
| join_column | json | numeric_text | SILENT_COERCION | value_changed | fed "4", graded 4 |
| join_column | json | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 json_arrival, golden ); emitter compiled and stored ABSENT |
| join_column | json | json_object | IDENTICAL | lossless | {"key":1} |
| join_column | json | json_array | IDENTICAL | lossless | [1,2] |
| join_column | json | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 golden_oracle, golden ); emitter compiled and stored 1 |
| join_column | json | wide_int | DIVERGENT | emitter_modes_disagree | incremental "9007199254740992" vs naive "9007199254740990" vs oracle "9007199254740992" |
| join_column | json | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| join_column | json | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| join_column | list_text | int | IDENTICAL | lossless | 4 |
| join_column | list_text | float | IDENTICAL | lossless | 1.5 |
| join_column | list_text | numeric_text | SILENT_COERCION | value_changed | fed "4", graded 4 |
| join_column | list_text | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 json_arrival, golden ); emitter compiled and stored ABSENT |
| join_column | list_text | json_object | IDENTICAL | lossless | {"key":1} |
| join_column | list_text | json_array | IDENTICAL | lossless | [1,2] |
| join_column | list_text | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 golden_oracle, golden ); emitter compiled and stored 1 |
| join_column | list_text | wide_int | DIVERGENT | emitter_modes_disagree | incremental "9007199254740992" vs naive "9007199254740990" vs oracle "9007199254740992" |
| join_column | list_text | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| join_column | list_text | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| join_column | undeclared | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "\"4\"" |
| join_column | undeclared | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "\"1.5\"" |
| join_column | undeclared | numeric_text | IDENTICAL | lossless | "4" |
| join_column | undeclared | plain_text | IDENTICAL | lossless | "north" |
| join_column | undeclared | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| join_column | undeclared | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| join_column | undeclared | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "\"1\"" |
| join_column | undeclared | wide_int | DIVERGENT | emitter_modes_disagree | incremental "\"9007199254740992\"" vs naive "\"9.00719925474099e+15\"" vs oracle "9007199254740992" |
| join_column | undeclared | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "\"1\"" |
| join_column | undeclared | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "\"0\"" |
| seed | int_vs_text | int | NAMED_REFUSAL | compiler_only | join_column_type_mismatch |
| seed | int | plain_text | NAMED_REFUSAL | compiler_only | decl_type_conflicts_witness |

