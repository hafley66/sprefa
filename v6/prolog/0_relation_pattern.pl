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
% ── why this file exists ─────────────────────────────────────────────────────
%
% There is ONE relation-value spelling in this system and it is obj(SortedPairs)
% (0_type_plane.pl header; the arrival path has produced it since
% canonicalize_world_rows/3 landed). A rule head that BUILT a relation value
% never went through that path: engine.pl stored the raw compound, so
%
%   the tick log printed prolog term text (`"repo(acme)"`) where the emitted
%   side printed canonical JSON (`{"name":"acme"}`) -- divergent at depth 1,
%   never graded because no fixture built a relation value in a rule; and
%
%   body.pl json_decode/2, which has no clause for a compound, failed on every
%   decode/2 over a constructed value, which is
%   plans/2026-07-30-file-span-spine-reconciled.md section 3.3's "emitter
%   right, oracle empty".
%
% Rewriting to the object fixes both at once and costs no new syntax: the term
% is sugar, the object is the value, and a PATTERN (`file(_, fpath(Path))`)
% rewrites to a pattern object that unifies against a stored value at any
% depth, which is why depth-N reads need no depth-N machinery here.
%
% ── what it deliberately does not do ─────────────────────────────────────────
%
% It is not part of 1_expansion.pl's shared phase table. The compiler needs the
% TERM form intact so lower.pl can turn each level into a `__ref_<type>`
% dictionary join; handing it the object instead would replace one unlowerable
% shape with another. The two doors agree on the VALUE, not on the intermediate
% representation, and the tick-log grade is what says so.
%
% Malformed shapes never reach here: they are the shared refusal
% relation_pattern_not_a_relation_value in 0_program_check.pl, which both doors
% run before this pass.

:- module(relation_pattern,
          [ expand_relation_values/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('0_type_plane',
              [ type_definitions/2, declared_type_name/2,
                relation_columns_and_types/5, relation_value_object/4 ]).
:- use_module('compile/registry', [body_surface_for_term/6]).

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
expand_surface_goal(Decls, Types, _, Goal0, Goal) :-
    (   compound(Goal0),
        functor(Goal0, Wrapper, 1),
        memberchk(Wrapper, [latest, pre, finalize])
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
