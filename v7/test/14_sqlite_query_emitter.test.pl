:- begin_tests(dl7_sqlite_query_emitter).

:- use_module(library(http/json), [atom_json_dict/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../src/3_emit/1c_sqlite_query_emitter',
              [emit_sqlite_query/4, sqlite_identifier/2]).
:- use_module('../src/4_tool/4_sqlite_query_mainer',
              [ compile_sqlite_query/4,
                install_database/7,
                query_database/5,
                run_sqlite/6
              ]).

:- dynamic cached_fixture_artifact/1.
:- dynamic test_directory/1.

test(real_compiler_emits_deterministic_keyed_inner_joins, [nondet]) :-
    fixture_paths(Program, Layout),
    compile_sqlite_query(Program, Layout, First, FirstDiagnostics),
    compile_sqlite_query(Program, Layout, Second, SecondDiagnostics),
    FirstDiagnostics == [],
    SecondDiagnostics == [],
    get_dict(select_sql, First, FirstSql),
    get_dict(select_sql, Second, SecondSql),
    FirstSql == SecondSql,
    Sql = FirstSql,
    sub_string(Sql, _, _, _,
               "INNER JOIN \"node\" AS \"g1\" ON"),
    sub_string(Sql, _, _, _,
               "\"g1\".\"_input_path\" = \"g0\".\"_input_path\""),
    sub_string(Sql, _, _, _,
               "\"g2\".\"span__end\" = \"g0\".\"to__end\""),
    sub_string(Sql, _, _, _,
               "\"g0\".\"_input_path\" IS NOT NULL"),
    \+ sub_string(Sql, _, _, _,
                  "FROM \"edge\" AS \"g0\", \"node\"").

test(native_ivm_lifecycle_preserves_set_support_and_sources,
     [ setup(lifecycle_setup(Database, Artifact, Sqlite, Extension)),
       cleanup(delete_test_database(Database))
     ]) :-
    install_database(
        Sqlite, Extension, Database, "node", Artifact,
        "", SourceConflictDiagnostics),
    assertion(SourceConflictDiagnostics ==
        [diagnostic(sqlite, none,
                    conflicting_sqlite_output("node", "table"))]),
    get_dict(sources, Artifact, [NodeSource, EdgeSource]),
    get_dict(columns, NodeSource, NodeColumns),
    append(NodeColumns, ["missing_column"], MissingColumns),
    put_dict(columns, NodeSource, MissingColumns, MissingNodeSource),
    put_dict(sources, Artifact,
             [MissingNodeSource, EdgeSource], MissingArtifact),
    install_database(
        Sqlite, Extension, Database, "missing_result", MissingArtifact,
        "", MissingColumnDiagnostics),
    assertion(MissingColumnDiagnostics ==
        [diagnostic(sqlite, none,
                    missing_sqlite_source_column(
                        "Node", "node", "missing_column"))]),
    install_database(
        Sqlite, Extension, Database, "cst_edge_kinds", Artifact,
        InstallOutput, InstallDiagnostics),
    assertion(InstallDiagnostics == []),
    json_rows(InstallOutput, InitialRows),
    expected_row("root-a", "child-a", InitialExpected),
    assertion(InitialRows = [InitialExpected]),
    install_database(
        Sqlite, Extension, Database, "cst_edge_kinds", Artifact,
        "", DuplicateOutputDiagnostics),
    assertion(DuplicateOutputDiagnostics ==
        [diagnostic(sqlite, none,
                    conflicting_sqlite_output(
                        "cst_edge_kinds", "table"))]),
    query_database(
        Sqlite, Database, Artifact, OrdinaryOutput, OrdinaryDiagnostics),
    assertion(OrdinaryDiagnostics == []),
    json_rows(OrdinaryOutput, OrdinaryRows),
    assertion(OrdinaryRows == InitialRows),
    ivm_exec(
        Sqlite, Extension, Database,
        "DELETE FROM node WHERE rowid = (SELECT min(rowid) FROM node WHERE _input_path='/repo/a/src/lib.rs' AND span__start=0); SELECT * FROM cst_edge_kinds;",
        OneSupportOutput),
    json_rows(OneSupportOutput, OneSupportRows),
    assertion(OneSupportRows = [InitialExpected]),
    ivm_exec(
        Sqlite, Extension, Database,
        "DELETE FROM node WHERE _input_path='/repo/a/src/lib.rs' AND span__start=0; SELECT * FROM cst_edge_kinds;",
        LastSupportOutput),
    json_rows(LastSupportOutput, LastSupportRows),
    assertion(LastSupportRows == []),
    ivm_exec(
        Sqlite, Extension, Database,
        "INSERT INTO node VALUES('/repo/a/src/lib.rs','same-content','cst',0,1,'root-a'); UPDATE node SET kind='child-updated' WHERE _input_path='/repo/a/src/lib.rs' AND span__start=2; SELECT * FROM cst_edge_kinds;",
        UpdateOutput),
    json_rows(UpdateOutput, UpdateRows),
    expected_row("root-a", "child-updated", UpdatedExpected),
    assertion(UpdateRows = [UpdatedExpected]),
    ivm_exec(
        Sqlite, Extension, Database,
        "BEGIN; DELETE FROM edge; ROLLBACK; SELECT * FROM cst_edge_kinds;",
        RollbackOutput),
    json_rows(RollbackOutput, RollbackRows),
    assertion(RollbackRows = [UpdatedExpected]),
    ivm_exec(
        Sqlite, Extension, Database,
        "SELECT * FROM cst_edge_kinds;",
        ReopenOutput),
    json_rows(ReopenOutput, ReopenRows),
    assertion(ReopenRows = [UpdatedExpected]),
    ivm_exec(
        Sqlite, Extension, Database,
        "DROP TABLE cst_edge_kinds; SELECT name FROM sqlite_schema WHERE type='table' AND name IN ('node','edge') ORDER BY name;",
        DropOutput),
    json_rows(DropOutput, SourceObjects),
    assertion(SourceObjects = [_{name:"edge"}, _{name:"node"}]).

test(connected_body_reordering_and_rule_union_are_explicit, [nondet]) :-
    connected_union_runtime(Runtime),
    Layout = [ sqlite_source("Input", "input", ["x"]),
               sqlite_source("Other", "other", ["y"]),
               sqlite_source("Bridge", "bridge", ["x", "y"]),
               sqlite_output("Output", ["x", "y"])
             ],
    emit_sqlite_query(compiled_unit([], Runtime, []), Layout,
                      Artifact, Diagnostics),
    Diagnostics == [],
    get_dict(select_sql, Artifact, SelectSql),
    sub_string(SelectSql, _, _, _, " UNION "),
    sub_string(SelectSql, _, _, _,
               "INNER JOIN \"bridge\" AS \"g1\" ON"),
    get_dict(rules, Artifact,
             [_{rule:0, sql:_}, _{rule:1, sql:_}]).

test(unsupported_program_and_layout_shapes_are_rejected) :-
    boundary_runtime(
        [rule(call(ref(output), [aggregate(count, var(value))]),
              [checked_goal(positive,
                            call(ref(input), [var(value)]))])],
        [], AggregateRuntime),
    basic_layout(Layout),
    emit_sqlite_query(
        compiled_unit([], AggregateRuntime, []), Layout, _, Aggregate),
    boundary_runtime(
        [rule(call(ref(output), [var(value)]),
              [checked_goal(negative,
                            call(ref(input), [var(value)]))])],
        [], NegativeRuntime),
    emit_sqlite_query(
        compiled_unit([], NegativeRuntime, []), Layout, _, Negative),
    boundary_runtime(
        [rule(call(ref(output), [var(value)]),
              [checked_goal(positive,
                            call(ref(output), [var(value)]))])],
        [], RecursiveRuntime),
    emit_sqlite_query(
        compiled_unit([], RecursiveRuntime, []), Layout, _, Recursive),
    boundary_runtime(
        [rule(call(ref(output), [var(value)]),
              [checked_goal(positive,
                            call(ref(input), [var(value)]))])],
        [call(ref(input), [const(1)])], SeedRuntime),
    emit_sqlite_query(
        compiled_unit([], SeedRuntime, []), Layout, _, Seed),
    boundary_runtime(
        [rule(call(ref(output), [var(missing)]),
              [checked_goal(positive,
                            call(ref(input), [var(value)]))])],
        [], UnboundRuntime),
    emit_sqlite_query(
        compiled_unit([], UnboundRuntime, []), Layout, _, Unbound),
    boundary_runtime(
        [rule(call(ref(output), [var(value)]),
              [checked_goal(positive,
                            call(ref(input), [var(value)]))]),
         rule(call(ref(input), [var(value)]),
              [checked_goal(positive,
                            call(ref(other), [var(value)]))])],
        [], DerivedRuntime),
    emit_sqlite_query(
        compiled_unit([], DerivedRuntime, []), Layout, _, Derived),
    boundary_runtime(
        [rule(call(ref(output), [const(1)]), [])],
        [], ZeroBodyRuntime),
    emit_sqlite_query(
        compiled_unit([], ZeroBodyRuntime, []), Layout, _, ZeroBody),
    emit_sqlite_query(
        compiled_unit([], UnboundRuntime, []),
        [ sqlite_source("Input", "input", ["value"]),
          sqlite_output("Output", ["value", "VALUE"])
        ], _, DuplicateColumns),
    catch(sqlite_identifier("bad\u0000name", _),
          sqlite_query_error(NulReason), true),
    Observed = [Aggregate, Negative, Recursive, Seed, Unbound,
                Derived, ZeroBody, DuplicateColumns, NulReason],
    Observed ==
        [ [diagnostic(emit, none,
                      unsupported_sqlite_aggregate(rule_id(0), count))],
          [diagnostic(emit, none,
                      unsupported_sqlite_goal_polarity(
                          rule_id(0), negative))],
          [diagnostic(emit, none,
                      unsupported_sqlite_recursion(rule_id(0), output))],
          [diagnostic(emit, none,
                      unsupported_sqlite_seed(seed_id(0), input))],
          [diagnostic(emit, none,
                      unsupported_sqlite_unbound_output(
                          rule_id(0), missing))],
          [diagnostic(emit, none,
                      unsupported_sqlite_derived_source(
                          "Input", rule_id(1)))],
          [diagnostic(emit, none,
                      unsupported_sqlite_zero_body_rule(rule_id(0)))],
          [diagnostic(emit, none, duplicate_sqlite_output_column)],
          invalid_sqlite_layout_name(identifier, "bad\u0000name")
        ].

test(cli_read_only_query_smoke,
     [ setup(cli_setup(Database, Sqlite)),
       cleanup(delete_test_database(Database))
     ]) :-
    fixture_paths(Program, Layout),
    tool_path(Tool),
    process_create(
        path(swipl),
        [ '-q', '-s', Tool, '--', Program, Layout, Database,
          '--query', '--sqlite3', Sqlite
        ],
        [ stdout(pipe(Out)), stderr(pipe(Err)), process(Pid) ]),
    read_string(Out, _, Output),
    close(Out),
    read_string(Err, _, _Error),
    close(Err),
    process_wait(Pid, Status),
    Status == exit(0),
    json_rows(Output, Rows),
    expected_row("root-a", "child-a", Expected),
    Rows = [Expected].

lifecycle_setup(Database, Artifact, Sqlite, Extension) :-
    sqlite_binary(Sqlite),
    extension_library(Extension),
    fresh_database(Database),
    fixture_artifact(Artifact),
    create_fixture_database(Sqlite, Database).

cli_setup(Database, Sqlite) :-
    sqlite_binary(Sqlite),
    fresh_database(Database),
    create_fixture_database(Sqlite, Database).

fixture_artifact(Artifact) :-
    cached_fixture_artifact(Artifact),
    !.
fixture_artifact(Artifact) :-
    fixture_paths(Program, Layout),
    compile_sqlite_query(Program, Layout, Artifact, Diagnostics),
    Diagnostics == [],
    assertz(cached_fixture_artifact(Artifact)).

create_fixture_database(Sqlite, Database) :-
    Sql = "CREATE TABLE node(_input_path TEXT,_content_id TEXT,family TEXT,span__start INTEGER,span__end INTEGER,kind TEXT); CREATE TABLE edge(_input_path TEXT,_content_id TEXT,family TEXT,kind TEXT,from__start INTEGER,from__end INTEGER,to__start INTEGER,to__end INTEGER); INSERT INTO node VALUES('/repo/a/src/lib.rs','same-content','cst',0,1,'root-a'),('/repo/a/src/lib.rs','same-content','cst',0,1,'root-a'),('/repo/a/src/lib.rs','same-content','cst',2,3,'child-a'),('/repo/b/src/lib.rs','same-content','cst',0,1,'root-b'),('/repo/b/src/lib.rs','same-content','cst',2,3,'child-b'),(NULL,'same-content','cst',0,1,'null-root'),(NULL,'same-content','cst',2,3,'null-child'); INSERT INTO edge VALUES('/repo/a/src/lib.rs','same-content','cst','child',0,1,2,3),(NULL,'same-content','cst','child',0,1,2,3);",
    run_sqlite(Sqlite, Database, readwrite, none, Sql, Result),
    Result = sqlite_result(0, _, "").

ivm_exec(Sqlite, Extension, Database, Sql, Output) :-
    run_sqlite(
        Sqlite, Database, readwrite, Extension, Sql,
        sqlite_result(0, Output, "")).

expected_row(SourceKind, TargetKind,
             _{ input_path:"/repo/a/src/lib.rs",
                content_id:"same-content",
                edge_kind:"child",
                from_start:0,
                from_end:1,
                source_kind:SourceKind,
                to_start:2,
                to_end:3,
                target_kind:TargetKind
              }).

json_rows(Output, Rows) :-
    normalize_space(string(Normalized), Output),
    (   Normalized == ""
    ->  Json = "[]"
    ;   Json = Output
    ),
    atom_json_dict(Json, Rows, [value_string_as(string)]).

boundary_runtime(Rules, Seeds,
                 checked_datalog(
                     root_graph([], Edges),
                     datalog_program(Relations, Seeds, Rules), [], [])) :-
    Edges = [ ':'(module(file('/fixture.dl7')), 'Input',
                  ref(input), 0),
              ':'(module(file('/fixture.dl7')), 'Output',
                  ref(output), 1),
              ':'(module(file('/fixture.dl7')), 'Other',
                  ref(other), 2)
            ],
    Relations = [ relation(ref(input), 1, []),
                  relation(ref(output), 1, []),
                  relation(ref(other), 1, [])
                ].

basic_layout([
    sqlite_source("Input", "input", ["value"]),
    sqlite_output("Output", ["value"])
]).

connected_union_runtime(
    checked_datalog(
        root_graph([], Edges),
        datalog_program(Relations, [], Rules), [], [])) :-
    Owner = module(file('/connected.dl7')),
    Edges = [ ':'(Owner, 'Input', ref(input), 0),
              ':'(Owner, 'Other', ref(other), 1),
              ':'(Owner, 'Bridge', ref(bridge), 2),
              ':'(Owner, 'Output', ref(output), 3)
            ],
    Relations = [ relation(ref(input), 1, []),
                  relation(ref(other), 1, []),
                  relation(ref(bridge), 2, []),
                  relation(ref(output), 2, [])
                ],
    Rules = [ rule(
                  call(ref(output), [var(x), var(y)]),
                  [ checked_goal(positive,
                                 call(ref(input), [var(x)])),
                    checked_goal(positive,
                                 call(ref(other), [var(y)])),
                    checked_goal(positive,
                                 call(ref(bridge), [var(x), var(y)]))
                  ]),
              rule(
                  call(ref(output), [var(x), var(y)]),
                  [ checked_goal(positive,
                                 call(ref(bridge), [var(x), var(y)])),
                    checked_goal(positive,
                                 call(ref(input), [var(x)])),
                    checked_goal(positive,
                                 call(ref(other), [var(y)]))
                  ])
            ].

fixture_paths(Program, Layout) :-
    test_directory(TestDirectory),
    directory_file_path(
        TestDirectory, 'fixtures/sqlite_query/0_cst_edge_kinds.dl7',
        Program),
    directory_file_path(
        TestDirectory, 'fixtures/sqlite_query/1_cst_edge_kinds.layout.json',
        Layout).

tool_path(Tool) :-
    test_directory(TestDirectory),
    directory_file_path(
        TestDirectory, '../src/4_tool/4_sqlite_query_mainer.pl', Tool0),
    absolute_file_name(Tool0, Tool, [access(read)]).

sqlite_binary(Sqlite) :-
    (   getenv('SQLITE3', Environment),
        Environment \== ''
    ->  atom_string(Environment, Sqlite)
    ;   Sqlite = "/opt/homebrew/opt/sqlite/bin/sqlite3"
    ).

extension_library(Extension) :-
    (   getenv('IVM_EXTENSION', Environment),
        Environment \== ''
    ->  atom_string(Environment, Extension)
    ;   test_directory(TestDirectory),
        directory_file_path(
            TestDirectory,
            '../../sqlite_ivm/target/release/libsqlite_ivm.dylib',
            Candidate),
        absolute_file_name(Candidate, Extension, [access(read)])
    ).

fresh_database(Database) :-
    tmp_file(dl7_sqlite_query, Database),
    delete_test_database(Database).

delete_test_database(Database) :-
    (   exists_file(Database)
    ->  delete_file(Database)
    ;   true
    ).

:- prolog_load_context(directory, TestDirectory),
   assertz(test_directory(TestDirectory)).

:- end_tests(dl7_sqlite_query_emitter).
