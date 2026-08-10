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

:- end_tests(emit_dd_plan).
