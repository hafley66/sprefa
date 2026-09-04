:- begin_tests(dl7_dbsp_plan).

:- use_module('../src/3_emit/1a_dbsp_plan_emitter',
              [emit_dbsp_plan/3, dbsp_plan_json/2]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7_project/5]).
:- use_module('../src/3_emit/0_logical_program_reifier',
              [logical_program_rows/2, logical_program_calls/4]).
:- use_module('../src/3_emit/1_artifact_emitter', [emit_compiled/4]).

fixture(
    checked_datalog(
        root_graph(
            [],
            [ ':'(module(file('/fixture.dl7')), joined,
                  ref(owner(file('/fixture.dl7'), relation(0))), 0),
              ':'(module(file('/fixture.dl7')), 'tsi.name',
                  ref(tsi_relation(source, 'tsi.name')), 1),
              ':'(owner(file('/fixture.dl7'), relation(0)), name,
                  ref(primitive(text)), 0)
            ]),
        datalog_program(
            [ relation(ref(owner(file('/fixture.dl7'), relation(0))), 1, []),
              relation(ref(tsi_relation(source, 'tsi.name')), 2, []),
              relation(ref(owner(prelude, unused)), 1, [])
            ],
            [],
            [ rule(
                  call(ref(owner(file('/fixture.dl7'), relation(0))),
                       [var(name)]),
                  [ checked_goal(
                        positive,
                        call(ref(tsi_relation(source, 'tsi.name')),
                             [var(identity), var(name)]))
                  ])
            ]),
        [],
        [])).

test(source_visible_plan_preserves_dotted_relation_labels) :-
    fixture(Runtime),
    emit_dbsp_plan(Runtime, Plan, Diagnostics),
    Diagnostics == [],
    Plan.rels =@=
        [ _{columns:[name], input:false, name:joined, output:true,
             select_all:"SELECT (SELECT s.\"content\" FROM \"__str\" s WHERE s.\"__id\" = t.\"name\") AS \"name\" FROM \"joined\" t"},
          _{columns:[c0,c1], input:true, name:'tsi.name', output:false,
             select_all:"SELECT json(t.\"c0\") AS \"c0\", json(t.\"c1\") AS \"c1\" FROM \"tsi.name\" t"}
        ],
    Plan.ddl ==
        [ "CREATE TABLE IF NOT EXISTS \"__str\" (\"__id\" INTEGER PRIMARY KEY, \"content\" TEXT NOT NULL UNIQUE)",
          "CREATE TABLE IF NOT EXISTS \"joined\" (\"name\" INTEGER NOT NULL, UNIQUE (\"name\"))",
          "CREATE TABLE IF NOT EXISTS \"tsi.name\" (\"c0\" TEXT NOT NULL, \"c1\" TEXT NOT NULL, UNIQUE (\"c0\", \"c1\"))"
        ],
    Plan.rules =@=
        [ _{delete:"DELETE FROM \"joined\"",
             head:joined,
             id:map_0,
             inserts:["INSERT OR IGNORE INTO \"joined\" (\"name\") SELECT \"b0\".\"c1\" FROM \"tsi.name\" \"b0\""]}
        ],
    Plan.operators =@=
        [ _{ bindings:bindings{b0:'tsi.name'},
             classification:"level",
             head:joined,
             id:map_0,
             kind:"map",
             predicates:[],
             projection:[_{head:name, source:'b0.c1'}],
             refs:['tsi.name']
           }
        ],
    dbsp_plan_json(Plan, Json),
    once(sub_atom(Json, _, _, _, '"tsi.name"')).

test(negative_goal_is_a_named_emitter_gap) :-
    fixture(checked_datalog(Graph,
                            datalog_program(Relations, Seeds,
                                            [rule(Head, [checked_goal(_, Call)])]),
                            Dependencies, Strata)),
    Runtime = checked_datalog(
                  Graph,
                  datalog_program(
                      Relations, Seeds,
                      [rule(Head, [checked_goal(negative, Call)])]),
                  Dependencies, Strata),
    emit_dbsp_plan(Runtime, _, Diagnostics),
    Diagnostics ==
        [diagnostic(emit, none,
                    unsupported_dbsp_negation(rule_id(0)))].

test(reified_program_graph_reconstructs_the_checked_executable_exactly) :-
    fixture(Runtime),
    Runtime = checked_datalog(
                  _, datalog_program(_, ExpectedSeeds, ExpectedRules), _, _),
    logical_program_rows(Runtime, LogicalRows),
    dl7_dbsp_plan_emitter:logical_executable(
        LogicalRows, Seeds, Rules),
    executable(Seeds, Rules) ==
        executable(ExpectedSeeds, ExpectedRules).

test(checked_argument_alternatives_are_ordinary_edges) :-
    ProtocolRows = [
        call(ref(kernel(':')),
             [ref(module(prelude)), const(program_relation),
              ref(protocol(program_relation)), const(0)]),
        call(ref(kernel(':')),
             [ref(module(prelude)), const(program_seed),
              ref(protocol(program_seed)), const(1)]),
        call(ref(kernel(':')),
             [ref(module(prelude)), const(program_apply),
              ref(protocol(program_apply)), const(2)]),
        call(ref(kernel(':')),
             [ref(module(prelude)), const(program_argument),
              ref(protocol(program_argument)), const(3)]),
        call(ref(kernel(':')),
             [ref(module(prelude)), const(program_edge),
              ref(protocol(program_edge)), const(4)])
    ],
    Arguments = [var(value), ref(other), const("Ada"),
                 aggregate(count, var(value))],
    Runtime = checked_datalog(
                  graph,
                  datalog_program(
                      [relation(ref(source), 4, [])],
                      [call(ref(source), Arguments)],
                      []),
                  [], []),
    logical_program_calls(ProtocolRows, Runtime, Calls, Diagnostics),
    SeedCall = call_id(seed, 0),
    findall(argument(Position, Argument, Edges),
            ( between(0, 3, Position),
              Argument = argument_id(SeedCall, Position),
              findall([Label, Target, Index],
                      member(call(ref(protocol(program_edge)),
                                  [ ref(logical_program(Argument)),
                                    const(Label), Target, const(Index)
                                  ]),
                             Calls),
                      Edges)
            ),
            ArgumentRows),
    ArgumentRows == [
        argument(0, argument_id(SeedCall, 0),
                 [[variable, const(value), 0]]),
        argument(1, argument_id(SeedCall, 1),
                 [[reference, ref(logical_program(other)), 0]]),
        argument(2, argument_id(SeedCall, 2),
                 [[literal, const("Ada"), 0]]),
        argument(3, argument_id(SeedCall, 3),
                 [[aggregate, const(count), 0],
                  [input,
                   ref(logical_program(argument_child(
                           argument_id(SeedCall, 3), input))), 1]])
    ],
    AggregateInput = argument_child(argument_id(SeedCall, 3), input),
    memberchk(call(ref(protocol(program_edge)),
                   [ ref(logical_program(AggregateInput)),
                     const(variable), const(value), const(0)
                   ]),
              Calls),
    Diagnostics == [].

test(dl7_dbsp_emitter_derives_exact_checked_program_rows) :-
    garbage_collect,
    Paths = [ 'v7/emitters/0_dbsp.dl7',
              'v7/test/fixtures/10_dbsp_source.dl7'
            ],
    compile_dl7_project('.', Paths, CompilerRows, RuntimeProgram,
                        CompileDiagnostics),
    named_owner(CompilerRows, 'DbspEmitter', DbspEmitter),
    Compiled = compiled_unit([], RuntimeProgram, CompilerRows),
    emit_compiled(dl7(DbspEmitter), Compiled,
                  artifacts(Artifacts), EmitDiagnostics),
    logical_program_rows(RuntimeProgram, LogicalRows),
    expected_dbsp_relation_rows(LogicalRows, ExpectedRelations),
    expected_dbsp_operator_rows(LogicalRows, ExpectedOperators),
    expected_dbsp_read_rows(LogicalRows, ExpectedReads),
    expected_dbsp_projection_rows(LogicalRows, ExpectedProjections),
    expected_dbsp_call_rows(LogicalRows, ExpectedCalls),
    expected_dbsp_argument_rows(LogicalRows, ExpectedArguments),
    expected_dbsp_argument_edge_rows(LogicalRows, ExpectedArgumentEdges),
    artifact_rows(Artifacts, "relations", RelationRows),
    artifact_rows(Artifacts, "operators", OperatorRows),
    artifact_rows(Artifacts, "reads", ReadRows),
    artifact_rows(Artifacts, "projections", ProjectionRows),
    artifact_rows(Artifacts, "calls", CallRows),
    artifact_rows(Artifacts, "arguments", ArgumentRows),
    artifact_rows(Artifacts, "argument_edges", ArgumentEdgeRows),
    memberchk(
        [ Aggregate, const(aggregate), const(count), const(0) ],
        ArgumentEdgeRows),
    memberchk([Aggregate, const(input), Input, const(1)], ArgumentEdgeRows),
    memberchk([Input, const(variable), const(_), const(0)],
              ArgumentEdgeRows),
    Observed = dbsp_artifacts(
                   compile(CompileDiagnostics),
                   emit(EmitDiagnostics),
                   relations(RelationRows),
                   operators(OperatorRows),
                   reads(ReadRows),
                   projections(ProjectionRows),
                   calls(CallRows),
                   arguments(ArgumentRows),
                   argument_edges(ArgumentEdgeRows)),
    Observed == dbsp_artifacts(
                    compile([]),
                    emit([]),
                    relations(ExpectedRelations),
                    operators(ExpectedOperators),
                    reads(ExpectedReads),
                    projections(ExpectedProjections),
                    calls(ExpectedCalls),
                    arguments(ExpectedArguments),
                    argument_edges(ExpectedArgumentEdges)).

test(dl7_clock_emitter_queries_level_dependencies_during_comptime) :-
    garbage_collect,
    Paths = [ 'v7/emitters/0_dbsp.dl7',
              'v7/emitters/1_clock.dl7',
              'v7/test/fixtures/10_dbsp_source.dl7'
            ],
    compile_dl7_project('.', Paths, CompilerRows, RuntimeProgram,
                        CompileDiagnostics),
    named_owner(CompilerRows, 'ClockEmitter', ClockEmitter),
    Compiled = compiled_unit([], RuntimeProgram, CompilerRows),
    emit_compiled(dl7(ClockEmitter), Compiled,
                  artifacts(Artifacts), EmitDiagnostics),
    artifact_rows(Artifacts, "dependencies", DependencyRows),
    logical_program_rows(RuntimeProgram, LogicalRows),
    expected_clock_dependency_rows(LogicalRows, ExpectedRows),
    Observed = clock_artifact(
                   compile(CompileDiagnostics),
                   emit(EmitDiagnostics),
                   dependencies(DependencyRows)),
    Observed == clock_artifact(
                    compile([]),
                    emit([]),
                    dependencies(ExpectedRows)).

named_owner(Rows, Name, Owner) :-
    member(call(ref(kernel(':')),
                [ref(_), const(Name), ref(Owner), const(_)]),
           Rows),
    !.

artifact_rows(Artifacts, Name, Rows) :-
    memberchk(artifact(Name, _, Rows), Artifacts).

expected_dbsp_relation_rows(LogicalRows, Rows) :-
    findall([ref(Relation), const(Arity)],
            member(program_relation(Relation, Arity), LogicalRows),
            Rows0),
    sort(Rows0, Rows).

expected_dbsp_operator_rows(LogicalRows, Rows) :-
    findall(
        [ ref(logical_program(Rule)), ref(Head), const(KindText) ],
        ( member(program_rule(Rule, HeadCall), LogicalRows),
          memberchk(program_rule_kind(Rule, Kind), LogicalRows),
          atom_string(Kind, KindText),
          memberchk(program_apply(HeadCall, Head), LogicalRows)
        ),
        Rows0),
    sort(Rows0, Rows).

expected_dbsp_read_rows(LogicalRows, Rows) :-
    findall(
        [ ref(logical_program(Rule)), const(Position),
          const(PolarityText), ref(Relation)
        ],
        ( member(program_goal(Rule, Position, Polarity, Call), LogicalRows),
          memberchk(program_apply(Call, Relation), LogicalRows),
          atom_string(Polarity, PolarityText)
        ),
        Rows0),
    sort(Rows0, Rows).

expected_dbsp_projection_rows(LogicalRows, Rows) :-
    findall(
        [ ref(logical_program(Rule)), const(Position),
          ref(logical_program(Argument)) ],
        ( member(program_rule(Rule, HeadCall), LogicalRows),
          member(program_argument(HeadCall, Position, Argument), LogicalRows)
        ),
        Rows0),
    sort(Rows0, Rows).

expected_dbsp_call_rows(LogicalRows, Rows) :-
    findall(
        [ ref(logical_program(Rule)), const(Role), const(Position),
          ref(logical_program(Call)), ref(Relation) ],
        logical_dbsp_call(LogicalRows, Rule, Role, Position, Call, Relation),
        Rows0),
    sort(Rows0, Rows).

logical_dbsp_call(LogicalRows, Rule, "head", 0, Call, Relation) :-
    member(program_rule(Rule, Call), LogicalRows),
    memberchk(program_apply(Call, Relation), LogicalRows).
logical_dbsp_call(LogicalRows, Rule, Role, Position, Call, Relation) :-
    member(program_goal(Rule, Position, Polarity, Call), LogicalRows),
    atom_string(Polarity, Role),
    memberchk(program_apply(Call, Relation), LogicalRows).

expected_dbsp_argument_rows(LogicalRows, Rows) :-
    findall(
        [ ref(logical_program(Call)), const(Position),
          ref(logical_program(Argument)) ],
        member(program_argument(Call, Position, Argument), LogicalRows),
        Rows0),
    sort(Rows0, Rows).

expected_dbsp_argument_edge_rows(LogicalRows, Rows) :-
    findall(
        [ ref(logical_program(Argument)), const(Label), Target,
          const(Index) ],
        ( member(program_edge(Argument, Label, RawTarget, Index),
                 LogicalRows),
          expected_logical_target(RawTarget, Target)
        ),
        Rows0),
    sort(Rows0, Rows).

expected_logical_target(ref(Identity), ref(logical_program(Identity))).
expected_logical_target(const(Value), const(Value)).

expected_clock_dependency_rows(LogicalRows, Rows) :-
    findall(
        [ ref(logical_program(Rule)), ref(From), ref(To),
          const("relation"), const("relation"), const(Sign), const(0),
          const(Role)
        ],
        ( member(program_rule(Rule, HeadCall), LogicalRows),
          memberchk(program_rule_kind(Rule, level), LogicalRows),
          memberchk(program_apply(HeadCall, To), LogicalRows),
          member(program_goal(Rule, _, Polarity, BodyCall), LogicalRows),
          memberchk(program_apply(BodyCall, From), LogicalRows),
          level_clock_role(Polarity, Sign, Role)
        ),
        Rows0),
    sort(Rows0, Rows).

level_clock_role(positive, "positive", "level_read").
level_clock_role(negative, "negative", "level_absence").

:- end_tests(dl7_dbsp_plan).
