% 5_value_plane.pl : phase-5 bool and finite binary64 value receipts.

:- op(1150, xfx, <-).

fixture(bool_literals_round_trip,
  prog([ col_type(flag/2, name, text),
         col_type(flag/2, enabled, bool) ],
       []),
  [ flag(alpha, bool_lit(true)) ],
  [ [ +flag(beta, bool_lit(false)) ] ],
  [ deltas(flag/2, [ [ +flag(beta, bool_lit(false)) ] ]),
    final(flag/2, [ flag(alpha, bool_lit(true)),
                    flag(beta, bool_lit(false)) ]) ]).

fixture(bool_identity_comparison_filters,
  prog([ col_type(flag/2, name, text),
         col_type(flag/2, enabled, bool),
         col_type(enabled_name/1, name, text) ],
       [ (enabled_name(Name) <-
             flag(Name, Enabled),
             Enabled == bool_lit(true)) ]),
  [ flag(alpha, bool_lit(true)),
    flag(beta, bool_lit(false)) ],
  [],
  [ final(enabled_name/1, [ enabled_name(alpha) ]) ]).

fixture(bool_relation_negation_is_two_valued,
  prog([ col_type(item/1, name, text),
         col_type(disabled/2, name, text),
         col_type(disabled/2, value, bool),
         col_type(active/1, name, text) ],
       [ (active(Name) <-
             item(Name),
             not(disabled(Name, bool_lit(true)))) ]),
  [ item(alpha), item(beta),
    disabled(beta, bool_lit(true)),
    disabled(alpha, bool_lit(false)) ],
  [],
  [ final(active/1, [ active(alpha) ]) ]).

fixture(float_arithmetic_is_binary64,
  prog([ col_type(score/2, name, text),
         col_type(score/2, value, float),
         col_type(adjusted/2, name, text),
         col_type(adjusted/2, value, float) ],
       [ (adjusted(Name, Value + 0.2) <- score(Name, Value)) ]),
  [ score(alpha, 0.1) ],
  [],
  [ final(adjusted/2, [ adjusted(alpha, 0.30000000000000004) ]) ]).

fixture(int_float_arithmetic_keeps_real_result,
  prog([ col_type(measure/3, name, text),
         col_type(measure/3, whole, int),
         col_type(measure/3, fraction, float),
         col_type(total/2, name, text),
         col_type(total/2, value, float) ],
       [ (total(Name, Whole + Fraction) <- measure(Name, Whole, Fraction)) ]),
  [ measure(alpha, 5, 0.5) ],
  [],
  [ final(total/2, [ total(alpha, 5.5) ]) ]).

fixture(float_avg_is_grouped,
  prog([ col_type(score/2, group, text),
         col_type(score/2, value, float),
         col_type(mean/2, group, text),
         col_type(mean/2, value, float) ],
       [ (mean(Group, avg(Value)) <- score(Group, Value)) ]),
  [ score(a, 0.5), score(a, 1.5), score(b, 0.25) ],
  [],
  [ final(mean/2, [ mean(a, 1.0), mean(b, 0.25) ]) ]).

fixture(float_exact_comparison_has_no_epsilon,
  prog([ col_type(score/2, name, text),
         col_type(score/2, value, float),
         col_type(exact/1, name, text) ],
       [ (exact(Name) <-
             score(Name, Value),
             Value == 0.30000000000000004) ]),
  [ score(sum, 0.30000000000000004),
    score(decimal, 0.3) ],
  [],
  [ final(exact/1, [ exact(sum) ]) ]).

fixture(float_exact_join_has_no_epsilon,
  prog([ col_type(left/2, name, text),
         col_type(left/2, value, float),
         col_type(right/2, name, text),
         col_type(right/2, value, float),
         col_type(matched/1, name, text) ],
       [ (matched(Name) <- left(Name, Value), right(Name, Value)) ]),
  [ left(alpha, 0.3),
    left(beta, 0.30000000000000004),
    right(alpha, 0.30000000000000004),
    right(beta, 0.30000000000000004) ],
  [],
  [ final(matched/1, [ matched(beta) ]) ]).

fixture(float_negative_zero_canonical_boundary,
  prog([ col_type(score/1, value, float) ], []),
  [ score(-0.0) ],
  [],
  [ final(score/1, [ score(-0.0) ]) ]).

% JS cannot retain the source spelling distinction between 1 and 1.0.
% The shared binder therefore sends an integer-valued number as bigint.
% REAL affinity must convert that exact value before the typeof CHECK.
fixture(float_integral_value_keeps_real_storage,
  prog([ col_type(score/1, value, float) ], []),
  [ score(1.0) ],
  [ [ +score(2.0) ] ],
  [ deltas(score/1, [ [ +score(2.0) ] ]),
    final(score/1, [ score(1.0), score(2.0) ]) ]).

fixture(float_shortest_round_trip_wire,
  prog([ col_type(sample/1, value, float) ], []),
  [ sample(1.0e20) ],
  [ [ +sample(1.0e-7) ],
    [ +sample(1.2345678901234567e20) ] ],
  [ deltas(sample/1, [ [ +sample(1.0e-7) ],
                       [ +sample(1.2345678901234567e20) ] ]),
    final(sample/1, [ sample(1.0e-7),
                      sample(1.0e20),
                      sample(1.2345678901234567e20) ]) ]).

fixture(float_avg_retracts_to_empty_group,
  prog([ col_type(score/2, group, text),
         col_type(score/2, value, float),
         col_type(mean/2, group, text),
         col_type(mean/2, value, float) ],
       [ (mean(Group, avg(Value)) <- score(Group, Value)) ]),
  [ score(a, 0.5), score(a, 1.5), score(b, 2.0) ],
  [ [ -score(a, 0.5) ],
    [ -score(a, 1.5) ],
    [ +score(a, 3.0) ] ],
  [ final(score/2, [ score(a, 3.0), score(b, 2.0) ]),
    final(mean/2, [ mean(a, 3.0), mean(b, 2.0) ]) ]).

fixture(int_out_of_range_is_named_refusal,
  prog([ col_type(measure/1, value, int) ], []),
  [ measure(9007199254740993) ],
  [],
  [ throws(int_out_of_range(measure/1, value, 9007199254740993)) ]).

fixture(bool_rejects_text_ingress,
  prog([ col_type(flag/1, value, bool) ], []),
  [ flag(true) ],
  [],
  [ throws(type_arrival_shape_mismatch(
               flag/1, value, bool, field_not_bool(true))) ]).

fixture(float_rejects_non_float_ingress,
  prog([ col_type(score/1, value, float) ], []),
  [ score(not_a_number) ],
  [],
  [ throws(type_arrival_shape_mismatch(
               score/1, value, float, field_not_finite_float(not_a_number))) ]).

% ── the widened type gate (ruling type_gate_widening, 2026-07-31) ───────────
% Fail-first, one per movement class. Each of these ran and produced a value
% before the widening; the receipt is that each one now names the mix.
%
% SABOTAGE RECEIPTS, run and recorded rather than asserted. Disabling one gate
% at a time turns exactly the fixtures that gate owns red and nothing else:
%
%   text arm of column_value_shape_error/4  -> text_rejects_number_ingress
%   int arm of the same                     -> int_rejects_fractional_ingress,
%                                              typed_int_contradicts_text_witness
%   the decl-independent wide-integer pass  -> wide_int_refused_at_undeclared_column,
%                                              wide_int_refused_inside_json_document
%   the float widening in canonicalize_column/6
%                                           -> float_widens_integer_ingress,
%                                              float_widens_wide_integer_ingress
%   column_type_assignable/3 in the head wall
%                                           -> head_column_type_conflict_is_refused

% Class 1: a number at a TEXT column. This was the largest divergence family
% in the type matrix -- the reference engine kept the integer and printed 4,
% the emitted program's TEXT affinity stringified it and printed "4".
fixture(text_rejects_number_ingress,
  prog([ col_type(label/1, value, text) ], []),
  [ label(4) ],
  [],
  [ throws(type_arrival_shape_mismatch(
               label/1, value, text, field_not_text(4))) ]).

% Class 2: a non-integer at an INT column. SQLite INTEGER affinity converts a
% REAL only when the conversion is lossless, so 1.5 is a mix and 1.0 is not
% (int_accepts_integral_float below pins the other half).
fixture(int_rejects_fractional_ingress,
  prog([ col_type(measure/1, value, int) ], []),
  [ measure(1.5) ],
  [],
  [ throws(type_arrival_shape_mismatch(
               measure/1, value, int, field_not_int(1.5))) ]).

fixture(int_accepts_integral_float,
  prog([ col_type(measure/1, value, int) ], []),
  [ measure(1.0) ],
  [],
  [ final(measure/1, [ measure(1.0) ]) ]).

% Class 3: THE AFFINITY ACCEPTANCE the ruling names by hand. An integer at a
% REAL column widens to a double and is stored as one, so the tick log and the
% final state both read the widened value, not the integer that arrived.
fixture(float_widens_integer_ingress,
  prog([ col_type(score/1, value, float) ], []),
  [],
  [ [ +score(4) ] ],
  [ deltas(score/1, [ [ +score(4.0) ] ]),
    final(score/1, [ score(4.0) ]) ]).

% Class 4: a wide integer at a column with NO declared type. The refusal is
% decl-independent (ruling wide_int_fate) precisely so this reaches it: an
% undeclared column is stored as TEXT and the emitted program answered three
% different things for this value depending on emitter mode.
fixture(wide_int_refused_at_undeclared_column,
  prog([], []),
  [ untyped(9007199254740993) ],
  [],
  [ throws(int_out_of_range(untyped/1, 1, 9007199254740993)) ]).

% Class 5: a wide integer INSIDE a json document. The scan descends, so the
% value cannot enter through a json column either -- which is the one cell
% that used to reach SQLite and come back out as a driver RangeError naming
% no rel and no column at all.
fixture(wide_int_refused_inside_json_document,
  prog([ col_type(payload/1, document, json) ], []),
  [ payload(obj([field-9007199254740993])) ],
  [],
  [ throws(int_out_of_range(payload/1, document, 9007199254740993)) ]).

% Class 6: a wide integer at a FLOAT column is NOT refused. REAL affinity
% widens it before anything can ask how big it was, so what lands is the
% nearest double -- approximate by construction, and the same approximation
% on both doors.
fixture(float_widens_wide_integer_ingress,
  prog([ col_type(score/1, value, float) ], []),
  [ score(9007199254740993) ],
  [],
  [ final(score/1, [ score(9007199254740992.0) ]) ]).

% Class 7: the HEAD wall. A text-declared body column feeding an int-declared
% head. The engine used to copy the atom into the int column and print it as
% a string where the emitted program printed a number.
fixture(head_column_type_conflict_is_refused,
  prog([ col_type(source/1, name, text),
         col_type(target/1, total, int) ],
       [ (target(Value) <- source(Value)) ]),
  [],
  [],
  [ throws(head_column_type_conflict(target/1, total, int,
                                     source/1, name, text)) ]).

% Class 7b: the head wall's ACCEPTED direction. An int body column feeding a
% float head is the widening the ruling names, so the rule LOADS.
%
% Stated because the expectation says it: the widening happens at the WORLD
% BOUNDARY (canonicalize_world_rows/3), not at derivation, so the reference
% engine carries the integer 4 through the rule unchanged while the emitted
% program's REAL column holds 4.0. Both render `4` -- js_float_text/2 collapses
% an integral double to its integer text -- so the graded bytes agree and the
% difference is invisible to every grading leg there is. It is still a
% difference, and the place it would become visible is a float-only operation
% reading a column the engine filled with an integer.
fixture(head_column_int_widens_into_float,
  prog([ col_type(source/1, count, int),
         col_type(scaled/1, value, float) ],
       [ (scaled(Value) <- source(Value)) ]),
  [ source(4) ],
  [],
  [ final(scaled/1, [ scaled(4) ]) ]).

% Class 7c: the wall reads STORAGE, not spelling. `list(text)` and `json` are
% one column kind, so a value moving between them is not a mix and the rule
% loads.
fixture(head_column_list_and_json_share_storage,
  prog([ col_type(source/1, items, list(text)),
         col_type(copied/1, items, json) ],
       [ (copied(Items) <- source(Items)) ]),
  [ source([a, b]) ],
  [],
  [ final(copied/1, [ copied([a, b]) ]) ]).
