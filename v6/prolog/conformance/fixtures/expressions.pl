% fixtures/expressions.pl : the expressions lab's EVALUATION checks promoted
% into the shared fixture format. Source: v6/prolog/labs/expressions.pl +
% expressions.md, review: v6/prolog/labs/review_expressions.md.
%
% Scope note (engine.pl is frozen, read before touching this file again):
% engine.pl has no HM/enum type checker, no `quote(...)`/term-column type, no
% stdlib (strip_prefix, len, split, ...), no interpolated-string literal
% (`istr(...)`), and no recursive-head-arithmetic ban. What it DOES have is
% the reference EVALUATOR: `Var := Expr` / `Var is Expr` bind, comparisons
% `< =< > >= == \==` filter (Int arithmetic on the ordered ones, term
% equality on ==/\==), `+ - * / mod` (Int-only, `/` truncates), `concat([..])`
% as the interpolation lowering target, and head values evaluated as
% expressions after the body's joins. Every fixture below promotes one of
% those evaluation behaviors; nowhere here needs a `rel_decl`/column type,
% so every `Decls` list is `[]`, matching fixtures/json_arm.pl's level-only
% style.
%
% not promoted: (type-checker-shaped lab checks; static checker, not in
% reference engine yet; error-term contract stays in the lab until a checker
% promotion exists)
%   - unbound_variable_in_expression_rejected: static range-restriction
%     check, exact error unbound_variable(Name); the engine has no load-time
%     range-restriction pass (only a runtime var-still-unbound throw with a
%     different, nameless shape, see arithmetic_rejects_non_int_operand_at_runtime
%     below for the sibling runtime-throw style that IS promotable)
%   - unbound_variable_inside_quote_rejected: same static check, additionally
%     needs `quote(...)`, which engine.pl does not implement at all
%   - arithmetic_head_evaluates_by_default: the "evaluate" half of the
%     evaluate-vs-store collision; its actual runtime behavior (a head
%     expression evaluated from body bindings) is already covered by
%     head_expression_evaluates_derived_column below, and the half that makes
%     this check meaningful (contrasted against the quote-stores half) needs
%     the term-column type the engine does not have
%   - quote_stores_structure_with_substituted_leaves: needs `quote(...)`
%   - bare_arithmetic_into_term_column_rejected: needs `quote(...)` + the
%     term-column type + the HM checker's type_clash error
%   - quoted_term_into_int_column_rejected: same as above, mirror direction
%   - well_typed_programs_check: runs the whole HM checker over five programs;
%     no checker exists here to run
%   - plus_is_int_only: exact check calls expr_type/3 directly for a static
%     type_clash(int, str, _) error term; the engine enforces Int-only + at
%     RUNTIME instead, with a different throw shape
%     (arith_on_non_int(LeftValue, RightValue)) — that runtime behavior is
%     new coverage added below as
%     arithmetic_rejects_non_int_operand_at_runtime, but it is not a
%     promotion of this lab check
%   - function_argument_type_rejected: needs the stdlib signature table
%   - unknown_function_rejected: needs the stdlib name set
%   - comparison_is_not_a_value: static "comparison in value position" rule;
%     the engine has no value-position concept (untyped), and a comparison
%     compound written where an expression is expected does not throw here,
%     it silently falls through eval_expr's catch-all and is stored as an
%     unevaluated term — a divergence worth flagging to a future checker
%     author, not a fixture (no graded receipt exists for "silently stores
%     the unapplied comparison term" in either lab)
%   - len_overload_resolves_by_declared_column_type: needs stdlib `len` +
%     required column types, neither exists here
%   - interpolation_of_term_rejected: static not_displayable(term, ...)
%     check; needs the term type and the HM checker
%   - rel_read_in_expression_rejected: static purity check; the engine's
%     expressions are plain Prolog terms evaluated over an already-bound
%     row, there is no "is this name a rel" lookup at expression-eval time
%   - head_expression_in_recursive_rule_rejected: the tier-0 stratifier's
%     recursion-ban check; engine.pl has no stratifier, this is explicitly
%     called out as not-in-engine in the promotion brief
%   - stored_patch_applies_to_a_later_base: needs `quote(...)` + `apply`;
%     review_expressions.md section 3 additionally recommends cutting `apply`
%     from the stdlib outright (statically unsound, zero corpus occurrences)
%
% interpolation_auto_converts_int is not listed above: its behavior (an Int
% piece auto-converting to text inside a concat) is exercised directly by
% interpolation_desugars_to_concat below, so it is folded in rather than
% re-listed as a gap.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ head expressions: evaluate a head column from body bindings ═══════════
% examples/graph-measure.dl:64-71 transcribed straight (engine.pl has real
% arithmetic, no stdlib needed for this one). union_size alone first, so the
% "a head expression computes a derived column, per binding row, after the
% body's joins" behavior is graded on its own.

fixture(head_expression_evaluates_derived_column,
  prog([],
       [ (union_size(Left, Right, LeftSize + RightSize - Shared) <-
             callee_set_size(Left, LeftSize),
             callee_set_size(Right, RightSize),
             shared_count(Left, Right, Shared)) ]),
  [ callee_set_size('db.rs', 10),
    callee_set_size('cli.rs', 8),
    callee_set_size('ui.rs', 4),
    shared_count('db.rs', 'cli.rs', 6),
    shared_count('db.rs', 'ui.rs', 1) ],
  [],
  [ final(union_size/3, [ union_size('db.rs', 'cli.rs', 12),    % 10 + 8 - 6
                          union_size('db.rs', 'ui.rs', 13) ]) ]). % 10 + 4 - 1

% jaccard reads union_size (produced by the sibling rule in the SAME program,
% across the plain fixpoint) and a comparison over a head-shaped arithmetic
% expression filters the low-overlap row. Both rows compute; only one row
% survives the >= 40 filter — this is comparison_filters_rows, graded by the
% JACCARD final rows and needing only truncating Int division (no rationals),
% so it promotes whole.

fixture(comparison_filters_rows,
  prog([],
       [ (union_size(Left, Right, LeftSize + RightSize - Shared) <-
             callee_set_size(Left, LeftSize),
             callee_set_size(Right, RightSize),
             shared_count(Left, Right, Shared)),
         (jaccard(Left, Right, Shared * 100 / Union) <-
             shared_count(Left, Right, Shared),
             union_size(Left, Right, Union),
             Union > 0,
             Shared * 100 / Union >= 40) ]),
  [ callee_set_size('db.rs', 10),
    callee_set_size('cli.rs', 8),
    callee_set_size('ui.rs', 4),
    shared_count('db.rs', 'cli.rs', 6),
    shared_count('db.rs', 'ui.rs', 1) ],
  [],
  [ final(jaccard/3, [ jaccard('db.rs', 'cli.rs', 50) ]) ]).   % 6*100/12=50 kept
                                                                % 1*100/13= 7 dropped (>=40 fails)

% ═══ the range join: waiver-style Low =< Line, Line =< High ════════════════
% .dl/no-new-eprintln.dl:47-51 transcribed straight; both comparison sides
% are plain arithmetic over atom-bound columns, exact engine syntax
% (`=<`, not the lab's custom `<=` operator).

fixture(range_join_over_arithmetic,
  prog([],
       [ (eprintln_waived(Path, LineNumber) <-
             eprintln_waiver_line(Path, WaiverLine),
             eprintln_hit(Path, LineNumber),
             WaiverLine >= LineNumber - 1,
             WaiverLine =< LineNumber) ]),
  [ eprintln_hit('src/db.rs', 40),
    eprintln_hit('src/db.rs', 88),
    eprintln_waiver_line('src/db.rs', 39) ],
  [],
  [ final(eprintln_waived/2, [ eprintln_waived('src/db.rs', 40) ]) ]).
                                       % waiver 39: 39 >= 39 and 39 =< 40, kept
                                       % hit 88:    39 >= 87 is false, dropped

% ═══ bind then filter, goals run left to right ══════════════════════════════
% ADAPTED from examples/arch-conformance.dl:33 (the lab's
% bind_form_binds_for_later_goals): the original uses the stdlib function
% strip_prefix, which engine.pl does not implement. Kept the same shape —
% two atoms join (one carrying a wildcard column), a `:=` computes a value
% from names those atoms bound, a comparison filters using the `:=` result,
% and the head reads a name bound by `:=` — using arithmetic instead of a
% stdlib call. This is also the goals-run-left-to-right receipt: solve/2
% resolves body conjunctions left to right (engine.pl:207), so `seen/3`
% must bind Base before `Sum := Base + Extra` can read it, and `:=` must run
% before `Sum > 10` can filter on it; the correct final row only appears
% because that order holds.

fixture(bind_computes_derived_value_then_comparison_filters,
  prog([],
       [ (over_budget(Name, Sum) <-
             seen(Name, Base, _),
             bump(Name, Extra),
             Sum := Base + Extra,
             Sum > 10) ]),
  [ seen(alpha, 10, note_a),
    seen(beta, 4, note_b),
    bump(alpha, 3),
    bump(beta, 1) ],
  [],
  [ final(over_budget/2, [ over_budget(alpha, 13) ]) ]).   % alpha: 10+3=13 > 10, kept
                                                            % beta:  4+1=5, not > 10, dropped

% ═══ interpolation lowering: concat([..]) written directly ═════════════════
% The lab's interpolation_desugars_to_concat grades interp_desugar/2, a
% lab-internal scanner engine.pl does not have (`istr(...)` is not an engine
% form). What IS in engine.pl is the LOWERING TARGET: concat([..]) over mixed
% atom and Int pieces, with Int pieces auto-converting to text
% (engine.pl:135-138, atomic_list_concat). Writing the already-desugared form
% from .dl/no-new-eprintln.dl's waiver_note directly promotes both that
% target and interpolation_auto_converts_int's Int-auto-convert behavior in
% one graded receipt.

fixture(interpolation_desugars_to_concat,
  prog([],
       [ (message(Path, LineNumber, Text) <-
             eprintln_hit(Path, LineNumber),
             Text := concat([ 'eprintln at ', Path, ':', LineNumber, ' is waived' ])) ]),
  [ eprintln_hit('src/db.rs', 40) ],
  [],
  [ final(message/3,
          [ message('src/db.rs', 40, 'eprintln at src/db.rs:40 is waived') ]) ]).

% ═══ arithmetic edge receipts ═══════════════════════════════════════════════
% Verified against the live engine (swipl, not assumed): `/` truncates
% TOWARD ZERO (SWI's default integer_rounding_function), and `mod` takes the
% SIGN OF THE DIVISOR (floored modulo) — `-7 // 2` = -3, not -4; `7 mod -2`
% = -1. `%` from the lab's surface grammar is spelled `mod` here, matching
% engine.pl:143 and the lab's own arith_op((mod)).

fixture(division_truncates_toward_zero_mod_follows_divisor_sign,
  prog([],
       [ (probe(Label, Quotient, Remainder) <-
             division_input(Label, Numerator, Denominator),
             Quotient := Numerator / Denominator,
             Remainder := Numerator mod Denominator) ]),
  [ division_input(pos_pos, 7, 2),
    division_input(neg_numerator, -7, 2),
    division_input(neg_denominator, 7, -2),
    division_input(neg_both, -7, -2) ],
  [],
  [ final(probe/3, [ probe(neg_both, 3, -1),           % -7 / -2 =  3, -7 mod -2 = -1
                     probe(neg_denominator, -3, -1),   %  7 / -2 = -3,  7 mod -2 = -1
                     probe(neg_numerator, -3, 1),       % -7 /  2 = -3, -7 mod  2 =  1
                     probe(pos_pos, 3, 1) ]) ]).         %  7 /  2 =  3,  7 mod  2 =  1

% New engine-only coverage (not a promotion of the lab's plus_is_int_only,
% which grades a static type_clash error): the engine enforces Int-only `+`
% at RUNTIME by throwing arith_on_non_int/2 the moment a non-integer operand
% reaches eval_int2 (engine.pl:148-150). Graded by the exact thrown term.

fixture(arithmetic_rejects_non_int_operand_at_runtime,
  prog([],
       [ (probe(Sum) <-
             text_value(Value),
             Sum := Value + 1) ]),
  [ text_value(not_a_number) ],
  [],
  [ throws(arith_on_non_int(not_a_number, 1)) ]).

% Wave 2 declaration typing: the compiler has no literal witness for this
% relation, so the declaration is the source of its INTEGER storage type.
fixture(typed_int_without_literal_witness,
  prog([ col_type(typed_input/1, value, int) ],
       []),
  [],
  [],
  []).

% A declaration type that disagrees with a concrete witness is a compiler
% refusal. The reference engine still treats the bare relation as a set.
fixture(typed_int_contradicts_text_witness,
  prog([ col_type(typed_conflict/1, value, int) ],
       []),
  [ typed_conflict(text_value) ],
  [],
  []).
