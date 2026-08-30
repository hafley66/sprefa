:- module(dl7_generated_program_assembler,
          [ assemble_generated_program/5
          ]).

:- use_module(library(error), [must_be/2]).

%% assemble_generated_program(+CompilerRows, +BaseRelations,
%%                            -GeneratedRelations, -GeneratedRules,
%%                            -Diagnostics) is det.
%
% Turn the public def/head/body carrier relations into the checked IR used by
% the shared evaluator. Every position is zero-based and dense. Variable names
% are scoped by their generated rule identity before entering checked IR.
assemble_generated_program(CompilerRows, BaseRelations,
                           GeneratedRelations, GeneratedRules,
                           Diagnostics) :-
    must_be(ground, CompilerRows),
    must_be(ground, BaseRelations),
    assemble_definitions(CompilerRows, Relations0, DefinitionDiagnostics),
    append(BaseRelations, Relations0, AllRelations0),
    sort(AllRelations0, AllRelations),
    assemble_rules(CompilerRows, AllRelations, Rules0, RuleDiagnostics),
    append(DefinitionDiagnostics, RuleDiagnostics, Diagnostics0),
    sort(Diagnostics0, Diagnostics),
    (   Diagnostics == []
    ->  sort(Relations0, GeneratedRelations),
        sort(Rules0, GeneratedRules)
    ;   GeneratedRelations = [],
        GeneratedRules = []
    ).

assemble_definitions(Rows, Relations, Diagnostics) :-
    findall(Relation,
            member(call(ref(kernel(def)), [ref(Relation), _]), Rows),
            RelationIds0),
    sort(RelationIds0, RelationIds),
    assemble_definition_ids(RelationIds, Rows, Relations, Diagnostics).

assemble_definition_ids([], _, [], []).
assemble_definition_ids([Relation | RelationIds], Rows,
                        Relations, Diagnostics) :-
    findall(Arity,
            member(call(ref(kernel(def)),
                        [ref(Relation), const(Arity)]), Rows),
            Arities0),
    sort(Arities0, Arities),
    definition_result(Relation, Arities, RelationResult,
                      OwnDiagnostics),
    assemble_definition_ids(RelationIds, Rows,
                            RestRelations, RestDiagnostics),
    append_relation_result(RelationResult, RestRelations, Relations),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

definition_result(Relation, [Arity], relation(RelationRef, Arity, []), []) :-
    integer(Arity),
    Arity >= 0,
    RelationRef = ref(Relation),
    !.
definition_result(Relation, [Arity], none,
                  [diagnostic(assemble, none,
                              invalid_generated_arity(Relation, Arity))]) :-
    !.
definition_result(Relation, Arities, none,
                  [diagnostic(assemble, none,
                              conflicting_generated_definitions(
                                  Relation, Arities))]).

append_relation_result(none, Relations, Relations).
append_relation_result(Relation, Relations, [Relation | Relations]).

assemble_rules(Rows, Relations, Rules, Diagnostics) :-
    findall(Rule,
            member(call(ref(kernel(head)), [ref(Rule), _]), Rows),
            RuleIds0),
    sort(RuleIds0, RuleIds),
    assemble_rule_ids(RuleIds, Rows, Relations, Rules, Diagnostics).

assemble_rule_ids([], _, _, [], []).
assemble_rule_ids([RuleId | RuleIds], Rows, Relations,
                  Rules, Diagnostics) :-
    assemble_rule(RuleId, Rows, Relations, RuleResult, OwnDiagnostics),
    assemble_rule_ids(RuleIds, Rows, Relations,
                      RestRules, RestDiagnostics),
    append_rule_result(RuleResult, RestRules, Rules),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

assemble_rule(RuleId, Rows, Relations, RuleResult, Diagnostics) :-
    findall(Relation,
            member(call(ref(kernel(head)),
                        [ref(RuleId), ref(Relation)]), Rows),
            HeadRelations0),
    sort(HeadRelations0, HeadRelations),
    (   HeadRelations = [HeadRelation],
        memberchk(relation(ref(HeadRelation), HeadArity, _), Relations)
    ->  assemble_arguments(head, RuleId, none, HeadArity, Rows,
                           HeadArguments, HeadDiagnostics),
        assemble_body(RuleId, Rows, Relations, Body, BodyDiagnostics),
        append(HeadDiagnostics, BodyDiagnostics, Diagnostics),
        (   Diagnostics == []
        ->  RuleResult = rule(
                             call(ref(HeadRelation), HeadArguments),
                             Body)
        ;   RuleResult = none
        )
    ;   HeadRelations = [HeadRelation]
    ->  RuleResult = none,
        Diagnostics =
            [diagnostic(assemble, none,
                        generated_head_undeclared_relation(
                            RuleId, HeadRelation))]
    ;   RuleResult = none,
        Diagnostics =
            [diagnostic(assemble, none,
                        conflicting_generated_heads(
                            RuleId, HeadRelations))]
    ).

append_rule_result(none, Rules, Rules).
append_rule_result(Rule, Rules, [Rule | Rules]).

assemble_body(RuleId, Rows, Relations, Body, Diagnostics) :-
    findall(GoalIndex,
            member(call(ref(kernel(body)),
                        [ref(RuleId), const(GoalIndex), _, _]), Rows),
            GoalIndices0),
    sort(GoalIndices0, GoalIndices),
    length(GoalIndices, GoalCount),
    expected_positions(GoalCount, ExpectedIndices),
    (   GoalIndices == ExpectedIndices
    ->  assemble_goals(GoalIndices, RuleId, Rows, Relations,
                       Body, Diagnostics)
    ;   Body = [],
        Diagnostics =
            [diagnostic(assemble, none,
                        non_dense_generated_goals(
                            RuleId, GoalIndices))]
    ).

assemble_goals([], _, _, _, [], []).
assemble_goals([GoalIndex | GoalIndices], RuleId, Rows, Relations,
               Goals, Diagnostics) :-
    findall(goal(Polarity, Relation),
            member(call(ref(kernel(body)),
                        [ ref(RuleId), const(GoalIndex),
                          const(Polarity), ref(Relation)
                        ]), Rows),
            GoalHeaders0),
    sort(GoalHeaders0, GoalHeaders),
    assemble_goal_header(RuleId, GoalIndex, GoalHeaders, Rows, Relations,
                         GoalResult, OwnDiagnostics),
    assemble_goals(GoalIndices, RuleId, Rows, Relations,
                   RestGoals, RestDiagnostics),
    append_goal_result(GoalResult, RestGoals, Goals),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

assemble_goal_header(RuleId, GoalIndex, [goal(Polarity0, Relation)],
                     Rows, Relations, GoalResult, Diagnostics) :-
    !,
    (   generated_polarity(Polarity0, Polarity),
        memberchk(relation(ref(Relation), Arity, _), Relations)
    ->  assemble_arguments(body, RuleId, GoalIndex, Arity, Rows,
                           Arguments, Diagnostics),
        (   Diagnostics == []
        ->  GoalResult = checked_goal(
                                Polarity,
                                call(ref(Relation), Arguments))
        ;   GoalResult = none
        )
    ;   generated_polarity(Polarity0, _)
    ->  GoalResult = none,
        Diagnostics =
            [diagnostic(assemble, none,
                        generated_body_undeclared_relation(
                            RuleId, GoalIndex, Relation))]
    ;   GoalResult = none,
        Diagnostics =
            [diagnostic(assemble, none,
                        invalid_generated_polarity(
                            RuleId, GoalIndex, Polarity0))]
    ).
assemble_goal_header(RuleId, GoalIndex, GoalHeaders, _, _, none,
                     [diagnostic(assemble, none,
                                 conflicting_generated_body_goal(
                                     RuleId, GoalIndex, GoalHeaders))]).

append_goal_result(none, Goals, Goals).
append_goal_result(Goal, Goals, [Goal | Goals]).

assemble_arguments(Part, RuleId, GoalIndex, Arity, Rows,
                   Arguments, Diagnostics) :-
    argument_rows(Part, RuleId, GoalIndex, Rows, ArgumentRows),
    findall(Position,
            member(argument(Position, _, _), ArgumentRows),
            Positions0),
    sort(Positions0, Positions),
    expected_positions(Arity, ExpectedPositions),
    (   Positions == ExpectedPositions
    ->  assemble_argument_positions(ExpectedPositions, RuleId,
                                     ArgumentRows,
                                     Arguments, Diagnostics)
    ;   Arguments = [],
        Diagnostics =
            [diagnostic(assemble, none,
                        generated_argument_positions(
                            Part, RuleId, GoalIndex,
                            expected(ExpectedPositions),
                            observed(Positions)))]
    ).

argument_rows(head, RuleId, _, Rows, ArgumentRows) :-
    findall(argument(Position, Kind, Value),
            member(call(ref(kernel(head_arg)),
                        [ ref(RuleId), const(Position),
                          const(Kind), Value
                        ]), Rows),
            ArgumentRows).
argument_rows(body, RuleId, GoalIndex, Rows, ArgumentRows) :-
    findall(argument(Position, Kind, Value),
            member(call(ref(kernel(body_arg)),
                        [ ref(RuleId), const(GoalIndex), const(Position),
                          const(Kind), Value
                        ]), Rows),
            ArgumentRows).

assemble_argument_positions([], _, _, [], []).
assemble_argument_positions([Position | Positions], RuleId, Rows,
                            Arguments, Diagnostics) :-
    findall(kind_value(Kind, Value),
            member(argument(Position, Kind, Value), Rows),
            Values0),
    sort(Values0, Values),
    generated_argument_result(RuleId, Position, Values,
                              ArgumentResult, OwnDiagnostics),
    assemble_argument_positions(Positions, RuleId, Rows,
                                RestArguments, RestDiagnostics),
    append_argument_result(ArgumentResult, RestArguments, Arguments),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

generated_argument_result(RuleId, _, [kind_value(Kind0, const(Name))],
                          var(generated(RuleId, Name)), []) :-
    generated_kind(Kind0, variable),
    (atom(Name); string(Name)),
    !.
generated_argument_result(_, _, [kind_value(Kind0, Value)], Value, []) :-
    generated_kind(Kind0, constant),
    Value = const(_),
    !.
generated_argument_result(_, _, [kind_value(Kind0, Value)], Value, []) :-
    generated_kind(Kind0, reference),
    Value = ref(_),
    !.
generated_argument_result(RuleId, Position, Values, none,
                          [diagnostic(assemble, none,
                                      invalid_generated_argument(
                                          RuleId, Position, Values))]).

append_argument_result(none, Arguments, Arguments).
append_argument_result(Argument, Arguments, [Argument | Arguments]).

generated_kind(variable, variable).
generated_kind("variable", variable).
generated_kind(constant, constant).
generated_kind("constant", constant).
generated_kind(reference, reference).
generated_kind("reference", reference).

generated_polarity(positive, positive).
generated_polarity("positive", positive).
generated_polarity(negative, negative).
generated_polarity("negative", negative).

expected_positions(0, []) :-
    !.
expected_positions(Count, Positions) :-
    Last is Count - 1,
    numlist(0, Last, Positions).
