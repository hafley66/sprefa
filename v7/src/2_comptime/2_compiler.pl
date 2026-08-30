:- module(dl7_compiler,
          [ compile_dl7/4,
            compile_unit/3
          ]).

:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('../0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../1_libtime/0_evaluator',
              [ evaluate/4,
                validate_functional_rows/3
              ]).
:- use_module('0_lowerer', [lower_datalog/4]).
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
    once(type_prelude_path(PreludePath)),
    read_file_to_string(PreludePath, PreludeText, [encoding(utf8)]),
    read_file_to_string(ProgramPath, ProgramText, [encoding(utf8)]),
    format(string(Text), "~s~n~s", [PreludeText, ProgramText]),
    Origin = combined(PreludePath, ProgramPath),
    dl7_text_unit(Origin, Origin, Text, Unit, ReaderDiagnostics),
    compile_after_read(ReaderDiagnostics, Unit, Compiled, Diagnostics),
    compiled_outputs(Compiled, CompilerRows, RuntimeProgram).

type_prelude_path(Path) :-
    once(source_file(dl7_compiler:compile_dl7(_, _, _, _), SourcePath)),
    file_directory_name(SourcePath, ComptimeDirectory),
    directory_file_path(ComptimeDirectory, '../../prelude/0_types.dl7',
                        RelativePath),
    absolute_file_name(RelativePath, Path,
                       [access(read), file_errors(error)]).

compile_after_read([], Unit, Compiled, Diagnostics) :-
    !,
    compile_unit(Unit, Compiled, Diagnostics).
compile_after_read(Diagnostics, _, [], Diagnostics).

compiled_outputs(compiled_unit(_, RuntimeProgram, CompilerRows),
                 CompilerRows, RuntimeProgram).
compiled_outputs([], [], []).

%% compile_unit(+Unit, -Compiled, -Diagnostics) is det.
%
% The type graph and authored ground facts seed the shared evaluator. The
% complete checked program survives as runtime input while compiler closure is
% retained as immutable artifact data.
compile_unit(Unit, Compiled, Diagnostics) :-
    lower_datalog(Unit, Basement, Origins, LowerDiagnostics),
    compile_after_lower(LowerDiagnostics, Basement, Origins,
                        Compiled, Diagnostics).

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
        Diagnostics0),
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
         [ref(Owner), const(Name), Target, const(Index)]),
    ':'(Owner, Name, Target, Index)).
