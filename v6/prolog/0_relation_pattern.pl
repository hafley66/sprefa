% 0_relation_pattern.pl : the reference engine's half of the relation-value
% model. A relation-shaped TERM written in a rule is rewritten to the canonical
% OBJECT the rest of the system already speaks.
%
%   rel repo(name: text).
%   rel file(repo: repo, at: fpath).
%   rel span(file: file, start: int, end: int).
%
%   span(file(repo(Name), fpath(Path)), Start, End) <- raw(Name, Path, Start, End).
%
% becomes, before anything stores or unifies:
%
%   span(obj([at-obj([name-Path]), repo-obj([name-Name])]), Start, End) <- ...
%
% Relation-value terms are rewritten to canonical objects before execution.
% Malformed shapes are rejected by the shared program checks.

:- module(relation_pattern,
          [ expand_relation_values/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('0_dot_expand/0_type_plane',
              [ type_definitions/2, declared_type_name/2,
                relation_columns_and_types/5, relation_value_object/4 ]).
:- use_module('0_dot_expand/registry', [body_surface_for_term/6]).
:- use_module('0_body_walk', [relation_atom_wrapper/1]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

expand_relation_values(prog(Decls, Rules0), prog(Decls, Rules)) :-
    type_definitions(Decls, Types),
    (   Types == []
    ->  Rules = Rules0
    ;   maplist(expand_rule(Decls, Types), Rules0, Rules)
    ).

expand_rule(Decls, Types, (Head0 <- Body0), (Head <- Body)) :- !,
    expand_atom(Decls, Types, Head0, Head),
    expand_goal(Decls, Types, Body0, Body).
expand_rule(Decls, Types, (Head0 <+ Body0), (Head <+ Body)) :- !,
    expand_atom(Decls, Types, Head0, Head),
    expand_goal(Decls, Types, Body0, Body).
expand_rule(_, _, Rule, Rule).

% Conjunction is the body's spine and is always descended. Every other
% registry construct is descended only where its arguments are relation ATOMS:
% the not/1 arm, the latest/pre/finalize wrappers, and the splice families.
% decode/2, comparisons and := carry patterns and expressions, never rel
% columns, so their arguments are left exactly as written.
expand_goal(Decls, Types, (Left0, Right0), (Left, Right)) :- !,
    expand_goal(Decls, Types, Left0, Left),
    expand_goal(Decls, Types, Right0, Right).
expand_goal(Decls, Types, Goal0, Goal) :-
    nonvar(Goal0),
    body_surface_for_term(Goal0, _, _, AnalyzeRole, _, _),
    !,
    expand_surface_goal(Decls, Types, AnalyzeRole, Goal0, Goal).
expand_goal(Decls, Types, Goal0, Goal) :-
    expand_atom(Decls, Types, Goal0, Goal).

expand_surface_goal(Decls, Types, arm(neg), Goal0, Goal) :- !,
    Goal0 =.. [Functor | Args0],
    maplist(expand_goal(Decls, Types), Args0, Args),
    Goal =.. [Functor | Args].
expand_surface_goal(Decls, Types, splice_bare, Goal0, Goal) :- !,
    Goal0 =.. [Functor | Args0],
    maplist(expand_goal(Decls, Types), Args0, Args),
    Goal =.. [Functor | Args].
% Rank B11: the wrapper family is stated ONCE, in 0_body_walk.pl, and read
% here. This module keeps its own recursion for the reason that file's header
% already records for the other rewrites -- walk_body/3 observes a body and
% cannot rebuild one -- but it no longer keeps its own copy of WHICH wrappers
% carry a relation atom, which is the part that drifted.
expand_surface_goal(Decls, Types, _, Goal0, Goal) :-
    (   compound(Goal0),
        functor(Goal0, Wrapper, 1),
        relation_atom_wrapper(Wrapper)
    ->  arg(1, Goal0, Inner0),
        expand_atom(Decls, Types, Inner0, Inner),
        Goal =.. [Wrapper, Inner]
    ;   Goal = Goal0
    ).

expand_atom(Decls, Types, Atom0, Atom) :-
    (   compound(Atom0),
        functor(Atom0, Name, Arity),
        relation_columns_and_types(Decls, Types, Name/Arity, Columns, ColumnTypes),
        length(Columns, Arity)
    ->  Atom0 =.. [_ | Args0],
        maplist(expand_argument(Types), ColumnTypes, Args0, Args),
        Atom =.. [Name | Args]
    ;   Atom = Atom0
    ).

expand_argument(Types, ColumnType, Arg0, Arg) :-
    (   declared_type_name(Types, ColumnType),
        relation_value_object(Types, ColumnType, Arg0, Object)
    ->  Arg = Object
    ;   Arg = Arg0
    ).
