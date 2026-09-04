:- begin_tests(dl7_dbsp_plan).

:- use_module('../src/3_emit/1a_dbsp_plan_emitter',
              [emit_dbsp_plan/3, dbsp_plan_json/2]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7_project/5]).
:- use_module('../src/3_emit/0_logical_program_reifier',
              [logical_program_rows/2]).
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
             select_all:""},
          _{columns:[c0,c1], input:true, name:'tsi.name', output:false,
             select_all:""}
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

test(dl7_dbsp_emitter_derives_exact_checked_program_rows) :-
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
    artifact_rows(Artifacts, "relations", RelationRows),
    artifact_rows(Artifacts, "operators", OperatorRows),
    artifact_rows(Artifacts, "reads", ReadRows),
    artifact_rows(Artifacts, "projections", ProjectionRows),
    Observed = dbsp_artifacts(
                   compile(CompileDiagnostics),
                   emit(EmitDiagnostics),
                   relations(RelationRows),
                   operators(OperatorRows),
                   reads(ReadRows),
                   projections(ProjectionRows)),
    Observed == dbsp_artifacts(
                    compile([]),
                    emit([]),
                    relations(ExpectedRelations),
                    operators(ExpectedOperators),
                    reads(ExpectedReads),
                    projections(ExpectedProjections)).

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
        [ ref(logical_program(Rule)), ref(Head), const("level") ],
        ( member(program_rule(Rule, HeadCall), LogicalRows),
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
        [ ref(logical_program(Rule)), const(Position), const(Argument) ],
        ( member(program_rule(Rule, HeadCall), LogicalRows),
          member(program_argument(HeadCall, Position, Argument), LogicalRows)
        ),
        Rows0),
    sort(Rows0, Rows).

:- end_tests(dl7_dbsp_plan).
