:- begin_tests(dl7_entrypoints).

:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../4_loader', [load_dl7/3]).
:- use_module('fixtures/1_embedded', []).

test(file_and_bare_quasi_share_reader_and_expansion_pipeline) :-
    load_dl7('v7/0_SWIPL/test/fixtures/0_minimal.dl7',
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
    load_dl7('v7/0_SWIPL/test/fixtures/0_minimal.dl7',
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
          '-s', 'v7/0_SWIPL/5_driver.pl',
          '--', 'v7/0_SWIPL/test/fixtures/0_minimal.dl7'
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
