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
  [ deltas(sample/1, [ [ +sample(1.0e-7) ] ]),
    deltas(sample/1, [ [ +sample(1.2345678901234567e20) ] ]),
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
