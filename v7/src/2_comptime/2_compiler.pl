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
              [ lower_units/4,
                lower_units_with_exporter/5,
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
    compile_after_unit_lower(LowerDiagnostics,
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
    compile_after_unit_lower(ProjectDiagnostics,
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
    ->  lower_units_with_exporter(PreludeUnit, ImporterUnits,
                                  ModuleBasements, ModuleOrigins,
                                  Diagnostics)
    ;   lower_units(Units, ModuleBasements, ModuleOrigins, Diagnostics)
    ).

unit_has_origin(dl7_unit(Origin, _, _, _, _), Origin).

compile_after_unit_lower([], ModuleBasements, ModuleOrigins,
                         Compiled, Diagnostics) :-
    !,
    merge_module_basements(ModuleBasements, ModuleOrigins,
                           Basement, Origins),
    compile_after_lower([], Basement, Origins, Compiled, Diagnostics).
compile_after_unit_lower(Diagnostics, _, _, [], Diagnostics).

compile_after_lower([], Basement, Origins, Compiled, Diagnostics) :-
    !,
    check_datalog(Basement, Origins, Checked, CheckDiagnostics),
    compile_after_check(CheckDiagnostics, Checked, Compiled, Diagnostics).
compile_after_lower(Diagnostics, _, _, [], Diagnostics).

compile_after_check([], Checked, Compiled, Diagnostics) :-
    !,
    evaluate_checked(Checked, Compiled, Diagnostics).
compile_after_check(Diagnostics, _, [], Diagnostics).

evaluate_checked(
    checked_datalog(Graph,
                    datalog_program(Relations, AuthoredSeeds, Rules),
                    Depends, Strata),
    Compiled,
    Diagnostics) :-
    graph_seeds(Graph, GraphSeeds),
    append(GraphSeeds, AuthoredSeeds, BaseSeeds0),
    sort(BaseSeeds0, BaseSeeds),
    colon_rows(BaseSeeds, InitialEdges),
    intern_rows(BaseSeeds, InitialRequests),
    evaluate_compiler_rounds(Rules, Relations, BaseSeeds, InitialEdges,
                             InitialRequests, [], [], 1,
                             CompilerFacts, GeneratedProgram,
                             EvaluationDiagnostics),
    finish_evaluation(EvaluationDiagnostics, Relations, CompilerFacts,
                      GeneratedProgram,
                      Graph, AuthoredSeeds, Rules, Depends, Strata,
                      Compiled, Diagnostics).

finish_evaluation([], Relations, CompilerFacts, GeneratedProgram,
                  Graph, AuthoredSeeds, Rules,
                  Depends, Strata, Compiled, Diagnostics) :-
    !,
    GeneratedProgram = generated_program(GeneratedRelations, _, _, _),
    append(Relations, GeneratedRelations, AllRelations0),
    sort(AllRelations0, AllRelations),
    validate_functional_rows(AllRelations, CompilerFacts, KeyDiagnostics),
    finish_key_validation(KeyDiagnostics, CompilerFacts, Graph, Relations,
                          AuthoredSeeds, Rules, Depends, Strata,
                          GeneratedProgram,
                          Compiled, Diagnostics).
finish_evaluation(Diagnostics, _, _, _, _, _, _, _, _, [], Diagnostics).

finish_key_validation([], CompilerFacts, Graph, Relations, AuthoredSeeds,
                      Rules, Depends, Strata, GeneratedProgram,
                      compiled_unit(TypeGraphFacts, RuntimeProgram,
                                    CompilerFacts), []) :-
    !,
    type_graph_facts(CompilerFacts, TypeGraphFacts),
    GeneratedProgram = generated_program(
                           GeneratedRelations, GeneratedRules,
                           GeneratedDepends, GeneratedStrata),
    append(Relations, GeneratedRelations, RuntimeRelations0),
    sort(RuntimeRelations0, RuntimeRelations),
    append(Rules, GeneratedRules, RuntimeRules0),
    sort(RuntimeRules0, RuntimeRules),
    (   GeneratedRelations == [],
        GeneratedRules == []
    ->  RuntimeDepends = Depends,
        RuntimeStrata = Strata
    ;   RuntimeDepends = GeneratedDepends,
        RuntimeStrata = GeneratedStrata
    ),
    RuntimeProgram = checked_datalog(
                         Graph,
                         datalog_program(RuntimeRelations, AuthoredSeeds,
                                         RuntimeRules),
                         RuntimeDepends, RuntimeStrata).
finish_key_validation(Diagnostics, _, _, _, _, _, _, _, _, [], Diagnostics).

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
