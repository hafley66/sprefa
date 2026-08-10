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
                              rule_is_aggregate/1]).
:- use_module('../0_rel_record', [relplan_parts/6]).
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
             lowered(Name, _, _, _, _, _, RelPlans, _),
             dd_plan(Name, rels(Rels), arrangements(Arrangements),
                     operators(Operators), wires(Wires), tick_order(TickOrder))) :-
    maplist(rel_term, RelPlans, Rels),
    maplist(arrangement_term, RelPlans, Arrangements),
    ordered_rules(RuleOrder, Rules, OrderedRules),
    rule_operators(OrderedRules, Operators),
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

rule_operators(Rules, Operators) :-
    rule_operators(Rules, 1, Operators).

rule_operators([], _, []).
rule_operators([Rule | Rest], Number, Operators) :-
    rule_operator_terms(Rule, Number, Current),
    Next is Number + 1,
    rule_operators(Rest, Next, More),
    append(Current, More, Operators).

rule_operator_terms(Rule, Number, Operators) :-
    rule_head_ref(Rule, HeadRef),
    rule_body_uses(Rule, Uses),
    operator_id(map, Number, MapId),
    Map = op(MapId, map(HeadRef)),
    join_operators(Uses, Number, Joins),
    filter_operators(Uses, Number, Filters),
    reduce_operators(Rule, Number, Reduces),
    iterate_operators(HeadRef, Uses, Number, Iterates),
    append([[Map], Joins, Filters, Reduces, Iterates], Operators).

rule_body_uses((_ <- Body), Uses) :- body_ref_uses(Body, Uses).
rule_body_uses((_ <+ Body), Uses) :- body_ref_uses(Body, Uses).

join_operators(Uses, Number, Operators) :-
    include(positive_use, Uses, PositiveUses),
    join_operator_terms(PositiveUses, Number, 1, Operators).

positive_use(use(_, _, pos, _)).

join_operator_terms([_], _, _, []) :- !.
join_operator_terms([], _, _, []) :- !.
join_operator_terms([use(Left, _, _, _) | [use(Right, _, _, _) | Rest]], Number,
                    Index, [op(Id, join(Left, Right)) | More]) :-
    operator_id(join, Number-Index, Id),
    Next is Index + 1,
    join_operator_terms([use(Right, [], pos, unmarked) | Rest], Number, Next, More).

filter_operators(Uses, Number, Operators) :-
    findall(op(Id, filter(Ref)),
            ( nth1(Index, Uses, use(Ref, _, neg, _)),
              operator_id(filter, Number-Index, Id) ),
            Operators).

reduce_operators(Rule, Number, [op(Id, reduce)]) :-
    rule_is_aggregate(Rule),
    !,
    operator_id(reduce, Number, Id).
reduce_operators(_, _, []).

iterate_operators(HeadRef, Uses, Number, [op(Id, iterate(HeadRef))]) :-
    member(use(HeadRef, _, pos, _), Uses),
    !,
    operator_id(iterate, Number, Id).
iterate_operators(_, _, _, []).

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
    operator_id(map, Number, MapId),
    findall(wire(Ref, MapId, delta), member(use(Ref, _, _, _), Uses), Inputs),
    append(Inputs, [wire(MapId, HeadRef, delta)], Current),
    Next is Number + 1,
    rule_wires(Rest, Next, More),
    append(Current, More, Wires).

tick_order([ phase(absorb_arrivals), phase(index_delta),
             phase(level_before_edges), phase(edge_arrivals),
             phase(edge_departures), phase(level_after_edges),
             phase(iterate), phase(consolidate), phase(retain),
             phase(boundary), phase(carry), phase(drain) ]).
