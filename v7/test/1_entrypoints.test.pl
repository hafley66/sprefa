:- begin_tests(dl7_entrypoints).

:- use_module(library(aggregate), [aggregate_all/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../src/0_reader/3_file_loader', [load_dl7/3]).
:- use_module('../src/2_comptime/1_type_compiler', [compile_dl7/4]).
:- use_module('fixtures/1_embedded', []).

test(file_and_bare_quasi_share_reader_and_expansion_pipeline) :-
    load_dl7('v7/test/fixtures/0_minimal.dl7',
             FileUnit, FileDiagnostics),
    FileUnit = dl7_unit(FileOrigin, content_sha256(FileDigest),
                        FileForms, FileRows, FileExpansions),
    once(dl7_embedded_fixture:dl7_unit(
             EmbeddedOrigin, content_sha256(EmbeddedDigest),
             EmbeddedForms, EmbeddedRows, EmbeddedExpansions)),
    content_snapshot(FileForms, FileRows, FileContent),
    content_snapshot(EmbeddedForms, EmbeddedRows, EmbeddedContent),
    origin_kinds(FileOrigin, EmbeddedOrigin, OriginKinds),
    equality(FileDigest, EmbeddedDigest, DigestEqual),
    equality(FileContent, EmbeddedContent, ContentEqual),
    Observed = entrypoint_result(
                   OriginKinds, DigestEqual, ContentEqual,
                   FileDiagnostics, FileExpansions, EmbeddedExpansions),
    Observed == entrypoint_result(true, true, true, [], [], []).

test(driver_is_canonical_on_two_consecutive_runs) :-
    load_dl7('v7/test/fixtures/0_minimal.dl7',
             ExpectedUnit, []),
    driver_run(Status1, Stdout1, Stderr1),
    driver_run(Status2, Stdout2, Stderr2),
    term_string(Unit1, Stdout1),
    term_string(Unit2, Stdout2),
    equality(Stdout1, Stdout2, OutputEqual),
    equality(Unit1, ExpectedUnit, FirstUnitEqual),
    equality(Unit2, ExpectedUnit, SecondUnitEqual),
    Observed = driver_result(Status1, Status2, OutputEqual,
                             FirstUnitEqual, SecondUnitEqual,
                             Stderr1, Stderr2),
    Observed == driver_result(exit(0), exit(0), true,
                              true, true, "", "").

test(userland_partial_maps_type_edges_deterministically) :-
    compile_dl7('v7/test/fixtures/2_partial.dl7',
                Rows1, Runtime1, Diagnostics1),
    compile_dl7('v7/test/fixtures/2_partial.dl7',
                Rows2, Runtime2, Diagnostics2),
    once(partial_snapshot(Rows1, Snapshot)),
    runtime_snapshot(Runtime1, RuntimeSnapshot),
    evaluator_snapshot(EvaluatorSnapshot),
    equality(Rows1, Rows2, RowsEqual),
    equality(Runtime1, Runtime2, RuntimeEqual),
    length(Rows1, CompilerRowCount),
    Observed = partial_result(Diagnostics1, Diagnostics2,
                              CompilerRowCount, Snapshot,
                              RuntimeSnapshot, EvaluatorSnapshot,
                              RowsEqual, RuntimeEqual),
    Observed == partial_result(
                    [], [], 59,
                    partial(user,
                            [mapped(id, option(int), 0),
                             mapped(name, option(text), 1)]),
                    runtime(counts(28, 25, 11, 1, 5, 10, 11),
                            normalized(true)),
                    evaluator(temporary_rules(0), temporary_seeds(0)),
                    true, true),
    !.

partial_snapshot(Rows, Snapshot) :-
    member(call(ref(kernel(':')),
                [ref(Module), const('User'), ref(User), const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('Partial'), ref(PartialConstructor),
                 const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('Option'), ref(OptionConstructor),
                 const(_)]), Rows),
    Partial = application(PartialConstructor, [User]),
    member(call(ref(kernel(node)), [ref(Partial)]), Rows),
    member(call(ref(kernel(product)), [ref(Partial)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Partial), const(id),
                 ref(application(OptionConstructor, [primitive(int)])),
                 const(0)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Partial), const(name),
                 ref(application(OptionConstructor, [primitive(text)])),
                 const(1)]), Rows),
    Snapshot = partial(user,
                       [mapped(id, option(int), 0),
                        mapped(name, option(text), 1)]).

runtime_snapshot(
    checked_datalog(root_graph(Nodes, Edges),
                    datalog_program(Relations, Seeds, Rules),
                    Depends, Strata),
    runtime(counts(NodeCount, EdgeCount, RelationCount, SeedCount,
                   RuleCount, DependsCount, StrataCount),
            normalized(Normalized))) :-
    maplist(length,
            [Nodes, Edges, Relations, Seeds, Rules, Depends, Strata],
            [NodeCount, EdgeCount, RelationCount, SeedCount,
             RuleCount, DependsCount, StrataCount]),
    (   normalized_program(Relations, Seeds, Rules, Depends, Strata)
    ->  Normalized = true
    ;   Normalized = false
    ).

normalized_program(Relations, Seeds, Rules, Depends, Strata) :-
    maplist(normalized_relation, Relations),
    maplist(normalized_call, Seeds),
    maplist(normalized_rule, Rules),
    maplist(normalized_depends, Depends),
    maplist(normalized_stratum, Strata).

normalized_relation(relation(ref(_), Arity)) :- integer(Arity).
normalized_call(call(ref(_), Arguments)) :- is_list(Arguments).
normalized_rule(rule(Head, Body)) :-
    normalized_call(Head),
    maplist(normalized_call, Body).
normalized_depends(depends(ref(_), ref(_), positive)).
normalized_stratum(stratum(ref(_), 0)).

evaluator_snapshot(
    evaluator(temporary_rules(RuleFacts), temporary_seeds(SeedFacts))) :-
    aggregate_all(count, dl7_evaluator:evaluation_rule(_, _), RuleFacts),
    aggregate_all(count, dl7_evaluator:evaluation_seed(_, _), SeedFacts).

origin_kinds(file(_),
             embedded(_, position(_, _, _)),
             true) :-
    !.
origin_kinds(_, _, false).

equality(Left, Right, true) :-
    Left == Right,
    !.
equality(_, _, false).

content_snapshot(Forms, SourceRows,
                 content(FormSnapshot, SourceSnapshot)) :-
    maplist(content_node, Forms, FormSnapshot),
    maplist(content_source, SourceRows, SourceSnapshot).

content_node(node(reader_node(_, Index), Payload),
             node(Index, Snapshot)) :-
    content_payload(Payload, Snapshot).

content_payload(atom(Name), atom(Name)).
content_payload(literal(Value), literal(Value)).
content_payload(variable(VariableId, Name),
                variable(SnapshotId, Name)) :-
    content_variable_id(VariableId, SnapshotId).
content_payload(form(Nodes), form(Snapshots)) :-
    maplist(content_node, Nodes, Snapshots).

content_variable_id(variable(reader_node(_, Index), Name),
                    variable(Index, Name)).

content_source(
    source(reader_node(_, Index), _, StartOffset, EndOffset,
           StartLine, StartColumn, EndLine, EndColumn),
    source(Index, StartOffset, EndOffset,
           StartLine, StartColumn, EndLine, EndColumn)).

driver_run(Status, Stdout, Stderr) :-
    process_create(
        path(swipl),
        [ '-q',
          '-s', 'v7/src/0_reader/4_cli_mainer.pl',
          '--', 'v7/test/fixtures/0_minimal.dl7'
        ],
        [ stdout(pipe(StdoutStream)),
          stderr(pipe(StderrStream)),
          process(Process)
        ]),
    read_string(StdoutStream, _, Stdout),
    close(StdoutStream),
    read_string(StderrStream, _, Stderr),
    close(StderrStream),
    process_wait(Process, Status),
    !.

:- end_tests(dl7_entrypoints).
