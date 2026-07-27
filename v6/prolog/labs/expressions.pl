% expressions.pl : the tier-0 expression layer for the v6 surface.
%
% Run:     swipl -q -l v6/prolog/labs/expressions.pl -g go -g halt
% Trace:   swipl -q -l v6/prolog/labs/expressions.pl -g report -g halt
%
% Why this lab exists. plans/2026-07-27-lab-consolidation.md records the audit
% verdict: the candidate surface has NO expression syntax at all, while 166 of
% 173 v5 corpus files use arithmetic or comparison and 69 use string
% interpolation. merge_family.md closes by proposing head-position expressions
% (`counter(Name, Total + 1)`) and immediately parks them, because in a language
% with terms in columns `Total + 1` reads two ways: EVALUATE it to a number, or
% STORE the structure `+(Total, 1)`. That collision is this lab's centre.
%
% What is settled here, in one line each:
%   - evaluation is the default; `quote(...)` is the only way to store structure
%   - `:=` binds, `==` compares; `=` is not mode-polymorphic
%   - comparisons are GOALS, never values
%   - `+` is Int-only; string building goes through interpolation
%   - interpolation auto-converts Int, rejects Term / enum / struct
%   - functions are pure Term -> Term; a rel name in an expression is an error
%
% Verdict, deviations, stdlib table, and the numbered ambiguities are in
% v6/prolog/labs/expressions.md.
%
% READING THE ENCODING (deviations forced by prolog's reader, see the .md):
%   a bare lowercase atom in an argument or expression is a VARIABLE
%   lit(5) / str("x")   are literals
%   _                   is a wildcard column
%   istr("a ${b}")      is the surface `"a ${b}"`, marked because prolog's
%                       reader has no interpolation
%   \=                  is the surface `!=`, because `!` is a solo char and
%                       `!=` does not tokenize
%   The HM checker is the copy of books/v6/enum_match.pl asked for, grown a
%   second judgment: expressions have types, goals do not.

:- use_module(library(lists)).
:- use_module(library(apply)).

:- op(700, xfx, <=).            % surface `<=`; prolog spells its own `=<`
                                % `:=` already exists at 800 xfx, reused as-is

:- discontiguous rule_of/3, fact_of/2, check/2.

% ═══ 1. TYPES ══════════════════════════════════════════════════════════════
% Tier-0 types: int, str, term, list(T). No Bool: predicates are goals, so a
% boolean value never needs to exist (see .md ambiguity 5).
%
% `term` is OPAQUE. Nothing unifies with it but itself, which is what makes the
% collision decidable by ordinary typing instead of by type-directed
% elaboration.

displayable(int).
displayable(str).

% ═══ 2. THE STDLIB ═════════════════════════════════════════════════════════
% stdlib(Name, ArgTypes, ResultType). 13 names, 14 rows, all pure.
% Corpus receipts for each are in the .md table.

stdlib(len,          [str],                int).
stdlib(len,          [list(_Element)],     int).
stdlib(lower,        [str],                str).
stdlib(trim,         [str],                str).
stdlib(split,        [str, str, int],      str).
stdlib(join,         [list(str), str],     str).
stdlib(replace,      [str, str, str],      str).
stdlib(strip_prefix, [str, str],           str).
stdlib(strip_suffix, [str, str],           str).
stdlib(to_str,       [int],                str).
stdlib(abs,          [int],                int).
stdlib(digest,       [_Anything],          str).
stdlib(apply,        [term, Result],       Result).
% concat/1 is variadic over the displayable CLASS, so it cannot be a row here;
% it gets its own typing clause. It is the desugaring target of interpolation.

arith_op((+)).
arith_op((-)).
arith_op((*)).
arith_op((/)).           % truncating integer division; there are no floats
arith_op((mod)).

compare_op((<)).
compare_op((<=)).
compare_op((>)).
compare_op((>=)).
compare_op((==)).
compare_op((\=)).        % surface `!=`

ordered_op((<)).
ordered_op((<=)).
ordered_op((>)).
ordered_op((>=)).

% ═══ 3. REL DECLARATIONS ═══════════════════════════════════════════════════
% rel_decl(Name, [Column-Type, ...]). Column types are REQUIRED (LANG.md:17),
% and that requirement is exactly what keeps the overloaded `len` principal
% (graded by len_overload_resolves_by_declared_column_type).

rel_decl(callee_set_size,      [file-str, size-int]).
rel_decl(shared_count,         [left-str, right-str, shared-int]).
rel_decl(union_size,           [left-str, right-str, size-int]).
rel_decl(jaccard,              [left-str, right-str, percent-int]).

rel_decl(eprintln_hit,         [path-str, line_number-int]).
rel_decl(eprintln_waiver_line, [path-str, waiver_line-int]).
rel_decl(eprintln_waived,      [path-str, line_number-int]).
rel_decl(waiver_note,          [path-str, line_number-int, message-str]).

rel_decl(module_edge,          [from_file-str, to_file-str]).
rel_decl(layer,                [prefix-str, tier-str]).
rel_decl(file_stem,            [file-str, stem-str, tier-str]).

rel_decl(seen,                 [name-str, total-int, delta-int]).
rel_decl(counter,              [name-str, total-int]).
rel_decl(counter_patch,        [name-str, patch-term]).

rel_decl(queued,               [id-str, delta-int]).
rel_decl(pending,              [id-str, patch-term]).
rel_decl(base_value,           [id-str, value-int]).
rel_decl(optimistic,           [id-str, value-int]).

rel_decl(edge,                 [parent-str, child-str]).
rel_decl(depth,                [node-str, level-int]).

rel_decl(bundle,               [name-str, parts-list(str)]).

% ═══ 4. STRING INTERPOLATION ═══════════════════════════════════════════════
% Desugaring is SYNTACTIC and happens before typing. It never consults a
% column type, which is the same discipline the collision rule follows.
%
%   "banned word ${word} at ${path}"
%     -> concat([str("banned word "), word, str(" at "), path])

interp_desugar(Text, concat(Parts)) :-
    string_codes(Text, Codes),
    scan_interp(Codes, [], Pieces),
    maplist(piece_expr, Pieces, Parts).

piece_expr(text(Text), str(Text)).
piece_expr(hole(Name), Name).

scan_interp([], Reversed, Pieces) :- !,
    close_text(Reversed, [], Pieces).
scan_interp([0'$, 0'{ | Rest], Reversed, Pieces) :- !,
    take_hole_name(Rest, NameCodes, After),
    (   NameCodes == []
    ->  throw(empty_interpolation_hole)
    ;   true ),
    atom_codes(Name, NameCodes),
    scan_interp(After, [], Tail),
    close_text(Reversed, [hole(Name) | Tail], Pieces).
scan_interp([Code | Rest], Reversed, Pieces) :-
    scan_interp(Rest, [Code | Reversed], Pieces).

close_text([], Tail, Tail) :- !.
close_text(Reversed, Tail, [text(Text) | Tail]) :-
    reverse(Reversed, Codes),
    string_codes(Text, Codes).

take_hole_name([], _, _) :- !,
    throw(unterminated_interpolation_hole).
take_hole_name([0'} | Rest], [], Rest) :- !.
take_hole_name([Code | Rest], [Code | Name], After) :-
    take_hole_name(Rest, Name, After).

% ═══ 5. TYPING: expressions have types, goals do not ═══════════════════════
% expr_type(+Env, +Expr, -Type). Env is a list of Name-Type.
% Every rejection throws a NAMED term so the grader can assert which law fired.

expr_type(_, Wildcard, _) :- var(Wildcard), !,
    throw(wildcard_in_expression).
expr_type(_, lit(Number), int) :- !,
    (   number(Number) -> true ; throw(bad_literal(lit(Number))) ).
expr_type(_, str(Text), str) :- !,
    (   string(Text) -> true ; throw(bad_literal(str(Text))) ).
expr_type(Env, Name, Type) :- atom(Name), !,
    (   memberchk(Name-Type, Env)
    ->  true
    ;   throw(unbound_variable(Name)) ).

% THE COLLISION RULE, half one: quote stores structure. Its type is `term`
% regardless of what is inside, so no type-directed elaboration is needed.
% Range restriction still applies to the leaves: quote freezes the OPERATOR,
% not the variables.
expr_type(Env, quote(Inner), term) :- !,
    quote_range_check(Env, Inner).

expr_type(Env, istr(Text), str) :- !,
    interp_desugar(Text, Concat),
    expr_type(Env, Concat, str).

expr_type(Env, concat(Parts), str) :- !,
    (   is_list(Parts) -> true ; throw(bad_concat(Parts)) ),
    maplist(displayable_part(Env), Parts).

% THE COLLISION RULE, half two: a bare operator EVALUATES. Int-only, so
% `str("a") + str("b")` is rejected rather than silently concatenating (the v5
% overload this deliberately drops).
expr_type(Env, Expr, int) :-
    compound(Expr), Expr =.. [Operator, Left, Right], arith_op(Operator), !,
    expect_type(Env, Left, int, Expr),
    expect_type(Env, Right, int, Expr).

expr_type(_, Expr, _) :-
    compound(Expr), Expr =.. [Operator, _, _], compare_op(Operator), !,
    throw(comparison_in_value_position(Expr)).

% pure function application, and the purity law's enforcement point
expr_type(Env, Call, Result) :-
    compound(Call), Call =.. [Name | Args], length(Args, Arity),
    (   rel_decl(Name, Columns), length(Columns, Arity)
    ->  throw(rel_read_in_expression(Name/Arity))
    ;   true ),
    (   stdlib(Name, _, _)
    ->  true
    ;   throw(unknown_function(Name/Arity)) ),
    maplist(expr_type(Env), Args, GotTypes),
    findall(Signature,
            ( stdlib(Name, ArgTypes, ResultType),
              length(ArgTypes, Arity),
              \+ \+ ArgTypes = GotTypes,
              Signature = ArgTypes-ResultType ),
            Signatures),
    (   Signatures == []
    ->  throw(no_signature(Name/Arity, GotTypes))
    ;   true ),
    member(WantedTypes-Result, Signatures),
    WantedTypes = GotTypes.

expect_type(Env, Expr, Wanted, Where) :-
    expr_type(Env, Expr, Got),
    (   Got = Wanted -> true ; throw(type_clash(Wanted, Got, in(Where))) ).

displayable_part(Env, Part) :-
    expr_type(Env, Part, Type),
    (   displayable(Type)
    ->  true
    ;   throw(not_displayable(Type, in(Part))) ).

% `it` is the one reserved name: inside quote it stands for the value `apply`
% will later supply. Everything else inside quote obeys range restriction.
quote_range_check(_, it) :- !.
quote_range_check(_, lit(_)) :- !.
quote_range_check(_, str(_)) :- !.
quote_range_check(_, Wildcard) :- var(Wildcard), !,
    throw(wildcard_in_expression).
quote_range_check(Env, Name) :- atom(Name), !,
    (   memberchk(Name-_, Env) -> true ; throw(unbound_variable(Name)) ).
quote_range_check(Env, Compound) :-
    Compound =.. [_ | Args],
    maplist(quote_range_check(Env), Args).

% ── goal typing: a goal constrains, it does not produce a value ────────────
% goal_check(+Goal, +EnvIn, -EnvOut). Only atoms and `:=` extend the env.

goal_check(Binding, Env0, Env) :- nonvar(Binding), Binding = (Name := Expr), !,
    (   atom(Name) -> true ; throw(bad_binding_target(Name)) ),
    (   memberchk(Name-_, Env0) -> throw(rebinding(Name)) ; true ),
    expr_type(Env0, Expr, Type),
    Env = [Name-Type | Env0].
goal_check(Goal, Env, Env) :-
    compound(Goal), Goal =.. [Operator, Left, Right], compare_op(Operator), !,
    expr_type(Env, Left, LeftType),
    expr_type(Env, Right, RightType),
    (   LeftType = RightType
    ->  true
    ;   throw(type_clash(LeftType, RightType, in(Goal))) ),
    (   ordered_op(Operator)
    ->  (   LeftType == int -> true ; throw(unordered_type(LeftType, in(Goal))) )
    ;   true ).
goal_check(Atom, Env0, Env) :-
    Atom =.. [Rel | Args],
    (   rel_decl(Rel, Columns) -> true ; throw(unknown_rel(Rel)) ),
    (   same_length(Args, Columns) -> true ; throw(arity_mismatch(Rel)) ),
    arg_env(Args, Columns, Env0, Env).

arg_env([], [], Env, Env).
arg_env([Arg | Args], [_Column-Type | Columns], Env0, Env) :-
    arg_bind(Arg, Type, Env0, Env1),
    arg_env(Args, Columns, Env1, Env).

arg_bind(Wildcard, _, Env, Env) :- var(Wildcard), !.
arg_bind(lit(Number), Type, Env, Env) :- !,
    (   Type == int, number(Number)
    ->  true
    ;   throw(type_clash(Type, int, in(lit(Number)))) ).
arg_bind(str(Text), Type, Env, Env) :- !,
    (   Type == str, string(Text)
    ->  true
    ;   throw(type_clash(Type, str, in(str(Text)))) ).
arg_bind(Name, Type, Env0, Env) :- atom(Name), !,
    (   memberchk(Name-Known, Env0)
    ->  (   Known = Type -> Env = Env0 ; throw(type_clash(Known, Type, in(Name))) )
    ;   Env = [Name-Type | Env0] ).
% v5's law, restated: an expression never appears in a binding atom. It goes in
% a `:=` or in the head.
arg_bind(Other, _, _, _) :-
    throw(expression_in_atom_argument(Other)).

% ── rule typing ────────────────────────────────────────────────────────────

rule_check(Head, Body) :-
    recursive_head_check(Head, Body),
    body_env(Body, [], Env),
    head_check(Head, Env).

body_env([], Env, Env).
body_env([Goal | Rest], Env0, Env) :-
    goal_check(Goal, Env0, Env1),
    body_env(Rest, Env1, Env).

head_check(Head, Env) :-
    Head =.. [Rel | Exprs],
    (   rel_decl(Rel, Columns) -> true ; throw(unknown_rel(Rel)) ),
    (   same_length(Exprs, Columns) -> true ; throw(arity_mismatch(Rel)) ),
    head_columns(Env, Exprs, Columns).

head_columns(_, [], []).
head_columns(Env, [Expr | Exprs], [Column-Type | Columns]) :-
    expr_type(Env, Expr, Got),
    (   Got = Type
    ->  true
    ;   throw(type_clash(Type, Got, in(column(Column), Expr))) ),
    head_columns(Env, Exprs, Columns).

% Tier ruling: datalog terminates because heads only ever move existing values
% around. An arithmetic head inside a recursive stratum breaks that (AUDIT 18f;
% std/entry.dl caps its recursion at depth 64 by hand for this reason). Direct
% self-reference only in this lab; the mutual case wants the stratum SCC.
recursive_head_check(Head, Body) :-
    functor(Head, Rel, Arity),
    (   ( member(Goal, Body), nonvar(Goal), functor(Goal, Rel, Arity) )
    ->  (   ( Head =.. [_ | Exprs], member(Expr, Exprs),
              compound(Expr), \+ literal_form(Expr) )
        ->  throw(head_expression_in_recursive_rule(Rel/Arity))
        ;   true )
    ;   true ).

literal_form(lit(_)).
literal_form(str(_)).

program_check(Program) :-
    forall(rule_of(Program, Head, Body), rule_check(Head, Body)).

% ═══ 6. THE REFERENCE INTERPRETER (level rules only) ═══════════════════════
% Expressions evaluate PER BINDING ROW, after the joins that bind their
% variables. Body goals run left to right: an atom binds, a `:=` computes, a
% comparison filters. The head evaluates last.

eval_expr(_, Wildcard, _) :- var(Wildcard), !,
    throw(wildcard_in_expression).
eval_expr(_, lit(Number), Number) :- !.
eval_expr(_, str(Text), Text) :- !.
eval_expr(Row, Name, Value) :- atom(Name), !,
    (   memberchk(Name-Value, Row) -> true ; throw(unbound_variable(Name)) ).
eval_expr(Row, quote(Inner), Frozen) :- !,
    quote_value(Row, Inner, Frozen).
eval_expr(Row, istr(Text), Value) :- !,
    interp_desugar(Text, Concat),
    eval_expr(Row, Concat, Value).
eval_expr(Row, concat(Parts), Value) :- !,
    maplist(eval_expr(Row), Parts, Values),
    maplist(display_value, Values, Texts),
    join_strings(Texts, "", Value).
eval_expr(Row, Expr, Value) :-
    compound(Expr), Expr =.. [Operator, Left, Right], arith_op(Operator), !,
    eval_expr(Row, Left, LeftValue),
    eval_expr(Row, Right, RightValue),
    arith_apply(Operator, LeftValue, RightValue, Value).
eval_expr(Row, Call, Value) :-
    compound(Call), Call =.. [Name | Args],
    maplist(eval_expr(Row), Args, Values),
    apply_fn(Name, Values, Value).

arith_apply((+),   Left, Right, Value) :- Value is Left + Right.
arith_apply((-),   Left, Right, Value) :- Value is Left - Right.
arith_apply((*),   Left, Right, Value) :- Value is Left * Right.
arith_apply((/),   Left, Right, Value) :- Value is Left // Right.
arith_apply((mod), Left, Right, Value) :- Value is Left mod Right.

% quote builds a term: leaves are substituted from the binding row, the
% OPERATOR is the thing left unapplied. `it` survives as itself.
quote_value(_, it, it) :- !.
quote_value(_, lit(Number), Number) :- !.
quote_value(_, str(Text), Text) :- !.
quote_value(Row, Name, Value) :- atom(Name), !,
    (   memberchk(Name-Value, Row) -> true ; throw(unbound_variable(Name)) ).
quote_value(Row, Compound, t(Functor, Values)) :-
    compound(Compound), Compound =.. [Functor | Args],
    maplist(quote_value(Row), Args, Values).

% ── the stdlib runtime ─────────────────────────────────────────────────────

apply_fn(len, [Value], Length) :- !,
    (   is_list(Value) -> length(Value, Length) ; string_length(Value, Length) ).
apply_fn(lower, [Text], Result) :- !,
    string_lower(Text, Result).
apply_fn(trim, [Text], Result) :- !,
    split_string(Text, "", " \t\n\r", [Result]).
apply_fn(split, [Text, Separator, Index], Result) :- !,
    split_on(Text, Separator, Parts),
    signed_nth(Index, Parts, Result).
apply_fn(join, [Parts, Separator], Result) :- !,
    join_strings(Parts, Separator, Result).
apply_fn(replace, [Text, From, To], Result) :- !,
    split_on(Text, From, Parts),
    join_strings(Parts, To, Result).
apply_fn(strip_prefix, [Text, Prefix], Result) :- !,
    (   string_concat(Prefix, Rest, Text) -> Result = Rest ; Result = Text ).
apply_fn(strip_suffix, [Text, Suffix], Result) :- !,
    (   string_concat(Rest, Suffix, Text) -> Result = Rest ; Result = Text ).
apply_fn(to_str, [Number], Result) :- !,
    number_string(Number, Result).
apply_fn(abs, [Number], Result) :- !,
    Result is abs(Number).
apply_fn(digest, [Value], Result) :- !,
    term_to_atom(Value, Atom),
    atom_codes(Atom, Codes),
    foldl(digest_step, Codes, 5381, Accumulated),
    format(string(Result), "~16r", [Accumulated]).
apply_fn(apply, [Patch, Base], Result) :- !,
    patch_eval(Patch, Base, Result).
apply_fn(Name, Values, _) :-
    length(Values, Arity),
    throw(no_runtime_for(Name/Arity)).

digest_step(Code, Accumulated0, Accumulated) :-
    Accumulated is (Accumulated0 * 33 + Code) mod 0xffffffff.

% `apply` substitutes Base for `it` and runs the frozen operators. This is the
% optimistic-update shape: the patch is stored as data in a term column, moved
% around like any value, and applied against whatever base exists at use time.
patch_eval(it, Base, Base) :- !.
patch_eval(t(Operator, Args), Base, Value) :- !,
    maplist(patch_arg(Base), Args, Values),
    (   arith_op(Operator), Values = [Left, Right]
    ->  arith_apply(Operator, Left, Right, Value)
    ;   throw(unapplicable_patch(Operator)) ).
patch_eval(Atomic, _, Atomic).

patch_arg(Base, Arg, Value) :- patch_eval(Arg, Base, Value).

display_value(Value, Text) :-
    (   number(Value) -> number_string(Value, Text)
    ;   string(Value) -> Text = Value
    ;   throw(not_displayable_value(Value)) ).

split_on(Text, Separator, [Head | Rest]) :-
    (   sub_string(Text, Before, _, After, Separator)
    ->  sub_string(Text, 0, Before, _, Head),
        sub_string(Text, _, After, 0, Tail),
        split_on(Tail, Separator, Rest)
    ;   Head = Text, Rest = [] ).

join_strings([], _, "").
join_strings([Only], _, Only) :- !.
join_strings([Head | Rest], Separator, Result) :-
    join_strings(Rest, Separator, Tail),
    string_concat(Head, Separator, WithSeparator),
    string_concat(WithSeparator, Tail, Result).

signed_nth(Index, Parts, Result) :-
    (   Index >= 0
    ->  Position = Index
    ;   length(Parts, Count), Position is Count + Index ),
    nth0(Position, Parts, Result).

% ── body solving and the level fixpoint ────────────────────────────────────

solve_body([], _, Row, Row).
solve_body([Goal | Rest], Rows, Row0, Row) :-
    solve_goal(Goal, Rows, Row0, Row1),
    solve_body(Rest, Rows, Row1, Row).

solve_goal(Binding, _, Row0, Row) :- nonvar(Binding), Binding = (Name := Expr), !,
    eval_expr(Row0, Expr, Value),
    Row = [Name-Value | Row0].
solve_goal(Goal, _, Row, Row) :-
    compound(Goal), Goal =.. [Operator, Left, Right], compare_op(Operator), !,
    eval_expr(Row, Left, LeftValue),
    eval_expr(Row, Right, RightValue),
    compare_values(Operator, LeftValue, RightValue).
solve_goal(Atom, Rows, Row0, Row) :-
    Atom =.. [Rel | Args],
    member(Fact, Rows),
    Fact =.. [Rel | Values],
    match_args(Args, Values, Row0, Row).

compare_values((<),  Left, Right) :- Left <  Right.
compare_values((<=), Left, Right) :- Left =< Right.
compare_values((>),  Left, Right) :- Left >  Right.
compare_values((>=), Left, Right) :- Left >= Right.
compare_values((==), Left, Right) :- Left == Right.
compare_values((\=), Left, Right) :- Left \== Right.

match_args([], [], Row, Row).
match_args([Arg | Args], [Value | Values], Row0, Row) :-
    match_arg(Arg, Value, Row0, Row1),
    match_args(Args, Values, Row1, Row).

match_arg(Wildcard, _, Row, Row) :- var(Wildcard), !.
match_arg(lit(Number), Value, Row, Row) :- !, Number == Value.
match_arg(str(Text), Value, Row, Row) :- !, Text == Value.
match_arg(Name, Value, Row0, Row) :- atom(Name), !,
    (   memberchk(Name-Bound, Row0)
    ->  Bound == Value, Row = Row0
    ;   Row = [Name-Value | Row0] ).
match_arg(Other, _, _, _) :-
    throw(expression_in_atom_argument(Other)).

eval_head(Head, Row, Fact) :-
    Head =.. [Rel | Exprs],
    maplist(eval_expr(Row), Exprs, Values),
    Fact =.. [Rel | Values].

derive(Program, Rows0, Rows) :-
    findall(Fact,
            ( rule_of(Program, Head, Body),
              solve_body(Body, Rows0, [], Row),
              eval_head(Head, Row, Fact) ),
            Derived),
    append(Rows0, Derived, All),
    sort(All, Rows1),
    (   Rows1 == Rows0
    ->  Rows = Rows0
    ;   derive(Program, Rows1, Rows) ).

run_program(Program, Rows) :-
    findall(Fact, fact_of(Program, Fact), Facts),
    sort(Facts, Rows0),
    derive(Program, Rows0, Rows).

rel_rows(Rel/Arity, Rows, Selected) :-
    findall(Row, ( member(Row, Rows), functor(Row, Rel, Arity) ), Selected).

% ═══ 7. PROGRAMS ═══════════════════════════════════════════════════════════

% ── measure: examples/graph-measure.dl:64-71, the head-arithmetic receipt ──
%
%   union_size(a, b, ua + ub - sh) <-
%       callee_set_size(a, ua), callee_set_size(b, ub), shared_count(a, b, sh);
%   jaccard(a, b, sh * 100 / u) <-
%       shared_count(a, b, sh), union_size(a, b, u), u > 0, sh * 100 / u >= 40;

rule_of(measure, union_size(a, b, ua + ub - sh),
        [ callee_set_size(a, ua)
        , callee_set_size(b, ub)
        , shared_count(a, b, sh) ]).

rule_of(measure, jaccard(a, b, sh * lit(100) / u),
        [ shared_count(a, b, sh)
        , union_size(a, b, u)
        , u > lit(0)
        , sh * lit(100) / u >= lit(40) ]).

fact_of(measure, callee_set_size("db.rs",  10)).
fact_of(measure, callee_set_size("cli.rs",  8)).
fact_of(measure, callee_set_size("ui.rs",   4)).
fact_of(measure, shared_count("db.rs", "cli.rs", 6)).
fact_of(measure, shared_count("db.rs", "ui.rs",  1)).

% ── ratchet: .dl/no-new-eprintln.dl:47-51, the range join, plus a msg column ─
%
%   eprintln_waived(path, line_number) <-
%       eprintln_waiver_line(path, waiver_line), eprintln_hit(path, line_number),
%       waiver_line >= line_number - 1, waiver_line <= line_number;

rule_of(ratchet, eprintln_waived(path, line_number),
        [ eprintln_waiver_line(path, waiver_line)
        , eprintln_hit(path, line_number)
        , waiver_line >= line_number - lit(1)
        , waiver_line <= line_number ]).

rule_of(ratchet, waiver_note(path, line_number,
                             istr("eprintln at ${path}:${line_number} is waived")),
        [ eprintln_waived(path, line_number) ]).

fact_of(ratchet, eprintln_hit("src/db.rs", 40)).
fact_of(ratchet, eprintln_hit("src/db.rs", 88)).
fact_of(ratchet, eprintln_waiver_line("src/db.rs", 39)).

% ── layers: examples/arch-conformance.dl:33, the `:=` bind receipt ──────────
%
%   file_stem(f, s, t) <-
%       module_edge(f, _), layer(p, t), s := strip_prefix(f, p), s != f;

rule_of(layers, file_stem(f, s, t),
        [ module_edge(f, _)
        , layer(p, t)
        , s := strip_prefix(f, p)
        , s \= f ]).

fact_of(layers, module_edge("src/db/conn.rs", "src/cli/main.rs")).
fact_of(layers, layer("src/db/",  "storage")).
fact_of(layers, layer("src/cli/", "ui")).

% ── collision: the same expression in two columns of two different types ───
%
%   counter(name, total + delta)        <- seen(name, total, delta);
%   counter_patch(name, quote(total + delta)) <- seen(name, total, delta);

rule_of(collision, counter(name, total + delta),
        [ seen(name, total, delta) ]).
rule_of(collision, counter_patch(name, quote(total + delta)),
        [ seen(name, total, delta) ]).

fact_of(collision, seen("clicks", 5, 1)).

% the two ill-typed halves of the collision, one program each
rule_of(collision_bare_into_term, counter_patch(name, total + delta),
        [ seen(name, total, delta) ]).
fact_of(collision_bare_into_term, seen("clicks", 5, 1)).

rule_of(collision_quote_into_int, counter(name, quote(total + delta)),
        [ seen(name, total, delta) ]).
fact_of(collision_quote_into_int, seen("clicks", 5, 1)).

% ── optimistic: a stored patch applied against a later base ────────────────
%
%   pending(id, quote(it + delta))          <- queued(id, delta);
%   optimistic(id, apply(patch, value))     <- pending(id, patch),
%                                              base_value(id, value);

rule_of(optimistic, pending(id, quote(it + delta)),
        [ queued(id, delta) ]).
rule_of(optimistic, optimistic(id, apply(patch, value)),
        [ pending(id, patch)
        , base_value(id, value) ]).

fact_of(optimistic, queued("cart", 3)).
fact_of(optimistic, base_value("cart", 10)).

% ── recursive_head: the tier ruling, a rule that must not compile ──────────
%
%   depth(node, d + 1) <- edge(parent, node), depth(parent, d);

rule_of(recursive_head, depth(node, level + lit(1)),
        [ edge(parent, node)
        , depth(parent, level) ]).

% ── two range-restriction violations, one bare and one under quote ─────────

rule_of(unbound_bare, counter(name, total + missing),
        [ seen(name, total, delta) ]).

rule_of(unbound_quoted, counter_patch(name, quote(total + missing)),
        [ seen(name, total, delta) ]).

% ═══ 8. GRADING ════════════════════════════════════════════════════════════

rejects(Goal, Error) :-
    catch(( Goal, fail ), Error, true),
    nonvar(Error).

% ── evaluation semantics ───────────────────────────────────────────────────

% a head expression computes a derived column, per binding row, after the join
check(head_expression_evaluates_derived_column,
  ( run_program(measure, Rows),
    rel_rows(union_size/3, Rows, Sizes),
    Sizes == [ union_size("db.rs", "cli.rs", 12)      % 10 + 8 - 6
             , union_size("db.rs", "ui.rs",  13) ]    % 10 + 4 - 1
  )).

% a comparison filters binding rows and produces no value of its own
check(comparison_filters_rows,
  ( run_program(measure, Rows),
    rel_rows(jaccard/3, Rows, Similar),
    Similar == [ jaccard("db.rs", "cli.rs", 50) ]     % 6*100/12 = 50 >= 40
  )).                                                 % 1*100/13 =  7 dropped

% an expression inside a comparison, over a bound variable from another atom
check(range_join_over_arithmetic,
  ( run_program(ratchet, Rows),
    rel_rows(eprintln_waived/2, Rows, Waived),
    Waived == [ eprintln_waived("src/db.rs", 40) ]    % 39 >= 39 and 39 <= 40
  )).                                                 % line 88: 39 >= 87 false

% `:=` binds a name that later goals AND the head can both read
check(bind_form_binds_for_later_goals,
  ( run_program(layers, Rows),
    rel_rows(file_stem/3, Rows, Stems),
    Stems == [ file_stem("src/db/conn.rs", "conn.rs", "storage") ]
  )).

% ── range restriction ──────────────────────────────────────────────────────

check(unbound_variable_in_expression_rejected,
  ( rejects(program_check(unbound_bare), Error),
    Error == unbound_variable(missing) )).

% quote freezes the operator, not the variables, so range restriction still
% applies underneath it
check(unbound_variable_inside_quote_rejected,
  ( rejects(program_check(unbound_quoted), Error),
    Error == unbound_variable(missing) )).

% ── THE COLLISION, all four directions ─────────────────────────────────────

% (1) default reading: evaluate
check(arithmetic_head_evaluates_by_default,
  ( run_program(collision, Rows),
    rel_rows(counter/2, Rows, Counters),
    Counters == [ counter("clicks", 6) ] )).

% (2) opt-in reading: store the structure, with the LEAVES substituted
check(quote_stores_structure_with_substituted_leaves,
  ( run_program(collision, Rows),
    rel_rows(counter_patch/2, Rows, Patches),
    Patches == [ counter_patch("clicks", t(+, [5, 1])) ] )).

% (3) the ambiguous case, resolved by types not by type-direction: a bare
%     expression aimed at a term column is a clash, never a silent quote
check(bare_arithmetic_into_term_column_rejected,
  ( rejects(program_check(collision_bare_into_term), Error),
    Error = type_clash(term, int, in(column(patch), _)) )).

% (4) and the mirror: a quoted term aimed at an int column
check(quoted_term_into_int_column_rejected,
  ( rejects(program_check(collision_quote_into_int), Error),
    Error = type_clash(int, term, in(column(total), _)) )).

% ── typing ─────────────────────────────────────────────────────────────────

check(well_typed_programs_check,
  ( program_check(measure),
    program_check(ratchet),
    program_check(layers),
    program_check(collision),
    program_check(optimistic) )).

% `+` is Int-only. v5 overloaded it (text + text concatenates) and paid with a
% "mixed int/text is a typecheck error" special case; interpolation replaces it.
check(plus_is_int_only,
  ( rejects(expr_type([], str("x") + lit(1), _), MixedError),
    MixedError = type_clash(int, str, _),
    rejects(expr_type([], str("a") + str("b"), _), TextError),
    TextError = type_clash(int, str, _) )).

check(function_argument_type_rejected,
  ( rejects(expr_type([], lower(lit(3)), _), Error),
    Error == no_signature(lower/1, [int]) )).

check(unknown_function_rejected,
  ( rejects(expr_type([path-str], titlecase(path), _), Error),
    Error == unknown_function(titlecase/1) )).

% a comparison is a goal; putting one in a value position is a compile error,
% not an implicit boolean column
check(comparison_is_not_a_value,
  ( rejects(expr_type([total-int], (total > lit(5)), _), Error),
    Error = comparison_in_value_position(_) )).

% required column types are what keep the overloaded `len` principal: each call
% resolves to exactly ONE signature
check(len_overload_resolves_by_declared_column_type,
  ( findall(Type, expr_type([name-str], len(name), Type), StrTypes),
    StrTypes == [int],
    findall(Type, expr_type([parts-list(str)], len(parts), Type), ListTypes),
    ListTypes == [int] )).

% ── interpolation ──────────────────────────────────────────────────────────

check(interpolation_desugars_to_concat,
  ( interp_desugar("banned word ${word} at ${path}", Desugared),
    Desugared == concat([ str("banned word "), word, str(" at "), path ]) )).

check(interpolation_auto_converts_int,
  ( run_program(ratchet, Rows),
    rel_rows(waiver_note/3, Rows, Notes),
    Notes == [ waiver_note("src/db.rs", 40,
                           "eprintln at src/db.rs:40 is waived") ] )).

% a term has no canonical text form the language may pick, so interpolating one
% is a compile error naming the type
check(interpolation_of_term_rejected,
  ( rejects(expr_type([patch-term], istr("patch=${patch}"), _), Error),
    Error == not_displayable(term, in(patch)) )).

% ── purity ─────────────────────────────────────────────────────────────────

% functions are pure Term -> Term. A rel read is a join, and a join is a body
% atom, so a rel name inside an expression is rejected by name.
check(rel_read_in_expression_rejected,
  ( rejects(expr_type([total-int, name-str, delta-int],
                      total + seen(name, total, delta), _), Error),
    Error == rel_read_in_expression(seen/3) )).

% ── tier placement ─────────────────────────────────────────────────────────

% head arithmetic inside a recursive stratum removes datalog's termination
% guarantee (AUDIT 18f). Rejected here; `pre` breaks the stratum, so keyed
% scans like merge_family's counter are unaffected.
check(head_expression_in_recursive_rule_rejected,
  ( rejects(program_check(recursive_head), Error),
    Error == head_expression_in_recursive_rule(depth/2) )).

% ── what the term column unblocks ──────────────────────────────────────────

check(stored_patch_applies_to_a_later_base,
  ( run_program(optimistic, Rows),
    rel_rows(pending/2, Rows, Pending),
    Pending == [ pending("cart", t(+, [it, 3])) ],
    rel_rows(optimistic/2, Rows, Applied),
    Applied == [ optimistic("cart", 13) ] )).

go :-
    forall(check(Name, Goal),
           ( catch(Goal, Error, (print_message(error, Error), fail))
           -> format("PASS  ~w~n", [Name])
           ;  format("fail  ~w~n", [Name]) )).

% ═══ receipts printer (feeds the .md tables) ═══════════════════════════════

report :-
    forall(member(Program, [measure, ratchet, layers, collision, optimistic]),
           ( format("~w~n", [Program]),
             (   catch(run_program(Program, Rows), Thrown, true)
             ->  (   var(Thrown)
                 ->  forall(member(Row, Rows), format("  ~q~n", [Row]))
                 ;   format("  REJECTED ~q~n", [Thrown]) )
             ;   format("  no solution~n", []) ),
             nl )),
    forall(member(Program, [ collision_bare_into_term, collision_quote_into_int
                           , recursive_head, unbound_bare, unbound_quoted ]),
           ( format("~w~n", [Program]),
             (   rejects(program_check(Program), Error)
             ->  format("  REJECTED ~q~n", [Error])
             ;   format("  ACCEPTED (defect)~n", []) ),
             nl )).
