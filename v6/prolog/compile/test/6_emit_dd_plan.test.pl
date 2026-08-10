:- begin_tests(emit_dd_plan).

:- use_module('../6_emit_dd_plan', [fixture_dd_plan_text/3]).

dd_fixture_file(Base, File) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/../../conformance/fixtures/', Base], File).

dd_golden(Name, Text) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/dd/', Name, '.dd.pl'], Path),
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

test(every_operator_has_a_wire) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    forall(member(Name, [float_exact_join_has_no_epsilon, float_avg_is_grouped]),
           ( fixture_dd_plan_text(Fixture, Name, Text),
             text_plan(Text, dd_plan(_, _, _, operators(Operators), wires(Wires), _)),
             forall(member(op(Id, _), Operators), operator_has_wire(Id, Wires)) )).

test(join_inputs_have_keyed_arrangements) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_text(Fixture, float_exact_join_has_no_epsilon, Text),
    text_plan(Text, dd_plan(_, _, arrangements(Arrangements),
                            operators([_, op(_, join(_, _, LeftId, RightId))]), _, _)),
    memberchk(arr(LeftId, left/2, [name], [value], signed), Arrangements),
    memberchk(arr(RightId, right/2, [name], [value], signed), Arrangements).

text_plan(Text, Plan) :-
    atom_string(Atom, Text),
    read_term_from_atom(Atom, Plan, []).

operator_has_wire(Id, Wires) :- memberchk(wire(Id, _, _), Wires), !.
operator_has_wire(Id, Wires) :- memberchk(wire(_, Id, _), Wires).

:- end_tests(emit_dd_plan).
