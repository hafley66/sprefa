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
:- use_module('../compile/registry', [expression/5, expression_for_term/5]).

:- op(700, xfx, :=).

rel_ref(Atom, Name/Arity) :- functor(Atom, Name, Arity).

% ═══ expressions ════════════════════════════════════════════════════════════
% Evaluation is the default; goals run left to right: an atom binds, := / is
% computes, a comparison filters. / truncates (Int-only law). concat is the
% interpolation lowering target.

eval_expr(Value, _) :- var(Value), !, throw(unbound_in_expression).
eval_expr(bool_lit(Boolean), bool_lit(Boolean)) :- !.
eval_expr(Number, Number) :- number(Number), !.
eval_expr(concat(Parts), Out) :- !,
    maplist(eval_expr, Parts, Values),
    maplist(text_piece, Values, Pieces),
    atomic_list_concat(Pieces, Out).
eval_expr(Left + Right, Out)   :- !, eval_number2(Left, Right, LeftV, RightV), Out is LeftV + RightV.
eval_expr(Left - Right, Out)   :- !, eval_number2(Left, Right, LeftV, RightV), Out is LeftV - RightV.
eval_expr(Left * Right, Out)   :- !, eval_number2(Left, Right, LeftV, RightV), Out is LeftV * RightV.
eval_expr(Left / Right, Out)   :- !,
    eval_number2(Left, Right, LeftV, RightV),
    ( integer(LeftV), integer(RightV)
    -> Out is LeftV // RightV
    ; Out is LeftV / RightV
    ).
eval_expr(Left mod Right, Out) :- !, eval_int2(Left, Right, LeftV, RightV), Out is LeftV mod RightV.
% TEXT SCALARS, read off the same registry row the emitter lowers from
% (registry.pl expression/5, family text_scalar). That row shipped with a SQL
% rendering and no reference implementation at all, so `norm(Raw)` in a head
% left this engine holding the unevaluated term: the oracle printed
% "norm(Hello World)" where the emitter computed "helloworld". A live registry
% row whose two doors answer different things is the divergence this clause
% closes, and dispatching on the FAMILY rather than on norm/1 by name is what
% stops the next text scalar from repeating it.
eval_expr(Term, Out) :-
    nonvar(Term),
    expression_for_term(Term, text_scalar, _, Rendering, _),
    !,
    arg(1, Term, Argument),
    eval_expr(Argument, Value),
    text_scalar_value(Rendering, Value, Out).
% The empty object is the ATOM `{}` on both doors, so it canonicalizes here
% beside its arity-1 sibling; without this clause a stored `{}` stays an atom
% and every object pattern over it fails as if the value were a scalar.
eval_expr('{}', Canon) :- !, json_canon('{}', Canon).
eval_expr(Braces, Canon) :- Braces = {}(_), !, json_canon(Braces, Canon).
eval_expr([Head | Tail], Canon) :- !, json_canon([Head | Tail], Canon).
eval_expr(Value, Value).

% V5 `sprf_norm`, and the exact set the emitted SQL keeps: ASCII digits,
% ASCII letters, everything else dropped, letters lowercased. The SQL walks
% the string one character at a time and filters on
% `unicode("c") BETWEEN 48 AND 57 / 65 AND 90 / 97 AND 122`, so the character
% classes here are those three ranges written out, not a locale-aware
% code_type/2 that would answer differently for a non-ASCII letter.
text_scalar_value(ascii_alnum_lower, Value, Out) :-
    ( atomic(Value) -> true ; throw(non_display_in_concat(Value)) ),
    atom_codes(Value, Codes),
    include(ascii_alnum_code, Codes, Kept),
    maplist(ascii_lower_code, Kept, Lowered),
    atom_codes(Out, Lowered).

ascii_alnum_code(Code) :- between(0'0, 0'9, Code), !.
ascii_alnum_code(Code) :- between(0'A, 0'Z, Code), !.
ascii_alnum_code(Code) :- between(0'a, 0'z, Code).

ascii_lower_code(Code, Lower) :-
    ( between(0'A, 0'Z, Code) -> Lower is Code + 32 ; Lower = Code ).

eval_int2(Left, Right, LeftV, RightV) :-
    eval_expr(Left, LeftV), eval_expr(Right, RightV),
    ( integer(LeftV), integer(RightV) -> true ; throw(arith_on_non_int(LeftV, RightV)) ).

eval_number2(Left, Right, LeftV, RightV) :-
    eval_expr(Left, LeftV), eval_expr(Right, RightV),
    ( number(LeftV), number(RightV)
    -> true
    ; throw(arith_on_non_int(LeftV, RightV)) ).

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

solve_comparison(Left < Right)   :- eval_number2(Left, Right, LeftV, RightV), LeftV < RightV.
solve_comparison(Left =< Right)  :- eval_number2(Left, Right, LeftV, RightV), LeftV =< RightV.
solve_comparison(Left > Right)   :- eval_number2(Left, Right, LeftV, RightV), LeftV > RightV.
solve_comparison(Left >= Right)  :- eval_number2(Left, Right, LeftV, RightV), LeftV >= RightV.
solve_comparison(Left == Right)  :- eval_expr(Left, LeftV), eval_expr(Right, RightV), LeftV == RightV.
solve_comparison(Left \== Right) :- eval_expr(Left, LeftV), eval_expr(Right, RightV), LeftV \== RightV.

% ═══ json ═══════════════════════════════════════════════════════════════════

% AN UNBOUND VALUE IS A NAMED REFUSAL, not an answer. Without this clause the
% next one UNIFIES the unbound input with the atom `{}` and hands back
% `obj([])`: `json_canon(Free, C)` answered "the empty object" and bound the
% caller's variable to `{}` on the way out (json_flex lab receipt, 2026-07-30).
% The obj/1 clause below would do the same one step later. Every shipped path
% reaches json_canon through eval_expr/2, which already throws
% `unbound_in_expression` first, so this is a latent hole rather than a live
% one -- and latent silent-wrong-answer is exactly what a refusal is for.
json_canon(Value, _) :- var(Value), !, throw(json_value_unbound).
% The EMPTY object is the atom `{}`, not `{}`/1: that is what the term door's
% own reader produces for `{}` (term_to_atom gives arity 0), so the text door
% mints the same atom and both arrive here.
json_canon('{}', obj([])) :- !.
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
%
% NONDETERMINISM IS THE SEMANTICS, not a convenience. Three of the productions
% below fan out -- array spread, key capture and `**` descent -- and each one
% corresponds to exactly one SQL join in the emitted plan (json_syntax lab
% cost table: spread and key capture are `json_each`, descent is `json_tree`).
% A `memberchk` anywhere in those three would answer the first match only and
% silently lose rows the emitter derives.
json_decode(Value, Pattern) :- var(Pattern), !, Pattern = Value.
% `$Name` in VALUE position (ruling json_key_hole_marker = dollar: "so
% {$key: $value} reads uniformly on both planes"). The TERM door reads it as
% the `$`/1 compound, because `$` is a standard SWI prefix operator; the TEXT
% door resolves it to the plain variable, because on the value plane a bare
% identifier is already a variable and `$` is an alias rather than a second
% meaning. Both spellings are holes and both arrive here.
json_decode(Value, Pattern) :- nonvar(Pattern), Pattern = $(Hole), !,
    Hole = Value.
% The empty object pattern is OPEN with no members, so it matches any object
% and binds nothing.
json_decode(Value, '{}') :- !, Value = obj(_).
% Array spread `[... Sub]`: ONE SOLUTION PER ELEMENT. Lowers to a `json_each`
% join, which is one row per element with siblings correlated -- the flagship
% shape (json_syntax lab receipt L2, examples/gh-cache.dl:116-117).
json_decode(List, Pattern) :- nonvar(Pattern), Pattern = spread(Sub), !,
    is_list(List),
    member(Element, List),
    json_decode(Element, Sub).
json_decode(obj(Pairs), Pattern) :- nonvar(Pattern), Pattern = {}(Fields), !,
    braces_decode(Fields, Pairs).
json_decode(List, Pattern) :- is_list(Pattern), !,
    is_list(List),
    maplist(json_decode_flip, Pattern, List).
json_decode(Value, Pattern) :- Value = Pattern.

json_decode_flip(Pattern, Value) :- json_decode(Value, Pattern).

braces_decode((Left, Right), Pairs) :- !,
    braces_decode(Left, Pairs), braces_decode(Right, Pairs).
% `**` descent (ruling descent_depth_cap = uncapped): the sub-pattern is tried
% against this object AND every object below it, at any depth. Lowers to
% `json_tree`, whose first row IS the root, which is why the root is a
% candidate here too.
braces_decode('**': Pattern, Pairs) :- !,
    descendant_object(obj(Pairs), Descendant),
    json_decode(Descendant, Pattern).
% Key capture (ruling json_key_hole_marker = dollar): the key is data. Lowers
% to `json_each`, whose (key, value) columns already carry both -- the
% construct the recovery doc graded "needs new surface" and that needs ZERO
% new SQL machinery (lab receipt L3).
braces_decode(Key: Pattern, Pairs) :-
    nonvar(Key), Key = $(KeyHole), !,
    member(ActualKey-Value, Pairs),
    Value \== none,
    KeyHole = ActualKey,
    json_decode(Value, Pattern).
braces_decode(Key: Pattern, Pairs) :-
    memberchk(Key-Value, Pairs),
    Value \== none,
    json_decode(Value, Pattern).

% Every OBJECT in a value's tree, root first. Scalars contribute nothing, so
% descending into one is a silent non-match -- the same semantics the emitted
% `type = 'object'` guard buys in SQL, where without it json_extract raises
% `malformed JSON` and kills the whole statement (lab finding 6).
descendant_object(obj(Pairs), obj(Pairs)).
descendant_object(obj(Pairs), Descendant) :-
    member(_-Value, Pairs),
    descendant_object(Value, Descendant).
descendant_object(List, Descendant) :-
    is_list(List),
    member(Value, List),
    descendant_object(Value, Descendant).

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
