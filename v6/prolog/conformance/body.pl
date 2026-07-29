% body.pl : the value/goal layer of the reference interpreter.
% Expressions (evaluation is the default), json values (braces are the one
% grammar), and body-goal solving. No tick knowledge lives here: solve/2
% reads a ctx(Visible, PreState, Tick) the tick layer builds.

:- module(body,
          [ rel_ref/2, solve/2, body_atoms/2, eval_expr/2, eval_head/2,
            json_canon/2, comparison_goal/1, substitute_goal/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module('../0_body_walk', [walk_body/3]).
:- use_module('../compile/registry', [expression/5]).

:- op(700, xfx, :=).

rel_ref(Atom, Name/Arity) :- functor(Atom, Name, Arity).

% ═══ expressions ════════════════════════════════════════════════════════════
% Evaluation is the default; goals run left to right: an atom binds, := / is
% computes, a comparison filters. / truncates (Int-only law). concat is the
% interpolation lowering target.

eval_expr(Value, _) :- var(Value), !, throw(unbound_in_expression).
eval_expr(Number, Number) :- number(Number), !.
eval_expr(concat(Parts), Out) :- !,
    maplist(eval_expr, Parts, Values),
    maplist(text_piece, Values, Pieces),
    atomic_list_concat(Pieces, Out).
eval_expr(Left + Right, Out)   :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV + RightV.
eval_expr(Left - Right, Out)   :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV - RightV.
eval_expr(Left * Right, Out)   :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV * RightV.
eval_expr(Left / Right, Out)   :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV // RightV.
eval_expr(Left mod Right, Out) :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV mod RightV.
eval_expr(Braces, Canon) :- Braces = {}(_), !, json_canon(Braces, Canon).
eval_expr([Head | Tail], Canon) :- !, json_canon([Head | Tail], Canon).
eval_expr(Value, Value).

eval_int2(Left, Right, LeftV, RightV) :-
    eval_expr(Left, LeftV), eval_expr(Right, RightV),
    ( integer(LeftV), integer(RightV) -> true ; throw(arith_on_non_int(LeftV, RightV)) ).

text_piece(Value, Value) :- atomic(Value), !.
text_piece(Value, _) :- throw(non_display_in_concat(Value)).

% The six comparison functors come from registry.pl's expression/5 (rank R5 of
% plans/2026-07-29-prolog-org-review.md), which is also where the compiler's
% lowering reads them. solve_comparison/1 below stays a clause per operator,
% because those clauses are EXECUTION and differ in kind: the ordered four
% evaluate through eval_int2 and enforce the Int-only law, while ==/\== use
% eval_expr and then term identity.
comparison_goal(Goal) :-
    nonvar(Goal),
    functor(Goal, Name, 2),
    expression(Name/2, Family, _, _, _),
    memberchk(Family, [ordered_comparison, identity_comparison]).

solve_comparison(Left < Right)   :- eval_int2(Left, Right, LeftV, RightV), LeftV < RightV.
solve_comparison(Left =< Right)  :- eval_int2(Left, Right, LeftV, RightV), LeftV =< RightV.
solve_comparison(Left > Right)   :- eval_int2(Left, Right, LeftV, RightV), LeftV > RightV.
solve_comparison(Left >= Right)  :- eval_int2(Left, Right, LeftV, RightV), LeftV >= RightV.
solve_comparison(Left == Right)  :- eval_expr(Left, LeftV), eval_expr(Right, RightV), LeftV == RightV.
solve_comparison(Left \== Right) :- eval_expr(Left, LeftV), eval_expr(Right, RightV), LeftV \== RightV.

% ═══ json ═══════════════════════════════════════════════════════════════════

json_canon(Braces, obj(Sorted)) :- nonvar(Braces), Braces = {}(Fields), !,
    braces_pairs(Fields, Pairs),
    keysort(Pairs, Sorted),
    pairs_keys(Sorted, Keys),
    ( sort(Keys, Distinct), length(Keys, N), length(Distinct, N)
    -> true ; throw(json_dup_key(Keys)) ).
json_canon(List, Canon) :- is_list(List), !, maplist(json_canon, List, Canon).
json_canon(obj(Pairs), obj(Canon)) :- !,
    findall(Key-Value, ( member(Key-Raw, Pairs), json_canon(Raw, Value) ), Canon0),
    keysort(Canon0, Canon).
json_canon(Value, Value).

braces_pairs((Left, Right), Pairs) :- !,
    braces_pairs(Left, LeftPairs), braces_pairs(Right, RightPairs),
    append(LeftPairs, RightPairs, Pairs).
braces_pairs(Key: Raw, [Key-Value]) :- json_canon(Raw, Value).

% decode: open object patterns, holes bind canonical values.
json_decode(Value, Pattern) :- var(Pattern), !, Pattern = Value.
json_decode(obj(Pairs), Pattern) :- nonvar(Pattern), Pattern = {}(Fields), !,
    braces_decode(Fields, Pairs).
json_decode(List, Pattern) :- is_list(Pattern), !,
    is_list(List),
    maplist(json_decode_flip, Pattern, List).
json_decode(Value, Pattern) :- Value = Pattern.

json_decode_flip(Pattern, Value) :- json_decode(Value, Pattern).

braces_decode((Left, Right), Pairs) :- !,
    braces_decode(Left, Pairs), braces_decode(Right, Pairs).
braces_decode(Key: Pattern, Pairs) :-
    memberchk(Key-Value, Pairs),
    Value \== none,
    json_decode(Value, Pattern).

% ═══ body solving ═══════════════════════════════════════════════════════════
% ctx(Visible, PreState, Tick): Visible = rows body atoms read; PreState =
% evolving pre rows; Tick = the phantom clock for now/1.

solve(true, _) :- !.
solve((Left, Right), Ctx) :- !, solve(Left, Ctx), solve(Right, Ctx).
solve(not(Goal), Ctx) :- !, \+ solve(Goal, Ctx).
solve(latest(Atom), Ctx) :- !, Ctx = ctx(Visible, _, _), member(Atom, Visible).
solve(pre(Atom), Ctx) :- !, Ctx = ctx(_, PreState, _), member(Atom, PreState).
solve(now(Tick), Ctx) :- !, Ctx = ctx(_, _, Tick).
solve(finalize(_), _) :- !, fail.   % satisfiable only as a trigger (r4)
solve(Variable := Expr, _) :- !, eval_expr(Expr, Value), Variable = Value.
solve(Variable is Expr, _)  :- !, eval_expr(Expr, Value), Variable = Value.
solve(decode(Expr, Pattern), _) :- !,
    eval_expr(Expr, Value), json_decode(Value, Pattern).
solve(json_each(Expr, Element), _) :- !,
    eval_expr(Expr, List), is_list(List), member(Element, List).
solve(Comparison, _) :- comparison_goal(Comparison), !, solve_comparison(Comparison).
solve(Atom, Ctx) :- Ctx = ctx(Visible, _, _), member(Atom, Visible).

% This predicate currently has no caller in the tree outside that test; rank R7
% of the review owns deciding whether it survives at all.
body_atoms(Body, Atoms) :-
    walk_body(Body, walk_policy(descend_not(false), splice_bare(false)),
              Events),
    plain_body_atoms(Events, Atoms).

plain_body_atoms([], []).
plain_body_atoms([event(_, _, Surface, Term) | Rest], Atoms) :-
    ( Surface == plain_atom
    -> Atoms = [Term | More]
    ;  Atoms = More
    ),
    plain_body_atoms(Rest, More).


substitute_goal((Left0, Right0), Target, (Left, Right)) :- !,
    substitute_goal(Left0, Target, Left),
    substitute_goal(Right0, Target, Right).
substitute_goal(Goal, Target, true) :- Goal == Target, !.
substitute_goal(Goal, _, Goal).

% Head values are expressions (named-column rule); evaluate after the joins.
eval_head(Head, Evaluated) :-
    Head =.. [Name | Args],
    maplist(eval_expr, Args, Values),
    Evaluated =.. [Name | Values].
