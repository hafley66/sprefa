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
            expand_program/3,
            expand_program_with_bindings/4
          ]).

:- use_module(library(lists)).
:- use_module('0_enum_expand', [enum_context/2]).
:- use_module('0_match_expand', []).
:- use_module('0_seq_expand', []).
:- use_module('0_coalesce_expand', []).
:- use_module('0_dot_expand', []).
:- use_module('0_relation_edge_expand', []).
:- use_module('0_ast_expand', []).

% ── the order, stated once ───────────────────────────────────────────────────

expansion_phase(10, enum,        enum_expand:expand_enum_in_context).
expansion_phase(20, decl_spread, unwired).
expansion_phase(30, row_spread,  unwired).
expansion_phase(40, match,       match_expand:expand_match_program_in_context).
expansion_phase(42, seq,          seq_expand:expand_seq_in_context).
% AFTER match, BEFORE coalesce, and both halves of that decide the answer.
%
%   after match   a match arm is an ordinary rule body by then, so one walk
%                 reaches every dot chain.
%   before        every later phase must see the brace program the author
%   coalesce      could have typed by hand, so coalesce, relation_edge, and
%                 the checks cannot tell the two spellings apart.
expansion_phase(44, dot,         dot_expand:expand_dot_in_context).
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
expansion_phase(46, ast,         ast_expand:expand_ast_in_context).
expansion_phase(50, relation_edge,
                relation_edge_expand:expand_relation_edges_in_context).

% ── the fold ─────────────────────────────────────────────────────────────────

expand_program(SurfaceProgram, ExpandedProgram, ExpansionContext) :-
    expand_program_run(SurfaceProgram, [], ExpandedProgram,
                       ExpansionContext).

expand_program_with_bindings(SurfaceProgram, Bindings,
                             ExpandedProgram, ExpansionContext) :-
    expand_program_run(SurfaceProgram, Bindings, ExpandedProgram,
                       ExpansionContext).

expand_program_run(SurfaceProgram, Bindings, ExpandedProgram,
                   ExpansionContext) :-
    SurfaceProgram = prog(SurfaceDecls, _),
    enum_context(SurfaceDecls, EnumContext),
    findall(Order-Name-Expander,
            expansion_phase(Order, Name, Expander),
            UnorderedPhases),
    msort(UnorderedPhases, OrderedPhases),
    foldl(run_phase(expansion_context(EnumContext, Bindings)),
          OrderedPhases,
          SurfaceProgram, ExpandedProgram),
    ExpansionContext = EnumContext.

run_phase(_, _-_-unwired, Program, Program) :- !.
% ast takes the whole context, every other phase the enum half; without the cut
% the clause below re-runs ast on the wrong argument as a second solution.
run_phase(Context, _-ast-Expander, Program, Expanded) :- !,
    call(Expander, Context, Program, Expanded).
run_phase(expansion_context(EnumContext, _), _-_-Expander,
          Program, Expanded) :-
    call(Expander, EnumContext, Program, Expanded).
