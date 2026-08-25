% Emit a deterministic target-neutral DD plan term.
:- module(isolated_compiler_dd,
          [ compile_program/5,
            write_dd_plan/2,
            dd_plan_text/2,
            fixture_dd_plan_text/3,
            fixture_dd_plan_json_text/3,
            write_fixture_dd_plan_json/3
          ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

:- use_module('../compile', [read_fixture_term/4, program_plan/2, dd_compile_context/2]).
:- use_module('../next/2_lower/analyze', [body_ref_uses/2, rule_head_ref/2,
                              rule_is_aggregate/1,
                              aggregate_head_template/2]).
:- use_module('../strat', [recursive_stratum_groups/2]).
:- use_module('../0_rel_record', [relplan_parts/6, relplan_columns/3]).
:- use_module('../lower', [lower_program/2]).
:- use_module(library(http/json), [json_write_dict/3]).

fixture_dd_plan_text(FixtureFile, Name, Text) :-
    once(( read_fixture_term(FixtureFile, Name, Term, Bindings),
           program_plan(Term-Bindings, Plan),
           dd_plan_text(Plan, Text) )).

write_fixture_dd_plan_json(FixtureFile, Name, Path) :-
    fixture_dd_plan_json_text(FixtureFile, Name, Text),
    setup_call_cleanup(open(Path, write, Stream),
                       format(Stream, '~s', [Text]),
                       close(Stream)).

fixture_dd_plan_json_text(FixtureFile, Name, Text) :-
    once(( read_fixture_term(FixtureFile, Name,
                             fixture(Name, Program, Initial, Schedule, Expected), Bindings),
           program_plan(fixture(Name, Program, Initial, Schedule, Expected)-Bindings, Plan),
           lower_program(Plan, Lowered),
           dd_plan_term(Plan, Lowered, DdPlan),
           dd_plan_json_dict(Plan, Lowered, DdPlan, Initial, Schedule, Dict),
           with_output_to(string(Text),
                          ( json_write_dict(current_output, Dict, [width(0)]), nl )) )).

write_dd_plan(Plan, Path) :-
    dd_plan_text(Plan, Text),
    setup_call_cleanup(open(Path, write, Stream),
                       format(Stream, '~s', [Text]),
                       close(Stream)).

% The seam entry: compile_program/5, the same shape emit_ts:emit_program uses, so
% the text door routes to this module with no call-site special case.
% BootStatements is emit_ts's boot shape; the dd plan embeds its own initial
% rows, so that argument is taken and ignored. Initial and Schedule are not
% among the seam's five arguments, so compile.pl hands them over out of band
% through dd_compile_context/2; when the door passes none they stay empty.
compile_program(_Name, Plan, Lowered, _BootStatements, Text) :-
    (   dd_compile_context(Initial, Schedule)
    ->  true
    ;   Initial = [], Schedule = []
    ),
    dd_plan_term(Plan, Lowered, DdPlan),
    dd_plan_json_dict(Plan, Lowered, DdPlan, Initial, Schedule, Dict),
    with_output_to(string(Text),
                   ( json_write_dict(current_output, Dict, [width(0)]), nl )).

dd_plan_text(Plan, Text) :-
    lower_program(Plan, Lowered),
    dd_plan_term(Plan, Lowered, DdPlan),
    with_output_to(string(Text),
                   write_term(DdPlan, [quoted(true), numbervars(true),
                                       fullstop(true), nl(true)])).

dd_plan_json_dict(Plan,
                  lowered(_, Ddl, _, _, _LevelStatements, DeltaStatements, _, _),
                  dd_plan(Name, rels(Rels), arrangements(Arrangements),
                          operators(Operators), wires(Wires), tick_order(TickOrder)),
                  Initial, Schedule,
                  _{name:NameText, ddl:Ddl, rels:JsonRels,
                    arrangements:JsonArrangements, rules:Rules,
                    operators:JsonOperators, wires:JsonWires,
                    initial:JsonInitial, schedule:JsonSchedule,
                    tick_order:JsonTickOrder}) :-
    atom_string(Name, NameText),
    ordered_rules_for_json(Plan, OrderedRules),
    maplist(json_rel(DeltaStatements), Rels, JsonRels),
    maplist(json_arrangement, Arrangements, JsonArrangements),
    maplist(json_rule, Operators, Rules0),
    exclude(=(none), Rules0, Rules),
    maplist(json_operator(OrderedRules, Arrangements, Operators),
            Operators, JsonOperators),
    maplist(json_wire, Wires, JsonWires),
    maplist(json_row, Initial, JsonInitial),
    maplist(json_tick, Schedule, JsonSchedule),
    maplist(json_phase, TickOrder, JsonTickOrder).
ordered_rules_for_json(plan(_, prog(_, ProgramRules), _, _, _, RuleOrder, _, _, _),
                       OrderedRules) :-
    ordered_rules(RuleOrder, ProgramRules, OrderedRules).

json_phase(phase(Phase), Text) :- atom_string(Phase, Text).

json_rel(DeltaStatements, rel(Ref, Columns, _),
         _{name:Name, columns:Columns, select_all:SelectAll}) :-
    ref_name(Ref, Name),
    member(deltastmt(Ref, SelectAll, _, _, _), DeltaStatements).

json_arrangement(arr(Id, Ref, KeyColumns, ValueColumns, signed),
                 _{id:Id, rel:Name, key_columns:KeyColumns,
                   value_columns:ValueColumns}) :-
    ref_name(Ref, Name).

json_wire(wire(From, To, Delta),
          _{from:FromText, to:ToText, kind:DeltaText}) :-
    node_string(From, FromText),
    node_string(To, ToText),
    atom_string(Delta, DeltaText).

node_string(Node, String) :-
    (   compound(Node)
    ->  ref_name(Node, String)
    ;   atom_string(Node, String)
    ).

% B1-replay projection: map-owned level rules only, the exact shape the
% dd-runner consumes (id/head/delete/inserts); non-map/edge maps are excluded.
json_rule(op(Id, map(Ref), sqlite(_, Statements), _),
          _{id:IdText, head:Head, delete:Delete, inserts:Inserts}) :-
    member(levelstmt(Ref, Delete, Inserts, _, _, _, _), Statements),
    atom_string(Id, IdText),
    ref_name(Ref, Head),
    !.
json_rule(_, none).

% Full operator description for the kernel twin: kind, writes-head, input refs,
% edge/level classification, per-kind details (join keys, reduce aggregate), and
% for every op that reads rels the bindings / predicates / projection semantics.
json_operator(_OrderedRules, _Arrangements, _Operators,
              op(Id, map(Ref), sqlite(Refs, Statements), Semantics), Dict) :-
    atom_string(Id, IdText),
    ref_name(Ref, Head),
    subtract(Refs, [Ref], Removed),
    ref_name_list(Removed, InputRefs),
    op_semantics_json(Semantics, SemanticsJson),
    (   member(levelstmt(Ref, Delete, Inserts, _, _, _, _), Statements)
    ->  Dict = _{id:IdText, kind:map, head:Head, refs:InputRefs,
                 classification:level, delete:Delete, inserts:Inserts,
                 bindings:SemanticsJson.bindings,
                 predicates:SemanticsJson.predicates,
                 projection:SemanticsJson.projection}
    ;   member(edgestmt(Ref, TriggerRef, HeadColumns, KeyColumns,
                        ProjectSql, WriteSql, DeltaProjectSql,
                        TriggerKind, _EdgeInterns), Statements),
        ref_name(TriggerRef, Trigger),
        Dict = _{id:IdText, kind:map, head:Head, refs:InputRefs,
                 classification:edge, trigger:Trigger,
                 head_columns:HeadColumns, key_columns:KeyColumns,
                 project_sql:ProjectSql, write_sql:WriteSql,
                 delta_project_sql:DeltaProjectSql, trigger_kind:TriggerKind,
                 bindings:SemanticsJson.bindings,
                 predicates:SemanticsJson.predicates,
                 projection:SemanticsJson.projection}
    ).
json_operator(_OrderedRules, Arrangements, Operators,
              op(Id, join(Left, Right, LeftArr, RightArr), sqlite(_Refs, Payload),
                 Semantics), Dict) :-
    atom_string(Id, IdText),
    operator_owner_meta(Payload, Operators, HeadRef, Classification),
    ref_name(HeadRef, Head),
    ref_name(Left, LeftName),
    ref_name(Right, RightName),
    arrangement_dict(Arrangements, LeftArr, LeftDict),
    arrangement_dict(Arrangements, RightArr, RightDict),
    op_semantics_json(Semantics, SemanticsJson),
    Dict = _{id:IdText, kind:join, head:Head,
             refs:[LeftName, RightName],
             arrangement:[LeftArr, RightArr],
             join_keys:_{left:LeftDict, right:RightDict},
             constraints:_{left:LeftDict, right:RightDict},
             classification:Classification,
             bindings:SemanticsJson.bindings,
             predicates:SemanticsJson.predicates,
             projection:SemanticsJson.projection}.
json_operator(OrderedRules, Arrangements, Operators,
              op(Id, reduce(Arrangement), sqlite(Refs, Payload), Semantics),
              Dict) :-
    atom_string(Id, IdText),
    operator_owner_meta(Payload, Operators, HeadRef, Classification),
    ref_name(HeadRef, Head),
    subtract(Refs, [HeadRef], Scoped),
    ref_name_list(Scoped, InputRefs),
    Semantics = semantics(Bindings, _, _),
    reduce_arrangement_dict(Arrangements, Arrangement, ArrangementRef, GroupCols, ValueCols),
    binding_alias(Bindings, ArrangementRef, GroupAlias),
    qualify_columns(GroupAlias, GroupCols, GroupColumns),
    reduce_aggregate(Id, OrderedRules, AggregateKinds),
    op_semantics_json(Semantics, SemanticsJson),
    Dict = _{id:IdText, kind:reduce, head:Head, refs:InputRefs,
             arrangement:Arrangement,
             aggregate:_{kind:AggregateKinds, group:GroupColumns, value:ValueCols},
             classification:Classification,
             bindings:SemanticsJson.bindings,
             predicates:SemanticsJson.predicates,
             projection:SemanticsJson.projection}.
json_operator(_OrderedRules, _Arrangements, Operators,
              op(Id, filter(Ref), sqlite(_Refs, Payload), Semantics), Dict) :-
    atom_string(Id, IdText),
    operator_owner_meta(Payload, Operators, HeadRef, Classification),
    ref_name(HeadRef, Head),
    ref_name(Ref, Filtered),
    op_semantics_json(Semantics, SemanticsJson),
    Dict = _{id:IdText, kind:filter, head:Head, refs:[Filtered],
             classification:Classification,
             bindings:SemanticsJson.bindings,
             predicates:SemanticsJson.predicates,
             projection:SemanticsJson.projection}.
json_operator(_OrderedRules, _Arrangements, Operators,
              op(Id, iterate(Refs), sqlite(_Refs, Payload), Semantics), Dict) :-
    atom_string(Id, IdText),
    operator_owner_meta(Payload, Operators, HeadRef, Classification),
    ref_name(HeadRef, Head),
    iterate_ref_names(Refs, Iterated),
    op_semantics_json(Semantics, SemanticsJson),
    Dict = _{id:IdText, kind:iterate, head:Head, refs:Iterated,
             classification:Classification,
             bindings:SemanticsJson.bindings,
             predicates:SemanticsJson.predicates,
             projection:SemanticsJson.projection}.

iterate_ref_names(Refs, Names) :-
    is_list(Refs),
    !,
    ref_name_list(Refs, Names).
iterate_ref_names(Ref, [Name]) :- ref_name(Ref, Name).

op_semantics_json(semantics(Bindings, Predicates, Projection), Dict) :-
    bindings_json(Bindings, BindingsJson),
    predicates_json(Predicates, PredicatesJson),
    projection_json(Projection, ProjectionJson),
    Dict = _{bindings:BindingsJson, predicates:PredicatesJson,
             projection:ProjectionJson}.

bindings_json([], Dict) :- Dict = _{}.
bindings_json([binding(Alias, Ref) | Rest], Dict) :-
    ref_name(Ref, Name),
    bindings_json(Rest, RestDict),
    put_dict(Alias, RestDict, Name, Dict).

predicates_json([], []).
predicates_json([Predicate | Rest], [Json | More]) :-
    predicate_json(Predicate, Json),
    predicates_json(Rest, More).

predicate_json(eq(col(LeftAlias, LeftColumn), col(RightAlias, RightColumn)), Json) :-
    col_text(LeftAlias, LeftColumn, LeftText),
    col_text(RightAlias, RightColumn, RightText),
    Json = _{column_equals:[LeftText, RightText]}.
predicate_json(eq_lit(col(Alias, Column), Literal), Json) :-
    col_text(Alias, Column, ColumnText),
    Json = _{literal_equals:_{column:ColumnText, value:Literal}}.

col_text(Alias, Column, Text) :- format(atom(Text), '~w.~w', [Alias, Column]).

projection_json([], []).
projection_json([Projection | Rest], [Json | More]) :-
    projection_one_json(Projection, Json),
    projection_json(Rest, More).

projection_one_json(proj(HeadColumn, col(Alias, Column)), Json) :-
    col_text(Alias, Column, SourceText),
    Json = _{head:HeadColumn, source:SourceText}.
projection_one_json(proj_value(HeadColumn, Arg), Json) :-
    render_plain(Arg, ArgText),
    Json = _{head:HeadColumn, value:ArgText}.

render_plain(Term, Text) :-
    ( atomic(Term) -> Text = Term ; format(atom(Text), '~w', [Term]) ).

operator_owner_meta(owner(MapId), Operators, HeadRef, Classification) :-
    member(op(MapId, map(HeadRef), sqlite(_, OwnerStatements), _), Operators),
    statement_classification(OwnerStatements, Classification).

statement_classification(Statements, level) :-
    member(levelstmt(_, _, _, _, _, _, _), Statements), !.
statement_classification(Statements, edge) :-
    member(edgestmt(_, _, _, _, _, _, _, _, _), Statements).

arrangement_dict(Arrangements, ArrId,
                 _{id:ArrId, rel:Name, key_columns:KeyColumns,
                   value_columns:ValueColumns}) :-
    member(arr(ArrId, Ref, KeyColumns, ValueColumns, signed), Arrangements),
    ref_name(Ref, Name).

reduce_arrangement_dict(Arrangements, ArrId, Ref, GroupCols, ValueCols) :-
    member(arr(ArrId, Ref, GroupCols, ValueCols, signed), Arrangements).

binding_alias([binding(Alias, Ref) | _], Ref, Alias) :- !.
binding_alias([_ | Rest], Ref, Alias) :- binding_alias(Rest, Ref, Alias).

qualify_columns(_, [], []).
qualify_columns(Alias, [Column | Rest], [Text | More]) :-
    col_text(Alias, Column, Text),
    qualify_columns(Alias, Rest, More).

reduce_aggregate(ReduceId, OrderedRules, Kinds) :-
    reduce_number(ReduceId, Number),
    nth1(Number, OrderedRules, Rule),
    aggregate_head_template_from_rule(Rule, Template),
    aggregate_kinds(Template, Kinds).

reduce_number(ReduceId, Number) :-
    atom_string(ReduceId, String),
    string_concat("reduce_", NumberString, String),
    number_string(Number, NumberString).

aggregate_kinds([], []).
aggregate_kinds([agg(Kind, _) | Rest], [Kind | Kinds]) :- aggregate_kinds(Rest, Kinds).
aggregate_kinds([plain(_) | Rest], Kinds) :- aggregate_kinds(Rest, Kinds).

json_tick(Rows, JsonRows) :- maplist(json_signed_row, Rows, JsonRows).

json_signed_row(+Row, Dict) :- json_row(Row, RowDict), put_dict(sign, RowDict, 1, Dict).
json_signed_row(-Row, Dict) :- json_row(Row, RowDict), put_dict(sign, RowDict, -1, Dict).

json_row(Row, _{rel:Name, values:Args}) :-
    Row =.. [Functor | Args],
    functor(Row, Functor, Arity),
    format(atom(Name), '~w/~w', [Functor, Arity]).

ref_name(Functor/Arity, Name) :- format(atom(Name), '~w/~w', [Functor, Arity]).

ref_name_list(Refs, Names) :- maplist(ref_name, Refs, Names).

dd_plan_term(plan(Name, prog(_, Rules), _, RelPlans, _, RuleOrder, _, _, _),
             Lowered,
             dd_plan(Name, rels(Rels), arrangements(Arrangements),
                     operators(Operators), wires(Wires), tick_order(TickOrder))) :-
    maplist(rel_term, RelPlans, Rels),
    ordered_rules(RuleOrder, Rules, OrderedRules),
    arrangement_terms(RelPlans, OrderedRules, Arrangements),
    rule_operators(OrderedRules, OrderedRules, Lowered, RelPlans, Operators),
    rule_wires(OrderedRules, OrderedRules, Wires),
    tick_order(TickOrder).

ordered_rules([], Rules, Rules) :-
    !.
ordered_rules([Rule | Rest], Rules, [Rule | Ordered]) :-
    !,
    ordered_nonempty_rules(Rest, Rules, Ordered).
ordered_rules(_, Rules, Rules).

ordered_nonempty_rules([], _, []).
ordered_nonempty_rules([Rule | Rest], Rules, [Rule | Ordered]) :-
    ordered_nonempty_rules(Rest, Rules, Ordered).

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
                 _Rule, RelPlans, LeftKeys, RightKeys) :-
    shared_head_positions(LeftArgs, RightArgs, LeftPositions, RightPositions),
    relplan_columns(RelPlans, LeftRef, LeftColumns),
    relplan_columns(RelPlans, RightRef, RightColumns),
    positions_columns(LeftPositions, LeftColumns, LeftKeys),
    positions_columns(RightPositions, RightColumns, RightKeys).

shared_head_positions(LeftArgs, RightArgs, LeftPositions, RightPositions) :-
    findall(LeftPosition-RightPosition,
            ( nth1(LeftPosition, LeftArgs, Argument),
              nth1(RightPosition, RightArgs, OtherArgument),
              Argument == OtherArgument ),
            Pairs),
    pairs_positions(Pairs, LeftPositions, RightPositions).

pairs_positions([], [], []).
pairs_positions([Left-Right | Rest], [Left | Lefts], [Right | Rights]) :-
    pairs_positions(Rest, Lefts, Rights).

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

rule_operators(Rules, AllRules, Lowered, RelPlans, Operators) :-
    rule_operators(Rules, AllRules, Lowered, RelPlans, 1, Operators).

rule_operators([], _, _, _, _, []).
rule_operators([Rule | Rest], AllRules, Lowered, RelPlans, Number, Operators) :-
    rule_operator_terms(Rule, AllRules, Lowered, RelPlans, Number, Current),
    Next is Number + 1,
    rule_operators(Rest, AllRules, Lowered, RelPlans, Next, More),
    append(Current, More, Operators).

rule_operator_terms(Rule, AllRules, Lowered, RelPlans, Number, Operators) :-
    rule_head_ref(Rule, HeadRef),
    rule_body_uses(Rule, Uses),
    include(positive_use, Uses, PositiveUses),
    operator_semantics(Rule, HeadRef, PositiveUses, RelPlans,
                       Bindings, Predicates, MapProjection, ReduceProjection),
    operator_id(map, Number, MapId),
    operator_payload(Rule, HeadRef, Uses, Lowered, Sqlite),
    Map = op(MapId, map(HeadRef), Sqlite,
             semantics(Bindings, Predicates, MapProjection)),
    Owner = sqlite(Refs, owner(MapId)),
    Sqlite = sqlite(Refs, _),
    join_operators(Number, Owner, PositiveUses,
                   Bindings, Predicates, MapProjection, Joins),
    filter_operators(Uses, Number, Owner, Bindings, Predicates, Filters),
    reduce_operators(Rule, Number, Owner, Bindings, Predicates,
                     ReduceProjection, Reduces),
    iterate_operators(HeadRef, Uses, AllRules, Number, Owner, Bindings, Predicates,
                      Iterates),
    append([[Map], Joins, Filters, Reduces, Iterates], Operators).

operator_semantics(Rule, HeadRef, PositiveUses, RelPlans,
                   Bindings, Predicates, MapProjection, ReduceProjection) :-
    positive_bindings(PositiveUses, Bindings),
    positive_occurrences(PositiveUses, RelPlans, Occurrences),
    predicates_from_occurrences(Occurrences, [], [], Predicates),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    reduce_projection(Rule, HeadRef, RelPlans, Occurrences, ReduceProjection),
    ( ReduceProjection == []
    -> rule_head_arguments(Rule, HeadArgs),
       head_projection(HeadArgs, HeadColumns, Occurrences, MapProjection)
    ;  MapProjection = ReduceProjection
    ).

% The numbered aliases the level SQL gives its FROM sources: b0, b1, ... in
% positive body-atom order, mirroring lower.pl:compile_positive_uses/7.
positive_bindings(PositiveUses, Bindings) :-
    positive_bindings(PositiveUses, 0, Bindings).
positive_bindings([], _, []).
positive_bindings([use(Ref, _, pos, _) | Rest], Index,
                  [binding(Alias, Ref) | MoreBindings]) :-
    format(atom(Alias), 'b~w', [Index]),
    Next is Index + 1,
    positive_bindings(Rest, Next, MoreBindings).

% Every (variable | literal, aliased-column) pair a positive body atom reads,
% linearized left to right. Lowering binds exactly these into the FROM scope.
positive_occurrences(PositiveUses, RelPlans, Occurrences) :-
    positive_occurrences(PositiveUses, RelPlans, 0, Occurrences).
positive_occurrences([], _, _, []).
positive_occurrences([use(Ref, Args, pos, _) | Rest], RelPlans, Index, Occs) :-
    format(atom(Alias), 'b~w', [Index]),
    relplan_columns(RelPlans, Ref, Columns),
    args_occurrences(Args, Columns, Alias, HereOccs),
    Next is Index + 1,
    positive_occurrences(Rest, RelPlans, Next, MoreOccs),
    append(HereOccs, MoreOccs, Occs).

args_occurrences([], _, _, []).
args_occurrences([Arg | RestArgs], [Column | RestColumns], Alias,
                 [Arg-col(Alias, Column) | More]) :-
    args_occurrences(RestArgs, RestColumns, Alias, More).

% column-to-column equality for every SHARED body variable (its later columns
% fold onto its first occurrence) and a column-to-literal for every atomic
% argument. The result is the same data the SQL WHERE makes textual; here it is
% structured, and it is never SQL text. Occurrences are matched by VARIABLE
% IDENTITY (==), never unification, so two distinct unbound vars never collapse.
predicates_from_occurrences([], _, Acc, Predicates) :-
    reverse(Acc, Predicates).
predicates_from_occurrences([Arg-col(Alias, Column) | Rest], Seen, Acc, Predicates) :-
    ( var(Arg)
    -> ( seen_first_col(Arg, Seen, FirstCol)
       -> NextAcc = [eq(FirstCol, col(Alias, Column)) | Acc],
          NextSeen = Seen
       ;  NextAcc = Acc, NextSeen = [Arg-col(Alias, Column) | Seen] )
    ;  NextAcc = [eq_lit(col(Alias, Column), Arg) | Acc], NextSeen = Seen ),
    predicates_from_occurrences(Rest, NextSeen, NextAcc, Predicates).

% First stored occurrence column whose key is the SAME variable instance.
seen_first_col(Arg, [Key-Col | Rest], FirstCol) :-
    ( Key == Arg -> FirstCol = Col ; seen_first_col(Arg, Rest, FirstCol) ).

% Head projection: one row per head column, head column <- the source binding
% column that supplies it. A simple variable head argument resolves to its
% binding column (again by variable identity); a computed/literal head argument
% is carried as proj_value.
head_projection([], [], _, []).
head_projection([Arg | RestArgs], [HeadColumn | RestColumns], Occurrences,
                [Projection | RestProjection]) :-
    ( var(Arg),
      seen_first_col(Arg, Occurrences, FirstCol)
    -> Projection = proj(HeadColumn, FirstCol)
    ;  Projection = proj_value(HeadColumn, Arg)
    ),
    head_projection(RestArgs, RestColumns, Occurrences, RestProjection).

% The reduce result row projects the aggregate head's plain (group) and agg
% (value) arguments against the same source bindings; a non-aggregate rule has
% no reduce op and so no reduce projection.
reduce_projection(Rule, HeadRef, RelPlans, Occurrences, Projection) :-
    rule_is_aggregate(Rule),
    !,
    aggregate_head_template_from_rule(Rule, Template),
    aggregate_projection_args(Template, Args),
    relplan_columns(RelPlans, HeadRef, HeadColumns),
    head_projection(Args, HeadColumns, Occurrences, Projection).
reduce_projection(_, _, _, _, []).

aggregate_projection_args(Template, Args) :-
    maplist(template_arg, Template, Args).
template_arg(plain(Expr), Expr).
template_arg(agg(_, Expr), Expr).

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

join_operators(Number, Payload, PositiveUses,
               Bindings, Predicates, Projection, Operators) :-
    join_operator_terms(PositiveUses, Number, 1, Payload,
                        Bindings, Predicates, Projection, Operators).

positive_use(use(_, _, pos, _)).

join_operator_terms([_], _, _, _, _, _, _, []) :- !.
join_operator_terms([], _, _, _, _, _, _, []) :- !.
join_operator_terms([LeftUse | [RightUse | Rest]], Number, Index, Payload,
                    Bindings, Predicates, Projection,
                    [op(Id, join(Left, Right, LeftArrangement, RightArrangement), Payload,
                        semantics(Bindings, Predicates, Projection)) | More]) :-
    LeftUse = use(Left, _, _, _),
    RightUse = use(Right, _, _, _),
    operator_id(join, Number-Index, Id),
    join_arrangement_id(LeftUse, Number, Index, left, LeftArrangement),
    join_arrangement_id(RightUse, Number, Index, right, RightArrangement),
    Next is Index + 1,
    join_operator_terms([use(Right, [], pos, unmarked) | Rest], Number, Next, Payload,
                        Bindings, Predicates, Projection, More).

filter_operators(Uses, Number, Payload, Bindings, Predicates, Operators) :-
    findall(op(Id, filter(Ref), Payload, semantics(Bindings, Predicates, [])),
            ( nth1(Index, Uses, use(Ref, _, neg, _)),
              operator_id(filter, Number-Index, Id) ),
            Operators).

reduce_operators(Rule, Number, Payload, Bindings, Predicates,
                 Projection, [op(Id, reduce(Arrangement), Payload,
                                  semantics(Bindings, Predicates, Projection))]) :-
    rule_is_aggregate(Rule),
    !,
    operator_id(reduce, Number, Id),
    rule_body_uses(Rule, Uses),
    include(positive_use, Uses, [Use | _]),
    Use = use(Ref, _, _, _),
    Ref = Name/Arity,
    format(atom(Arrangement), 'arr_~w_~w_~w', [Name, Arity, Id]).
reduce_operators(_, _, _, _, _, _, []).

iterate_operators(HeadRef, _Uses, AllRules, Number, Payload, Bindings, Predicates,
                  [op(Id, iterate(HeadRefs), Payload,
                      semantics(Bindings, Predicates, []))]) :-
    recursive_stratum_groups(AllRules, RecursiveGroups),
    member(Group, RecursiveGroups),
    findall(Ref, ( member(Rule, Group), rule_head_ref(Rule, Ref) ), HeadRefs0),
    sort(HeadRefs0, HeadRefs),
    HeadRefs = [HeadRef | _],
    !,
    operator_id(iterate, Number, Id).
iterate_operators(HeadRef, Uses, _, Number, Payload, Bindings, Predicates,
                  [op(Id, iterate(HeadRef), Payload,
                      semantics(Bindings, Predicates, []))]) :-
    member(use(HeadRef, _, pos, _), Uses),
    !,
    operator_id(iterate, Number, Id).
iterate_operators(_, _, _, _, _, _, _, []).

operator_id(Kind, Number, Id) :-
    number_atom(Number, Suffix),
    format(atom(Id), '~w_~w', [Kind, Suffix]).

number_atom(Left-Right, Atom) :- !, format(atom(Atom), '~w_~w', [Left, Right]).
number_atom(Number, Atom) :- format(atom(Atom), '~w', [Number]).

rule_wires(Rules, AllRules, Wires) :-
    rule_wires(Rules, AllRules, 1, Wires).

rule_wires([], _, _, []).
rule_wires([Rule | Rest], AllRules, Number, Wires) :-
    rule_head_ref(Rule, HeadRef),
    rule_body_uses(Rule, Uses),
    rule_wires_for_operators(Rule, Uses, AllRules, Number, HeadRef, Current),
    Next is Number + 1,
    rule_wires(Rest, AllRules, Next, More),
    append(Current, More, Wires).

rule_wires_for_operators(Rule, Uses, AllRules, Number, HeadRef, Wires) :-
    include(positive_use, Uses, PositiveUses),
    filter_operators(Uses, Number, none, [], [], FilterOperators),
    reduce_operators(Rule, Number, none, [], [], [], ReduceOperators),
    iterate_operators(HeadRef, Uses, AllRules, Number, none, [], [], IterateOperators),
    operator_id(map, Number, MapId),
    join_operators(Number, none, PositiveUses, [], [], [], JoinOperators),
    append([FilterOperators, ReduceOperators, IterateOperators,
            [op(MapId, map(HeadRef), none, none)]],
           TailOperators),
    TailOperators = [op(FirstTailId, _, _, _) | _],
    wire_operator_inputs(PositiveUses, JoinOperators, FirstTailId, InputWires),
    operator_chain_wires(TailOperators, HeadRef, TailWires),
    negative_filter_wires(Uses, FilterOperators, NegativeWires),
    append([InputWires, TailWires, NegativeWires], Wires).

wire_operator_inputs([use(Ref, _, _, _)], [], FirstTailId,
                     [wire(Ref, FirstTailId, delta)]) :- !.
wire_operator_inputs([use(Left, _, _, _), use(Right, _, _, _) | Rest],
                     [op(JoinId, _, _, _) | JoinOperators], FirstTailId,
                     [wire(Left, JoinId, delta), wire(Right, JoinId, delta) | More]) :-
    wire_join_inputs(Rest, JoinId, JoinOperators, FirstTailId, More).
wire_operator_inputs([], [], _, []).

wire_join_inputs([], JoinId, [], FirstTailId, [wire(JoinId, FirstTailId, delta)]).
wire_join_inputs([use(Ref, _, _, _) | Rest], PreviousJoinId,
                 [op(JoinId, _, _, _) | JoinOperators], FirstTailId,
                 [wire(PreviousJoinId, JoinId, delta), wire(Ref, JoinId, delta) | More]) :-
    wire_join_inputs(Rest, JoinId, JoinOperators, FirstTailId, More).

operator_chain_wires([op(Id, _, _, _)], HeadRef, [wire(Id, HeadRef, delta)]).
operator_chain_wires([op(Id, _, _, _) | [op(NextId, _, _, _) | Rest]], HeadRef,
                     [wire(Id, NextId, delta) | More]) :-
    operator_chain_wires([op(NextId, _, _, _) | Rest], HeadRef, More).

negative_filter_wires(_, [], []).
negative_filter_wires(Uses, [op(FilterId, _, _, _) | _], Wires) :-
    findall(wire(Ref, FilterId, delta), member(use(Ref, _, neg, _), Uses), Wires).

tick_order([ phase(absorb_arrivals), phase(index_delta),
             phase(level_before_edges), phase(edge_arrivals),
             phase(edge_departures), phase(level_after_edges),
             phase(iterate), phase(consolidate), phase(retain),
             phase(boundary), phase(carry), phase(drain) ]).
