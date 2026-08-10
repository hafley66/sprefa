% Emit a deterministic target-neutral DD plan term.
:- module(emit_dd_plan,
          [ emit_dd_plan/2,
            dd_plan_text/2,
            fixture_dd_plan_text/3
          ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

:- use_module('../compile', [read_fixture_term/4, program_plan/2]).
:- use_module('../analyze', [body_ref_uses/2, rule_head_ref/2,
                              rule_is_aggregate/1,
                              aggregate_head_template/2]).
:- use_module('../0_rel_record', [relplan_parts/6, relplan_columns/3]).
:- use_module('../lower', [lower_program/2]).

fixture_dd_plan_text(FixtureFile, Name, Text) :-
    once(( read_fixture_term(FixtureFile, Name, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           dd_plan_text(Plan, Text) )).

emit_dd_plan(Plan, Path) :-
    dd_plan_text(Plan, Text),
    setup_call_cleanup(open(Path, write, Stream),
                       format(Stream, '~s', [Text]),
                       close(Stream)).

dd_plan_text(Plan, Text) :-
    lower_program(Plan, Lowered),
    dd_plan_term(Plan, Lowered, DdPlan),
    with_output_to(string(Text),
                   write_term(DdPlan, [quoted(true), numbervars(true),
                                       fullstop(true), nl(true)])).

dd_plan_term(plan(Name, prog(_, Rules), _, RelPlans, _, RuleOrder, _, _, _),
             Lowered,
             dd_plan(Name, rels(Rels), arrangements(Arrangements),
                     operators(Operators), wires(Wires), tick_order(TickOrder))) :-
    maplist(rel_term, RelPlans, Rels),
    ordered_rules(RuleOrder, Rules, OrderedRules),
    arrangement_terms(RelPlans, OrderedRules, Arrangements),
    rule_operators(OrderedRules, Lowered, Operators),
    rule_wires(OrderedRules, Wires),
    tick_order(TickOrder).

ordered_rules([], _, []).
ordered_rules([Rule | Rest], Rules, [Rule | Ordered]) :-
    !,
    ordered_rules(Rest, Rules, Ordered).
ordered_rules(_, Rules, Rules).

rel_term(RelPlan, rel(Ref, Columns, Kind)) :-
    relplan_parts(RelPlan, Ref, Kind, Columns, _, _).

arrangement_term(RelPlan, arr(Id, Ref, KeyColumns, ValueColumns, signed)) :-
    relplan_parts(RelPlan, Ref, Kind, Columns, KeyOrNone, _),
    arrangement_columns(Kind, KeyOrNone, Columns, KeyColumns, ValueColumns),
    arrangement_id(Ref, Id).

arrangement_columns(log, _, Columns, [], Columns) :- !.
arrangement_columns(set, key(Positions), Columns, KeyColumns, ValueColumns) :-
    !,
    positions_columns(Positions, Columns, KeyColumns),
    subtract(Columns, KeyColumns, ValueColumns).
arrangement_columns(set, none, Columns, Columns, []).

positions_columns([], _, []).
positions_columns([Position | Rest], Columns, [Column | More]) :-
    nth1(Position, Columns, Column),
    positions_columns(Rest, Columns, More).

arrangement_id(Name/Arity, Id) :-
    format(atom(Id), 'arr_~w_~w', [Name, Arity]).

arrangement_terms(RelPlans, Rules, Arrangements) :-
    maplist(arrangement_term, RelPlans, SetArrangements),
    rule_arrangements(Rules, RelPlans, 1, OperatorArrangements),
    append(SetArrangements, OperatorArrangements, Arrangements).

rule_arrangements([], _, _, []).
rule_arrangements([Rule | Rest], RelPlans, Number, Arrangements) :-
    rule_arrangement_terms(Rule, RelPlans, Number, Current),
    Next is Number + 1,
    rule_arrangements(Rest, RelPlans, Next, More),
    append(Current, More, Arrangements).

rule_arrangement_terms(Rule, RelPlans, Number, Arrangements) :-
    rule_body_uses(Rule, Uses),
    include(positive_use, Uses, PositiveUses),
    join_arrangements(PositiveUses, Rule, RelPlans, Number, 1, JoinArrangements),
    reduce_arrangements(Rule, PositiveUses, RelPlans, Number, ReduceArrangements),
    append(JoinArrangements, ReduceArrangements, Arrangements).

join_arrangements([_], _, _, _, _, []) :- !.
join_arrangements([], _, _, _, _, []) :- !.
join_arrangements([Left | [Right | Rest]], Rule, RelPlans, Number, Index,
                  Arrangements) :-
    join_key_columns(Left, Right, Rule, RelPlans, LeftKeys, RightKeys),
    join_arrangement_id(Left, Number, Index, left, LeftId),
    join_arrangement_id(Right, Number, Index, right, RightId),
    arrangement_for_use(Left, RelPlans, LeftKeys, LeftId, LeftArrangement),
    arrangement_for_use(Right, RelPlans, RightKeys, RightId, RightArrangement),
    Next is Index + 1,
    join_arrangements([Right | Rest], Rule, RelPlans, Number, Next, More),
    append([LeftArrangement, RightArrangement], More, Arrangements).

join_arrangement_id(use(Ref, _, _, _), Number, Index, Side, Id) :-
    operator_id(join, Number-Index, JoinId),
    Ref = Name/Arity,
    format(atom(Id), 'arr_~w_~w_~w_~w', [Name, Arity, JoinId, Side]).

join_key_columns(use(LeftRef, LeftArgs, _, _), use(RightRef, RightArgs, _, _),
                 Rule, RelPlans, LeftKeys, RightKeys) :-
    rule_head_arguments(Rule, HeadArgs),
    shared_head_positions(LeftArgs, RightArgs, HeadArgs, LeftPositions, RightPositions),
    relplan_columns(RelPlans, LeftRef, LeftColumns),
    relplan_columns(RelPlans, RightRef, RightColumns),
    positions_columns(LeftPositions, LeftColumns, LeftKeys),
    positions_columns(RightPositions, RightColumns, RightKeys).

shared_head_positions(LeftArgs, RightArgs, HeadArgs, LeftPositions, RightPositions) :-
    findall(LeftPosition-RightPosition,
            ( nth1(LeftPosition, LeftArgs, Argument),
              nth1(RightPosition, RightArgs, OtherArgument),
              Argument == OtherArgument,
              member_same_variable(Argument, HeadArgs) ),
            Pairs),
    pairs_positions(Pairs, LeftPositions, RightPositions).

pairs_positions([], [], []).
pairs_positions([Left-Right | Rest], [Left | Lefts], [Right | Rights]) :-
    pairs_positions(Rest, Lefts, Rights).

member_same_variable(Argument, [Candidate | _]) :- Argument == Candidate, !.
member_same_variable(Argument, [_ | Rest]) :- member_same_variable(Argument, Rest).

arrangement_for_use(use(Ref, _, _, _), RelPlans, KeyColumns, Id,
                    arr(Id, Ref, KeyColumns, ValueColumns, signed)) :-
    relplan_columns(RelPlans, Ref, Columns),
    subtract(Columns, KeyColumns, ValueColumns).

reduce_arrangements(Rule, [Use | _], RelPlans, Number, [Arrangement]) :-
    rule_is_aggregate(Rule),
    !,
    aggregate_key_columns(Rule, Use, RelPlans, KeyColumns),
    operator_id(reduce, Number, ReduceId),
    Use = use(Ref, _, _, _),
    Ref = Name/Arity,
    format(atom(Id), 'arr_~w_~w_~w', [Name, Arity, ReduceId]),
    arrangement_for_use(Use, RelPlans, KeyColumns, Id, Arrangement).
reduce_arrangements(_, _, _, _, []).

aggregate_key_columns(Rule, use(Ref, Args, _, _), RelPlans, KeyColumns) :-
    rule_head_arguments(Rule, HeadArgs),
    aggregate_head_template_from_rule(Rule, Template),
    aggregate_plain_positions(Template, 1, PlainPositions),
    arguments_positions(PlainPositions, HeadArgs, Args, ArgumentPositions),
    relplan_columns(RelPlans, Ref, Columns),
    positions_columns(ArgumentPositions, Columns, KeyColumns).

aggregate_head_template_from_rule((Head <- _), Template) :-
    aggregate_head_template(Head, Template).

aggregate_plain_positions([], _, []).
aggregate_plain_positions([plain(_) | Rest], Position, [Position | Positions]) :-
    !,
    Next is Position + 1,
    aggregate_plain_positions(Rest, Next, Positions).
aggregate_plain_positions([_ | Rest], Position, Positions) :-
    Next is Position + 1,
    aggregate_plain_positions(Rest, Next, Positions).

arguments_positions([], _, _, []).
arguments_positions([HeadPosition | Rest], HeadArgs, Args, [ArgPosition | Positions]) :-
    nth1(HeadPosition, HeadArgs, Argument),
    argument_position(Argument, Args, ArgPosition),
    arguments_positions(Rest, HeadArgs, Args, Positions).

argument_position(Argument, [Candidate | _], 1) :- Argument == Candidate, !.
argument_position(Argument, [_ | Rest], Position) :-
    argument_position(Argument, Rest, Previous),
    Position is Previous + 1.

rule_head_arguments((Head <- _), Arguments) :- Head =.. [_ | Arguments].
rule_head_arguments((Head <+ _), Arguments) :- Head =.. [_ | Arguments].

rule_operators(Rules, Lowered, Operators) :-
    rule_operators(Rules, Lowered, 1, Operators).

rule_operators([], _, _, []).
rule_operators([Rule | Rest], Lowered, Number, Operators) :-
    rule_operator_terms(Rule, Lowered, Number, Current),
    Next is Number + 1,
    rule_operators(Rest, Lowered, Next, More),
    append(Current, More, Operators).

rule_operator_terms(Rule, Lowered, Number, Operators) :-
    rule_head_ref(Rule, HeadRef),
    rule_body_uses(Rule, Uses),
    operator_id(map, Number, MapId),
    operator_payload(Rule, HeadRef, Uses, Lowered, Sqlite),
    Map = op(MapId, map(HeadRef), Sqlite),
    Owner = sqlite(Refs, owner(MapId)),
    Sqlite = sqlite(Refs, _),
    join_operators(Uses, Number, Owner, Joins),
    filter_operators(Uses, Number, Owner, Filters),
    reduce_operators(Rule, Number, Owner, Reduces),
    iterate_operators(HeadRef, Uses, Number, Owner, Iterates),
    append([[Map], Joins, Filters, Reduces, Iterates], Operators).

operator_payload(_Rule, HeadRef, Uses,
                 lowered(_, _, _, EdgeStatements, LevelStatements, _, _, _),
                 sqlite(Refs, Statements)) :-
    operator_refs(HeadRef, Uses, Refs),
    (   findall(Statement,
                ( member(Statement, EdgeStatements),
                  Statement = edgestmt(HeadRef, _, _, _, _, _, _, _, _) ),
                Statements),
        Statements \== []
    ->  true
    ;   findall(Statement,
                ( member(Statement, LevelStatements),
                  Statement = levelstmt(HeadRef, _, _, _, _, _, _) ),
                Statements)
    ).

operator_refs(HeadRef, Uses, Refs) :-
    findall(Ref, member(use(Ref, _, _, _), Uses), UseRefs),
    sort([HeadRef | UseRefs], Refs).

rule_body_uses((_ <- Body), Uses) :- body_ref_uses(Body, Uses).
rule_body_uses((_ <+ Body), Uses) :- body_ref_uses(Body, Uses).

join_operators(Uses, Number, Payload, Operators) :-
    include(positive_use, Uses, PositiveUses),
    join_operator_terms(PositiveUses, Number, 1, Payload, Operators).

positive_use(use(_, _, pos, _)).

join_operator_terms([_], _, _, _, []) :- !.
join_operator_terms([], _, _, _, []) :- !.
join_operator_terms([LeftUse | [RightUse | Rest]], Number, Index, Payload,
                    [op(Id, join(Left, Right, LeftArrangement, RightArrangement), Payload) | More]) :-
    LeftUse = use(Left, _, _, _),
    RightUse = use(Right, _, _, _),
    operator_id(join, Number-Index, Id),
    join_arrangement_id(LeftUse, Number, Index, left, LeftArrangement),
    join_arrangement_id(RightUse, Number, Index, right, RightArrangement),
    Next is Index + 1,
    join_operator_terms([use(Right, [], pos, unmarked) | Rest], Number, Next, Payload, More).

filter_operators(Uses, Number, Payload, Operators) :-
    findall(op(Id, filter(Ref), Payload),
            ( nth1(Index, Uses, use(Ref, _, neg, _)),
              operator_id(filter, Number-Index, Id) ),
            Operators).

reduce_operators(Rule, Number, Payload, [op(Id, reduce(Arrangement), Payload)]) :-
    rule_is_aggregate(Rule),
    !,
    operator_id(reduce, Number, Id),
    rule_body_uses(Rule, Uses),
    include(positive_use, Uses, [Use | _]),
    Use = use(Ref, _, _, _),
    Ref = Name/Arity,
    format(atom(Arrangement), 'arr_~w_~w_~w', [Name, Arity, Id]).
reduce_operators(_, _, _, []).

iterate_operators(HeadRef, Uses, Number, Payload, [op(Id, iterate(HeadRef), Payload)]) :-
    member(use(HeadRef, _, pos, _), Uses),
    !,
    operator_id(iterate, Number, Id).
iterate_operators(_, _, _, _, []).

operator_id(Kind, Number, Id) :-
    number_atom(Number, Suffix),
    format(atom(Id), '~w_~w', [Kind, Suffix]).

number_atom(Left-Right, Atom) :- !, format(atom(Atom), '~w_~w', [Left, Right]).
number_atom(Number, Atom) :- format(atom(Atom), '~w', [Number]).

rule_wires(Rules, Wires) :-
    rule_wires(Rules, 1, Wires).

rule_wires([], _, []).
rule_wires([Rule | Rest], Number, Wires) :-
    rule_head_ref(Rule, HeadRef),
    rule_body_uses(Rule, Uses),
    rule_wires_for_operators(Rule, Uses, Number, HeadRef, Current),
    Next is Number + 1,
    rule_wires(Rest, Next, More),
    append(Current, More, Wires).

rule_wires_for_operators(Rule, Uses, Number, HeadRef, Wires) :-
    include(positive_use, Uses, PositiveUses),
    filter_operators(Uses, Number, none, FilterOperators),
    reduce_operators(Rule, Number, none, ReduceOperators),
    iterate_operators(HeadRef, Uses, Number, none, IterateOperators),
    operator_id(map, Number, MapId),
    join_operators(PositiveUses, Number, none, JoinOperators),
    append([FilterOperators, ReduceOperators, IterateOperators, [op(MapId, map(HeadRef), none)]],
           TailOperators),
    TailOperators = [op(FirstTailId, _, _) | _],
    wire_operator_inputs(PositiveUses, JoinOperators, FirstTailId, InputWires),
    operator_chain_wires(TailOperators, HeadRef, TailWires),
    negative_filter_wires(Uses, FilterOperators, NegativeWires),
    append([InputWires, TailWires, NegativeWires], Wires).

wire_operator_inputs([use(Ref, _, _, _)], [], FirstTailId,
                     [wire(Ref, FirstTailId, delta)]) :- !.
wire_operator_inputs([use(Left, _, _, _), use(Right, _, _, _) | Rest],
                     [op(JoinId, _, _) | JoinOperators], FirstTailId,
                     [wire(Left, JoinId, delta), wire(Right, JoinId, delta) | More]) :-
    wire_join_inputs(Rest, JoinId, JoinOperators, FirstTailId, More).
wire_operator_inputs([], [], _, []).

wire_join_inputs([], JoinId, [], FirstTailId, [wire(JoinId, FirstTailId, delta)]).
wire_join_inputs([use(Ref, _, _, _) | Rest], PreviousJoinId,
                 [op(JoinId, _, _) | JoinOperators], FirstTailId,
                 [wire(PreviousJoinId, JoinId, delta), wire(Ref, JoinId, delta) | More]) :-
    wire_join_inputs(Rest, JoinId, JoinOperators, FirstTailId, More).

operator_chain_wires([op(Id, _, _)], HeadRef, [wire(Id, HeadRef, delta)]).
operator_chain_wires([op(Id, _, _) | [op(NextId, _, _) | Rest]], HeadRef,
                     [wire(Id, NextId, delta) | More]) :-
    operator_chain_wires([op(NextId, _, _) | Rest], HeadRef, More).

negative_filter_wires(_, [], []).
negative_filter_wires(Uses, [op(FilterId, _, _) | _], Wires) :-
    findall(wire(Ref, FilterId, delta), member(use(Ref, _, neg, _), Uses), Wires).

tick_order([ phase(absorb_arrivals), phase(index_delta),
             phase(level_before_edges), phase(edge_arrivals),
             phase(edge_departures), phase(level_after_edges),
             phase(iterate), phase(consolidate), phase(retain),
             phase(boundary), phase(carry), phase(drain) ]).
