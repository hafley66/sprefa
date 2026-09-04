:- module(dl7_logical_program_grapher,
          [ logical_program_graph_rows/2,
            logical_program_graph_calls/2,
            logical_program_graph_calls/3
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module('0_logical_program_reifier', [logical_program_rows/2]).

%% logical_program_graph_rows(+CheckedProgram, -GraphRows) is det.
%
% Normalize checked rule occurrences into ordinary node/product/: rows while
% retaining the existing logical-program relation view as the source of truth.
% Head and body edges point to distinct occurrence nodes, so later clock,
% trigger, read, and write annotations have an owner without changing calls.
logical_program_graph_rows(CheckedProgram, GraphRows) :-
    must_be(ground, CheckedProgram),
    logical_program_rows(CheckedProgram, Rows),
    findall(Row, logical_graph_row(Rows, Row), Rows0),
    sort(Rows0, GraphRows).

%% logical_program_graph_calls(+CheckedProgram, -Calls) is det.
logical_program_graph_calls(CheckedProgram, Calls) :-
    logical_program_graph_calls(CheckedProgram, all, Calls).

%% logical_program_graph_calls(+CheckedProgram, +Relations, -Calls) is det.
%
% Convert the normalized view to kernel calls, optionally retaining only the
% kernel relations in one emitter dependency cone.
logical_program_graph_calls(CheckedProgram, Relations, Calls) :-
    logical_program_graph_rows(CheckedProgram, Rows),
    findall(Call,
            ( member(Row, Rows),
              graph_row_call(Row, Relation, Call),
              requested_relation(Relations, Relation)
            ),
            Calls0),
    sort(Calls0, Calls).

graph_row_call(node(Id), kernel(node),
               call(ref(kernel(node)), [ref(Id)])).
graph_row_call(product(Id), kernel(product),
               call(ref(kernel(product)), [ref(Id)])).
graph_row_call(':'(Owner, Label, Target, Index), kernel(':'),
               call(ref(kernel(':')),
                    [ref(Owner), const(Label), Target, const(Index)])).

requested_relation(all, _) :- !.
requested_relation(Relations, Relation) :- memberchk(Relation, Relations).

logical_graph_row(Rows, node(Id)) :- occurrence_id(Rows, Id).
logical_graph_row(Rows, product(Id)) :- occurrence_id(Rows, Id).

logical_graph_row(Rows, ':'(Rule, head, ref(HeadCall), 0)) :-
    member(program_rule(RuleId, HeadCallId), Rows),
    logical_id(RuleId, Rule),
    logical_id(HeadCallId, HeadCall).
logical_graph_row(Rows, ':'(Rule, body, ref(Goal), EdgeIndex)) :-
    member(program_goal(RuleId, Position, _, _), Rows),
    logical_id(RuleId, Rule),
    goal_id(RuleId, Position, Goal),
    EdgeIndex is Position + 1.
logical_graph_row(Rows, ':'(Goal, polarity, const(Polarity), 0)) :-
    member(program_goal(RuleId, Position, Polarity, _), Rows),
    goal_id(RuleId, Position, Goal).
logical_graph_row(Rows, ':'(Goal, call, ref(Call), 1)) :-
    member(program_goal(RuleId, Position, _, CallId), Rows),
    goal_id(RuleId, Position, Goal),
    logical_id(CallId, Call).

logical_graph_row(Rows, ':'(Seed, call, ref(Call), 0)) :-
    member(program_seed(SeedId, CallId), Rows),
    logical_id(SeedId, Seed),
    logical_id(CallId, Call).

logical_graph_row(Rows, ':'(Call, apply, ref(Relation), 0)) :-
    member(program_apply(CallId, Relation), Rows),
    logical_id(CallId, Call).
logical_graph_row(Rows, ':'(Call, argument, ref(Argument), EdgeIndex)) :-
    member(program_argument(CallId, Position, ArgumentId), Rows),
    logical_id(CallId, Call),
    logical_id(ArgumentId, Argument),
    EdgeIndex is Position + 1.

logical_graph_row(Rows, ':'(Argument, Label, Target, Index)) :-
    member(program_edge(ArgumentId, Label, RawTarget, Index), Rows),
    logical_id(ArgumentId, Argument),
    argument_target(Label, RawTarget, Target).

occurrence_id(Rows, Id) :-
    member(program_seed(SeedId, _), Rows),
    logical_id(SeedId, Id).
occurrence_id(Rows, Id) :-
    member(program_rule(RuleId, _), Rows),
    logical_id(RuleId, Id).
occurrence_id(Rows, Id) :-
    member(program_goal(RuleId, Position, _, _), Rows),
    goal_id(RuleId, Position, Id).
occurrence_id(Rows, Id) :-
    member(program_apply(CallId, _), Rows),
    logical_id(CallId, Id).
occurrence_id(Rows, Id) :-
    member(program_argument(_, _, ArgumentId), Rows),
    logical_id(ArgumentId, Id).
occurrence_id(Rows, Id) :-
    member(program_edge(ArgumentId, input, ref(InputId), _), Rows),
    logical_id(ArgumentId, _),
    logical_id(InputId, Id).

logical_id(Id, logical_program(Id)).

goal_id(RuleId, Position,
        logical_program(goal_occurrence(RuleId, Position))).

argument_target(reference, ref(Identity), ref(Identity)) :- !.
argument_target(input, ref(Identity), Target) :-
    !,
    logical_id(Identity, LogicalIdentity),
    Target = ref(LogicalIdentity).
argument_target(_, const(Value), const(Value)).
