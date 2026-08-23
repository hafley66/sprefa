% 0_coalesce_expand.pl : `coalesce/2`, the use-site total read.
%
% One rule carrying a coalesce becomes two ordinary
% clauses of the same head:
%
%   h(...) <- Rest, latest_commit(Repo, Commit).
%   h(...) <- Rest, not(latest_commit(Repo, _)), Commit := 'absent'.
%
% Multiple coalesce goals produce 2^N clauses.
%
% In a LEVEL body the present arm is the bare relation atom: a level body reads
% state. In an EDGE body a bare atom is a TRIGGER, an occurrence, so the bare
% form would turn an optional lookup into a second firing source. The edge arm
% is therefore `latest(Atom)`, the sampled base-table read
% (registry.pl latest/1, the N->(0|1) coercion). The edge expander uses the
% same split for membership atoms.
%
% Both emitted clauses retain the source rule's variable identity.
%
% ── what is refused, and why each one is decidable here ──────────────────────
%
%   coalesce_source_not_rel_atom   the first argument is not a plain relation
%                                  atom (a wrapper, a conjunction, a variable).
%   coalesce_no_output             every variable in the atom is already bound
%                                  by the rest of the body, so there is nothing
%                                  for the default to bind and the goal is a
%                                  membership test wearing a default.
%   coalesce_multiple_outputs      two or more unbound variables. A default is
%                                  ONE value; which column it lands in would be
%                                  a guess.
%   coalesce_output_not_column     the unbound variable is nested inside a
%                                  compound argument rather than being a column
%                                  of the atom, so there is no column to
%                                  default.
%   coalesce_default_not_literal   a variable or compound default. The default
%                                  must be a value, and a variable one would
%                                  reintroduce exactly the unbound-hole the
%                                  ruling exists to prevent.
%   coalesce_not_top_level         a coalesce that survived the conjunction
%                                  spine: nested under not/1, next/1, combine,
%                                  or sitting in a head. This is the one that
%                                  keeps the construct from ever reaching
%                                  analyze.pl, where a live registry row with
%                                  no lowering would be read as an ordinary
%                                  join and drop the default in SILENCE.
%
% Every unsupported construct is thrown HERE, in the one expansion both doors consult
% (1_expansion.pl phase 45), so the oracle and the compiler cannot disagree
% about which programs are legal. The single-door unsupported construct is this repo's
% recurring defect class and a shared expander is the shape that closes it.

:- module(coalesce_expand,
          [ expand_coalesce_in_context/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module(library(occurs), [sub_term/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

expand_coalesce_in_context(coalesce_context(compiler, _),
                           prog(Decls, Rules), prog(Decls, Rules)) :-
    !,
    validate_compiler_rules(Rules).
expand_coalesce_in_context(_, prog(Decls, Rules0), prog(Decls, Rules)) :-
    expand_rules(Rules0, Rules).

validate_compiler_rules([]).
validate_compiler_rules([Rule | Rest]) :-
    validate_compiler_rule(Rule),
    validate_compiler_rules(Rest).

validate_compiler_rule((Head <- Body)) :-
    !,
    refuse_coalesce_in_head(Head),
    validate_level_body(Body).
validate_compiler_rule((Head <+ Body)) :-
    !,
    refuse_coalesce_in_head(Head),
    expand_clause(edge, Head, Body, _).
validate_compiler_rule(Rule) :-
    refuse_residual_coalesce(Rule).

validate_level_body(Body) :-
    conjunction_goals(Body, Goals),
    validate_level_goals(Goals, Goals),
    exclude(top_level_coalesce, Goals, OtherGoals),
    goals_conjunction(OtherGoals, OtherBody),
    refuse_residual_coalesce(OtherBody).

validate_level_goals([], _).
validate_level_goals([Goal | Rest], AllGoals) :-
    (   top_level_coalesce(Goal)
    ->  Goal = coalesce(Source, Default),
        select_eq(Goal, AllGoals, OtherGoals),
        validate_default(Source, Default),
        output_column(Source, OtherGoals, _, _)
    ;   true
    ),
    validate_level_goals(Rest, AllGoals).

top_level_coalesce(Goal) :-
    nonvar(Goal),
    Goal = coalesce(_, _).

select_eq(Target, [Head | Rest], Remaining) :-
    (   Target == Head
    ->  Remaining = Rest
    ;   Remaining = [Head | More],
        select_eq(Target, Rest, More)
    ).

expand_rules([], []).
expand_rules([Rule | Rest], Rules) :-
    expand_rule(Rule, Clauses),
    expand_rules(Rest, RestRules),
    append(Clauses, RestRules, Rules).

expand_rule((Head <- Body), Clauses) :-
    !,
    refuse_coalesce_in_head(Head),
    expand_clause(level, Head, Body, Clauses).
expand_rule((Head <+ Body), Clauses) :-
    !,
    refuse_coalesce_in_head(Head),
    expand_clause(edge, Head, Body, Clauses).
expand_rule(Rule, [Rule]) :-
    refuse_residual_coalesce(Rule).

% One coalesce per pass, recursively, so N goals fan out to 2^N clauses and
% each recursion re-derives "unbound by the REST of the body" against the body
% the previous split produced.
expand_clause(Arrow, Head, Body, Clauses) :-
    conjunction_goals(Body, Goals),
    (   split_first_coalesce(Goals, Before, Source, Default, After)
    ->  append(Before, After, RestGoals),
        validate_default(Source, Default),
        output_column(Source, RestGoals, Output, ProbeAtom),
        present_goal(Arrow, Source, PresentGoal),
        rebuild(Before, [PresentGoal], After, PresentBody),
        rebuild(Before, [not(ProbeAtom), Output := Default], After, AbsentBody),
        expand_clause(Arrow, Head, PresentBody, PresentClauses),
        expand_clause(Arrow, Head, AbsentBody, AbsentClauses),
        append(PresentClauses, AbsentClauses, Clauses)
    ;   refuse_residual_coalesce(Body),
        build_rule(Arrow, Head, Body, Rule),
        Clauses = [Rule]
    ).

build_rule(level, Head, Body, (Head <- Body)).
build_rule(edge,  Head, Body, (Head <+ Body)).

% A level body reads state, so the present arm is the bare atom. An edge body
% reads OCCURRENCES, so the bare atom would be a second trigger; latest/1 is
% the sampled read that keeps the lookup a lookup.
present_goal(level, Atom, Atom).
present_goal(edge,  Atom, latest(Atom)).

% ── the conjunction spine ────────────────────────────────────────────────────
% Flattened to a list and rebuilt only on the branch that actually carries a
% coalesce; a rule without one is returned byte-identical, so no existing
% fixture's body shape moves.

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

rebuild(Before, Middle, After, Body) :-
    append(Before, Middle, Front),
    append(Front, After, Goals),
    goals_conjunction(Goals, Body).

split_first_coalesce([Goal | Rest], [], Source, Default, Rest) :-
    nonvar(Goal),
    Goal = coalesce(Source, Default),
    !.
split_first_coalesce([Goal | Rest], [Goal | Before], Source, Default, After) :-
    split_first_coalesce(Rest, Before, Source, Default, After).

% ── the output column ────────────────────────────────────────────────────────

output_column(Source, RestGoals, Output, ProbeAtom) :-
    (   plain_relation_atom(Source)
    ->  true
    ;   throw(unsupported_construct(coalesce_source_not_rel_atom(Source)))
    ),
    functor(Source, Name, Arity),
    Ref = Name/Arity,
    term_variables(Source, SourceVariables),
    term_variables(RestGoals, BoundVariables),
    exclude(occurs_in(BoundVariables), SourceVariables, Unbound),
    (   Unbound == []
    ->  throw(unsupported_construct(coalesce_no_output(Ref)))
    ;   Unbound = [Output]
    ->  true
    ;   length(Unbound, Count),
        throw(unsupported_construct(coalesce_multiple_outputs(Ref, Count)))
    ),
    Source =.. [Name | Arguments],
    (   hole_arguments(Arguments, Output, ProbeArguments)
    ->  ProbeAtom =.. [Name | ProbeArguments]
    ;   throw(unsupported_construct(coalesce_output_not_column(Ref)))
    ).

% The absent arm asks "is there ANY row for the bound columns", so the output
% column becomes ONE fresh variable -- one, not one per position, or an atom
% written `r(Out, Out)` would lose its own equality constraint.
hole_arguments(Arguments, Output, ProbeArguments) :-
    maplist(hole_argument(Output, _Fresh), Arguments, ProbeArguments),
    memberchk_eq(Output, Arguments).

hole_argument(Output, Fresh, Argument, Hole) :-
    ( Argument == Output -> Hole = Fresh ; Hole = Argument ).

memberchk_eq(Variable, [Head | Rest]) :-
    ( Head == Variable -> true ; memberchk_eq(Variable, Rest) ).

occurs_in(Variables, Variable) :- memberchk_eq(Variable, Variables).

plain_relation_atom(Atom) :-
    nonvar(Atom),
    compound(Atom),
    functor(Atom, Name, Arity),
    atom(Name),
    Arity > 0,
    \+ language_form(Name/Arity).

% The body forms that are language, never a relation. Kept short on purpose:
% anything the registry knows as a body construct plus the conjunction spine.
language_form(','/2).
language_form(';'/2).
language_form((<-)/2).
language_form((<+)/2).
language_form((:=)/2).
language_form(is/2).
language_form(not/1).
language_form(latest/1).
language_form(finalize/1).
language_form(pre/1).
language_form(now/1).
language_form(next/1).
% Variadic, so it is stated as one clause over the arity rather than a row per
% width. It was missing entirely, which made `combine(...)` read as a plain
% RELATION ATOM here -- harmless while parse_dl desugared every combine away
% before this file ran, and a live hole the moment the term survives parsing.
language_form(combine/_).
language_form(coalesce/2).
language_form(decode/2).
language_form(json_each/2).
language_form(match/2).
language_form((<)/2).
language_form((=<)/2).
language_form((>)/2).
language_form((>=)/2).
language_form((==)/2).
language_form((\==)/2).

% ── the default ──────────────────────────────────────────────────────────────

% A VARIABLE default reports the atom `variable` rather than the variable
% itself. Two reasons, and the second is the operational one: `_586` in a
% unsupported construct message names nothing, and a unsupported construct term carrying a free variable
% cannot be written down in a fixture at all (engine.pl grades throws/1 by
% ==/2, so an unbound hole never matches).
validate_default(Source, Default) :-
    (   coalesce_literal(Default)
    ->  true
    ;   functor(Source, Name, Arity),
        ( var(Default) -> Reported = variable ; Reported = Default ),
        throw(unsupported_construct(
                  coalesce_default_not_literal(Name/Arity, Reported)))
    ).

coalesce_literal(Default) :- var(Default), !, fail.
coalesce_literal(bool_lit(_)) :- !.
coalesce_literal(Default) :- number(Default), !.
coalesce_literal(Default) :- string(Default), !.
coalesce_literal(Default) :- atom(Default).

% ── the survival check ───────────────────────────────────────────────────────
% Everything above consumes coalesce goals on the conjunction SPINE. A coalesce
% anywhere else -- under not/1, inside next/1 or combine, in a head -- would
% otherwise reach analyze.pl, where the live registry row's refs_of_arg role
% reads the source atom as an ordinary join and the default vanishes without a
% word. Refusing by name here is what keeps the row honest.

% Both name the SOURCE reference rather than echoing the whole term: the term
% carries the rule's own free variables, and engine.pl grades throws/1 by ==/2,
% so a reason with a hole in it is a reason no fixture can pin.
refuse_residual_coalesce(Term) :-
    (   residual_coalesce_source(Term, Ref)
    ->  throw(unsupported_construct(coalesce_not_top_level(Ref)))
    ;   true
    ).

refuse_coalesce_in_head(Head) :-
    (   residual_coalesce_source(Head, Ref)
    ->  throw(unsupported_construct(coalesce_in_head(Ref)))
    ;   true
    ).

residual_coalesce_source(Term, Ref) :-
    sub_term(Sub, Term),
    nonvar(Sub),
    Sub = coalesce(Source, _),
    !,
    ( nonvar(Source), compound(Source)
    -> functor(Source, Name, Arity), Ref = Name/Arity
    ;  Ref = Source
    ).
