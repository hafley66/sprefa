:- begin_tests(emit_dd_plan).

:- use_module('../6_emit_dd_plan', [fixture_dd_plan_text/3, fixture_dd_plan_json_text/3]).

dd_fixture_file(Base, File) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/../../conformance/fixtures/', Base], File).

dd_golden(Name, Text) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/dd/', Name, '.dd.pl'], Path),
    read_file_to_string(Path, Text, []).

dd_json_golden(Name, Text) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/dd/', Name, '.dd.json'], Path),
    read_file_to_string(Path, Text, []).

test(retraction_only_tick_retracts_level_view) :-
    dd_fixture_file('engine_core.pl', Fixture),
    fixture_dd_plan_text(Fixture, retraction_only_tick_retracts_level_view, Text),
    dd_golden(retraction_only_tick_retracts_level_view, Text).

test(float_exact_join_has_no_epsilon) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_text(Fixture, float_exact_join_has_no_epsilon, Text),
    dd_golden(float_exact_join_has_no_epsilon, Text).

test(float_avg_is_grouped) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_text(Fixture, float_avg_is_grouped, Text),
    dd_golden(float_avg_is_grouped, Text).

test(json_twins_are_deterministic) :-
    dd_fixture_file('engine_core.pl', EngineFixture),
    dd_fixture_file('5_value_plane.pl', ValueFixture),
    forall(member(Fixture-Name,
                  [ EngineFixture-retraction_only_tick_retracts_level_view,
                    ValueFixture-float_exact_join_has_no_epsilon,
                    ValueFixture-float_avg_is_grouped ]),
           ( fixture_dd_plan_json_text(Fixture, Name, First),
             fixture_dd_plan_json_text(Fixture, Name, Second),
             First == Second,
             dd_json_golden(Name, First) )).

test(every_operator_has_a_wire) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    forall(member(Name, [float_exact_join_has_no_epsilon, float_avg_is_grouped]),
           ( fixture_dd_plan_text(Fixture, Name, Text),
             text_plan(Text, dd_plan(_, _, _, operators(Operators), wires(Wires), _)),
             forall(member(op(Id, _, _), Operators), operator_has_wire(Id, Wires)) )).

test(each_rule_sql_appears_on_its_head_writing_map_once) :-
    dd_fixture_file('engine_core.pl', EngineFixture),
    dd_fixture_file('5_value_plane.pl', ValueFixture),
    forall(member(Fixture-Name,
                  [ EngineFixture-retraction_only_tick_retracts_level_view,
                    ValueFixture-float_exact_join_has_no_epsilon,
                    ValueFixture-float_avg_is_grouped ]),
           ( fixture_dd_plan_text(Fixture, Name, Text),
             text_plan(Text, dd_plan(_, rels(Rels), _, operators(Operators), _, _)),
             findall(Ref, member(rel(Ref, _, _), Rels), PlanRefs),
             forall(member(op(MapId, map(_), sqlite(PayloadRefs, Statements)), Operators),
                    ( PayloadRefs \== [],
                      Statements \== [],
                      forall(member(Ref, PayloadRefs), memberchk(Ref, PlanRefs)),
                      forall(member(Statement, Statements), payload_statement(Statement)),
                      forall(member(Statement, Statements),
                             ( findall(Id,
                                       member(op(Id, _, sqlite(_, Statements)), Operators),
                                       StatementIds),
                               StatementIds = [MapId] )) )),
             forall(member(op(Id, Description, sqlite(PayloadRefs, owner(MapId))), Operators),
                    ( Description \= map(_),
                      PayloadRefs \== [],
                      memberchk(op(MapId, map(_), sqlite(PayloadRefs, _)), Operators),
                      Id \= MapId )) )).

test(join_inputs_have_keyed_arrangements) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_text(Fixture, float_exact_join_has_no_epsilon, Text),
    text_plan(Text, dd_plan(_, _, arrangements(Arrangements),
                            operators([_, op(_, join(_, _, LeftId, RightId), _)]), _, _)),
    memberchk(arr(LeftId, left/2, [name], [value], signed), Arrangements),
    memberchk(arr(RightId, right/2, [name], [value], signed), Arrangements).

text_plan(Text, Plan) :-
    atom_string(Atom, Text),
    read_term_from_atom(Atom, Plan, []).

operator_has_wire(Id, Wires) :- memberchk(wire(Id, _, _), Wires), !.
operator_has_wire(Id, Wires) :- memberchk(wire(_, Id, _), Wires).

payload_statement(edgestmt(_, _, _, _, _, _, _, _, _)).
payload_statement(levelstmt(_, _, _, _, _, _, _)).

:- end_tests(emit_dd_plan).
