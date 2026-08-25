% 0_negated_guard_expand.pl : a not/1 over ONE guard comparison is inverted to
% its complement (not(X > 1) -> X =< 1); both doors run this phase.

:- module(negated_guard_expand,
          [ expand_negated_guards_in_context/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('../../compile/registry', [expression/5]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

expand_negated_guards_in_context(_EnumContext, prog(Decls, Rules0), prog(Decls, Rules)) :-
    maplist(flip_negated_guards_in_rule, Rules0, Rules).

flip_negated_guards_in_rule((Head <- Body0), (Head <- Body)) :- !,
    flip_body(Body0, Body).
flip_negated_guards_in_rule((Head <+ Body0), (Head <+ Body)) :- !,
    flip_body(Body0, Body).
flip_negated_guards_in_rule(Rule, Rule).

flip_body(Body0, Body) :-
    conjunction_goals(Body0, Goals0),
    maplist(flip_goal, Goals0, Goals),
    goals_conjunction(Goals, Body).

% A not/1 wrapping a single comparison becomes the complement comparison,
% applied in place.
flip_goal(not(Inner), Complement) :-
    comparison_guard(Inner, Complement),
    !.
flip_goal(Goal, Goal).

comparison_guard(Inner, Complement) :-
    nonvar(Inner),
    Inner =.. [Operator, Left, Right],
    expression(Operator/2, Family, _, _, _),
    memberchk(Family, [ordered_comparison, identity_comparison]),
    negate_operator(Operator, ComplementOperator),
    Complement =.. [ComplementOperator, Left, Right].

negate_operator('<', '>=').
negate_operator('=<', '>').
negate_operator('>', '=<').
negate_operator('>=', '<').
negate_operator('==', '\\==').
negate_operator('\\==', '==').
negate_operator('=:=', '=\\=').
negate_operator('=\\=', '=:=').

% ── the conjunction spine (same shape as 0_dot_expand / 0_coalesce_expand) ──

conjunction_goals(Body, Goals) :-
    ( nonvar(Body), Body = (Left, Right)
    -> conjunction_goals(Left, LeftGoals),
       conjunction_goals(Right, RightGoals),
       append(LeftGoals, RightGoals, Goals)
    ;  Body == true
    -> Goals = []
    ;  Goals = [Body]
    ).

goals_conjunction([], true) :- !.
goals_conjunction([Goal], Goal) :- !.
goals_conjunction([Goal | Rest], (Goal, Conjunction)) :-
    goals_conjunction(Rest, Conjunction).
