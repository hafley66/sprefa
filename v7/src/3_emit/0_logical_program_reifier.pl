:- module(dl7_logical_program_reifier,
          [ logical_program_rows/2
          ]).

:- use_module(library(error), [must_be/2]).

%% logical_program_rows(+CheckedProgram, -Rows) is det.
%
% Reify checked Datalog into target-neutral compiler data. Occurrence IDs are
% structural and stable because the checker has already fixed relation, seed,
% rule, and goal order. No storage or renderer vocabulary enters these rows.
logical_program_rows(
    checked_datalog(_, datalog_program(Relations, Seeds, Rules),
                    Dependencies, Strata),
    Rows) :-
    must_be(ground, Relations),
    must_be(ground, Seeds),
    must_be(ground, Rules),
    must_be(ground, Dependencies),
    must_be(ground, Strata),
    relation_rows(Relations, RelationRows),
    seed_rows(Seeds, 0, SeedRows),
    rule_rows(Rules, 0, RuleRows),
    dependency_rows(Dependencies, DependencyRows),
    stratum_rows(Strata, StratumRows),
    append([RelationRows, SeedRows, RuleRows,
            DependencyRows, StratumRows], Rows0),
    sort(Rows0, Rows).

relation_rows([], []).
relation_rows([relation(Relation, Arity, KeySets) | Relations], Rows) :-
    key_rows(KeySets, Relation, 0, KeyRows),
    relation_rows(Relations, RestRows),
    append([program_relation(Relation, Arity) | KeyRows], RestRows, Rows).

key_rows([], _, _, []).
key_rows([Positions | KeySets], Relation, KeyOrdinal,
         [program_key(Relation, KeyOrdinal) | Rows]) :-
    key_position_rows(Positions, Relation, KeyOrdinal, PositionRows),
    NextKeyOrdinal is KeyOrdinal + 1,
    key_rows(KeySets, Relation, NextKeyOrdinal, RestRows),
    append(PositionRows, RestRows, Rows).

key_position_rows([], _, _, []).
key_position_rows([Position | Positions], Relation, KeyOrdinal,
                  [program_key_position(Relation, KeyOrdinal, Position)
                   | Rows]) :-
    key_position_rows(Positions, Relation, KeyOrdinal, Rows).

seed_rows([], _, []).
seed_rows([Call | Seeds], Index, Rows) :-
    SeedId = seed_id(Index),
    CallId = call_id(seed, Index),
    call_rows(CallId, Call, CallRows),
    NextIndex is Index + 1,
    seed_rows(Seeds, NextIndex, RestRows),
    append([program_seed(SeedId, CallId) | CallRows], RestRows, Rows).

rule_rows([], _, []).
rule_rows([rule(Head, Goals) | Rules], Index, Rows) :-
    RuleId = rule_id(Index),
    HeadCallId = call_id(rule(Index), head),
    call_rows(HeadCallId, Head, HeadRows),
    goal_rows(Goals, RuleId, Index, 0, GoalRows),
    NextIndex is Index + 1,
    rule_rows(Rules, NextIndex, RestRows),
    append([[program_rule(RuleId, HeadCallId)], HeadRows, GoalRows,
            RestRows], Rows).

goal_rows([], _, _, _, []).
goal_rows([checked_goal(Polarity, Call) | Goals], RuleId, RuleIndex,
          GoalIndex, Rows) :-
    CallId = call_id(rule(RuleIndex), goal(GoalIndex)),
    call_rows(CallId, Call, CallRows),
    NextGoalIndex is GoalIndex + 1,
    goal_rows(Goals, RuleId, RuleIndex, NextGoalIndex, RestRows),
    append([program_goal(RuleId, GoalIndex, Polarity, CallId) | CallRows],
           RestRows, Rows).

call_rows(CallId, call(Relation, Arguments),
          [program_apply(CallId, Relation) | ArgumentRows]) :-
    argument_rows(Arguments, CallId, 0, ArgumentRows).

argument_rows([], _, _, []).
argument_rows([Argument | Arguments], CallId, Position,
              [program_argument(CallId, Position, Argument) | Rows]) :-
    NextPosition is Position + 1,
    argument_rows(Arguments, CallId, NextPosition, Rows).

dependency_rows([], []).
dependency_rows([depends(Head, Body, Polarity) | Dependencies],
                [program_dependency(Head, Body, Polarity) | Rows]) :-
    dependency_rows(Dependencies, Rows).

stratum_rows([], []).
stratum_rows([stratum(Relation, Level) | Strata],
             [program_stratum(Relation, Level) | Rows]) :-
    stratum_rows(Strata, Rows).
