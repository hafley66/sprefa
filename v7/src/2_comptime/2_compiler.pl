:- module(dl7_compiler,
          [ compile_dl7/4,
            compile_dl7_project/5,
            compile_dl7_project_rows/6,
            compile_unit/3,
            compile_unit_with_macros/4,
            compile_units/3,
            type_prelude_paths/1
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('../0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../0_reader/1a_syntax_grapher', [reify_syntax/4]).
:- use_module('../0_reader/1b_syntax_materializer', [materialize_syntax/4]).
:- use_module('../0_reader/4_module_loader', [load_dl7_project/4]).
:- use_module('../1_libtime/0_evaluator',
              [ evaluate/4,
                validate_functional_rows/3
              ]).
:- use_module('../1_libtime/1_syntax_expander', [expand_syntax/5]).
:- use_module('0a_module_lowerer',
              [ lower_units_deferred/4,
                lower_units_with_environment/5,
                lower_units_with_environment_deferred/5,
                lower_units_with_exporter_deferred/5,
                lower_units_with_exporter_and_environment/6,
                lower_units_with_exporter_and_environment_deferred/6,
                merge_module_basements/4,
                install_module_aliases/6
              ]).
:- use_module('0b_filesystem_grapher', [install_project_graph/6]).
:- use_module('0c_extract_loader',
              [ load_tsi_stream/3,
                install_tsi_graph/6,
                tsi_expression_environment/3
              ]).
:- use_module('1_checker',
              [ check_datalog/4,
                check_resolved_rules/5
              ]).
:- use_module('1a_generated_program_assembler',
              [assemble_generated_program/5]).
:- use_module('1b_compiler_tracer',
              [ with_compile_trace/2,
                run_compile_phase/3,
                run_compile_step/4
              ]).
:- use_module('1c_compiler_cacher',
              [ with_prelude_cache/5,
                with_compilation_cache/5
              ]).
:- use_module('1d_host_planner',
              [ validate_hosted_relations/4,
                erase_host_planning_rows/7
              ]).

%% compile_dl7(+Path, -CompilerRows, -RuntimeProgram, -Diagnostics) is det.
%
% Load the userland type prelude and one source file through the same reader,
% then run every compile-known positive rule over the initial type graph.
compile_dl7(Path, CompilerRows, RuntimeProgram, Diagnostics) :-
    compile_trace_program_name(Path, ProgramName),
    with_compile_trace(
        ProgramName,
        compile_dl7_traced(
            Path, CompilerRows, RuntimeProgram, Diagnostics)).

compile_dl7_traced(Path, CompilerRows, RuntimeProgram, Diagnostics) :-
    once(absolute_file_name(Path, ProgramPath,
                            [access(read), file_errors(error)])),
    run_compile_phase(
        read,
        read_program_texts(ProgramPath, PreludeText, ProgramText),
        _),
    CompileKey = compile(ProgramPath, PreludeText, ProgramText),
    run_compile_step(
        driver, compilation_cache,
        with_compilation_cache(
            CompileKey,
            compile_program_texts(ProgramPath, PreludeText, ProgramText),
            Compiled, Diagnostics, CacheHit),
        cache_compile_metrics(CacheHit)),
    compiled_outputs(Compiled, CompilerRows, RuntimeProgram),
    !.

read_program_texts(ProgramPath, PreludeText, ProgramText) :-
    once(type_prelude_paths(PreludePaths)),
    read_prelude_texts(PreludePaths, PreludeTexts),
    join_prelude_texts(PreludeTexts, PreludeText),
    read_file_to_string(ProgramPath, ProgramText, [encoding(utf8)]),
    !.

compile_program_texts(
    ProgramPath, PreludeText, ProgramText, Compiled, Diagnostics) :-
    run_compile_phase(
        expand,
        parse_program_texts(
            ProgramPath, PreludeText, ProgramText,
            PreludeUnit, PreludeDiagnostics,
            ProgramUnit, ProgramDiagnostics),
        _),
    append(PreludeDiagnostics, ProgramDiagnostics, ReaderDiagnostics),
    compile_after_reads(ReaderDiagnostics, [PreludeUnit, ProgramUnit],
                        Compiled, Diagnostics).

parse_program_texts(
    ProgramPath, PreludeText, ProgramText,
    PreludeUnit, PreludeDiagnostics,
    ProgramUnit, ProgramDiagnostics) :-
    cached_prelude_text_unit(
        PreludeText, PreludeUnit, PreludeDiagnostics, _),
    dl7_text_unit(file(ProgramPath), ProgramPath, ProgramText,
                  ProgramUnit, ProgramDiagnostics).

%% compile_dl7_project(+Root, +Paths,
%%                     -CompilerRows, -RuntimeProgram, -Diagnostics) is det.
%
% Load several filesystem-owned source units, add their directory and module
% products to the initial graph, then run the same checker and fixpoint used
% by the single-file compiler.
compile_dl7_project(Root, Paths,
                    CompilerRows, RuntimeProgram, Diagnostics) :-
    compile_trace_program_name(Root, ProgramName),
    with_compile_trace(
        ProgramName,
        compile_dl7_project_traced(
            Root, Paths, CompilerRows, RuntimeProgram, Diagnostics)).

compile_dl7_project_traced(Root, Paths,
                           CompilerRows, RuntimeProgram, Diagnostics) :-
    project_stream_paths(Paths, SourcePaths, StreamPaths),
    run_compile_phase(
        read,
        read_project_units(
            Root, SourcePaths, StreamPaths, PreludeUnit, PreludeDiagnostics,
            Project, ProjectDiagnostics, TsiRows, TsiDiagnostics),
        _),
    append([PreludeDiagnostics, ProjectDiagnostics, TsiDiagnostics],
           ReaderDiagnostics),
    compile_after_project_reads(ReaderDiagnostics, PreludeUnit, Project,
                                TsiRows, Compiled, Diagnostics),
    compiled_outputs(Compiled, CompilerRows, RuntimeProgram),
    !.

%% compile_dl7_project_rows(+Root, +Paths, +TsiRows,
%%                          -CompilerRows, -RuntimeProgram,
%%                          -Diagnostics) is det.
%
% Compile already-decoded TSI observations without materializing a temporary
% JSONL stream. Host tools use this after reading an extractor process pipe.
compile_dl7_project_rows(Root, Paths, TsiRows,
                         CompilerRows, RuntimeProgram, Diagnostics) :-
    compile_trace_program_name(Root, ProgramName),
    with_compile_trace(
        ProgramName,
        compile_dl7_project_rows_traced(
            Root, Paths, TsiRows,
            CompilerRows, RuntimeProgram, Diagnostics)).

compile_dl7_project_rows_traced(
    Root, Paths, TsiRows, CompilerRows, RuntimeProgram, Diagnostics) :-
    run_compile_phase(
        read,
        read_project_units(
            Root, Paths, [], PreludeUnit, PreludeDiagnostics,
            Project, ProjectDiagnostics, _, StreamDiagnostics),
        _),
    append([PreludeDiagnostics, ProjectDiagnostics, StreamDiagnostics],
           ReaderDiagnostics),
    compile_after_project_reads(ReaderDiagnostics, PreludeUnit, Project,
                                TsiRows, Compiled, Diagnostics),
    compiled_outputs(Compiled, CompilerRows, RuntimeProgram),
    !.

% A `tsi_streams(Paths)` term among the project paths names foreign type
% streams rather than source units.
project_stream_paths([], [], []).
project_stream_paths([tsi_streams(Streams) | Paths],
                     SourcePaths, StreamPaths) :-
    !,
    project_stream_paths(Paths, SourcePaths, RestStreamPaths),
    append(Streams, RestStreamPaths, StreamPaths).
project_stream_paths([Path | Paths], [Path | SourcePaths], StreamPaths) :-
    project_stream_paths(Paths, SourcePaths, StreamPaths).

read_project_units(
    Root, SourcePaths, StreamPaths, PreludeUnit, PreludeDiagnostics,
    Project, ProjectDiagnostics, TsiRows, TsiDiagnostics) :-
    load_type_prelude(PreludeUnit, PreludeDiagnostics),
    load_dl7_project(Root, SourcePaths, Project, ProjectDiagnostics),
    load_tsi_streams(StreamPaths, TsiRows, TsiDiagnostics).

load_tsi_streams([], [], []).
load_tsi_streams([Path | Paths], Rows, Diagnostics) :-
    load_tsi_stream(Path, StreamRows, StreamDiagnostics),
    load_tsi_streams(Paths, RestRows, RestDiagnostics),
    append(StreamRows, RestRows, Rows),
    append(StreamDiagnostics, RestDiagnostics, Diagnostics).

compile_trace_program_name(Path, ProgramName) :-
    file_base_name(Path, BaseName),
    (   file_name_extension(Stem, _, BaseName)
    ->  ProgramName = Stem
    ;   ProgramName = BaseName
    ).

load_type_prelude(PreludeUnit, Diagnostics) :-
    once(type_prelude_paths(PreludePaths)),
    read_prelude_texts(PreludePaths, PreludeTexts),
    join_prelude_texts(PreludeTexts, PreludeText),
    cached_prelude_text_unit(
        PreludeText, PreludeUnit, Diagnostics, _).

cached_prelude_text_unit(Text, Unit, Diagnostics, Hit) :-
    run_compile_step(
        expand, prelude_cache,
        with_prelude_cache(
            Text, parse_prelude_text(Text), Unit, Diagnostics, Hit),
        cache_compile_metrics(Hit)).

parse_prelude_text(Text, Unit, Diagnostics) :-
    dl7_text_unit(prelude, prelude, Text, Unit, Diagnostics).

cache_compile_metrics(Hit, [metric(cache_hit, HitValue)]) :-
    (   Hit == hit
    ->  HitValue = 1
    ;   HitValue = 0
    ).

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
                            TsiRows, Compiled, Diagnostics) :-
    !,
    compile_project_units(
        dl7_project(CanonicalRoot, Units), TsiRows,
        [PreludeUnit | Units], Compiled, Diagnostics).
compile_after_project_reads(Diagnostics, _, _, _, [], Diagnostics).

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

%% compile_unit_with_macros(+Unit, +MacroProgram,
%%                          -Compiled, -Diagnostics) is det.
%
% Transitional graph-first entry point. Reify the unit's current reader tree,
% run a checked DL7 macro program to closure, materialize the active graph for
% the existing lowerer, then use the ordinary compiler path.
compile_unit_with_macros(Unit, MacroProgram, Compiled, Diagnostics) :-
    must_be(ground, Unit),
    must_be(ground, MacroProgram),
    (   Unit = dl7_unit(Origin, Digest, Forms, SourceRows, ExpansionRows)
    ->  reify_syntax(Forms, SourceRows, SyntaxRows, ReifyDiagnostics),
        expand_unit_after_reify(
            ReifyDiagnostics, SyntaxRows, MacroProgram,
            Origin, Digest, ExpansionRows,
            ExpandedUnit, ExpansionDiagnostics),
        compile_expanded_unit(
            ExpansionDiagnostics, ExpandedUnit, Compiled, Diagnostics)
    ;   Compiled = [],
        Diagnostics = [diagnostic(
                           macrotime, none, invalid_dl7_unit(Unit))]
    ).

expand_unit_after_reify(
    [], SyntaxRows, MacroProgram, Origin, Digest, ExpansionRows,
    ExpandedUnit, Diagnostics) :-
    !,
    expand_syntax(SyntaxRows, MacroProgram,
                  ExpandedRows, MacroProvenance, MacroDiagnostics),
    materialize_unit_after_expansion(
        MacroDiagnostics, ExpandedRows, MacroProvenance,
        Origin, Digest, ExpansionRows, ExpandedUnit, Diagnostics).
expand_unit_after_reify(Diagnostics, _, _, _, _, _, [], Diagnostics).

materialize_unit_after_expansion(
    [], ExpandedRows, MacroProvenance, Origin, Digest, ExpansionRows,
    ExpandedUnit, Diagnostics) :-
    !,
    materialize_syntax(
        ExpandedRows, Forms, SourceRows, MaterializeDiagnostics),
    append(ExpansionRows, MacroProvenance, AllExpansionRows),
    ExpandedUnit = dl7_unit(
                       Origin, Digest, Forms, SourceRows, AllExpansionRows),
    Diagnostics = MaterializeDiagnostics.
materialize_unit_after_expansion(
    Diagnostics, _, _, _, _, _, [], Diagnostics).

compile_expanded_unit([], Unit, Compiled, Diagnostics) :-
    !,
    compile_unit(Unit, Compiled, Diagnostics).
compile_expanded_unit(Diagnostics, _, [], Diagnostics).

%% compile_units(+Units, -Compiled, -Diagnostics) is det.
%
% Lower source modules independently, expose the synthetic prelude through
% ordinary alias edges, merge the owned rows, then enter the existing checker
% and comptime fixpoint.
compile_units(Units, Compiled, Diagnostics) :-
    with_compile_trace(
        units,
        compile_units_traced(Units, Compiled, Diagnostics)).

compile_units_traced(Units, Compiled, Diagnostics) :-
    run_compile_phase(
        lower,
        lower_compiler_units(Units, ModuleBasements, ModuleOrigins,
                             LowerDiagnostics),
        _),
    Context = compile_context(Units, none),
    compile_after_unit_lower(LowerDiagnostics, Context,
                             ModuleBasements, ModuleOrigins,
                             Compiled, Diagnostics),
    !.

compile_project_units(Project, TsiRows, Units, Compiled, Diagnostics) :-
    source_unit_module_owners(Units, SourceOwners),
    tsi_expression_environment(TsiRows, SourceOwners, TsiEnvironment),
    run_compile_phase(
        lower,
        lower_compiler_units(Units, TsiEnvironment,
                             ModuleBasements0, ModuleOrigins0,
                             LowerDiagnostics),
        _),
    install_project_after_lower(
        LowerDiagnostics, Project, TsiRows, ModuleBasements0, ModuleOrigins0,
        ModuleBasements, ModuleOrigins, ProjectDiagnostics),
    Context = compile_context(Units, project(Project, TsiRows)),
    compile_after_unit_lower(ProjectDiagnostics, Context,
                             ModuleBasements, ModuleOrigins,
                             Compiled, Diagnostics),
    !.

install_project_after_lower(
    [], Project, TsiRows, Basements0, Origins0,
    Basements, Origins, Diagnostics) :-
    !,
    install_graphs(Project, TsiRows, Basements0, Origins0,
                   Basements, Origins, Diagnostics).
install_project_after_lower(
    Diagnostics, _, _, Basements, Origins,
    Basements, Origins, Diagnostics).

% Foreign rows enter after the filesystem products so the loader reads the
% prelude's primitive classes out of the basements already installed.
install_graphs(Project, TsiRows, Basements0, Origins0,
               Basements, Origins, Diagnostics) :-
    install_project_graph(Project, Basements0, Origins0,
                          Basements1, Origins1, ProjectDiagnostics),
    (   ProjectDiagnostics == []
    ->  install_tsi_graph(TsiRows, Basements1, Origins1,
                          Basements2, Origins2, TsiDiagnostics),
        expose_tsi_relations(
            TsiDiagnostics, Project, Basements1, Basements2, Origins2,
            Basements, Origins, Diagnostics)
    ;   Basements = Basements1,
        Origins = Origins1,
        Diagnostics = ProjectDiagnostics
    ).

expose_tsi_relations([], dl7_project(_, Units), BasementsBefore,
                     Basements0, Origins0,
                     Basements, Origins, []) :-
    !,
    unit_module_owners(Units, Importers),
    added_module_owners(BasementsBefore, Basements0, Exporters),
    install_exporter_aliases(Exporters, Importers,
                             Basements0, Origins0, Basements, Origins).
expose_tsi_relations(Diagnostics, _, _, Basements, Origins,
                     Basements, Origins, Diagnostics).

unit_module_owners([], []).
unit_module_owners([dl7_unit(Origin, _, _, _, _) | Units],
                   [module(Origin) | Owners]) :-
    unit_module_owners(Units, Owners).

added_module_owners(BasementsBefore, BasementsAfter, Owners) :-
    findall(Owner,
            ( member(module_basement(Owner, _), BasementsAfter),
              \+ memberchk(module_basement(Owner, _), BasementsBefore)
            ),
            Owners).

install_exporter_aliases([], _, Basements, Origins, Basements, Origins).
install_exporter_aliases([Exporter | Exporters], Importers,
                         Basements0, Origins0, Basements, Origins) :-
    install_module_aliases(Exporter, Importers,
                           Basements0, Origins0, Basements1, Origins1),
    install_exporter_aliases(Exporters, Importers,
                             Basements1, Origins1, Basements, Origins).

lower_compiler_units(Units, ModuleBasements, ModuleOrigins, Diagnostics) :-
    (   select(PreludeUnit, Units, ImporterUnits),
        unit_has_origin(PreludeUnit, prelude)
    ->  lower_units_with_exporter_deferred(
            PreludeUnit, ImporterUnits,
            ModuleBasements, ModuleOrigins, Diagnostics)
    ;   lower_units_deferred(Units, ModuleBasements, ModuleOrigins,
                             Diagnostics)
    ).

lower_compiler_units(Units, Environment,
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

source_unit_module_owners([], []).
source_unit_module_owners([dl7_unit(prelude, _, _, _, _) | Units], Owners) :-
    !,
    source_unit_module_owners(Units, Owners).
source_unit_module_owners([dl7_unit(Origin, _, _, _, _) | Units],
                          [module(Origin) | Owners]) :-
    source_unit_module_owners(Units, Owners).

unit_has_origin(dl7_unit(Origin, _, _, _, _), Origin).

compile_after_unit_lower([], Context, ModuleBasements, ModuleOrigins,
                         Compiled, Diagnostics) :-
    !,
    run_compile_step(
        lower, merge_modules,
        merge_module_basements(ModuleBasements, ModuleOrigins,
                               Basement, Origins),
        basement_compile_metrics(Basement)),
    compile_after_lower([], Context, Basement, Origins,
                        Compiled, Diagnostics).
compile_after_unit_lower(Diagnostics, _, _, _, [], Diagnostics).

compile_after_lower([], Context, Basement, Origins, Compiled, Diagnostics) :-
    !,
    run_compile_phase(
        check,
        check_datalog(Basement, Origins, Checked, CheckDiagnostics),
        _),
    compile_after_check(CheckDiagnostics, Context, Checked,
                        Compiled, Diagnostics).
compile_after_lower(Diagnostics, _, _, _, [], Diagnostics).

compile_after_check([], Context, Checked, Compiled, Diagnostics) :-
    !,
    run_compile_phase(
        comptime,
        evaluate_checked(Context, Checked, Compiled, Diagnostics),
        _).
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
    colon_rows(BaseSeeds, BaseEdges),
    source_application_edges(Rules, SourceApplicationEdges),
    append(BaseEdges, SourceApplicationEdges, InitialEdges0),
    sort(InitialEdges0, InitialEdges),
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
        colon_rows(CompilerFacts, CompilerEdges),
        source_application_edges(Rules, SourceApplicationEdges),
        append(CompilerEdges, SourceApplicationEdges, FrozenEdges0),
        sort(FrozenEdges0, FrozenEdges),
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
                       Graph, datalog_program(Relations, _, _), _, _),
    validate_functional_rows(Relations, CompilerFacts, KeyDiagnostics),
    validate_hosted_relations(
        Graph, Relations, CompilerFacts, HostDiagnostics),
    append(KeyDiagnostics, HostDiagnostics, ValidationDiagnostics0),
    sort(ValidationDiagnostics0, ValidationDiagnostics),
    finish_key_validation(ValidationDiagnostics, CompilerFacts, FinalChecked,
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
    sort(RuntimeRelations0, RuntimeRelations1),
    append(Rules, GeneratedRules, RuntimeRules0),
    sort(RuntimeRules0, RuntimeRules1),
    erase_host_planning_rows(
        Graph, RuntimeRelations1, AuthoredSeeds, RuntimeRules1,
        RuntimeRelations, RuntimeSeeds, RuntimeRules),
    check_resolved_rules(RuntimeRelations, RuntimeRules,
                         RuntimeDepends, RuntimeStrata,
                         RuntimeDiagnostics),
    finish_runtime_program(
        RuntimeDiagnostics, Graph, RuntimeRelations, RuntimeSeeds,
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
    Context = compile_context(Units, ProjectContext, DerivedBindSlots),
    final_expression_environment(
        CompilerFacts, GeneratedRelations, DerivedBindSlots,
        Units, ProjectContext, Environment),
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
    Context = compile_context(Units, ProjectContext, DerivedBindSlots),
    final_expression_environment(
        CompilerFacts, GeneratedRelations, DerivedBindSlots,
        Units, ProjectContext, Environment),
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
    [], project(Project, TsiRows), Basements0, Origins0,
    Basements, Origins, Diagnostics) :-
    !,
    install_graphs(Project, TsiRows, Basements0, Origins0,
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

final_expression_environment(
    CompilerFacts, GeneratedRelations, DerivedBindSlots,
    Units, ProjectContext, Environment) :-
    generated_expression_environment(
        CompilerFacts, GeneratedRelations, DerivedBindSlots,
        GeneratedEnvironment),
    source_unit_module_owners(Units, SourceOwners),
    project_tsi_environment(ProjectContext, SourceOwners, TsiEnvironment),
    merge_expression_environments(
        GeneratedEnvironment, TsiEnvironment, Environment).

project_tsi_environment(project(_, TsiRows), SourceOwners, Environment) :-
    !,
    tsi_expression_environment(TsiRows, SourceOwners, Environment).
project_tsi_environment(_, _, expression_environment([], [], [])).

merge_expression_environments(
    expression_environment(Reservations0, Relations0, Edges0),
    expression_environment(Reservations1, Relations1, Edges1),
    expression_environment(Reservations, Relations, Edges)) :-
    append(Reservations0, Reservations1, Reservations2),
    append(Relations0, Relations1, Relations2),
    append(Edges0, Edges1, Edges2),
    sort(Reservations2, Reservations),
    sort(Relations2, Relations),
    sort(Edges2, Edges).

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
    run_compile_step(
        comptime, evaluate_round(Round),
        evaluate_compiler_program(ProgramDiagnostics, Rules, RoundSeeds,
                                  RoundClosure0, EvaluationDiagnostics),
        compiler_round_metrics(
            Rules, RoundSeeds, FrozenEdges, FrozenRequests,
            FrozenGeneratedRelations, FrozenGeneratedRules,
            RoundClosure0)),
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

basement_compile_metrics(
    basement_program(root_graph(Nodes, Edges),
                     datalog_program(Relations, Seeds, Rules)),
    [ metric(nodes, NodeCount),
      metric(edges, EdgeCount),
      metric(relations, RelationCount),
      metric(seeds, SeedCount),
      metric(rules, RuleCount)
    ]) :-
    length(Nodes, NodeCount),
    length(Edges, EdgeCount),
    length(Relations, RelationCount),
    length(Seeds, SeedCount),
    length(Rules, RuleCount).

compiler_round_metrics(
    Rules, Seeds, FrozenEdges, FrozenRequests,
    FrozenGeneratedRelations, FrozenGeneratedRules, Closure,
    [ metric(rules, RuleCount),
      metric(seed_rows, SeedCount),
      metric(closure_rows, ClosureCount),
      metric(derived_rows, DerivedCount),
      metric(frozen_edges, FrozenEdgeCount),
      metric(frozen_interns, FrozenRequestCount),
      metric(generated_relations, GeneratedRelationCount),
      metric(generated_rules, GeneratedRuleCount)
    ]) :-
    length(Rules, RuleCount),
    length(Seeds, SeedCount),
    length(Closure, ClosureCount),
    DerivedCount is max(0, ClosureCount - SeedCount),
    length(FrozenEdges, FrozenEdgeCount),
    length(FrozenRequests, FrozenRequestCount),
    length(FrozenGeneratedRelations, GeneratedRelationCount),
    length(FrozenGeneratedRules, GeneratedRuleCount).

assembly_compile_metrics(
    Closure, GeneratedRelations, GeneratedRules,
    [ metric(closure_rows, ClosureCount),
      metric(generated_relations, GeneratedRelationCount),
      metric(generated_rules, GeneratedRuleCount)
    ]) :-
    length(Closure, ClosureCount),
    length(GeneratedRelations, GeneratedRelationCount),
    length(GeneratedRules, GeneratedRuleCount).

continue_compiler_rounds([], AuthoredRules, BaseRelations, BaseSeeds,
                         FrozenEdges, FrozenRequests,
                         FrozenGeneratedRelations, FrozenGeneratedRules,
                         Depends, Strata, Round, RoundClosure,
                         Closure, GeneratedProgram, Diagnostics) :-
    !,
    colon_rows(RoundClosure, NextEdges),
    intern_rows(RoundClosure, NextRequests),
    run_compile_step(
        comptime, assemble_round(Round),
        assemble_generated_program(
            RoundClosure, BaseRelations,
            NextGeneratedRelations, NextGeneratedRules,
            AssemblyDiagnostics),
        assembly_compile_metrics(
            RoundClosure, NextGeneratedRelations, NextGeneratedRules)),
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

% Escaping partials lower their call-owned edges as ground fact rules. Expose
% those edges to edge_snapshot/4 at the start of the same compiler round.
% Generated type edges still cross the ordinary end-of-round freeze.
source_application_edges(Rules, Edges) :-
    findall(
        Edge,
        ( member(rule(Edge, []), Rules),
          Edge = call(ref(kernel(':')),
                      [ref(call(_, _)), _, _, _])
        ),
        Edges0),
    sort(Edges0, Edges).

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
