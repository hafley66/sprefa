% 1_expansion.pl : the declared order in which surface sugar expands, and the
% one fold that runs it.
%
% Rank 3 of plans/2026-07-29-prolog-org-review.md. Before this file, each
% consumer hand-wrote the sequence: engine.pl and compile.pl each called
% prepare_program/5 then expand_match_program/2, expand_match_program/2 itself
% called expand_enum_program/2, and analyze.pl expanded a SECOND time inside
% its own supported-subset gate. Four order sites, one of them redundant, and
% nothing stating what the order is.
%
%   expansion_phase(+Order, +Name, +Expander)
%   expand_program(+SurfaceProgram, -ExpandedProgram, -ExpansionContext)
%
% Expander is `unwired` for a phase whose expander does not exist yet, or
% Module:Predicate called as Predicate(+Context, +Program, -Program).
%
% ── why the order is what it is ──────────────────────────────────────────────
%
% plans/2026-07-29-rel-spreading-verdict.md fixes it as
% enum -> declaration spread -> row spread -> match. Spreading is a design
% record and is NOT wired, so phases 20 and 30 are placeholder rows with no
% expander: the order is stated once, here, and inserting a spread expander
% later is filling in one field rather than finding and editing four call
% sites.
%
% ── the trap this file exists to avoid ───────────────────────────────────────
%
% Enum must run BEFORE match under that order, and moving the calls alone
% SILENTLY breaks match exhaustiveness. Measured, not assumed:
%
%   expand_enum_program/2 removes every enum_decl/2 entry, and match coverage
%   used to read enum_decl/2 straight out of the declarations. Running enum
%   first therefore left validate_match_coverage/2 looking at declarations
%   that no longer mention any enum, so a match block missing an arm expanded
%   quietly instead of throwing match_nonexhaustive.
%
%   Receipt, taken against the unmodified expanders: for a two-variant enum
%   with a one-arm match, `expand_match_program/2` throws
%   unsupported_construct(match_nonexhaustive(body, redirect)), while
%   expand_enum_program/2 followed by expand_match_program/2 succeeds. The
%   expanded TERMS are identical either way, so no output diff would have
%   caught it.
%
% The fix is the ExpansionContext: enum metadata is computed from the SURFACE
% declarations before any phase runs, and handed to whichever later phase needs
% it. enum_context/2 lives in 0_enum_expand.pl, the file that owns variant
% naming, so the context cannot drift from what expansion produces.

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
