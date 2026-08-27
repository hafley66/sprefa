:- begin_tests(isolated_compiler_dd).

:- use_module('../6_isolated_compiler_dd', [dd_plan_text/2, fixture_dd_plan_text/3, fixture_dd_plan_json_text/3]).
:- use_module('../../compile', [program_plan/2, read_fixture_term/4, compile_dl6/3]).
:- use_module('../../3_analyze/analyze', [body_ref_uses/2]).
:- use_module('../../7_lower/lower', [lower_program/2]).
:- use_module(library(http/json), [json_read_dict/3]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

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

test(json_twin_carries_join_keys_outside_sql) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_json_text(Fixture, float_exact_join_has_no_epsilon, Text),
    json_text_dict(Text, Dict),
    get_dict(operators, Dict, Operators),
    member(Op, Operators),
    get_dict(kind, Op, join),
    get_dict(join_keys, Op, JoinKeys),
    get_dict(left, JoinKeys, Left),
    get_dict(key_columns, Left, [name, value]),
    get_dict(right, JoinKeys, Right),
    get_dict(key_columns, Right, [name, value]).

test(json_twin_carries_reduce_aggregate) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_json_text(Fixture, float_avg_is_grouped, Text),
    json_text_dict(Text, Dict),
    get_dict(operators, Dict, Operators),
    member(Op, Operators),
    get_dict(kind, Op, reduce),
    get_dict(aggregate, Op, Agg),
    get_dict(kind, Agg, Kinds),
    memberchk(avg, Kinds),
    get_dict(group, Agg, Group), Group == ['b0.group'],
    get_dict(value, Agg, Value), Value == ['value'].

test(reduce_group_columns_resolve_against_bindings) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_json_text(Fixture, float_avg_is_grouped, Text),
    json_text_dict(Text, Dict),
    get_dict(operators, Dict, Operators),
    member(Op, Operators),
    get_dict(kind, Op, reduce),
    get_dict(bindings, Op, Bindings),
    get_dict(b0, Bindings, 'score/2'),
    get_dict(aggregate, Op, Agg),
    get_dict(group, Agg, [Group]),
    Group == 'b0.group',
    get_dict(projection, Op, Projection),
    proj_head_source(Projection, group, Group).

test(json_twin_carries_arrangements_and_wires) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_json_text(Fixture, float_exact_join_has_no_epsilon, Text),
    json_text_dict(Text, Dict),
    get_dict(arrangements, Dict, Arrangements),
    member(App, Arrangements),
    get_dict(rel, App, 'left/2'),
    get_dict(key_columns, App, [name, value]),
    get_dict(wires, Dict, Wires), Wires \== [],
    member(Wire, Wires),
    get_dict(from, Wire, 'join_1_1'),
    get_dict(to, Wire, 'map_1').

test(json_twin_carries_map_bindings_predicates_projection) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_json_text(Fixture, float_exact_join_has_no_epsilon, Text),
    json_text_dict(Text, Dict),
    get_dict(operators, Dict, Operators),
    member(Op, Operators),
    get_dict(kind, Op, map),
    get_dict(bindings, Op, Bindings),
    get_dict(b0, Bindings, 'left/2'),
    get_dict(b1, Bindings, 'right/2'),
    get_dict(predicates, Op, Predicates),
    member(NameEquals, Predicates),
    get_dict(column_equals, NameEquals, ['b0.name', 'b1.name']),
    member(ValueEquals, Predicates),
    get_dict(column_equals, ValueEquals, ['b0.value', 'b1.value']),
    get_dict(projection, Op, Projection),
    member(Proj, Projection),
    get_dict(head, Proj, name),
    get_dict(source, Proj, 'b0.name').

test(json_twin_carries_reduce_projection) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_json_text(Fixture, float_avg_is_grouped, Text),
    json_text_dict(Text, Dict),
    get_dict(operators, Dict, Operators),
    member(Op, Operators),
    get_dict(kind, Op, reduce),
    get_dict(projection, Op, Projection),
    member(GroupProj, Projection),
    get_dict(head, GroupProj, group),
    get_dict(source, GroupProj, 'b0.group'),
    member(ValueProj, Projection),
    get_dict(head, ValueProj, value),
    get_dict(source, ValueProj, 'b0.value').

test(operator_semantics_columns_exist_on_bound_rels) :-
    dd_fixture_file('engine_core.pl', EngineFixture),
    dd_fixture_file('5_value_plane.pl', ValueFixture),
    forall(member(Fixture-Name,
                  [ EngineFixture-retraction_only_tick_retracts_level_view,
                    ValueFixture-float_exact_join_has_no_epsilon,
                    ValueFixture-float_avg_is_grouped ]),
           ( fixture_dd_plan_text(Fixture, Name, Text),
             text_plan(Text, dd_plan(_, rels(Rels), _, operators(Operators), _, _)),
             forall(member(Op, Operators), op_semantics_columns_valid(Op, Rels)) )).

op_semantics_columns_valid(op(_, _, _, semantics(Bindings, Predicates, Projection)), Rels) :-
    binding_columns(Bindings, Rels, BindingMap),
    forall(member(P, Predicates), predicate_cols_valid(P, BindingMap)),
    forall(member(Pr, Projection), projection_col_valid(Pr, BindingMap)).

binding_columns(Bindings, Rels, BindingMap) :-
    findall(Alias-Columns,
            ( member(binding(Alias, Ref), Bindings),
              member(rel(Ref, Columns, _), Rels) ),
            BindingMap).

predicate_cols_valid(eq(Left, Right), BindingMap) :-
    col_valid(Left, BindingMap),
    col_valid(Right, BindingMap).
predicate_cols_valid(eq_lit(Col, _), BindingMap) :-
    col_valid(Col, BindingMap).

col_valid(col(Alias, Column), BindingMap) :-
    memberchk(Alias-Columns, BindingMap),
    memberchk(Column, Columns).

projection_col_valid(proj(_, col(Alias, Column)), BindingMap) :-
    memberchk(Alias-Columns, BindingMap),
    memberchk(Column, Columns).
projection_col_valid(proj_value(_, _), _).

test(every_operator_has_a_wire) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    forall(member(Name, [float_exact_join_has_no_epsilon, float_avg_is_grouped]),
           ( fixture_dd_plan_text(Fixture, Name, Text),
             text_plan(Text, dd_plan(_, _, _, operators(Operators), wires(Wires), _)),
             forall(member(op(Id, _, _, _), Operators), operator_has_wire(Id, Wires)) )).

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
             forall(member(op(MapId, map(_), sqlite(PayloadRefs, Statements), _), Operators),
                    ( PayloadRefs \== [],
                      Statements \== [],
                      forall(member(Ref, PayloadRefs), memberchk(Ref, PlanRefs)),
                      forall(member(Statement, Statements), payload_statement(Statement)),
                      forall(member(Statement, Statements),
                             ( findall(Id,
                                       member(op(Id, _, sqlite(_, Statements), _), Operators),
                                       StatementIds),
                               StatementIds = [MapId] )) )),
             forall(member(op(Id, Description, sqlite(PayloadRefs, owner(MapId)), _), Operators),
                    ( Description \= map(_),
                      PayloadRefs \== [],
                      memberchk(op(MapId, map(_), sqlite(PayloadRefs, _), _), Operators),
                      Id \= MapId )) )).

test(join_inputs_have_keyed_arrangements) :-
    dd_fixture_file('5_value_plane.pl', Fixture),
    fixture_dd_plan_text(Fixture, float_exact_join_has_no_epsilon, Text),
    text_plan(Text, dd_plan(_, _, arrangements(Arrangements),
                            operators([_, op(_, join(_, _, LeftId, RightId), _, _)]), _, _)),
    memberchk(arr(LeftId, left/2, [name,value], [], signed), Arrangements),
    memberchk(arr(RightId, right/2, [name,value], [], signed), Arrangements).

test(description_join_keys_cover_body_argument_equalities) :-
    dd_fixture_file('engine_core.pl', EngineFixture),
    dd_fixture_file('5_value_plane.pl', ValueFixture),
    forall(member(Fixture-Name,
                  [ EngineFixture-retraction_only_tick_retracts_level_view,
                    ValueFixture-float_exact_join_has_no_epsilon,
                    ValueFixture-float_avg_is_grouped ]),
           ( fixture_dd_plan_text(Fixture, Name, Text),
             text_plan(Text, dd_plan(_, rels(Rels), arrangements(Arrangements), _, _, _)),
             fixture_rules(Fixture, Name, Rules),
             forall(member(Rule, Rules),
                    rule_shared_columns_are_arrangement_keys(Rule, Rels, Arrangements)) )).

test(edge_rule_operator_serializes_with_classification) :-
    test_dir_fact(Here),
    atomic_list_concat([Here, '/dd/isolated_compiler_dd_unsupported_fixture.pl'], Fixture),
    fixture_dd_plan_json_text(Fixture, isolated_compiler_dd_edge_rule, Text),
    json_text_dict(Text, Dict),
    get_dict(operators, Dict, Operators),
    member(Op, Operators),
    forall(member(Key-Value,
                  [kind-map, classification-edge, head-'output/1',
                   trigger-'input/1', id-map_1]),
           get_dict(Key, Op, Value)),
    get_dict(rules, Dict, Rules),
    Rules == [].

% The seam entry, compile_program/5, carries Initial and Schedule from the text
% door's out-of-band context into the JSON, since the seam's five arguments do
% not include them. Exercised through the real text door (compile_dl6/3 with
% the emitter + schedule options), so the seed facts a .dl6 surfaces and the
% external arrival schedule both land in the emitted dd_plan JSON.
test(text_door_dd_emit_seeds_initial_and_schedule) :-
    tmp_file_prefix(Prefix),
    atomic_list_concat([Prefix, '.dl6'], Dl6),
    atomic_list_concat([Prefix, '.sched.json'], Sched),
    atomic_list_concat([Prefix, '.out.json'], Out),
    write_string_file(Dl6,
        "rel probe_in(probe_value) log keep(all).\nprobe_out(ProbeValue) <- probe_in(ProbeValue).\nprobe_in(\"a\").\n"),
    write_string_file(Sched,
        "[[{\"rel\":\"probe_in\",\"sign\":\"add\",\"row\":[\"b\"]}]]"),
    compile_dl6(Dl6, Out,
                [emitter(isolated_compiler_dd:compile_program), schedule(Sched)]),
    setup_call_cleanup(true,
        ( read_file_to_string(Out, Text, []),
          json_text_dict(Text, Dict),
          get_dict(initial, Dict, [SeedRow]),
          get_dict(rel, SeedRow, 'probe_in/1'),
          get_dict(values, SeedRow, ['a']),
          get_dict(schedule, Dict, [[Arrival]]),
          get_dict(rel, Arrival, 'probe_in/1'),
          get_dict(sign, Arrival, 1),
          get_dict(values, Arrival, ['b']) ),
        ( delete_file(Dl6), delete_file(Sched), catch(delete_file(Out), _, true) )).

write_string_file(Path, String) :-
    setup_call_cleanup(open(Path, write, Stream),
                       format(Stream, "~s", [String]),
                       close(Stream)).

tmp_file_prefix(Prefix) :-
    current_prolog_flag(pid, Pid),
    format(atom(Prefix), '/tmp/v6_seam_~w', [Pid]).

json_text_dict(Text, Dict) :-
    atom_string(Atom, Text),
    open_string(Atom, Stream),
    json_read_dict(Stream, Dict, [value_string_as(atom)]),
    close(Stream).

proj_head_source(Projection, Head, Source) :-
    member(Item, Projection),
    get_dict(head, Item, Head),
    get_dict(source, Item, Source),
    !.

test(empty_rule_order_falls_back_to_program_rules) :-
    Program = prog([], [(copy(Item) <- source(Item))]),
    program_plan(fixture(empty_rule_order, Program, [], [], [])-[], Plan),
    Plan = plan(Name, Prog, Types, RelPlans, ArrivalTargets, _RuleOrder, EdgeRules, SubscribedRels, Mode),
    dd_plan_text(plan(Name, Prog, Types, RelPlans, ArrivalTargets, [], EdgeRules, SubscribedRels, Mode), Text),
    text_plan(Text, dd_plan(_, _, _, operators(Operators), _, _)),
    memberchk(op(map_1, map(copy/1), _, _), Operators).

test(mutual_recursion_emits_joint_iterate) :-
    dd_fixture_file('engine_core.pl', Fixture),
    fixture_dd_plan_text(Fixture, mutual_recursion_matches_oracle, Text),
    text_plan(Text, dd_plan(_, _, _, operators(Operators), _, _)),
    memberchk(op(iterate_1, iterate([even/1, odd/1]), _, _), Operators).

fixture_rules(Fixture, Name, Rules) :-
    read_fixture_term(Fixture, Name, Term, Bindings),
    program_plan(Term-Bindings, plan(_, prog(_, ProgramRules), _, _, _, RuleOrder, _, _, _)),
    ordered_fixture_rules(RuleOrder, ProgramRules, Rules).

ordered_fixture_rules([], Rules, Rules) :- !.
ordered_fixture_rules(Rules, _, Rules).

rule_shared_columns_are_arrangement_keys(Rule, Rels, Arrangements) :-
    body_ref_uses_from_rule(Rule, Uses),
    forall(( select(Left, Uses, Rest),
             member(Right, Rest),
             shared_use_columns(Left, Right, Rels, LeftColumns, RightColumns),
             LeftColumns \== [] ),
           arrangement_pair_has_keys(Left, Right, LeftColumns, RightColumns, Arrangements)).

body_ref_uses_from_rule((_ <- Body), Uses) :- body_ref_uses(Body, Uses).
body_ref_uses_from_rule((_ <+ Body), Uses) :- body_ref_uses(Body, Uses).

shared_use_columns(use(LeftRef, LeftArgs, pos, _), use(RightRef, RightArgs, pos, _),
                   Rels, LeftColumns, RightColumns) :-
    findall(LeftPosition-RightPosition,
            ( nth1(LeftPosition, LeftArgs, Argument),
              nth1(RightPosition, RightArgs, OtherArgument),
              Argument == OtherArgument ),
            Pairs),
    pairs_positions(Pairs, LeftPositions, RightPositions),
    member(rel(LeftRef, AvailableLeftColumns, _), Rels),
    member(rel(RightRef, AvailableRightColumns, _), Rels),
    positions_columns(LeftPositions, AvailableLeftColumns, LeftColumns),
    positions_columns(RightPositions, AvailableRightColumns, RightColumns).

arrangement_pair_has_keys(use(LeftRef, _, _, _), use(RightRef, _, _, _),
                          LeftColumns, RightColumns, Arrangements) :-
    member(arr(_, LeftRef, LeftColumns, _, signed), Arrangements),
    member(arr(_, RightRef, RightColumns, _, signed), Arrangements).

pairs_positions([], [], []).
pairs_positions([Left-Right | Rest], [Left | Lefts], [Right | Rights]) :-
    pairs_positions(Rest, Lefts, Rights).

positions_columns([], _, []).
positions_columns([Position | Rest], Columns, [Column | More]) :-
    nth1(Position, Columns, Column),
    positions_columns(Rest, Columns, More).

text_plan(Text, Plan) :-
    atom_string(Atom, Text),
    read_term_from_atom(Atom, Plan, []).

operator_has_wire(Id, Wires) :- memberchk(wire(Id, _, _), Wires), !.
operator_has_wire(Id, Wires) :- memberchk(wire(_, Id, _), Wires).

payload_statement(edgestmt(_, _, _, _, _, _, _, _, _)).
payload_statement(levelstmt(_, _, _, _, _, _, _)).

:- end_tests(isolated_compiler_dd).
