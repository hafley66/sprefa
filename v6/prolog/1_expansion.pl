% Declared order for surface-sugar expansion.
%
%   expansion_phase(+Order, +Name, +Expander)
%   expand_program(+SurfaceProgram, -ExpandedProgram, -ExpansionContext)
%
% Expander is `unwired` for a phase whose expander does not exist yet, or
% Module:Predicate called as Predicate(+Context, +Program, -Program).
%
:- module(expansion,
          [ expansion_phase/3,
            expand_program/3
          ]).

:- use_module(library(lists)).
:- use_module('0_enum_expand', [enum_context/2]).
:- use_module('0_match_expand', []).
:- use_module('0_seq_expand', []).
:- use_module('0_coalesce_expand', []).
:- use_module('0_relation_edge_expand', []).

% ── the order, stated once ───────────────────────────────────────────────────

expansion_phase(10, enum,        enum_expand:expand_enum_in_context).
expansion_phase(20, decl_spread, unwired).
expansion_phase(30, row_spread,  unwired).
expansion_phase(40, match,       match_expand:expand_match_program_in_context).
expansion_phase(42, seq,          seq_expand:expand_seq_in_context).
% AFTER match, BEFORE relation_edge, and both halves of that are load-bearing.
%
%   after match   a match arm is an ordinary rule body and may carry a
%                 coalesce; running first would leave those unexpanded, and
%                 the survival refusal (coalesce_not_top_level) would then
%                 fire on a legal program.
%   before        coalesce SPLITS one rule into two clauses. relation_edge
%   relation_edge appends the head's relation-value membership atom per
%                 CLAUSE, so it must see the split set or the absent arm
%                 loses the atom that makes its head target visible to
%                 stratification.
expansion_phase(45, coalesce,    coalesce_expand:expand_coalesce_in_context).
expansion_phase(50, relation_edge,
                relation_edge_expand:expand_relation_edges_in_context).

% ── the fold ─────────────────────────────────────────────────────────────────

expand_program(SurfaceProgram, ExpandedProgram, ExpansionContext) :-
    SurfaceProgram = prog(SurfaceDecls, _),
    enum_context(SurfaceDecls, ExpansionContext),
    findall(Order-Name-Expander,
            expansion_phase(Order, Name, Expander),
            UnorderedPhases),
    msort(UnorderedPhases, OrderedPhases),
    foldl(run_phase(ExpansionContext), OrderedPhases,
          SurfaceProgram, ExpandedProgram).

run_phase(_, _-_-unwired, Program, Program) :- !.
run_phase(Context, _-_-Expander, Program, Expanded) :-
    call(Expander, Context, Program, Expanded).
