% body.pl : the value/goal layer of the reference interpreter.
% Expressions (evaluation is the default), json values (braces are the one
% grammar), and body-goal solving. No tick knowledge lives here: solve/2
% reads a ctx(Visible, PreState, Tick) the tick layer builds.

:- module(body,
          [ rel_ref/2, solve/2, rows_index/2, body_atoms/2, eval_expr/2, eval_head/2,
            json_canon/2, comparison_goal/1, substitute_goal/3,
            % Exported for the json_typed_capture agreement unit: this table
            % has a clause-for-clause twin in lower.pl and the two doors
            % drifting apart is not something a byte-diff can catch (the
            % compiled side never produces a log to diff when it refuses).
            json_capture_type/2,
            % RFC 7396 merge patch, exported for the same reason: the emitted
            % side renders it as native SQL, so the two doors' agreement is a
            % unit assertion, not something the byte diff alone can hold.
            json_scalar_value/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(pairs)).
:- use_module(library(assoc)).
:- use_module(library(pcre)).
:- use_module('../0_body_walk', [walk_body/3]).
:- use_module('../compile/registry',
              [expression/5, expression_for_term/5, surface_for_term/6]).

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
% Text scalar evaluation follows the registry family used by the emitter.
eval_expr(Term, Out) :-
    nonvar(Term),
    expression_for_term(Term, text_scalar, _, Rendering, _),
    !,
    Term =.. [_ | Args],
    maplist(eval_expr, Args, Values),
    text_scalar_value(Rendering, Values, Out).
% Same registry dispatch, json operands: the arguments canonicalize first, so
% a braces literal reaches the patch algebra as obj(SortedPairs).
eval_expr(Term, Out) :-
    nonvar(Term),
    expression_for_term(Term, json_scalar, _, Rendering, _),
    !,
    Term =.. [_ | Args],
    maplist(eval_expr, Args, Raw),
    maplist(json_canon, Raw, Values),
    json_scalar_value(Rendering, Values, Out).
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
text_scalar_value(ascii_alnum_lower, [Value], Out) :-
    ( atomic(Value) -> true ; throw(non_display_in_concat(Value)) ),
    atom_codes(Value, Codes),
    include(ascii_alnum_code, Codes, Kept),
    maplist(ascii_lower_code, Kept, Lowered),
    atom_codes(Out, Lowered).

% SQLite rtrim(X, Y): strip trailing characters that are members of the set Y.
text_scalar_value(rtrim, [Text, Chars], Out) :-
    ( atomic(Text) -> true ; throw(non_display_in_concat(Text)) ),
    atom_codes(Text, TextCodes),
    atom_codes(Chars, CharsCodes),
    reverse(TextCodes, Rev),
    drop_leading_in_set(Rev, CharsCodes, RevTrimmed),
    reverse(RevTrimmed, Trimmed),
    atom_codes(Out, Trimmed).

% SQLite replace(X, Y, Z): replace every occurrence of Y in X with Z.
text_scalar_value(replace, [Text, From, To], Out) :-
    ( atomic(Text) -> true ; throw(non_display_in_concat(Text)) ),
    atom_codes(Text, TextCodes),
    atom_codes(From, FromCodes),
    atom_codes(To, ToCodes),
    replace_codes(TextCodes, FromCodes, ToCodes, OutCodes),
    atom_codes(Out, OutCodes).

drop_leading_in_set([], _, []).
drop_leading_in_set([Code | Rest], Chars, Out) :-
    ( memberchk(Code, Chars)
    -> drop_leading_in_set(Rest, Chars, Out)
    ;  Out = [Code | Rest]
    ).

replace_codes([], _, _, []).
replace_codes(Codes, [], _, Codes) :- !.
replace_codes(Codes, Needle, Repl, Out) :-
    (   append(Needle, Rest, Codes)
    ->  append(Repl, OutRest, Out),
        replace_codes(Rest, Needle, Repl, OutRest)
    ;   Codes = [Code | Rest],
        replace_codes(Rest, Needle, Repl, RestOut),
        Out = [Code | RestOut]
    ).

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

% Ordered comparisons enforce numeric evaluation; ==/\== use term identity.
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
solve_comparison(Left =:= Right) :- eval_number2(Left, Right, LeftV, RightV), LeftV =:= RightV.
solve_comparison(Left =\= Right) :- eval_number2(Left, Right, LeftV, RightV), LeftV =\= RightV.

% ═══ json ═══════════════════════════════════════════════════════════════════

% An unbound value is a named unsupported construct. It must not unify with the empty object.
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

% ── RFC 7396 JSON Merge Patch ───────────────────────────────────────────────
%
% RFC 7396 §2 is the whole algorithm, four behaviors:
%   §2 `if Patch is an Object` / `else: return Patch`
%       a non-object patch replaces the target entire (§1 says the same in
%       prose: "if the patch is anything other than an object, the result
%       will always be to replace the entire target with the entire patch").
%       An array is not an object, so arrays replace wholesale.
%   §2 `if Target is not an Object: Target = {}`
%       a non-object target's contents are discarded, never merged into.
%   §2 `Target[Name] = MergePatch(Target[Name], Value)`
%       objects merge recursively, key by key.
%   §1/§2 `if Value is null: remove the Name/Value pair from Target`
%       the json-null stand-in `none` merges as a VALUE, not a deletion: the
%       decision (2026-08-11) makes JSON null the atom `none`, and a
%       none-valued key is kept and rendered `"key":null` (the rendering rule
%       in the lane brief). json_merge_patch/3 already does this through its
%       scalar fallback, so a patch carrying `none` composes to a null-valued
%       key instead of throwing.
json_scalar_value(json_patch, [Target, Patch], Out) :-
    json_merge_patch(Target, Patch, Out).

json_merge_patch(Target, Patch, obj(Sorted)) :-
    nonvar(Patch), Patch = obj(PatchPairs), !,
    ( nonvar(Target), Target = obj(TargetPairs) -> true ; TargetPairs = [] ),
    foldl(json_merge_patch_pair, PatchPairs, TargetPairs, Merged),
    keysort(Merged, Sorted).
json_merge_patch(_, Patch, Patch).

% A key the target lacks recurses against `absent`, which is not an object,
% so an object value there reduces to the value itself.
json_merge_patch_pair(Key-none, Pairs0, Rest) :- !,
    ( selectchk(Key-_, Pairs0, Rest) -> true ; Rest = Pairs0 ).
json_merge_patch_pair(Key-Value, Pairs0, [Key-Merged | Rest]) :-
    ( selectchk(Key-Prior, Pairs0, Rest) -> true ; Prior = absent, Rest = Pairs0 ),
    json_merge_patch(Prior, Value, Merged).

json_patch_carries_null(Value) :- Value == none, !.
json_patch_carries_null(obj(Pairs)) :- is_list(Pairs), !,
    member(_-Child, Pairs), json_patch_carries_null(Child), !.
json_patch_carries_null(Values) :- is_list(Values),
    member(Child, Values), json_patch_carries_null(Child), !.

% decode: open object patterns, holes bind canonical values.
%
% NONDETERMINISM IS THE SEMANTICS, not a convenience. Three of the productions
% below fan out -- array spread, key capture and `**` descent -- and each one
% corresponds to exactly one SQL join in the emitted plan (json_syntax lab
% cost table: spread and key capture are `json_each`, descent is `json_tree`).
% A `memberchk` anywhere in those three would answer the first match only and
% silently lose rows the emitter derives.
json_decode(Value, Pattern) :- var(Pattern), !, Pattern = Value.
% `$Name` in value position is a hole on both doors.
json_decode(Value, Pattern) :- nonvar(Pattern), Pattern = $(Hole), !,
    Hole = Value.
% A typed capture uses the same colon marker as typed declarations.
% `:` is 600 xfy in SWI, which makes `stars: Stars: int` read as
% `:(stars, :(Stars, int))` with no term-door parser work at all.
%
% The type is a matcher, not a cast. Both doors filter on the declared type.
%
% A type mismatch filters the document. Invalid program-level column types are
% rejected during compilation.
json_decode(Value, Pattern) :- nonvar(Pattern), Pattern = (Hole : Type),
    var(Hole), atom(Type), !,
    json_capture_type(Type, Value),
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

% The capture types, one clause per json1 `json_type` answer the emitted guard
% tests for, so the two doors cannot drift apart on which values pass:
%
%   int    json_type = 'integer'   integer/1
%   float  json_type = 'real'      float/1
%   text   json_type = 'text'      an atom that is not the json-null stand-in
%
% `bool` is deliberately ABSENT and therefore refused by name. A json boolean
% has no settled storage on this plane yet -- the json_flex verdict's card C4
% measured a top-level `true` DOCUMENT degrading to the integer 1 through the
% real emitted arrival statement -- so a `bool` capture would have to pick a
% side of an open card. Refusing it says that; accepting it would guess.
%
% An unrecognised type name is a NAMED REFUSAL rather than a capture that
% never matches: a typo would otherwise turn into a rule that silently
% derives nothing, which is the exact silence class this file's headers keep
% naming. lower.pl throws the same term on the compiled side.
json_capture_type(int,   Value) :- !, integer(Value).
json_capture_type(float, Value) :- !, float(Value).
json_capture_type(text,  Value) :- !, atom(Value), Value \== none.
json_capture_type(Type,  _) :- throw(json_capture_type_unknown(Type)).

braces_decode((Left, Right), Pairs) :- !,
    braces_decode(Left, Pairs), braces_decode(Right, Pairs).
% `**` descent (ruling descent_depth_cap = uncapped): the sub-pattern is tried
% against this object AND every object below it, at any depth. Lowers to
% `json_tree`, whose first row IS the root, which is why the root is a
% candidate here too.
braces_decode('**': Pattern, Pairs) :- !,
    descendant_object(obj(Pairs), Descendant),
    json_decode(Descendant, Pattern).
% Key capture treats the key as data and binds it alongside the value.
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
solve(latest(Atom), Ctx) :- !, Ctx = ctx(Visible, _, _), rows_member(Atom, Visible).
solve(pre(Atom), Ctx) :- !, Ctx = ctx(_, PreState, _), rows_member(Atom, PreState).
solve(pre(Atom, Seed), Ctx) :- !,
    Ctx = ctx(_, PreState, _),
    ( rows_member(Atom, PreState) -> true ; pre_seed(Atom, Seed) ).
solve(now(Tick), Ctx) :- !, Ctx = ctx(_, _, Tick).
solve(finalize(_), _) :- !, fail.   % satisfiable only as a trigger (r4)
% next/1 and combine are transparent registry-defined splice rows. Their
% arguments solve left to right as a conjunction.
solve(Term, Ctx) :-
    nonvar(Term),
    surface_for_term(Term, _, _, splice_bare, _, _),
    !,
    Term =.. [_ | Goals],
    solve_spliced(Goals, Ctx).
solve(Variable := Expr, _) :- !, eval_expr(Expr, Value), Variable = Value.
solve(Variable is Expr, _)  :- !, eval_expr(Expr, Value), Variable = Value.
solve(decode(Expr, Pattern), _) :- !,
    eval_expr(Expr, Value), json_decode(Value, Pattern).
solve(json_each(Expr, Element), _) :- !,
    eval_expr(Expr, List), is_list(List), member(Element, List).
solve(regexp(Operand, Pattern), _) :- !,
    eval_expr(Operand, Value),
    re_match(Pattern, Value).
solve(Comparison, _) :- comparison_goal(Comparison), !, solve_comparison(Comparison).
solve(Atom, Ctx) :- Ctx = ctx(Visible, _, _), rows_member(Atom, Visible).

% A body goal names one relation and the row list holds every relation, so an
% unindexed member/2 makes a k-goal rule O(N^k) over the whole visible set.
% rows_index/2 buckets a STABLE row list by Name/Arity; rows(Index, Growing)
% keeps the fixpoint's accumulating half as a plain list, because rebuilding
% the buckets per iteration costs more than the scan it saves. keysort/2 is
% stable and the index half is enumerated first, so solution order matches what
% append(Stable, Growing) fed member/2.
rows_index(Rows, rows_index(Assoc, Rows)) :-
    findall(Name/Arity-Row, ( member(Row, Rows), functor(Row, Name, Arity) ), Pairs),
    keysort(Pairs, Sorted),
    group_pairs_by_key(Sorted, Grouped),
    list_to_assoc(Grouped, Assoc).

rows_member(Atom, rows(Index, Growing)) :-
    !,
    ( rows_member(Atom, Index) ; member(Atom, Growing) ).
rows_member(Atom, rows_index(Assoc, All)) :-
    !,
    (   var(Atom)
    ->  member(Atom, All)
    ;   functor(Atom, Name, Arity),
        get_assoc(Name/Arity, Assoc, Rows),
        member(Atom, Rows)
    ).
rows_member(Atom, Rows) :- member(Atom, Rows).

pre_seed(Atom, Seed) :-
    Atom =.. [_ | Args],
    include(var, Args, [Before]),
    Before = Seed.

% Left to right, backtracking across the whole list, which is what makes a
% splice indistinguishable from writing the same goals separated by commas.
solve_spliced([], _).
solve_spliced([Goal | Rest], Ctx) :- solve(Goal, Ctx), solve_spliced(Rest, Ctx).

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
