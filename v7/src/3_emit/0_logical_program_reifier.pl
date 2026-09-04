:- module(dl7_logical_program_reifier,
          [ logical_program_rows/2,
            logical_program_calls/4,
            logical_program_calls/5,
            logical_program_rows_calls/5
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

%% logical_program_calls(+CompilerFacts, +CheckedProgram,
%%                       -Calls, -Diagnostics) is det.
%
% Resolve the prelude's public program relations once the compiler graph is
% closed, then encode the checked program as ground DL7 calls for a second
% comptime evaluation. Rule and call occurrences use opaque reference
% identities. Each argument occurrence is an opaque node whose checked
% variable, reference, literal, or aggregate alternative is another ordinary
% relation. Aggregate inputs recursively receive their own node identities.
logical_program_calls(CompilerFacts, CheckedProgram, Calls, Diagnostics) :-
    logical_program_calls(
        CompilerFacts, CheckedProgram, all, Calls, Diagnostics).

%% logical_program_calls(+CompilerFacts, +CheckedProgram, +Relations,
%%                       -Calls, -Diagnostics) is det.
%
% `Relations` is either `all` or the closed relation-ID set read by one
% emitter. Filtering before conversion avoids allocating representations for
% compiler rows outside that emitter's dependency cone.
logical_program_calls(
    CompilerFacts, CheckedProgram, Relations, Calls, Diagnostics) :-
    logical_program_rows(CheckedProgram, Rows),
    logical_program_rows_calls(
        CompilerFacts, Rows, Relations, Calls, Diagnostics).

%% logical_program_rows_calls(+CompilerFacts, +Rows, +Relations,
%%                            -Calls, -Diagnostics) is det.
%
% Convert an existing immutable logical-program view. The artifact emitter
% already owns this view, so it must not construct and retain a second copy.
logical_program_rows_calls(
    CompilerFacts, Rows, Relations, Calls, Diagnostics) :-
    logical_rows_calls(
        Rows, CompilerFacts, Relations, Calls0, Diagnostics0),
    Calls = Calls0,
    sort(Diagnostics0, Diagnostics).

logical_rows_calls([], _, _, [], []).
logical_rows_calls([Row | Rows], CompilerFacts, Relations,
                   Calls, Diagnostics) :-
    logical_row_call_if_requested(
        Relations, Row, CompilerFacts, Result),
    logical_rows_calls(
        Rows, CompilerFacts, Relations, RestCalls, RestDiagnostics),
    prepend_logical_call(Result, RestCalls, RestDiagnostics,
                         Calls, Diagnostics).

prepend_logical_call(ok(Call), Calls, Diagnostics,
                     [Call | Calls], Diagnostics).
prepend_logical_call(error(Diagnostic), Calls, Diagnostics,
                     Calls, [Diagnostic | Diagnostics]).
prepend_logical_call(skip, Calls, Diagnostics, Calls, Diagnostics).

logical_row_call_if_requested(all, Row, CompilerFacts, Result) :-
    !,
    logical_row_call(Row, CompilerFacts, Result).
logical_row_call_if_requested(Relations, Row, CompilerFacts, Result) :-
    logical_row_relation(Row, CompilerFacts, RelationResult),
    logical_row_call_for_relation(
        RelationResult, Relations, Row, CompilerFacts, Result).

logical_row_call_for_relation(ok(Relation), Relations, Row,
                              CompilerFacts, Result) :-
    !,
    (   memberchk(Relation, Relations)
    ->  logical_row_call(Row, CompilerFacts, Result)
    ;   Result = skip
    ).
logical_row_call_for_relation(error(Reason), _, _, _,
                              error(diagnostic(emit, none, Reason))).

logical_row_relation(program_argument_edge(_, _, _, _), _,
                     ok(kernel(':'))) :-
    !.
logical_row_relation(Row, CompilerFacts, Result) :-
    functor(Row, Name, _),
    prelude_relation_id(CompilerFacts, Name, Result).

logical_row_call(Row, _, Result) :-
    Row = program_argument_edge(Argument, Label, Target, Index),
    !,
    logical_identity(Argument, LogicalArgument),
    logical_argument_target(Target, LogicalTarget),
    Result = ok(call(ref(kernel(':')),
                     [ LogicalArgument, const(Label), LogicalTarget,
                       const(Index)
                     ])).
logical_row_call(Row, CompilerFacts, Result) :-
    functor(Row, Name, _),
    prelude_relation_id(CompilerFacts, Name, RelationResult),
    logical_row_call_after_relation(RelationResult, Row, Result).

logical_argument_target(ref(Identity), LogicalIdentity) :-
    !,
    logical_identity(Identity, LogicalIdentity).
logical_argument_target(const(Value), const(Value)).

logical_row_call_after_relation(ok(Relation), Row,
                                ok(call(ref(Relation), Arguments))) :-
    !,
    logical_row_arguments(Row, Arguments).
logical_row_call_after_relation(error(Reason), _,
                                error(diagnostic(emit, none, Reason))).

prelude_relation_id(CompilerFacts, Name, Result) :-
    findall(
        Relation,
        member(call(ref(kernel(':')),
                    [ ref(module(prelude)), const(Name), ref(Relation),
                      const(_)
                    ]),
               CompilerFacts),
        Relations0),
    sort(Relations0, Relations),
    prelude_relation_result(Name, Relations, Result).

prelude_relation_result(_, [Relation], ok(Relation)) :- !.
prelude_relation_result(Name, [],
                        error(logical_program_protocol_missing(Name))) :- !.
prelude_relation_result(Name, Relations,
                        error(logical_program_protocol_ambiguous(
                                  Name, Relations))).

logical_row_arguments(program_relation(Relation, Arity),
                      [ref(Relation), const(Arity)]).
logical_row_arguments(program_key(Relation, Ordinal),
                      [ref(Relation), const(Ordinal)]).
logical_row_arguments(program_key_position(Relation, KeyOrdinal, Position),
                      [ref(Relation), const(KeyOrdinal), const(Position)]).
logical_row_arguments(program_seed(Seed, Call),
                      [LogicalSeed, LogicalCall]) :-
    logical_identity(Seed, LogicalSeed),
    logical_identity(Call, LogicalCall).
logical_row_arguments(program_rule(Rule, HeadCall),
                      [LogicalRule, LogicalHeadCall]) :-
    logical_identity(Rule, LogicalRule),
    logical_identity(HeadCall, LogicalHeadCall).
logical_row_arguments(program_rule_kind(Rule, Kind),
                      [LogicalRule, const(KindText)]) :-
    logical_identity(Rule, LogicalRule),
    atom_string(Kind, KindText).
logical_row_arguments(program_goal(Rule, Position, Polarity, Call),
                      [ LogicalRule, const(Position), const(PolarityText),
                        LogicalCall
                      ]) :-
    logical_identity(Rule, LogicalRule),
    atom_string(Polarity, PolarityText),
    logical_identity(Call, LogicalCall).
logical_row_arguments(program_apply(Call, Relation),
                      [LogicalCall, ref(Relation)]) :-
    logical_identity(Call, LogicalCall).
logical_row_arguments(program_argument(Call, Position, Argument),
                      [LogicalCall, const(Position), LogicalArgument]) :-
    logical_identity(Call, LogicalCall),
    logical_identity(Argument, LogicalArgument).
logical_row_arguments(program_dependency(Head, Body, Polarity),
                      [ref(Head), ref(Body), const(PolarityText)]) :-
    atom_string(Polarity, PolarityText).
logical_row_arguments(program_stratum(Relation, Level),
                      [ref(Relation), const(Level)]).

logical_identity(Identity, ref(logical_program(Identity))).

relation_rows([], []).
relation_rows([relation(ref(Relation), Arity, KeySets) | Relations], Rows) :-
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
    append([[ program_rule(RuleId, HeadCallId),
              program_rule_kind(RuleId, level)
            ], HeadRows, GoalRows,
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

call_rows(CallId, call(ref(Relation), Arguments),
          [program_apply(CallId, Relation) | ArgumentRows]) :-
    argument_rows(Arguments, CallId, 0, ArgumentRows).

argument_rows([], _, _, []).
argument_rows([Argument | Arguments], CallId, Position, Rows) :-
    ArgumentId = argument_id(CallId, Position),
    argument_value_rows(Argument, ArgumentId, ValueRows),
    NextPosition is Position + 1,
    argument_rows(Arguments, CallId, NextPosition, RestRows),
    append([program_argument(CallId, Position, ArgumentId) | ValueRows],
           RestRows, Rows).

argument_value_rows(var(Variable), Argument,
                    [program_argument_edge(Argument, variable,
                                           const(Variable), 0)]).
argument_value_rows(ref(Reference), Argument,
                    [program_argument_edge(Argument, reference,
                                           ref(Reference), 0)]).
argument_value_rows(const(Value), Argument,
                    [program_argument_edge(Argument, literal,
                                           const(Value), 0)]).
argument_value_rows(aggregate(Operator, Input), Argument,
                    [ program_argument_edge(Argument, aggregate,
                                            const(Operator), 0),
                      program_argument_edge(Argument, input,
                                            ref(InputArgument), 1)
                    | InputRows]) :-
    InputArgument = argument_child(Argument, input),
    argument_value_rows(Input, InputArgument, InputRows).

dependency_rows([], []).
dependency_rows([depends(ref(Head), ref(Body), Polarity) | Dependencies],
                [program_dependency(Head, Body, Polarity) | Rows]) :-
    dependency_rows(Dependencies, Rows).

stratum_rows([], []).
stratum_rows([stratum(ref(Relation), Level) | Strata],
             [program_stratum(Relation, Level) | Rows]) :-
    stratum_rows(Strata, Rows).
