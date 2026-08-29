:- module(dl7_type_compiler,
          [ compile_dl7/4,
            compile_unit/3
          ]).

:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('../0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../1_libtime/0_evaluator', [evaluate/4]).
:- use_module('0_compiler', [lower_datalog/4, check_datalog/4]).

%% compile_dl7(+Path, -CompilerRows, -RuntimeProgram, -Diagnostics) is det.
%
% Load the userland type prelude and one source file through the same reader,
% then run every compile-known positive rule over the initial type graph.
compile_dl7(Path, CompilerRows, RuntimeProgram, Diagnostics) :-
    absolute_file_name(Path, ProgramPath,
                       [access(read), file_errors(error)]),
    type_prelude_path(PreludePath),
    read_file_to_string(PreludePath, PreludeText, [encoding(utf8)]),
    read_file_to_string(ProgramPath, ProgramText, [encoding(utf8)]),
    format(string(Text), "~s~n~s", [PreludeText, ProgramText]),
    Origin = combined(PreludePath, ProgramPath),
    dl7_text_unit(Origin, Origin, Text, Unit, ReaderDiagnostics),
    compile_after_read(ReaderDiagnostics, Unit, Compiled, Diagnostics),
    compiled_outputs(Compiled, CompilerRows, RuntimeProgram).

type_prelude_path(Path) :-
    source_file(dl7_type_compiler:compile_dl7(_, _, _, _), SourcePath),
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
    compiled_unit(TypeGraphFacts, RuntimeProgram, CompilerFacts),
    Diagnostics) :-
    graph_seeds(Graph, GraphSeeds),
    append(GraphSeeds, AuthoredSeeds, Seeds),
    evaluate(Rules, Seeds, CompilerFacts, Diagnostics),
    type_graph_facts(CompilerFacts, TypeGraphFacts),
    RuntimeProgram = checked_datalog(
                         Graph,
                         datalog_program(Relations, AuthoredSeeds, Rules),
                         Depends, Strata).

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
