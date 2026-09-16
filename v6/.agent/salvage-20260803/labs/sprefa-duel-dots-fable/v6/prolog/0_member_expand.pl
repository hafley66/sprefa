% 0_member_expand.pl : `Base.field` member access, the dot spelling of the
% decode/2 brace pattern.
%
% pluck(BaseVar, Fields) is the parsed form of `Base.f.g` (parse_dl.pl
% member_fields/3; print_dl.pl prints it back). This expander erases every
% pluck before checks, typing, or either lowering door can see one:
%
%   Leaf := Base.f.g          becomes   decode(Base, {f: {g: Leaf}})
%   any other body position   becomes   a fresh leaf variable plus that same
%                                       decode goal beside the host goal
%
% so the desugared program is term-identical to the brace spelling and the
% pipeline learns nothing new (the ruled semantics, 2026-08-03 stance 9).
%
% Placement of the synthesized decode: AFTER a plain relation atom (the atom
% may be the goal that binds the base, `f(X, X.a)`), BEFORE any other goal (a
% bind/guard/negation reads its operands, so the leaf must be bound first).
%
% ── what is refused, and why each one is decidable here ──────────────────────
%
%   unresolvable_member     the base is not a variable bound by this body
%                           (resolution is bound-variable-first; an unbound
%                           base must refuse by name, never read as some
%                           other construct). The reported path spells the
%                           base as `_`: the parse keeps variable IDENTITY,
%                           not surface names, and a refusal term carrying a
%                           free variable cannot be pinned by a fixture
%                           (engine.pl grades throws/1 by ==/2).
%   member_in_head          scope fence: member access is a BODY read; a
%                           dotted head argument is out of the ruled scope.
%   member_not_a_goal       a pluck sitting where a goal belongs has no value
%                           position to desugar into.
%
% Every refusal is thrown HERE, in the one expansion both doors consult, so
% the oracle and the compiler cannot disagree about which programs are legal.

:- module(member_expand,
          [ expand_member_in_context/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(occurs), [sub_term/2]).
:- use_module('compile/registry', [surface/5]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

expand_member_in_context(_, prog(Decls, Rules0), prog(Decls, Rules)) :-
    maplist(expand_member_rule, Rules0, Rules).

expand_member_rule((Head <- Body0), (Head <- Body)) :-
    !,
    refuse_member_in_head(Head),
    expand_member_body(Body0, Body).
expand_member_rule((Head <+ Body0), (Head <+ Body)) :-
    !,
    refuse_member_in_head(Head),
    expand_member_body(Body0, Body).
expand_member_rule(Rule, Rule).

% A body without a pluck is returned byte-identical, so no existing rule's
% body shape moves.
expand_member_body(Body0, Body) :-
    conjunction_goals(Body0, Goals0),
    (   \+ ( member(Goal, Goals0), contains_pluck(Goal) )
    ->  Body = Body0
    ;   bound_body_vars(Goals0, BoundVars),
        maplist(rewrite_goal(BoundVars), Goals0, GoalLists),
        append(GoalLists, Goals),
        goals_conjunction(Goals, Body)
    ).

% ── goal rewriting ───────────────────────────────────────────────────────────

rewrite_goal(_, Goal, [Goal]) :-
    \+ contains_pluck(Goal),
    !.
rewrite_goal(_, Goal, _) :-
    nonvar(Goal),
    Goal = pluck(_, Fields),
    !,
    member_path_atom(Fields, Path),
    throw(unsupported_construct(member_not_a_goal(Path))).
% The whole-RHS bind is rewritten IN PLACE with the bind's own left side as
% the pattern leaf, which is what makes the dot twin of a decode fixture
% expand to the brace original term for term.
rewrite_goal(BoundVars, (Lhs := Rhs), [decode(Base, Pattern)]) :-
    nonvar(Rhs),
    Rhs = pluck(Base, Fields),
    \+ contains_pluck(Lhs),
    !,
    check_member_base(BoundVars, Base, Fields),
    fields_pattern(Fields, Lhs, Pattern).
rewrite_goal(BoundVars, Goal0, GoalList) :-
    replace_plucks(Goal0, BoundVars, Goal, Decodes),
    (   plain_relation_goal(Goal)
    ->  GoalList = [Goal | Decodes]
    ;   append(Decodes, [Goal], GoalList)
    ).

replace_plucks(Term, _, Term, []) :-
    var(Term),
    !.
replace_plucks(Term, BoundVars, Leaf, [decode(Base, Pattern)]) :-
    Term = pluck(Base, Fields),
    !,
    check_member_base(BoundVars, Base, Fields),
    fields_pattern(Fields, Leaf, Pattern).
replace_plucks(Term, BoundVars, Out, Decodes) :-
    compound(Term),
    !,
    Term =.. [Functor | Args0],
    foldl(replace_plucks_arg(BoundVars), Args0, Args, [], Decodes),
    Out =.. [Functor | Args].
replace_plucks(Term, _, Term, []).

replace_plucks_arg(BoundVars, Arg0, Arg, Acc, Decodes) :-
    replace_plucks(Arg0, BoundVars, Arg, ArgDecodes),
    append(Acc, ArgDecodes, Decodes).

fields_pattern([Field], Leaf, '{}'(Field:Leaf)) :-
    atom(Field),
    !.
fields_pattern([Field | Rest], Leaf, '{}'(Field:Sub)) :-
    atom(Field),
    Rest = [_ | _],
    !,
    fields_pattern(Rest, Leaf, Sub).
fields_pattern(Fields, _, _) :-
    member_path_atom(Fields, Path),
    throw(unsupported_construct(unresolvable_member(Path))).

check_member_base(BoundVars, Base, Fields) :-
    (   var(Base),
        memberchk_eq(Base, BoundVars)
    ->  true
    ;   member_path_atom(Fields, Path),
        throw(unsupported_construct(unresolvable_member(Path)))
    ).

member_path_atom(Fields, Path) :-
    (   is_list(Fields),
        Fields \== [],
        maplist(atom, Fields)
    ->  atomic_list_concat(['_' | Fields], '.', Path)
    ;   Path = '_'
    ).

% ── what binds a variable, for the bound-variable-first check ────────────────
% Vars are collected from the pluck-stripped goals (in `f(X.a)` the base X is
% READ through the dot, not bound by f). LHS of a bind and a decode pattern
% both count: each binds its captures exactly as the synthesized decode will.

bound_body_vars(Goals, BoundVars) :-
    foldl(goal_bound_vars, Goals, [], BoundVars).

goal_bound_vars(Goal, Acc, BoundVars) :-
    binding_positions(Goal, Positions),
    strip_plucks(Positions, Stripped),
    term_variables(Stripped, GoalVars),
    append(Acc, GoalVars, BoundVars).

binding_positions(Goal, []) :- var(Goal), !.
binding_positions((Lhs := _), [Lhs]) :- !.
binding_positions(is(Lhs, _), [Lhs]) :- !.
binding_positions(not(_), []) :- !.
binding_positions(decode(_, Pattern), [Pattern]) :- !.
binding_positions(latest(Atom), [Atom]) :- !.
binding_positions(pre(Atom), [Atom]) :- !.
binding_positions(finalize(Atom), [Atom]) :- !.
binding_positions(next(Atom), [Atom]) :- !.
binding_positions(coalesce(Atom, _), [Atom]) :- !.
binding_positions(now(Value), [Value]) :- !.
binding_positions(probe(_, _, Outputs, _), [Outputs]) :- !.
binding_positions(Goal, Positions) :-
    functor(Goal, Functor, Arity),
    (   Functor == combine
    ->  Goal =.. [_ | Positions]
    ;   surface(Functor/Arity, _, _, _, _)
    ->  Positions = []
    ;   Goal =.. [_ | Positions]
    ).

strip_plucks(Term, Stripped) :-
    (   var(Term)
    ->  Stripped = Term
    ;   Term = pluck(_, _)
    ->  Stripped = _Fresh
    ;   compound(Term)
    ->  Term =.. [Functor | Args],
        maplist(strip_plucks, Args, StrippedArgs),
        Stripped =.. [Functor | StrippedArgs]
    ;   Stripped = Term
    ).

plain_relation_goal(Goal) :-
    nonvar(Goal),
    compound(Goal),
    functor(Goal, Functor, Arity),
    atom(Functor),
    \+ surface(Functor/Arity, _, _, _, _),
    Functor/Arity \== probe/4.

% ── the conjunction spine (same shape as 0_coalesce_expand.pl) ───────────────

conjunction_goals(Body, Goals) :-
    (   nonvar(Body), Body = (Left, Right)
    ->  conjunction_goals(Left, LeftGoals),
        conjunction_goals(Right, RightGoals),
        append(LeftGoals, RightGoals, Goals)
    ;   Body == true
    ->  Goals = []
    ;   Goals = [Body]
    ).

goals_conjunction([], true) :- !.
goals_conjunction([Goal], Goal) :- !.
goals_conjunction([Goal | Rest], (Goal, Conjunction)) :-
    goals_conjunction(Rest, Conjunction).

% ── residuals ────────────────────────────────────────────────────────────────

refuse_member_in_head(Head) :-
    (   sub_term(Sub, Head),
        nonvar(Sub),
        Sub = pluck(_, _)
    ->  functor(Head, Name, Arity),
        throw(unsupported_construct(member_in_head(Name/Arity)))
    ;   true
    ).

contains_pluck(Term) :-
    sub_term(Sub, Term),
    nonvar(Sub),
    Sub = pluck(_, _),
    !.

memberchk_eq(Variable, [Head | Rest]) :-
    ( Head == Variable -> true ; memberchk_eq(Variable, Rest) ).
