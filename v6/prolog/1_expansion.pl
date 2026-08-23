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
:- use_module('0_enum_expand', [enum_context/2,
                                merge_enum_type_rows/3,
                                merge_option_type_rows/2,
                                drop_minted_keyed_on_derived/3]).
:- use_module('0_generic_expand', [expand_generic_program/2, freeze_type_rows/2,
                                   reset_type_row_memo/0]).
:- use_module('0_match_expand', []).
:- use_module('0_seq_expand', []).
:- use_module('0_coalesce_expand', []).
:- use_module('0_dot_expand', []).
:- use_module('0_negated_guard_expand', []).
:- use_module('0_relation_edge_expand', []).
:- use_module('0_ast_expand', []).
:- use_module(compile_messages,
              [ dl6_debug/3, dl6_debugging/1, dl6_program_sizes/3 ]).
:- use_module('compile/0_trace', [run_compile_step/4]).

% ── the order, stated once ───────────────────────────────────────────────────

expansion_phase(5,  option,      generic_expand:expand_generic_in_context).
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
%                 the survival unsupported construct (coalesce_not_top_level) would then
%                 fire on a legal program.
%   before        coalesce SPLITS one rule into two clauses. relation_edge
%   relation_edge appends the head's relation-value membership atom per
%                 CLAUSE, so it must see the split set or the absent arm
%                 loses the atom that makes its head target visible to
%                 stratification.
expansion_phase(45, coalesce,    coalesce_expand:expand_coalesce_in_context).
expansion_phase(46, ast,         ast_expand:expand_ast_in_context).
% AFTER coalesce, BEFORE relation_edge: a comparison under not/1 is inverted
% into its complement (not(X > 1) -> X =< 1) so a guard is never dropped by
% the NOT EXISTS lowering; the checks that follow see the complement, not the
% negation.
expansion_phase(47, negated_guard,
                negated_guard_expand:expand_negated_guards_in_context).
expansion_phase(50, relation_edge,
                relation_edge_expand:expand_relation_edges_in_context).

% ── the fold ─────────────────────────────────────────────────────────────────

expand_program(SurfaceProgram, ExpandedProgram, ExpansionContext) :-
    expand_program_run(oracle, SurfaceProgram, [], ExpandedProgram,
                       ExpansionContext).

expand_program_with_bindings(SurfaceProgram, Bindings,
                             ExpandedProgram, ExpansionContext) :-
    expand_program_run(compiler, SurfaceProgram, Bindings, ExpandedProgram,
                       ExpansionContext).

expand_program_run(Door, SurfaceProgram0, Bindings, ExpandedProgram,
                   ExpansionContext) :-
    reset_type_row_memo,
    % BEFORE phase 5. `list(orchard.tree)` mints its artifact name from the
    % element, and a path is not a name until the decl tree resolves it.
    run_compile_step(plan, expansion:resolve_qualified_types,
                     dot_expand:resolve_qualified_types(SurfaceProgram0,
                                                        SurfaceProgram), _),
    SurfaceProgram = prog(SurfaceDecls, _),
    % The context includes enums option expansion mints.  Generic expansion is
    % deterministic and idempotent for its rewritten declaration form.  This
    % prepass has no rule facts or source bindings, so annotation applications
    % belong exclusively to phase 5's bound program expansion below.
    run_compile_step(plan, expansion:annotation_context_decls,
                     annotation_context_decls(SurfaceDecls, ContextDecls), _),
    run_compile_step(plan, expansion:context_generic_expand,
                     expand_generic_program(prog(ContextDecls, []),
                                            prog(DeclsForEnumContext, _)), _),
    run_compile_step(plan, expansion:enum_context,
                     enum_context(DeclsForEnumContext, EnumContext), _),
    findall(Order-Name-Expander,
            expansion_phase(Order, Name, Expander),
            UnorderedPhases),
    msort(UnorderedPhases, OrderedPhases),
    expansion_phase_start(SurfaceProgram, Bindings, SurfaceDecls, ContextDecls,
                          DeclsForEnumContext, OrderedPhases,
                          PhaseProgram, RemainingPhases),
    foldl(run_phase(expansion_context(Door, EnumContext, Bindings)),
          RemainingPhases,
          PhaseProgram, PhasedProgram),
    run_compile_step(plan, expansion:drop_minted_keyed_on_derived,
                     drop_minted_keyed_on_derived(EnumContext, PhasedProgram,
                                                  DroppedProgram), _),
    run_compile_step(plan, expansion:merge_enum_type_rows,
                     merge_enum_type_rows(SurfaceDecls, DroppedProgram,
                                          EnumRowedProgram), _),
    run_compile_step(plan, expansion:merge_option_type_rows,
                     merge_option_type_rows(EnumRowedProgram,
                                            OptionRowedProgram), _),
    OptionRowedProgram = prog(OptionDecls, OptionRules),
    run_compile_step(plan, expansion:freeze_type_rows,
                     freeze_type_rows(OptionDecls, FrozenDecls), _),
    ExpandedProgram = prog(FrozenDecls, OptionRules),
    ExpansionContext = EnumContext.

expansion_phase_start(prog(SurfaceDecls, []), Bindings,
                      SurfaceDecls, ContextDecls, DeclsForEnumContext,
                      [_-option-_ | RemainingPhases],
                      prog(DeclsForEnumContext, []), RemainingPhases) :-
    Bindings == [],
    ContextDecls == SurfaceDecls,
    !.
expansion_phase_start(SurfaceProgram, _, _, _, _, OrderedPhases,
                      SurfaceProgram, OrderedPhases).

annotation_context_decls(Decls0, Decls) :-
    maplist(annotation_context_decl(Decls0), Decls0, Decls).

annotation_context_decl(Decls, col_type(Ref, Column, Type0),
                        col_type(Ref, Column, Type)) :-
    !,
    annotation_context_type(Decls, Type0, Type).
annotation_context_decl(_, Decl, Decl).

annotation_context_type(Decls, annotated_type(Type0, _), Type) :-
    !,
    annotation_context_type(Decls, Type0, Type).
annotation_context_type(Decls, Type0, Type) :-
    direct_compiler_type_call(Decls, Type0, Input),
    !,
    annotation_context_type(Decls, Input, Type).
annotation_context_type(Decls, Type0, Type) :-
    compound(Type0),
    !,
    Type0 =.. [Name | Arguments0],
    maplist(annotation_context_type(Decls), Arguments0, Arguments),
    Type =.. [Name | Arguments].
annotation_context_type(_, Type, Type).

direct_compiler_type_call(Decls, Type, Input) :-
    compound(Type),
    Type =.. [Name, Input | _],
    member(col_type(Ref, return, type), Decls),
    Ref = Name/_.

% The entering line runs unconditionally so a phase that WEDGES or fails names
% itself with the topic off; the sizes cost a walk of both programs and are
% taken only under the topic.
run_phase(Context, Order-Name-Expander, Program, Expanded) :-
    dl6_debug(expand, "enter ~w (order ~w)", [Name, Order]),
    (   dl6_debugging(expand)
    ->  dl6_program_sizes(Program, DeclsIn, RulesIn),
        run_phase_step(Context, Order-Name-Expander, Program, Expanded),
        dl6_program_sizes(Expanded, DeclsOut, RulesOut),
        dl6_debug(expand, "~w decls ~d->~d rules ~d->~d",
                  [Name, DeclsIn, DeclsOut, RulesIn, RulesOut])
    ;   run_phase_step(Context, Order-Name-Expander, Program, Expanded)
    ).

run_phase_step(Context, Order-Name-Expander, Program, Expanded) :-
    run_compile_step(plan, expansion:Name,
                     run_phase_call(Context, Order-Name-Expander,
                                    Program, Expanded), _).

run_phase_call(_, _-_-unwired, Program, Program) :- !.
% ast takes the whole context, every other phase the enum half; without the cut
% the clause below re-runs ast on the wrong argument as a second solution.
run_phase_call(Context, _-ast-Expander, Program, Expanded) :- !,
    call(Expander, Context, Program, Expanded).
run_phase_call(Context, _-option-Expander, Program, Expanded) :- !,
    call(Expander, Context, Program, Expanded).
run_phase_call(expansion_context(Door, EnumContext, _), _-coalesce-Expander,
               Program, Expanded) :- !,
    call(Expander, coalesce_context(Door, EnumContext), Program, Expanded).
run_phase_call(expansion_context(_, EnumContext, _), _-_-Expander,
               Program, Expanded) :-
    call(Expander, EnumContext, Program, Expanded).
