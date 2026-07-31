# Type matrix -- generated, do not hand-edit

Regenerate: `bash v6/prolog/labs/type_matrix/matrix.sh`

Cells: 422 constructible / 0 not run / 0 n-a

## Verdict counts

| verdict / label | cells |
|---|---|
| NAMED_REFUSAL/compiler_only | 86 |
| IDENTICAL/lossless | 79 |
| SILENT_COERCION/value_changed | 50 |
| DIVERGENT/doors_disagree | 49 |
| DIVERGENT/emitter_modes_disagree | 48 |
| DIVERGENT/oracle_only_refusal | 40 |
| NAMED_REFUSAL/both | 30 |
| SILENT_COERCION/row_absent | 21 |
| DIVERGENT/emitter_run_error | 19 |

## The two `.dl6` oracle doors

dl6_oracle.pl accepted the arrival in 352 cells, golden_oracle.pl carried 0 more that dl6_oracle refused outright, and 0 cells ran on BOTH doors and produced DIFFERENT tick logs.

| dl6_oracle refuses, golden_oracle accepts | cells |
|---|---|

| both doors ran, logs differ | cells |
|---|---|

## Per position

| position | IDENTICAL | SILENT_COERCION | DIVERGENT | NAMED_REFUSAL |
|---|---|---|---|---|
| arrival | 13 | 12 | 45 | 0 |
| level_head | 22 | 10 | 38 | 0 |
| edge_head | 15 | 4 | 2 | 49 |
| json_capture | 6 | 23 | 11 | 30 |
| aggregate_head | 10 | 10 | 15 | 35 |
| join_column | 13 | 12 | 45 | 0 |
| seed | 0 | 0 | 0 | 2 |

## Per declared type

| declared | IDENTICAL | SILENT_COERCION | DIVERGENT | NAMED_REFUSAL |
|---|---|---|---|---|
| int | 13 | 20 | 14 | 14 |
| float | 8 | 18 | 22 | 12 |
| text | 10 | 11 | 26 | 13 |
| bool | 4 | 0 | 32 | 24 |
| json | 12 | 6 | 19 | 23 |
| list_text | 12 | 6 | 19 | 23 |
| undeclared | 20 | 10 | 24 | 6 |
| int_vs_text | 0 | 0 | 0 | 1 |

## Per fed value

| value | IDENTICAL | SILENT_COERCION | DIVERGENT | NAMED_REFUSAL |
|---|---|---|---|---|
| int | 15 | 2 | 17 | 9 |
| float | 17 | 2 | 15 | 8 |
| numeric_text | 10 | 6 | 11 | 15 |
| plain_text | 13 | 2 | 12 | 16 |
| json_object | 10 | 9 | 9 | 14 |
| json_array | 10 | 9 | 9 | 14 |
| bool | 4 | 3 | 19 | 16 |
| wide_int | 0 | 0 | 34 | 8 |
| float_integral | 0 | 19 | 15 | 8 |
| neg_zero | 0 | 19 | 15 | 8 |

## Every cell

| position | declared | value | verdict | label | receipt |
|---|---|---|---|---|---|
| arrival | int | int | IDENTICAL | lossless | 4 |
| arrival | int | float | IDENTICAL | lossless | 1.5 |
| arrival | int | numeric_text | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "4" vs oracle "\"4\"" |
| arrival | int | plain_text | IDENTICAL | lossless | "north" |
| arrival | int | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| arrival | int | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| arrival | int | bool | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1" vs oracle "true" |
| arrival | int | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| arrival | int | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| arrival | int | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| arrival | float | int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored 4 |
| arrival | float | float | IDENTICAL | lossless | 1.5 |
| arrival | float | numeric_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | float | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | float | json_object | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | float | json_array | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | float | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | float | wide_int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored 9007199254740992 |
| arrival | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| arrival | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| arrival | text | int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"4\"" vs oracle "4" |
| arrival | text | float | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1.5\"" vs oracle "1.5" |
| arrival | text | numeric_text | IDENTICAL | lossless | "4" |
| arrival | text | plain_text | IDENTICAL | lossless | "north" |
| arrival | text | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| arrival | text | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| arrival | text | bool | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1\"" vs oracle "true" |
| arrival | text | wide_int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"9007199254740992\"" vs oracle "9007199254740992" |
| arrival | text | float_integral | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1\"" vs oracle "1" |
| arrival | text | neg_zero | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"0\"" vs oracle "0" |
| arrival | bool | int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | bool | float | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | bool | numeric_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | bool | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | bool | json_object | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | bool | json_array | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | bool | bool | IDENTICAL | lossless | true |
| arrival | bool | wide_int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | bool | float_integral | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | bool | neg_zero | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| arrival | json | int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "4" vs oracle "4" |
| arrival | json | float | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1.5" vs oracle "1.5" |
| arrival | json | numeric_text | SILENT_COERCION | value_changed | fed "4", graded 4 |
| arrival | json | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 json_arrival, golden ); emitter compiled and stored ABSENT |
| arrival | json | json_object | IDENTICAL | lossless | {"key":1} |
| arrival | json | json_array | IDENTICAL | lossless | [1,2] |
| arrival | json | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 golden_oracle, golden ); emitter compiled and stored ABSENT |
| arrival | json | wide_int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "9007199254740992" vs oracle "9007199254740992" |
| arrival | json | float_integral | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1" vs oracle "1" |
| arrival | json | neg_zero | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "0" vs oracle "0" |
| arrival | list_text | int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "4" vs oracle "4" |
| arrival | list_text | float | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1.5" vs oracle "1.5" |
| arrival | list_text | numeric_text | SILENT_COERCION | value_changed | fed "4", graded 4 |
| arrival | list_text | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 json_arrival, golden ); emitter compiled and stored ABSENT |
| arrival | list_text | json_object | IDENTICAL | lossless | {"key":1} |
| arrival | list_text | json_array | IDENTICAL | lossless | [1,2] |
| arrival | list_text | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 golden_oracle, golden ); emitter compiled and stored ABSENT |
| arrival | list_text | wide_int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "9007199254740992" vs oracle "9007199254740992" |
| arrival | list_text | float_integral | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1" vs oracle "1" |
| arrival | list_text | neg_zero | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "0" vs oracle "0" |
| arrival | undeclared | int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"4\"" vs oracle "4" |
| arrival | undeclared | float | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1.5\"" vs oracle "1.5" |
| arrival | undeclared | numeric_text | IDENTICAL | lossless | "4" |
| arrival | undeclared | plain_text | IDENTICAL | lossless | "north" |
| arrival | undeclared | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| arrival | undeclared | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| arrival | undeclared | bool | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1\"" vs oracle "true" |
| arrival | undeclared | wide_int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"9007199254740992\"" vs oracle "9007199254740992" |
| arrival | undeclared | float_integral | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1\"" vs oracle "1" |
| arrival | undeclared | neg_zero | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"0\"" vs oracle "0" |
| level_head | int | int | IDENTICAL | lossless | 4 |
| level_head | int | float | IDENTICAL | lossless | 1.5 |
| level_head | int | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "4" |
| level_head | int | plain_text | IDENTICAL | lossless | "north" |
| level_head | int | json_object | DIVERGENT | doors_disagree | oracle "{\"key\":1}" vs emitter "\"{\\\"key\\\":1}\"" |
| level_head | int | json_array | DIVERGENT | doors_disagree | oracle "[1,2]" vs emitter "\"[1,2]\"" |
| level_head | int | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "1" |
| level_head | int | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| level_head | int | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | int | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| level_head | float | int | IDENTICAL | lossless | 4 |
| level_head | float | float | IDENTICAL | lossless | 1.5 |
| level_head | float | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "4" |
| level_head | float | plain_text | DIVERGENT | doors_disagree | oracle "\"north\"" vs emitter "ABSENT" |
| level_head | float | json_object | DIVERGENT | doors_disagree | oracle "{\"key\":1}" vs emitter "ABSENT" |
| level_head | float | json_array | DIVERGENT | doors_disagree | oracle "[1,2]" vs emitter "ABSENT" |
| level_head | float | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "1" |
| level_head | float | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| level_head | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| level_head | text | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "\"4\"" |
| level_head | text | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "\"1.5\"" |
| level_head | text | numeric_text | IDENTICAL | lossless | "4" |
| level_head | text | plain_text | IDENTICAL | lossless | "north" |
| level_head | text | json_object | DIVERGENT | doors_disagree | oracle "{\"key\":1}" vs emitter "\"{\\\"key\\\":1}\"" |
| level_head | text | json_array | DIVERGENT | doors_disagree | oracle "[1,2]" vs emitter "\"[1,2]\"" |
| level_head | text | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "\"1\"" |
| level_head | text | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| level_head | text | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "\"1.0\"" |
| level_head | text | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "\"0.0\"" |
| level_head | bool | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "ABSENT" |
| level_head | bool | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "ABSENT" |
| level_head | bool | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "ABSENT" |
| level_head | bool | plain_text | DIVERGENT | doors_disagree | oracle "\"north\"" vs emitter "ABSENT" |
| level_head | bool | json_object | DIVERGENT | doors_disagree | oracle "{\"key\":1}" vs emitter "ABSENT" |
| level_head | bool | json_array | DIVERGENT | doors_disagree | oracle "[1,2]" vs emitter "ABSENT" |
| level_head | bool | bool | IDENTICAL | lossless | true |
| level_head | bool | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| level_head | bool | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "true" |
| level_head | bool | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "false" |
| level_head | json | int | IDENTICAL | lossless | 4 |
| level_head | json | float | IDENTICAL | lossless | 1.5 |
| level_head | json | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "4" |
| level_head | json | plain_text | DIVERGENT | doors_disagree | oracle "\"north\"" vs emitter "ABSENT" |
| level_head | json | json_object | IDENTICAL | lossless | {"key":1} |
| level_head | json | json_array | IDENTICAL | lossless | [1,2] |
| level_head | json | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "1" |
| level_head | json | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| level_head | json | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | json | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| level_head | list_text | int | IDENTICAL | lossless | 4 |
| level_head | list_text | float | IDENTICAL | lossless | 1.5 |
| level_head | list_text | numeric_text | DIVERGENT | doors_disagree | oracle "\"4\"" vs emitter "4" |
| level_head | list_text | plain_text | DIVERGENT | doors_disagree | oracle "\"north\"" vs emitter "ABSENT" |
| level_head | list_text | json_object | IDENTICAL | lossless | {"key":1} |
| level_head | list_text | json_array | IDENTICAL | lossless | [1,2] |
| level_head | list_text | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "1" |
| level_head | list_text | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| level_head | list_text | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | list_text | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| level_head | undeclared | int | IDENTICAL | lossless | 4 |
| level_head | undeclared | float | IDENTICAL | lossless | 1.5 |
| level_head | undeclared | numeric_text | IDENTICAL | lossless | "4" |
| level_head | undeclared | plain_text | IDENTICAL | lossless | "north" |
| level_head | undeclared | json_object | IDENTICAL | lossless | {"key":1} |
| level_head | undeclared | json_array | IDENTICAL | lossless | [1,2] |
| level_head | undeclared | bool | DIVERGENT | doors_disagree | oracle "true" vs emitter "\"1\"" |
| level_head | undeclared | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| level_head | undeclared | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| level_head | undeclared | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| edge_head | int | int | IDENTICAL | lossless | 4 |
| edge_head | int | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | json_object | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | json_array | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| edge_head | int | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | int | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | float | IDENTICAL | lossless | 1.5 |
| edge_head | float | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | json_object | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | json_array | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | wide_int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| edge_head | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| edge_head | text | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | numeric_text | IDENTICAL | lossless | "4" |
| edge_head | text | plain_text | IDENTICAL | lossless | "north" |
| edge_head | text | json_object | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | json_array | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | wide_int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | text | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | json_object | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | json_array | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | bool | IDENTICAL | lossless | true |
| edge_head | bool | wide_int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | bool | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | json_object | IDENTICAL | lossless | {"key":1} |
| edge_head | json | json_array | IDENTICAL | lossless | [1,2] |
| edge_head | json | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | wide_int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | json | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | float | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | numeric_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | plain_text | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | json_object | IDENTICAL | lossless | {"key":1} |
| edge_head | list_text | json_array | IDENTICAL | lossless | [1,2] |
| edge_head | list_text | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | wide_int | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | float_integral | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | list_text | neg_zero | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | undeclared | int | IDENTICAL | lossless | 4 |
| edge_head | undeclared | float | IDENTICAL | lossless | 1.5 |
| edge_head | undeclared | numeric_text | IDENTICAL | lossless | "4" |
| edge_head | undeclared | plain_text | IDENTICAL | lossless | "north" |
| edge_head | undeclared | json_object | IDENTICAL | lossless | {"key":1} |
| edge_head | undeclared | json_array | IDENTICAL | lossless | [1,2] |
| edge_head | undeclared | bool | NAMED_REFUSAL | compiler_only | edge_head_column_type_mismatch |
| edge_head | undeclared | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
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
| aggregate_head | int | float | IDENTICAL | lossless | 1.5 |
| aggregate_head | int | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | int | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| aggregate_head | int | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | int | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| aggregate_head | float | int | IDENTICAL | lossless | 4 |
| aggregate_head | float | float | IDENTICAL | lossless | 1.5 |
| aggregate_head | float | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | float | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| aggregate_head | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| aggregate_head | text | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "\"4\"" |
| aggregate_head | text | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "\"1.5\"" |
| aggregate_head | text | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | text | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| aggregate_head | text | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "\"1.0\"" |
| aggregate_head | text | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "\"0.0\"" |
| aggregate_head | bool | int | DIVERGENT | doors_disagree | oracle "4" vs emitter "ABSENT" |
| aggregate_head | bool | float | DIVERGENT | doors_disagree | oracle "1.5" vs emitter "ABSENT" |
| aggregate_head | bool | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | bool | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| aggregate_head | bool | float_integral | DIVERGENT | doors_disagree | oracle "1" vs emitter "true" |
| aggregate_head | bool | neg_zero | DIVERGENT | doors_disagree | oracle "0" vs emitter "false" |
| aggregate_head | json | int | IDENTICAL | lossless | 4 |
| aggregate_head | json | float | IDENTICAL | lossless | 1.5 |
| aggregate_head | json | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | json | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| aggregate_head | json | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | json | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| aggregate_head | list_text | int | IDENTICAL | lossless | 4 |
| aggregate_head | list_text | float | IDENTICAL | lossless | 1.5 |
| aggregate_head | list_text | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | list_text | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| aggregate_head | list_text | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | list_text | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| aggregate_head | undeclared | int | IDENTICAL | lossless | 4 |
| aggregate_head | undeclared | float | IDENTICAL | lossless | 1.5 |
| aggregate_head | undeclared | numeric_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | plain_text | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | json_object | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | json_array | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | bool | NAMED_REFUSAL | compiler_only | aggregate_operand_not_number |
| aggregate_head | undeclared | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| aggregate_head | undeclared | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| aggregate_head | undeclared | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| join_column | int | int | IDENTICAL | lossless | 4 |
| join_column | int | float | IDENTICAL | lossless | 1.5 |
| join_column | int | numeric_text | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "4" vs oracle "\"4\"" |
| join_column | int | plain_text | IDENTICAL | lossless | "north" |
| join_column | int | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| join_column | int | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| join_column | int | bool | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1" vs oracle "true" |
| join_column | int | wide_int | DIVERGENT | emitter_run_error | RangeError: Received integer which cannot be safely represented as a JavaScript number |
| join_column | int | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| join_column | int | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| join_column | float | int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored 4 |
| join_column | float | float | IDENTICAL | lossless | 1.5 |
| join_column | float | numeric_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | float | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | float | json_object | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | float | json_array | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | float | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | float | wide_int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored 9007199254740992 |
| join_column | float | float_integral | SILENT_COERCION | value_changed | fed 1.0, graded 1 |
| join_column | float | neg_zero | SILENT_COERCION | value_changed | fed -0.0, graded 0 |
| join_column | text | int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"4\"" vs oracle "4" |
| join_column | text | float | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1.5\"" vs oracle "1.5" |
| join_column | text | numeric_text | IDENTICAL | lossless | "4" |
| join_column | text | plain_text | IDENTICAL | lossless | "north" |
| join_column | text | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| join_column | text | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| join_column | text | bool | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1\"" vs oracle "true" |
| join_column | text | wide_int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"9007199254740992\"" vs oracle "9007199254740992" |
| join_column | text | float_integral | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1\"" vs oracle "1" |
| join_column | text | neg_zero | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"0\"" vs oracle "0" |
| join_column | bool | int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | bool | float | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | bool | numeric_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | bool | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | bool | json_object | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | bool | json_array | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | bool | bool | IDENTICAL | lossless | true |
| join_column | bool | wide_int | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | bool | float_integral | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | bool | neg_zero | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 type_arrival_shape_mismatch, golden error); emitter compiled and stored emitter run_error |
| join_column | json | int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "4" vs oracle "4" |
| join_column | json | float | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1.5" vs oracle "1.5" |
| join_column | json | numeric_text | SILENT_COERCION | value_changed | fed "4", graded 4 |
| join_column | json | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 json_arrival, golden ); emitter compiled and stored ABSENT |
| join_column | json | json_object | IDENTICAL | lossless | {"key":1} |
| join_column | json | json_array | IDENTICAL | lossless | [1,2] |
| join_column | json | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 golden_oracle, golden ); emitter compiled and stored ABSENT |
| join_column | json | wide_int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "9007199254740992" vs oracle "9007199254740992" |
| join_column | json | float_integral | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1" vs oracle "1" |
| join_column | json | neg_zero | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "0" vs oracle "0" |
| join_column | list_text | int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "4" vs oracle "4" |
| join_column | list_text | float | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1.5" vs oracle "1.5" |
| join_column | list_text | numeric_text | SILENT_COERCION | value_changed | fed "4", graded 4 |
| join_column | list_text | plain_text | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 json_arrival, golden ); emitter compiled and stored ABSENT |
| join_column | list_text | json_object | IDENTICAL | lossless | {"key":1} |
| join_column | list_text | json_array | IDENTICAL | lossless | [1,2] |
| join_column | list_text | bool | DIVERGENT | oracle_only_refusal | both oracle doors refuse (dl6 golden_oracle, golden ); emitter compiled and stored ABSENT |
| join_column | list_text | wide_int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "9007199254740992" vs oracle "9007199254740992" |
| join_column | list_text | float_integral | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "1" vs oracle "1" |
| join_column | list_text | neg_zero | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "0" vs oracle "0" |
| join_column | undeclared | int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"4\"" vs oracle "4" |
| join_column | undeclared | float | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1.5\"" vs oracle "1.5" |
| join_column | undeclared | numeric_text | IDENTICAL | lossless | "4" |
| join_column | undeclared | plain_text | IDENTICAL | lossless | "north" |
| join_column | undeclared | json_object | SILENT_COERCION | value_changed | fed {"key":1}, graded "{\"key\":1}" |
| join_column | undeclared | json_array | SILENT_COERCION | value_changed | fed [1,2], graded "[1,2]" |
| join_column | undeclared | bool | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1\"" vs oracle "true" |
| join_column | undeclared | wide_int | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"9007199254740992\"" vs oracle "9007199254740992" |
| join_column | undeclared | float_integral | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"1\"" vs oracle "1" |
| join_column | undeclared | neg_zero | DIVERGENT | emitter_modes_disagree | incremental "ABSENT" vs naive "\"0\"" vs oracle "0" |
| seed | int_vs_text | int | NAMED_REFUSAL | compiler_only | join_column_type_mismatch |
| seed | int | plain_text | NAMED_REFUSAL | compiler_only | decl_type_conflicts_witness |

