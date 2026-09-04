:- begin_tests(dl7_dbsp_plan).

:- use_module('../src/3_emit/1a_dbsp_plan_emitter',
              [emit_dbsp_plan/3, dbsp_plan_json/2]).

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

:- end_tests(dl7_dbsp_plan).
