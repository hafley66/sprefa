:- module(dl7_compiler,
          [ compile_dl7/4,
            compile_dl7_project/5,
            compile_unit/3,
            compile_units/3,
            type_prelude_paths/1
          ]).

:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('../0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../0_reader/4_module_loader', [load_dl7_project/4]).
:- use_module('../1_libtime/0_evaluator',
              [ evaluate/4,
                validate_functional_rows/3
              ]).
:- use_module('0a_module_lowerer',
              [ lower_units_deferred/4,
                lower_units_with_environment/5,
                lower_units_with_environment_deferred/5,
                lower_units_with_exporter_deferred/5,
                lower_units_with_exporter_and_environment/6,
                lower_units_with_exporter_and_environment_deferred/6,
                merge_module_basements/4
              ]).
:- use_module('0b_filesystem_grapher', [install_project_graph/6]).
:- use_module('1_checker',
              [ check_datalog/4,
                check_resolved_rules/5
              ]).
:- use_module('1a_generated_program_assembler',
              [assemble_generated_program/5]).

%% compile_dl7(+Path, -CompilerRows, -RuntimeProgram, -Diagnostics) is det.
%
% Load the userland type prelude and one source file through the same reader,
% then run every compile-known positive rule over the initial type graph.
compile_dl7(Path, CompilerRows, RuntimeProgram, Diagnostics) :-
    once(absolute_file_name(Path, ProgramPath,
                            [access(read), file_errors(error)])),
    load_type_prelude(PreludeUnit, PreludeDiagnostics),
    read_file_to_string(ProgramPath, ProgramText, [encoding(utf8)]),
    dl7_text_unit(file(ProgramPath), ProgramPath, ProgramText,
                  ProgramUnit, ProgramDiagnostics),
    append(PreludeDiagnostics, ProgramDiagnostics, ReaderDiagnostics),
    compile_after_reads(ReaderDiagnostics, [PreludeUnit, ProgramUnit],
                        Compiled, Diagnostics),
    compiled_outputs(Compiled, CompilerRows, RuntimeProgram),
    !,
    garbage_collect.

%% compile_dl7_project(+Root, +Paths,
%%                     -CompilerRows, -RuntimeProgram, -Diagnostics) is det.
%
% Load several filesystem-owned source units, add their directory and module
% products to the initial graph, then run the same checker and fixpoint used
% by the single-file compiler.
compile_dl7_project(Root, Paths,
                    CompilerRows, RuntimeProgram, Diagnostics) :-
    load_type_prelude(PreludeUnit, PreludeDiagnostics),
    load_dl7_project(Root, Paths, Project, ProjectDiagnostics),
    append(PreludeDiagnostics, ProjectDiagnostics, ReaderDiagnostics),
    compile_after_project_reads(ReaderDiagnostics, PreludeUnit, Project,
                                Compiled, Diagnostics),
    compiled_outputs(Compiled, CompilerRows, RuntimeProgram),
    !,
    garbage_collect.

load_type_prelude(PreludeUnit, Diagnostics) :-
    once(type_prelude_paths(PreludePaths)),
    read_prelude_texts(PreludePaths, PreludeTexts),
    join_prelude_texts(PreludeTexts, PreludeText),
    dl7_text_unit(prelude, prelude, PreludeText,
                  PreludeUnit, Diagnostics).

type_prelude_paths(Paths) :-
    once(source_file(dl7_compiler:compile_dl7(_, _, _, _), SourcePath)),
    once(absolute_file_name(SourcePath, AbsoluteSourcePath,
                            [access(read), file_errors(error)])),
    file_directory_name(AbsoluteSourcePath, ComptimeDirectory),
    directory_file_path(ComptimeDirectory, '../../prelude', PreludeDirectory),
    once(absolute_file_name(PreludeDirectory, AbsolutePreludeDirectory,
                            [file_type(directory), access(read),
                             file_errors(error)])),
    directory_files(AbsolutePreludeDirectory, Entries),
    include(numbered_dl7_file, Entries, NumberedEntries),
    sort(NumberedEntries, SortedEntries),
    maplist(prelude_path(AbsolutePreludeDirectory), SortedEntries, Paths).

numbered_dl7_file(Entry) :-
    file_name_extension(Stem, dl7, Entry),
    sub_atom(Stem, Before, 1, _, '_'),
    Before > 0,
    sub_atom(Stem, 0, Before, _, Prefix),
    atom_number(Prefix, _).

prelude_path(Directory, Entry, Path) :-
    directory_file_path(Directory, Entry, Path).

read_prelude_texts([], []).
read_prelude_texts([Path|Paths], [Text|Texts]) :-
    read_file_to_string(Path, Text, [encoding(utf8)]),
    read_prelude_texts(Paths, Texts).

join_prelude_texts([], "").
join_prelude_texts([Text|Texts], Joined) :-
    join_prelude_texts(Texts, TextsJoined),
    (   Texts == []
    ->  Joined = Text
    ;   format(string(Joined), "~s~n~s", [Text, TextsJoined])
    ).

compile_after_reads([], Units, Compiled, Diagnostics) :-
    !,
    compile_units(Units, Compiled, Diagnostics).
compile_after_reads(Diagnostics, _, [], Diagnostics).

compile_after_project_reads([], PreludeUnit,
                            dl7_project(CanonicalRoot, Units),
                            Compiled, Diagnostics) :-
    !,
    compile_project_units(
        dl7_project(CanonicalRoot, Units),
        [PreludeUnit | Units], Compiled, Diagnostics).
compile_after_project_reads(Diagnostics, _, _, [], Diagnostics).

compiled_outputs(compiled_unit(_, RuntimeProgram, CompilerRows),
                 CompilerRows, RuntimeProgram).
compiled_outputs([], [], []).

%% compile_unit(+Unit, -Compiled, -Diagnostics) is det.
%
% The type graph and authored ground facts seed the shared evaluator. The
% complete checked program survives as runtime input while compiler closure is
% retained as immutable artifact data.
compile_unit(Unit, Compiled, Diagnostics) :-
    compile_units([Unit], Compiled, Diagnostics).

%% compile_units(+Units, -Compiled, -Diagnostics) is det.
%
% Lower source modules independently, expose the synthetic prelude through
% ordinary alias edges, merge the owned rows, then enter the existing checker
% and comptime fixpoint.
compile_units(Units, Compiled, Diagnostics) :-
    lower_compiler_units(Units, ModuleBasements, ModuleOrigins,
                         LowerDiagnostics),
    Context = compile_context(Units, none),
    compile_after_unit_lower(LowerDiagnostics, Context,
                             ModuleBasements, ModuleOrigins,
                             Compiled, Diagnostics),
    !,
    garbage_collect.

compile_project_units(Project, Units, Compiled, Diagnostics) :-
    lower_compiler_units(Units, ModuleBasements0, ModuleOrigins0,
                         LowerDiagnostics),
    install_project_after_lower(
        LowerDiagnostics, Project, ModuleBasements0, ModuleOrigins0,
        ModuleBasements, ModuleOrigins, ProjectDiagnostics),
    Context = compile_context(Units, project(Project)),
    compile_after_unit_lower(ProjectDiagnostics, Context,
                             ModuleBasements, ModuleOrigins,
                             Compiled, Diagnostics),
    !,
    garbage_collect.

install_project_after_lower(
    [], Project, Basements0, Origins0,
    Basements, Origins, Diagnostics) :-
    !,
    install_project_graph(Project, Basements0, Origins0,
                          Basements, Origins, Diagnostics).
install_project_after_lower(
    Diagnostics, _, Basements, Origins,
    Basements, Origins, Diagnostics).

lower_compiler_units(Units, ModuleBasements, ModuleOrigins, Diagnostics) :-
    (   select(PreludeUnit, Units, ImporterUnits),
        unit_has_origin(PreludeUnit, prelude)
    ->  lower_units_with_exporter_deferred(
            PreludeUnit, ImporterUnits,
            ModuleBasements, ModuleOrigins, Diagnostics)
    ;   lower_units_deferred(Units, ModuleBasements, ModuleOrigins,
                             Diagnostics)
    ).

unit_has_origin(dl7_unit(Origin, _, _, _, _), Origin).

compile_after_unit_lower([], Context, ModuleBasements, ModuleOrigins,
                         Compiled, Diagnostics) :-
    !,
    merge_module_basements(ModuleBasements, ModuleOrigins,
                           Basement, Origins),
    compile_after_lower([], Context, Basement, Origins,
                        Compiled, Diagnostics).
compile_after_unit_lower(Diagnostics, _, _, _, [], Diagnostics).

compile_after_lower([], Context, Basement, Origins, Compiled, Diagnostics) :-
    !,
    check_datalog(Basement, Origins, Checked, CheckDiagnostics),
    compile_after_check(CheckDiagnostics, Context, Checked,
                        Compiled, Diagnostics).
compile_after_lower(Diagnostics, _, _, _, [], Diagnostics).

compile_after_check([], Context, Checked, Compiled, Diagnostics) :-
    !,
    evaluate_checked(Context, Checked, Compiled, Diagnostics).
compile_after_check(Diagnostics, _, _, [], Diagnostics).

evaluate_checked(
    Context,
    checked_datalog(Graph,
                    datalog_program(Relations, AuthoredSeeds, Rules),
                    _, _),
    Compiled,
    Diagnostics) :-
    graph_seeds(Graph, GraphSeeds),
    append(GraphSeeds, AuthoredSeeds, BaseSeeds0),
    sort(BaseSeeds0, BaseSeeds),
    colon_rows(BaseSeeds, InitialEdges),
    intern_rows(BaseSeeds, InitialRequests),
    Context = compile_context(Units, ProjectContext),
    derived_bind_slots(Rules, DerivedBindSlots),
    EvaluationContext = compile_context(
                            Units, ProjectContext, DerivedBindSlots),
    evaluate_compiler_rounds(Rules, Relations, BaseSeeds, InitialEdges,
                             InitialRequests, [], [], 1,
                             CompilerFacts, GeneratedProgram,
                             EvaluationDiagnostics),
    finish_evaluation(EvaluationDiagnostics, EvaluationContext,
                      CompilerFacts, GeneratedProgram,
                      Compiled, Diagnostics).

derived_bind_slots(Rules, Slots) :-
    findall(
        derived_bind_slot(Owner, Name, Index),
        member(rule(call(ref(kernel(':')),
                         [ ref(Owner), const(Name),
                           var(derived_bind(_)), const(Index)
                         ]),
                    _),
               Rules),
        Slots0),
    sort(Slots0, Slots).

finish_evaluation([], Context, CompilerFacts, GeneratedProgram,
                  Compiled, Diagnostics) :-
    !,
    GeneratedProgram = generated_program(GeneratedRelations, _, _, _),
    final_checked_program(Context, CompilerFacts, GeneratedRelations,
                          FinalChecked, FinalDiagnostics),
    continue_source_refreeze(
        FinalDiagnostics, Context, FinalChecked,
        CompilerFacts, GeneratedProgram, 1,
        Compiled, Diagnostics).
finish_evaluation(Diagnostics, _, _, _, [], Diagnostics).

continue_source_refreeze(
    [], Context, Checked, CompilerFacts, GeneratedProgram, OuterRound,
    Compiled, Diagnostics) :-
    !,
    Checked = checked_datalog(_, datalog_program(_, _, Rules), _, _),
    derived_bind_diagnostics(Rules, CompilerFacts, BindDiagnostics),
    (   BindDiagnostics == []
    ->  finish_final_check([], Checked, CompilerFacts, GeneratedProgram,
                           Compiled, Diagnostics)
    ;   expand_source_compiler(
            Context, Checked, CompilerFacts, GeneratedProgram, OuterRound,
            Compiled, Diagnostics)
    ).
continue_source_refreeze(
    Diagnostics0, Context, _, CompilerFacts, GeneratedProgram, OuterRound,
    Compiled, Diagnostics) :-
    deferrable_source_diagnostics(Diagnostics0),
    !,
    GeneratedProgram = generated_program(GeneratedRelations, _, _, _),
    deferred_checked_program(
        Context, CompilerFacts, GeneratedRelations,
        DeferredChecked, DeferredDiagnostics),
    expand_after_deferred_check(
        DeferredDiagnostics, Context, DeferredChecked,
        CompilerFacts, GeneratedProgram, OuterRound,
        Compiled, Diagnostics).
continue_source_refreeze(
    Diagnostics, _, _, _, _, _, [], Diagnostics).

deferrable_source_diagnostics([Diagnostic | Diagnostics]) :-
    deferrable_source_diagnostic(Diagnostic),
    deferrable_source_diagnostics_rest(Diagnostics).

deferrable_source_diagnostics_rest([]).
deferrable_source_diagnostics_rest([Diagnostic | Diagnostics]) :-
    deferrable_source_diagnostic(Diagnostic),
    deferrable_source_diagnostics_rest(Diagnostics).

deferrable_source_diagnostic(
    diagnostic(lower, _, not_relation(_))).

expand_after_deferred_check(
    [], Context, Checked, CompilerFacts, GeneratedProgram, OuterRound,
    Compiled, Diagnostics) :-
    !,
    expand_source_compiler(
        Context, Checked, CompilerFacts, GeneratedProgram, OuterRound,
        Compiled, Diagnostics).
expand_after_deferred_check(
    Diagnostics, _, _, _, _, _, [], Diagnostics).

expand_source_compiler(
    Context,
    checked_datalog(Graph,
                    datalog_program(Relations, AuthoredSeeds, Rules),
                    _, _),
    CompilerFacts,
    generated_program(GeneratedRelations, GeneratedRules, _, _),
    OuterRound, Compiled, Diagnostics) :-
    compiler_round_limit(Limit),
    (   OuterRound >= Limit
    ->  Compiled = [],
        Diagnostics = [diagnostic(
                           compile, none,
                           source_refreeze_limit_exhausted(Limit))]
    ;   subtract(Relations, GeneratedRelations, BaseRelations),
        graph_seeds(Graph, GraphSeeds),
        append(GraphSeeds, AuthoredSeeds, BaseSeeds0),
        sort(BaseSeeds0, BaseSeeds),
        colon_rows(CompilerFacts, FrozenEdges),
        intern_rows(CompilerFacts, FrozenRequests),
        evaluate_compiler_rounds(
            Rules, BaseRelations, BaseSeeds,
            FrozenEdges, FrozenRequests,
            GeneratedRelations, GeneratedRules, 1,
            NextFacts, NextGeneratedProgram, EvaluationDiagnostics),
        continue_source_expansion(
            EvaluationDiagnostics, Context,
            NextFacts, NextGeneratedProgram, OuterRound,
            Compiled, Diagnostics)
    ).

continue_source_expansion(
    [], Context, CompilerFacts, GeneratedProgram, OuterRound,
    Compiled, Diagnostics) :-
    !,
    GeneratedProgram = generated_program(GeneratedRelations, _, _, _),
    final_checked_program(Context, CompilerFacts, GeneratedRelations,
                          FinalChecked, FinalDiagnostics),
    NextOuterRound is OuterRound + 1,
    continue_source_refreeze(
        FinalDiagnostics, Context, FinalChecked,
        CompilerFacts, GeneratedProgram, NextOuterRound,
        Compiled, Diagnostics).
continue_source_expansion(
    Diagnostics, _, _, _, _, [], Diagnostics).

finish_final_check([], FinalChecked, CompilerFacts, GeneratedProgram,
                   Compiled, Diagnostics) :-
    !,
    FinalChecked = checked_datalog(
                       _, datalog_program(Relations, _, _), _, _),
    validate_functional_rows(Relations, CompilerFacts, KeyDiagnostics),
    finish_key_validation(KeyDiagnostics, CompilerFacts, FinalChecked,
                          GeneratedProgram, Compiled, Diagnostics).
finish_final_check(Diagnostics, _, _, _, [], Diagnostics).

finish_key_validation([], CompilerFacts,
                      checked_datalog(Graph,
                                      datalog_program(Relations,
                                                      AuthoredSeeds, Rules),
                                      _, _),
                      GeneratedProgram, Compiled, Diagnostics) :-
    !,
    type_graph_facts(CompilerFacts, TypeGraphFacts),
    GeneratedProgram = generated_program(
                           GeneratedRelations, GeneratedRules, _, _),
    append(Relations, GeneratedRelations, RuntimeRelations0),
    sort(RuntimeRelations0, RuntimeRelations),
    append(Rules, GeneratedRules, RuntimeRules0),
    sort(RuntimeRules0, RuntimeRules),
    check_resolved_rules(RuntimeRelations, RuntimeRules,
                         RuntimeDepends, RuntimeStrata,
                         RuntimeDiagnostics),
    finish_runtime_program(
        RuntimeDiagnostics, Graph, RuntimeRelations, AuthoredSeeds,
        RuntimeRules, RuntimeDepends, RuntimeStrata,
        CompilerFacts, TypeGraphFacts,
        Compiled, Diagnostics).
finish_key_validation(Diagnostics, _, _, _, [], Diagnostics).

finish_runtime_program([], Graph, Relations, Seeds, Rules,
                       Depends, Strata, CompilerFacts, TypeGraphFacts,
                       compiled_unit(
                           TypeGraphFacts,
                           checked_datalog(
                               Graph,
                               datalog_program(Relations, Seeds, Rules),
                               Depends, Strata),
                           CompilerFacts), []).
finish_runtime_program(Diagnostics, _, _, _, _, _, _, _, _,
                       [], Diagnostics).

%% final_checked_program(+Context, +CompilerFacts, +GeneratedRelations,
%%                       -Checked, -Diagnostics) is det.
%
% Generated declarations and their final owner bindings form an ordinary
% expression environment for one strict source lowering. The resulting
% basement then passes through the same resolver and Datalog checker as every
% authored program.
final_checked_program(Context, CompilerFacts, GeneratedRelations,
                      Checked, Diagnostics) :-
    generated_expression_environment(
        CompilerFacts, GeneratedRelations, DerivedBindSlots, Environment),
    Context = compile_context(Units, ProjectContext, DerivedBindSlots),
    lower_final_units(Units, Environment,
                      ModuleBasements0, ModuleOrigins0, LowerDiagnostics),
    freeze_after_final_lower(
        LowerDiagnostics, Environment,
        ModuleBasements0, ModuleOrigins0,
        ModuleBasements1, ModuleOrigins1, FreezeDiagnostics),
    install_final_project_graph(
        FreezeDiagnostics, ProjectContext,
        ModuleBasements1, ModuleOrigins1,
        ModuleBasements, ModuleOrigins, ProjectDiagnostics),
    check_final_basements(ProjectDiagnostics,
                          ModuleBasements, ModuleOrigins,
                          GeneratedRelations, Checked, Diagnostics).

deferred_checked_program(Context, CompilerFacts, GeneratedRelations,
                         Checked, Diagnostics) :-
    generated_expression_environment(
        CompilerFacts, GeneratedRelations, DerivedBindSlots, Environment),
    Context = compile_context(Units, ProjectContext, DerivedBindSlots),
    lower_deferred_final_units(
        Units, Environment,
        ModuleBasements0, ModuleOrigins0, LowerDiagnostics),
    freeze_after_final_lower(
        LowerDiagnostics, Environment,
        ModuleBasements0, ModuleOrigins0,
        ModuleBasements1, ModuleOrigins1, FreezeDiagnostics),
    install_final_project_graph(
        FreezeDiagnostics, ProjectContext,
        ModuleBasements1, ModuleOrigins1,
        ModuleBasements, ModuleOrigins, ProjectDiagnostics),
    check_final_basements(ProjectDiagnostics,
                          ModuleBasements, ModuleOrigins,
                          GeneratedRelations, Checked, Diagnostics).

lower_final_units(Units, Environment,
                  ModuleBasements, ModuleOrigins, Diagnostics) :-
    (   select(PreludeUnit, Units, ImporterUnits),
        unit_has_origin(PreludeUnit, prelude)
    ->  lower_units_with_exporter_and_environment(
            PreludeUnit, ImporterUnits, Environment,
            ModuleBasements, ModuleOrigins, Diagnostics)
    ;   lower_units_with_environment(
            Units, Environment,
            ModuleBasements, ModuleOrigins, Diagnostics)
    ).

lower_deferred_final_units(Units, Environment,
                           ModuleBasements, ModuleOrigins, Diagnostics) :-
    (   select(PreludeUnit, Units, ImporterUnits),
        unit_has_origin(PreludeUnit, prelude)
    ->  lower_units_with_exporter_and_environment_deferred(
            PreludeUnit, ImporterUnits, Environment,
            ModuleBasements, ModuleOrigins, Diagnostics)
    ;   lower_units_with_environment_deferred(
            Units, Environment,
            ModuleBasements, ModuleOrigins, Diagnostics)
    ).

freeze_after_final_lower(
    [], Environment, Basements0, Origins,
    Basements, Origins, []) :-
    !,
    freeze_module_basements(Environment, Basements0, Basements).
freeze_after_final_lower(
    Diagnostics, _, Basements, Origins,
    Basements, Origins, Diagnostics).

freeze_module_basements(_, [], []).
freeze_module_basements(
    Environment,
    [module_basement(Owner,
                     basement_program(root_graph(Nodes, Edges0), Program))
     | Basements0],
    [module_basement(Owner,
                     basement_program(root_graph(Nodes, Edges), Program))
     | Basements]) :-
    Environment = expression_environment(Reservations, _, _),
    maplist(freeze_pending_edge(Reservations), Edges0, Edges),
    freeze_module_basements(Environment, Basements0, Basements).

freeze_pending_edge(
    Reservations,
    pending_edge(Owner, Name, deferred_expression(_), Index),
    pending_edge(Owner, Name, target(Relation), Index)) :-
    memberchk(reservation(Owner, Name, target(Relation), product),
              Reservations),
    !.
freeze_pending_edge(_, Edge, Edge).

install_final_project_graph(
    [], none, Basements, Origins,
    Basements, Origins, []) :-
    !.
install_final_project_graph(
    [], project(Project), Basements0, Origins0,
    Basements, Origins, Diagnostics) :-
    !,
    install_project_graph(Project, Basements0, Origins0,
                          Basements, Origins, Diagnostics).
install_final_project_graph(
    Diagnostics, _, Basements, Origins,
    Basements, Origins, Diagnostics).

check_final_basements([], ModuleBasements, ModuleOrigins,
                      GeneratedRelations, Checked, Diagnostics) :-
    !,
    merge_module_basements(ModuleBasements, ModuleOrigins,
                           Basement0, Origins),
    add_generated_relations(Basement0, GeneratedRelations, Basement),
    check_datalog(Basement, Origins, Checked, Diagnostics).
check_final_basements(Diagnostics, _, _, _, [], Diagnostics).

add_generated_relations(
    basement_program(Graph,
                     datalog_program(Relations0, Seeds, Rules)),
    GeneratedRelations,
    basement_program(Graph,
                     datalog_program(Relations, Seeds, Rules))) :-
    maplist(unref_generated_relation,
            GeneratedRelations, SourceGeneratedRelations),
    append(Relations0, SourceGeneratedRelations, Relations1),
    sort(Relations1, Relations).

unref_generated_relation(relation(ref(Relation), Arity, KeySets),
                         relation(Relation, Arity, KeySets)).

generated_expression_environment(
    CompilerFacts, GeneratedRelations, DerivedBindSlots,
    expression_environment(Reservations, Relations, Edges)) :-
    maplist(generated_environment_relation(CompilerFacts),
            GeneratedRelations, Relations0),
    sort(Relations0, Relations),
    findall(
        reservation(Owner, Name, target(Relation), Kind),
        ( member(call(ref(kernel(':')),
                      [ ref(Owner), const(Name), ref(Relation), const(Index) ]),
                 CompilerFacts),
          generated_callable_reservation(
              Owner, Name, Index, Relation,
              GeneratedRelations, DerivedBindSlots, Kind),
          atom(Name)
        ),
        Reservations0),
    sort(Reservations0, Reservations),
    findall(
        pending_edge(Owner, Name, Target, Index),
        ( member(call(ref(kernel(':')),
                      [ ref(Owner), const(Name), Value, const(Index) ]),
                 CompilerFacts),
          compiler_value_target(Value, Target)
        ),
        Edges0),
    sort(Edges0, Edges).

generated_callable_reservation(_, _, _, Relation,
                               GeneratedRelations, _, product) :-
    memberchk(relation(ref(Relation), _, _), GeneratedRelations),
    !.
generated_callable_reservation(Owner, Name, Index, _, _, DerivedBindSlots,
                               derived_callable) :-
    memberchk(derived_bind_slot(Owner, Name, Index), DerivedBindSlots).

generated_environment_relation(
    CompilerFacts,
    relation(ref(Relation), Arity, KeySets0),
    relation(Relation, Arity, KeySets)) :-
    generated_return_key_sets(
        CompilerFacts, Relation, Arity, KeySets0, KeySets).

generated_return_key_sets(_, _, _, KeySets, KeySets) :-
    KeySets \== [],
    !.
generated_return_key_sets(CompilerFacts, Relation, Arity, [], KeySets) :-
    findall(
        Index,
        member(call(ref(kernel(':')),
                    [ ref(Relation), const(return), _, const(Index) ]),
               CompilerFacts),
        ReturnIndices0),
    sort(ReturnIndices0, ReturnIndices),
    (   ReturnIndices = [ReturnIndex]
    ->  generated_positions_except(0, Arity, ReturnIndex, Inputs),
        KeySets = [Inputs]
    ;   KeySets = []
    ).

generated_positions_except(Index, Arity, _, []) :-
    Index >= Arity,
    !.
generated_positions_except(Index, Arity, Except, Positions) :-
    NextIndex is Index + 1,
    generated_positions_except(NextIndex, Arity, Except, Rest),
    (   Index =:= Except
    ->  Positions = Rest
    ;   Positions = [Index | Rest]
    ).

compiler_value_target(ref(Identity), target(Identity)).
compiler_value_target(const(Value), const(Value)).

%% evaluate_compiler_rounds(+Rules, +Relations, +BaseSeeds, +FrozenEdges,
%%                          +FrozenRequests, +FrozenGeneratedRelations,
%%                          +FrozenGeneratedRules, +Round,
%%                          -Closure, -GeneratedProgram, -Diagnostics) is det.
%
% One round exposes the previous round's complete edge set through the
% read-only edge_snapshot/4 input. Generated colon edges become inputs only
% after the next freeze. Every round starts again from authored seeds, frozen
% edges, and deterministic ordering rows, so negation and aggregates never
% retain stale conclusions from an earlier snapshot.
evaluate_compiler_rounds(AuthoredRules, BaseRelations, BaseSeeds, FrozenEdges,
                         FrozenRequests, FrozenGeneratedRelations,
                         FrozenGeneratedRules, Round,
                         Closure, GeneratedProgram, Diagnostics) :-
    append(BaseRelations, FrozenGeneratedRelations, Relations0),
    sort(Relations0, Relations),
    append(AuthoredRules, FrozenGeneratedRules, Rules0),
    sort(Rules0, Rules),
    check_resolved_rules(Relations, Rules, Depends, Strata,
                         ProgramDiagnostics),
    compiler_round_seeds(BaseSeeds, FrozenEdges, FrozenRequests, RoundSeeds),
    evaluate_compiler_program(ProgramDiagnostics, Rules, RoundSeeds,
                              RoundClosure0, EvaluationDiagnostics),
    strip_snapshot_rows(RoundClosure0, RoundClosure),
    continue_compiler_rounds(EvaluationDiagnostics,
                             AuthoredRules, BaseRelations, BaseSeeds,
                             FrozenEdges, FrozenRequests,
                             FrozenGeneratedRelations, FrozenGeneratedRules,
                             Depends, Strata, Round, RoundClosure,
                             Closure, GeneratedProgram, Diagnostics).

evaluate_compiler_program([], Rules, Seeds, Closure, Diagnostics) :-
    !,
    evaluate(Rules, Seeds, Closure, Diagnostics).
evaluate_compiler_program(Diagnostics, _, _, [], Diagnostics).

continue_compiler_rounds([], AuthoredRules, BaseRelations, BaseSeeds,
                         FrozenEdges, FrozenRequests,
                         FrozenGeneratedRelations, FrozenGeneratedRules,
                         Depends, Strata, Round, RoundClosure,
                         Closure, GeneratedProgram, Diagnostics) :-
    !,
    colon_rows(RoundClosure, NextEdges),
    intern_rows(RoundClosure, NextRequests),
    assemble_generated_program(RoundClosure, BaseRelations,
                               NextGeneratedRelations, NextGeneratedRules,
                               AssemblyDiagnostics),
    continue_after_assembly(
        AssemblyDiagnostics,
        AuthoredRules, BaseRelations, BaseSeeds,
        FrozenEdges, FrozenRequests,
        FrozenGeneratedRelations, FrozenGeneratedRules,
        Depends, Strata, Round, RoundClosure,
        NextEdges, NextRequests,
        NextGeneratedRelations, NextGeneratedRules,
        Closure, GeneratedProgram, Diagnostics).
continue_compiler_rounds(Diagnostics, _, _, _, _, _, _, _, _, _, _, _, _,
                         [], generated_program([], [], [], []), Diagnostics).

continue_after_assembly(
    [], AuthoredRules, BaseRelations, BaseSeeds,
    FrozenEdges, FrozenRequests,
    FrozenGeneratedRelations, FrozenGeneratedRules,
    Depends, Strata, Round, RoundClosure,
    NextEdges, NextRequests, NextGeneratedRelations, NextGeneratedRules,
    Closure, GeneratedProgram, Diagnostics) :-
    !,
    (   NextEdges == FrozenEdges,
        NextRequests == FrozenRequests,
        NextGeneratedRelations == FrozenGeneratedRelations,
        NextGeneratedRules == FrozenGeneratedRules
    ->  append(BaseRelations, NextGeneratedRelations, Relations0),
        sort(Relations0, Relations),
        derived_bind_diagnostics(AuthoredRules, RoundClosure,
                                 DerivedBindDiagnostics),
        validate_functional_rows(Relations, RoundClosure, KeyDiagnostics),
        append(DerivedBindDiagnostics, KeyDiagnostics, StableDiagnostics0),
        sort(StableDiagnostics0, StableDiagnostics),
        finish_stable_round(StableDiagnostics, RoundClosure,
                            NextGeneratedRelations, NextGeneratedRules,
                            Depends, Strata,
                            Closure, GeneratedProgram, Diagnostics)
    ;   compiler_round_limit(Limit),
        (   Round >= Limit
        ->  Closure = [],
            GeneratedProgram = generated_program([], [], [], []),
            Diagnostics = [diagnostic(
                               compile, none,
                               compiler_round_limit_exhausted(Limit))]
        ;   NextRound is Round + 1,
            evaluate_compiler_rounds(
                AuthoredRules, BaseRelations, BaseSeeds,
                NextEdges, NextRequests,
                NextGeneratedRelations, NextGeneratedRules, NextRound,
                Closure, GeneratedProgram, Diagnostics)
        )
    ).
continue_after_assembly(
    Diagnostics, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _,
    [], generated_program([], [], [], []), Diagnostics).

finish_stable_round([], RoundClosure,
                    GeneratedRelations, GeneratedRules, Depends, Strata,
                    Closure,
                    generated_program(GeneratedRelations, GeneratedRules,
                                      Depends, Strata), []) :-
    !,
    strip_intern_rows(RoundClosure, Closure).
finish_stable_round(Diagnostics, _, _, _, _, _, [],
                    generated_program([], [], [], []), Diagnostics).

compiler_round_limit(16).

derived_bind_diagnostics(Rules, Rows, Diagnostics) :-
    findall(
        diagnostic(compile, NodeId,
                   missing_derived_bind(Owner, Name, Index)),
        ( member(
              rule(call(ref(kernel(':')),
                        [ ref(Owner), const(Name),
                          var(derived_bind(NodeId)), const(Index)
                        ]),
                   _),
              Rules),
          \+ memberchk(
                 call(ref(kernel(':')),
                      [ref(Owner), const(Name), _, const(Index)]),
                 Rows)
        ),
        BindDiagnostics),
    findall(
        diagnostic(compile, NodeId,
                   missing_derived_edge_label(Owner, Index)),
        ( member(
              rule(call(ref(kernel(':')),
                        [ ref(Owner), var(derived_label(NodeId)), _,
                          const(Index)
                        ]),
                   _),
              Rules),
          \+ memberchk(
                 call(ref(kernel(':')),
                      [ref(Owner), _, _, const(Index)]),
                 Rows)
        ),
        LabelDiagnostics),
    append(BindDiagnostics, LabelDiagnostics, Diagnostics0),
    sort(Diagnostics0, Diagnostics).

compiler_round_seeds(BaseSeeds, FrozenEdges, FrozenRequests, Seeds) :-
    maplist(snapshot_edge, FrozenEdges, SnapshotRows),
    maplist(snapshot_intern, FrozenRequests, RequestSnapshotRows),
    frozen_predecessor_rows(FrozenEdges, PredecessorRows),
    append([ BaseSeeds,
             SnapshotRows,
             RequestSnapshotRows,
             PredecessorRows
           ], Seeds0),
    sort(Seeds0, Seeds).

snapshot_edge(call(ref(kernel(':')), Arguments),
              call(ref(kernel(edge_snapshot)), Arguments)).

snapshot_intern(call(ref(kernel(intern)), Arguments),
                call(ref(kernel(intern_snapshot)), Arguments)).

frozen_predecessor_rows(FrozenEdges, Rows) :-
    findall(call(ref(kernel(predecessor)),
                 [Owner, const(EarlierIndex), const(LaterIndex)]),
            ( member(call(ref(kernel(':')),
                          [Owner, _, _, const(LaterIndex)]),
                     FrozenEdges),
              LaterIndex > 0,
              EarlierIndex is LaterIndex - 1
            ),
            Rows0),
    sort(Rows0, Rows).

strip_snapshot_rows(Rows0, Rows) :-
    exclude(snapshot_row, Rows0, Rows).

snapshot_row(call(ref(kernel(edge_snapshot)), _)).
snapshot_row(call(ref(kernel(intern_snapshot)), _)).

intern_rows(Rows, Requests) :-
    include(intern_row, Rows, Requests0),
    sort(Requests0, Requests).

strip_intern_rows(Rows0, Rows) :-
    exclude(intern_row, Rows0, Rows).

intern_row(call(ref(kernel(intern)), _)).

colon_rows(Rows, Edges) :-
    include(colon_row, Rows, Edges0),
    sort(Edges0, Edges).

colon_row(call(ref(kernel(':')), _)).

graph_seeds(root_graph(Nodes, Edges), Seeds) :-
    maplist(node_seed, Nodes, NodeSeeds),
    maplist(edge_seed, Edges, EdgeSeeds),
    append(NodeSeeds, EdgeSeeds, Seeds).

node_seed(node(Identity),
          call(ref(kernel(node)), [ref(Identity)])).
node_seed(module(Identity),
          call(ref(kernel(module)), [ref(Identity)])).
node_seed(product(Identity),
          call(ref(kernel(product)), [ref(Identity)])).
node_seed(sum(Identity),
          call(ref(kernel(sum)), [ref(Identity)])).

edge_seed(':'(Owner, Name, Target, Index),
          call(ref(kernel(':')),
               [ref(Owner), const(Name), Target, const(Index)])).

type_graph_facts(CompilerFacts, TypeGraphFacts) :-
    findall(Row,
            ( member(Call, CompilerFacts),
              type_graph_fact(Call, Row)
            ),
            Rows),
    sort(Rows, TypeGraphFacts).

type_graph_fact(call(ref(kernel(node)), [ref(Identity)]), node(Identity)).
type_graph_fact(call(ref(kernel(module)), [ref(Identity)]), module(Identity)).
type_graph_fact(call(ref(kernel(product)), [ref(Identity)]), product(Identity)).
type_graph_fact(call(ref(kernel(sum)), [ref(Identity)]), sum(Identity)).
type_graph_fact(
    call(ref(kernel(':')),
         [ref(Owner), Label, Target, const(Index)]),
    ':'(Owner, SemanticLabel, Target, Index)) :-
    semantic_label(Label, SemanticLabel).

semantic_label(const(Value), Value).
semantic_label(ref(Identity), Identity).
