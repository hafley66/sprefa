% 0_relation_edge_expand.pl : relation-shaped head values require target
% membership through ordinary body relations.
%
%   rel user(id: int, name: text) key(1).
%   rel post(author: user).
%
%   post(user(Id, Name)) <- source(Id, Name).
%
% expands to:
%
%   post(user(Id, Name)) <- source(Id, Name), user(Id, Name).
%
% The added atom makes the dependency visible to stratification and the
% level fixpoint. An edge rule receives latest(user(...)) so target
% membership is sampled without adding a second trigger occurrence. If the
% body already reads the identical target pattern, expansion is a no-op.

:- module(relation_edge_expand,
          [ expand_relation_edges_in_context/3 ]).

:- use_module('0_dot_expand/0_type_plane',
              [type_definitions/2, type_definition/4,
               declared_type_name/2]).
:- use_module('0_body_walk', [walk_body/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

expand_relation_edges_in_context(_, prog(Decls, Rules0),
                                 prog(Decls, Rules)) :-
    type_definitions(Decls, Types),
    maplist(expand_rule_relation_edges(Decls, Types), Rules0, Rules).

expand_rule_relation_edges(Decls, Types, (Head <- Body0), (Head <- Body)) :-
    !,
    missing_head_target_atoms(Decls, Types, Head, Body0, Missing),
    append_body_goals(Body0, Missing, Body).
expand_rule_relation_edges(Decls, Types, (Head <+ Body0), (Head <+ Body)) :-
    !,
    missing_head_target_atoms(Decls, Types, Head, Body0, Missing),
    maplist(wrap_latest, Missing, Samples),
    append_body_goals(Body0, Samples, Body).
expand_rule_relation_edges(_, _, Rule, Rule).

missing_head_target_atoms(Decls, Types, Head, Body, Missing) :-
    functor(Head, Name, Arity),
    Ref = Name/Arity,
    findall(Type, member(col_type(Ref, _, Type), Decls), ColumnTypes),
    Head =.. [_ | Args],
    head_target_atoms(Types, Args, ColumnTypes, Targets),
    include(target_absent_from_body(Body), Targets, Missing).

head_target_atoms(_, [], [], []) :- !.
head_target_atoms(Types, [Arg | Args], [Type | Types0], Targets) :-
    ( relation_value_atom(Types, Type, Arg)
    -> Targets = [Arg | Rest]
    ;  Targets = Rest
    ),
    head_target_atoms(Types, Args, Types0, Rest),
    !.
head_target_atoms(_, _, _, []) :- !.

relation_value_atom(Types, Type, Arg) :-
    declared_type_name(Types, Type),
    compound(Arg),
    functor(Arg, Type, Arity),
    type_definition(Types, Type, Columns, _),
    length(Columns, Arity).

target_absent_from_body(Body, Target) :-
    \+ body_reads_identical_target(Body, Target).

body_reads_identical_target(Body, Target) :-
    walk_body(Body, walk_policy(descend_not(false), splice_bare(false)),
              Events),
    member(event(_, _, _, Term), Events),
    unwrap_membership_sample(Term, Atom),
    Atom == Target,
    !.

unwrap_membership_sample(latest(Atom), Atom) :- !.
unwrap_membership_sample(Atom, Atom).

wrap_latest(Atom, latest(Atom)).

append_body_goals(Body, [], Body) :- !.
append_body_goals(Body0, [Goal | Rest], Body) :-
    append_one_goal(Body0, Goal, Body1),
    append_body_goals(Body1, Rest, Body).

append_one_goal(true, Goal, Goal) :- !.
append_one_goal(Body, Goal, (Body, Goal)).
