:- module(dl7_generated_program_assembler,
          [ assemble_generated_program/5
          ]).

:- use_module(library(error), [must_be/2]).

%% assemble_generated_program(+CompilerRows, +BaseRelations,
%%                            -GeneratedRelations, -GeneratedRules,
%%                            -Diagnostics) is det.
%
% Turn public def/head/body rows plus application nodes into the checked IR
% used by the shared evaluator. Every position is zero-based and dense.
% Variable names are scoped by their generated rule identity before entering
% checked IR.
assemble_generated_program(CompilerRows, BaseRelations,
                           GeneratedRelations, GeneratedRules,
                           Diagnostics) :-
    must_be(ground, CompilerRows),
    must_be(ground, BaseRelations),
    assemble_definitions(CompilerRows, Relations0, DefinitionDiagnostics),
    generated_relation_collision_diagnostics(Relations0, BaseRelations,
                                             CollisionDiagnostics),
    append(BaseRelations, Relations0, AllRelations0),
    sort(AllRelations0, AllRelations),
    assemble_rules(CompilerRows, AllRelations, Rules0, RuleDiagnostics),
    orphan_fragment_diagnostics(CompilerRows, OrphanDiagnostics),
    append([DefinitionDiagnostics, CollisionDiagnostics,
            RuleDiagnostics, OrphanDiagnostics], Diagnostics0),
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

generated_relation_collision_diagnostics([], _, []).
generated_relation_collision_diagnostics(
    [relation(Relation, _, _) | Relations], BaseRelations, Diagnostics) :-
    (   memberchk(relation(Relation, _, _), BaseRelations)
    ->  Diagnostics =
            [diagnostic(assemble, none,
                        generated_relation_already_declared(Relation))
             | RestDiagnostics]
    ;   Diagnostics = RestDiagnostics
    ),
    generated_relation_collision_diagnostics(Relations, BaseRelations,
                                             RestDiagnostics).

assemble_rules(Rows, Relations, Rules, Diagnostics) :-
    generated_application_context(Rows, ApplicationContext),
    findall(Rule,
            member(call(ref(kernel(head)), [ref(Rule), _]), Rows),
            RuleIds0),
    sort(RuleIds0, RuleIds),
    assemble_rule_ids(RuleIds, Rows, ApplicationContext, Relations,
                      Rules, Diagnostics).

assemble_rule_ids([], _, _, _, [], []).
assemble_rule_ids([RuleId | RuleIds], Rows, ApplicationContext, Relations,
                  Rules, Diagnostics) :-
    assemble_rule(RuleId, Rows, ApplicationContext, Relations,
                  RuleResult, OwnDiagnostics),
    assemble_rule_ids(RuleIds, Rows, ApplicationContext, Relations,
                      RestRules, RestDiagnostics),
    append_rule_result(RuleResult, RestRules, Rules),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

assemble_rule(RuleId, Rows, ApplicationContext, Relations,
              RuleResult, Diagnostics) :-
    findall(Application,
            member(call(ref(kernel(head)),
                        [ref(RuleId), ref(Application)]), Rows),
            HeadApplications0),
    sort(HeadApplications0, HeadApplications),
    (   HeadApplications = [HeadApplication]
    ->  assemble_application(
            head, RuleId, none, HeadApplication,
            ApplicationContext, Relations, HeadCall, HeadDiagnostics),
        assemble_body(RuleId, Rows, ApplicationContext, Relations,
                      Body, BodyDiagnostics),
        append(HeadDiagnostics, BodyDiagnostics, Diagnostics),
        (   Diagnostics == []
        ->  RuleResult = rule(HeadCall, Body)
        ;   RuleResult = none
        )
    ;   RuleResult = none,
        Diagnostics =
            [diagnostic(assemble, none,
                        conflicting_generated_heads(
                            RuleId, HeadApplications))]
    ).

append_rule_result(none, Rules, Rules).
append_rule_result(Rule, Rules, [Rule | Rules]).

orphan_fragment_diagnostics(Rows, Diagnostics) :-
    generated_head_ids(Rows, HeadIds),
    generated_fragment_ids(Rows, FragmentIds),
    findall(diagnostic(assemble, none,
                       orphan_generated_rule_fragment(RuleId)),
            ( member(RuleId, FragmentIds),
              \+ memberchk(RuleId, HeadIds)
            ),
            RuleDiagnostics),
    Diagnostics = RuleDiagnostics.

generated_head_ids(Rows, HeadIds) :-
    findall(RuleId,
            member(call(ref(kernel(head)), [ref(RuleId), _]), Rows),
            HeadIds0),
    sort(HeadIds0, HeadIds).

generated_fragment_ids(Rows, FragmentIds) :-
    findall(RuleId,
            generated_fragment_id(Rows, RuleId),
            FragmentIds0),
    sort(FragmentIds0, FragmentIds).

generated_fragment_id(Rows, RuleId) :-
    member(call(ref(kernel(body)), [ref(RuleId) | _]), Rows).

assemble_body(RuleId, Rows, ApplicationContext, Relations,
              Body, Diagnostics) :-
    findall(GoalIndex,
            member(call(ref(kernel(body)),
                        [ref(RuleId), const(GoalIndex), _, _]), Rows),
            GoalIndices0),
    sort(GoalIndices0, GoalIndices),
    length(GoalIndices, GoalCount),
    expected_positions(GoalCount, ExpectedIndices),
    (   GoalIndices == ExpectedIndices
    ->  assemble_goals(GoalIndices, RuleId, Rows, ApplicationContext,
                       Relations,
                       Body, Diagnostics)
    ;   Body = [],
        Diagnostics =
            [diagnostic(assemble, none,
                        non_dense_generated_goals(
                            RuleId, GoalIndices))]
    ).

assemble_goals([], _, _, _, _, [], []).
assemble_goals([GoalIndex | GoalIndices], RuleId, Rows,
               ApplicationContext, Relations,
               Goals, Diagnostics) :-
    findall(goal(Polarity, Application),
            member(call(ref(kernel(body)),
                        [ ref(RuleId), const(GoalIndex),
                          const(Polarity), ref(Application)
                        ]), Rows),
            GoalHeaders0),
    sort(GoalHeaders0, GoalHeaders),
    assemble_goal_header(RuleId, GoalIndex, GoalHeaders,
                         ApplicationContext, Relations,
                         GoalResult, OwnDiagnostics),
    assemble_goals(GoalIndices, RuleId, Rows, ApplicationContext, Relations,
                   RestGoals, RestDiagnostics),
    append_goal_result(GoalResult, RestGoals, Goals),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

assemble_goal_header(RuleId, GoalIndex, [goal(Polarity0, Application)],
                     ApplicationContext, Relations,
                     GoalResult, Diagnostics) :-
    !,
    (   generated_polarity(Polarity0, Polarity)
    ->  assemble_application(
            body, RuleId, GoalIndex, Application,
            ApplicationContext, Relations, Call, Diagnostics),
        (   Diagnostics == []
        ->  GoalResult = checked_goal(Polarity, Call)
        ;   GoalResult = none
        )
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

assemble_application(Part, RuleId, GoalIndex, Application,
                     ApplicationContext, Relations, Call, Diagnostics) :-
    application_callables(ApplicationContext, Application, Callables),
    (   Callables = [Relation],
        memberchk(relation(ref(Relation), Arity, _), Relations)
    ->  assemble_arguments(
            Application, RuleId, Arity, ApplicationContext,
            Arguments, Diagnostics),
        (   Diagnostics == []
        ->  Call = call(ref(Relation), Arguments)
        ;   Call = none
        )
    ;   Callables = [Relation]
    ->  Call = none,
        undeclared_application_diagnostic(
            Part, RuleId, GoalIndex, Relation, Diagnostics)
    ;   Call = none,
        Diagnostics =
            [diagnostic(
                 assemble, none,
                 generated_application_callable(
                     Part, RuleId, GoalIndex, Application, Callables))]
    ).

undeclared_application_diagnostic(
    head, RuleId, _, Relation,
    [diagnostic(assemble, none,
                generated_head_undeclared_relation(RuleId, Relation))]).
undeclared_application_diagnostic(
    body, RuleId, GoalIndex, Relation,
    [diagnostic(assemble, none,
                generated_body_undeclared_relation(
                    RuleId, GoalIndex, Relation))]).

assemble_arguments(Application, RuleId, Arity, ApplicationContext,
                   Arguments, Diagnostics) :-
    argument_rows(Application, ApplicationContext, ArgumentRows),
    findall(Position,
            member(argument(Position, _), ArgumentRows),
            Positions0),
    sort(Positions0, Positions),
    expected_positions(Arity, ExpectedPositions),
    (   Positions == ExpectedPositions
    ->  assemble_argument_positions(ExpectedPositions, RuleId,
                                     ArgumentRows, ApplicationContext,
                                     Arguments, Diagnostics)
    ;   Arguments = [],
        Diagnostics =
            [diagnostic(assemble, none,
                        generated_argument_positions(
                            application, RuleId, Application,
                            expected(ExpectedPositions),
                            observed(Positions)))]
    ).

argument_rows(Application, application_context(Rows, _, _, _),
              ArgumentRows) :-
    findall(argument(Position, Value),
            member(call(ref(kernel(':')),
                        [ ref(Application), _, Value, const(Position) ]),
                   Rows),
            ArgumentRows).

assemble_argument_positions([], _, _, _, [], []).
assemble_argument_positions([Position | Positions], RuleId, Rows,
                            ApplicationContext,
                            Arguments, Diagnostics) :-
    findall(Value,
            member(argument(Position, Value), Rows),
            Values0),
    sort(Values0, Values),
    generated_argument_result(RuleId, Position, Values, ApplicationContext,
                              ArgumentResult, OwnDiagnostics),
    assemble_argument_positions(Positions, RuleId, Rows, ApplicationContext,
                                RestArguments, RestDiagnostics),
    append_argument_result(ArgumentResult, RestArguments, Arguments),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

generated_argument_result(RuleId, Position, [ref(Node)],
                          ApplicationContext, Result, Diagnostics) :-
    !,
    generated_node_result(
        RuleId, Position, Node, ApplicationContext, Result, Diagnostics).
generated_argument_result(RuleId, Position, Values, _, none,
                          [diagnostic(assemble, none,
                                      invalid_generated_argument(
                                          RuleId, Position, Values))]).

generated_node_result(RuleId, Position, Node, ApplicationContext,
                      Result, Diagnostics) :-
    variable_node_rows(ApplicationContext, Node, Variables),
    literal_node_rows(ApplicationContext, Node, Literals),
    generated_node_rows_result(
        RuleId, Position, Node, Variables, Literals, Result, Diagnostics).

generated_node_rows_result(
    RuleId, _, _, [variable(RuleId, Name)], [],
    var(generated(RuleId, Name)), []) :-
    (atom(Name); string(Name)),
    !.
generated_node_rows_result(_, _, _, [], [literal(_, Value)], Value, []) :-
    Value = const(_),
    !.
generated_node_rows_result(_, _, Node, [], [], ref(Node), []) :-
    !.
generated_node_rows_result(
    RuleId, Position, Node, Variables, Literals, none,
    [diagnostic(assemble, none,
                invalid_generated_value_node(
                    RuleId, Position, Node, Variables, Literals))]).

generated_application_context(
    Rows,
    application_context(Rows, ApplyRelations, LiteralRelations,
                        VariableRelations)) :-
    named_relation_targets(Rows, 'Apply', ApplyRelations),
    named_relation_targets(Rows, 'Literal', LiteralRelations),
    named_relation_targets(Rows, 'Variable', VariableRelations).

named_relation_targets(Rows, Name, Relations) :-
    findall(Relation,
            member(call(ref(kernel(':')),
                        [ref(_), const(Name), ref(Relation), const(_)]),
                   Rows),
            Relations0),
    sort(Relations0, Relations).

application_callables(
    application_context(Rows, ApplyRelations, _, _), Application,
    Callables) :-
    findall(Callable,
            ( member(Apply, ApplyRelations),
              member(call(ref(Apply),
                          [ref(Application), ref(Callable)]), Rows)
            ),
            Callables0),
    sort(Callables0, Callables).

variable_node_rows(
    application_context(Rows, _, _, VariableRelations), Node, Variables) :-
    findall(variable(Scope, Name),
            ( member(Variable, VariableRelations),
              member(call(ref(Variable),
                          [ref(Node), ref(Scope), const(Name)]), Rows)
            ),
            Variables0),
    sort(Variables0, Variables).

literal_node_rows(
    application_context(Rows, _, LiteralRelations, _), Node, Literals) :-
    findall(literal(Primitive, Value),
            ( member(Literal, LiteralRelations),
              member(call(ref(Literal),
                          [ref(Node), ref(Primitive), Value]), Rows)
            ),
            Literals0),
    sort(Literals0, Literals).

append_argument_result(none, Arguments, Arguments).
append_argument_result(Argument, Arguments, [Argument | Arguments]).

generated_polarity(positive, positive).
generated_polarity("positive", positive).
generated_polarity(negative, negative).
generated_polarity("negative", negative).

expected_positions(0, []) :-
    !.
expected_positions(Count, Positions) :-
    Last is Count - 1,
    numlist(0, Last, Positions).
