:- module(dl7_lowerer,
          [ lower_datalog/4,
            kernel_relation/2
          ]).

:- use_module(library(error), [must_be/2]).

%% lower_datalog(+Unit, -Program, -Origins, -Diagnostics) is det.
%
% Lower the ground reader tree in three passes: mint constructor owners,
% reserve every bind name, then lower top-level facts and rules against the
% complete file-owner reservation table. The result remains ground compiler
% data; reference resolution and Datalog checks run later in this module.
lower_datalog(Unit, Program, Origins, Diagnostics) :-
    must_be(ground, Unit),
    (   Unit = dl7_unit(Origin, _, Forms, _, _)
    ->  ModuleIdentity = Origin,
        ModuleOwner = module(ModuleIdentity),
        lower_declarations(Forms, ModuleOwner, ModuleIdentity,
                           DeclarationResult),
        lower_after_declarations(DeclarationResult, Forms, ModuleOwner,
                                 Program, Origins, Diagnostics)
    ;   Program = [],
        Origins = [],
        Diagnostics = [diagnostic(lower, unit, invalid_dl7_unit(Unit))]
    ),
    !.

lower_after_declarations(error(Diagnostic), _, _, [], [], [Diagnostic]).
lower_after_declarations(
    ok(Nodes0, Edges, Relations, DeclarationOrigins, Reservations),
    Forms, ModuleOwner, Program, Origins, Diagnostics) :-
    Environment = expression_environment(Reservations, Relations, Edges),
    lower_derived_bind_rules(Reservations, Environment, 0,
                             DerivedResult),
    (   DerivedResult = ok(DerivedRules, DerivedOrigins)
    ->  length(DerivedRules, RuleIndex),
        lower_executables(Forms, ModuleOwner, Environment,
                          RuleIndex, ExecutableResult),
        finish_lowered_executables(
            ExecutableResult, DerivedRules, DerivedOrigins,
            Nodes0, Edges, Relations, DeclarationOrigins, ModuleOwner,
            Program, Origins, Diagnostics)
    ;   DerivedResult = error(Diagnostic),
        Program = [],
        Origins = [],
        Diagnostics = [Diagnostic]
    ).

finish_lowered_executables(
    ok(Seeds, AuthoredRules, ExecutableOrigins),
    DerivedRules, DerivedOrigins,
    Nodes0, Edges, Relations, DeclarationOrigins, ModuleOwner,
    basement_program(root_graph(Nodes, Edges),
                     datalog_program(Relations, Seeds, Rules)),
    Origins, []) :-
    Nodes = [node(ModuleOwner), module(ModuleOwner) | Nodes0],
    append(DerivedRules, AuthoredRules, Rules),
    append([DeclarationOrigins, DerivedOrigins, ExecutableOrigins], Origins).
finish_lowered_executables(
    error(Diagnostic), _, _, _, _, _, _, _, _, [], [Diagnostic]).

lower_derived_bind_rules([], _, _, ok([], [])).
lower_derived_bind_rules(
    [reservation(Owner, _,
                 derived_compound_edge(
                     LabelNode, TargetTerm, BindNodeId, Index),
                 compound_edge) | Reservations],
    Environment, RuleIndex, Result) :-
    !,
    lower_expression(LabelNode, Owner, Environment,
                     LabelValue, Goals, GoalNodes, Diagnostics),
    (   Diagnostics == [],
        LabelValue = partial_application(_, _)
    ->  Result = error(diagnostic(
                           lower, BindNodeId,
                           partial_edge_label_requires_more_arguments))
    ;   Diagnostics == []
    ->  DerivedLabel = var(derived_label(BindNodeId)),
        replace_expression_value(
            LabelValue, DerivedLabel, Goals, LabelGoals),
        edge_rule_target(TargetTerm, RuleTarget),
        Head = call(name(Owner, ':'),
                    [ref(Owner), DerivedLabel, RuleTarget, const(Index)]),
        Rule = rule(Head, LabelGoals),
        indexed_goal_origins(GoalNodes, RuleIndex, 0, GoalOrigins),
        RuleOrigins = [origin(rule(RuleIndex), BindNodeId) | GoalOrigins],
        NextRuleIndex is RuleIndex + 1,
        lower_derived_bind_rules(Reservations, Environment, NextRuleIndex,
                                 RestResult),
        prepend_derived_rule(RestResult, Rule, RuleOrigins, Result)
    ;   Diagnostics = [Diagnostic | _],
        Result = error(Diagnostic)
    ).
lower_derived_bind_rules(
    [reservation(Owner, Name,
                 deferred_expression(TargetNode, BindNodeId, Index),
                 expression) | Reservations],
    Environment, RuleIndex, Result) :-
    !,
    lower_expression(TargetNode, Owner, Environment,
                     Value, Goals, GoalNodes, Diagnostics),
    (   Diagnostics == [],
        Value = partial_application(_, _)
    ->  Result = error(diagnostic(
                           lower, BindNodeId,
                           partial_application_requires_more_arguments(
                               Name)))
    ;   Diagnostics == []
    ->  BindValue = var(derived_bind(BindNodeId)),
        replace_expression_value(Value, BindValue, Goals, BindGoals),
        Head = call(name(Owner, ':'),
                    [ref(Owner), const(Name), BindValue, const(Index)]),
        Rule = rule(Head, BindGoals),
        indexed_goal_origins(GoalNodes, RuleIndex, 0, GoalOrigins),
        RuleOrigins = [origin(rule(RuleIndex), BindNodeId) | GoalOrigins],
        NextRuleIndex is RuleIndex + 1,
        lower_derived_bind_rules(Reservations, Environment, NextRuleIndex,
                                 RestResult),
        prepend_derived_rule(RestResult, Rule, RuleOrigins, Result)
    ;   Diagnostics = [Diagnostic | _],
        Result = error(Diagnostic)
    ).
lower_derived_bind_rules([_ | Reservations], Environment, RuleIndex,
                         Result) :-
    lower_derived_bind_rules(Reservations, Environment, RuleIndex, Result).

edge_rule_target(target(Target), ref(Target)).
edge_rule_target(Target, Target).

prepend_derived_rule(error(Diagnostic), _, _, error(Diagnostic)).
prepend_derived_rule(ok(Rules, Origins0), Rule, RuleOrigins,
                     ok([Rule | Rules], Origins)) :-
    append(RuleOrigins, Origins0, Origins).

indexed_goal_origins([], _, _, []).
indexed_goal_origins([NodeId | NodeIds], RuleIndex, GoalIndex,
                     [origin(goal(RuleIndex, GoalIndex), NodeId) | Origins]) :-
    NextGoalIndex is GoalIndex + 1,
    indexed_goal_origins(NodeIds, RuleIndex, NextGoalIndex, Origins).

replace_expression_value(Value, BindValue, Goals, BindGoals) :-
    maplist(replace_reified_term(Value, BindValue), Goals, BindGoals).

replace_reified_term(From, To, Term, Replaced) :-
    (   Term == From
    ->  Replaced = To
    ;   compound(Term)
    ->  Term =.. [Functor | Arguments],
        maplist(replace_reified_term(From, To), Arguments,
                ReplacedArguments),
        Replaced =.. [Functor | ReplacedArguments]
    ;   Replaced = Term
    ).

%% Pass 1 and 2: mint every nested owner and reserve every bind in that owner.
lower_declarations(Forms, Owner, ModuleIdentity, Result) :-
    lower_declarations(Forms, Owner, ModuleIdentity, 0, Result).

lower_declarations([], _, _, _, ok([], [], [], [], [])).
lower_declarations([Form | Forms], Owner, ModuleIdentity, Index, Result) :-
    (   bind_form(Form, _, _, _)
    ->  lower_bind(Form, Owner, ModuleIdentity, Index, BindResult),
        NextIndex is Index + 1,
        continue_declarations(BindResult, Forms, Owner, ModuleIdentity,
                              NextIndex, Result)
    ;   lower_declarations(Forms, Owner, ModuleIdentity, Index, Result)
    ).

continue_declarations(error(Diagnostic), _, _, _, _, error(Diagnostic)).
continue_declarations(ok(Nodes0, Edges0, Relations0, Origins0,
                         Reservations0),
                      Forms, Owner, ModuleIdentity, Index, Result) :-
    lower_declarations(Forms, Owner, ModuleIdentity, Index, RestResult),
    (   RestResult = ok(Nodes1, Edges1, Relations1, Origins1, Reservations1)
    ->  append(Nodes0, Nodes1, Nodes),
        append(Edges0, Edges1, Edges),
        append(Relations0, Relations1, Relations),
        append(Origins0, Origins1, Origins),
        append(Reservations0, Reservations1, Reservations),
        Result = ok(Nodes, Edges, Relations, Origins, Reservations)
    ;   Result = RestResult
    ).

lower_bind(BindNode, Owner, ModuleIdentity, Index, Result) :-
    (   bind_form(BindNode, BindNodeId, Name, TargetNode)
    ->  (   expression_bind_target(TargetNode)
        ->  finish_derived_bind(BindNodeId, Owner, Name, TargetNode, Index,
                               Result)
        ;   lower_target(TargetNode, Owner, ModuleIdentity, TargetResult),
            finish_bind(TargetResult, BindNodeId, Owner, Name, Index, Result)
        )
    ;   node_id(BindNode, NodeId),
        Result = error(diagnostic(lower, NodeId, expected_bind))
    ).

expression_bind_target(node(_, form([node(_, atom('*')) | _]))) :- !, fail.
expression_bind_target(node(_, form([node(_, atom('+')) | _]))) :- !, fail.
expression_bind_target(node(_, form(_))).

finish_derived_bind(BindNodeId, Owner, Name, TargetNode, Index,
                    ok([], [Edge], [], Origins, [Reservation])) :-
    Edge = pending_edge(Owner, Name, deferred_expression(TargetNode), Index),
    Reservation = reservation(
                      Owner, Name,
                      deferred_expression(TargetNode, BindNodeId, Index),
                      expression),
    Origins = [origin(edge(Owner, Name, Index), BindNodeId)].

finish_bind(error(Diagnostic), _, _, _, _, error(Diagnostic)).
finish_bind(ok(TargetTerm, Kind, Nodes, NestedEdges, Relations,
               Origins0, Reservations0),
            BindNodeId, Owner, Name, Index,
            ok(Nodes, [Edge | NestedEdges], Relations, Origins,
               [Reservation | Reservations0])) :-
    Edge = pending_edge(Owner, Name, TargetTerm, Index),
    Reservation = reservation(Owner, Name, TargetTerm, Kind),
    bind_origins(Kind, TargetTerm, Owner, Name, Index, BindNodeId,
                 BindOrigins),
    append(BindOrigins, Origins0, Origins).

bind_origins(product, target(Target), Owner, Name, Index, BindNodeId,
             [ origin(edge(Owner, Name, Index), BindNodeId),
               origin(relation(Target), BindNodeId)
             ]) :-
    !.
bind_origins(_, _, Owner, Name, Index, BindNodeId,
             [origin(edge(Owner, Name, Index), BindNodeId)]).

lower_target(node(NodeId, form([node(_, atom('*')) | Bindings])),
             _ParentOwner, ModuleIdentity, Result) :-
    !,
    Owner = owner(ModuleIdentity, NodeId),
    lower_bind_list(Bindings, Owner, ModuleIdentity, 0, BindResult),
    finish_constructor_target(BindResult, NodeId, Owner, product, Result).
lower_target(node(NodeId, form([node(_, atom('+')) | Bindings])),
             _ParentOwner, ModuleIdentity, Result) :-
    !,
    Owner = owner(ModuleIdentity, NodeId),
    lower_bind_list(Bindings, Owner, ModuleIdentity, 0, BindResult),
    finish_constructor_target(BindResult, NodeId, Owner, sum, Result).
lower_target(node(_, atom(Name)), Owner, _,
             ok(name(Owner, Name), reference, [], [], [], [], [])).
lower_target(node(_, literal(Value)), _, _,
             ok(const(Value), literal, [], [], [], [], [])).
lower_target(node(NodeId, variable(_, _)), _, _,
             error(diagnostic(lower, NodeId, variable_bind_target))).
lower_target(node(NodeId, form(_)), _, _,
             error(diagnostic(lower, NodeId, unsupported_bind_target))).

finish_constructor_target(error(Diagnostic), _, _, _, error(Diagnostic)).
finish_constructor_target(
    ok(NestedNodes, Edges, NestedRelations, NestedOrigins, Reservations),
    NodeId, Owner, Kind,
    ok(target(Owner), Kind, Nodes, Edges, Relations, Origins,
       Reservations)) :-
    classifier_row(Kind, Owner, Row),
    Nodes = [node(Owner), Row | NestedNodes],
    constructor_relations(Kind, Owner, Edges, OwnRelations),
    append(OwnRelations, NestedRelations, Relations),
    Origins = [origin(node(Owner), NodeId) | NestedOrigins].

%% node/1 is the identity carrier; Kind is an ordinary classifier row.
classifier_row(product, Owner, product(Owner)).
classifier_row(sum, Owner, sum(Owner)).

constructor_relations(product, Owner, Edges,
                      [relation(Owner, Arity, KeySets)]) :-
    include(edge_owned_by(Owner), Edges, OwnEdges),
    length(OwnEdges, Arity),
    return_key_sets(Owner, OwnEdges, Arity, KeySets).
constructor_relations(sum, _, _, []).

edge_owned_by(Owner, pending_edge(Owner, _, _, _)).

return_key_sets(Owner, Edges, Arity, KeySets) :-
    findall(Index,
            member(pending_edge(Owner, return, _, Index), Edges),
            ReturnIndices),
    (   ReturnIndices = [ReturnIndex]
    ->  positions_except(Arity, ReturnIndex, Inputs),
        KeySets = [Inputs]
    ;   KeySets = []
    ).

positions_except(Arity, Except, Positions) :-
    positions_except(0, Arity, Except, Positions).

positions_except(Index, Arity, _, []) :-
    Index >= Arity,
    !.
positions_except(Index, Arity, Except, Positions) :-
    NextIndex is Index + 1,
    positions_except(NextIndex, Arity, Except, Rest),
    (   Index =:= Except
    ->  Positions = Rest
    ;   Positions = [Index | Rest]
    ).

lower_bind_list([], _, _, _, ok([], [], [], [], [])).
lower_bind_list([Bind | Binds], Owner, ModuleIdentity, Index, Result) :-
    lower_edge_bind(Bind, Owner, ModuleIdentity, Index, BindResult),
    NextIndex is Index + 1,
    continue_bind_list(BindResult, Binds, Owner, ModuleIdentity, NextIndex,
                       Result).

continue_bind_list(error(Diagnostic), _, _, _, _, error(Diagnostic)).
continue_bind_list(ok(Nodes0, Edges0, Relations0, Origins0, Reservations0),
                   Binds, Owner, ModuleIdentity, Index, Result) :-
    lower_bind_list(Binds, Owner, ModuleIdentity, Index, RestResult),
    (   RestResult = ok(Nodes1, Edges1, Relations1, Origins1, Reservations1)
    ->  append(Nodes0, Nodes1, Nodes),
        append(Edges0, Edges1, Edges),
        append(Relations0, Relations1, Relations),
        append(Origins0, Origins1, Origins),
        append(Reservations0, Reservations1, Reservations),
        Result = ok(Nodes, Edges, Relations, Origins, Reservations)
    ;   Result = RestResult
    ).

lower_edge_bind(BindNode, Owner, ModuleIdentity, Index, Result) :-
    (   edge_bind_form(BindNode, BindNodeId, LabelNode, TargetNode)
    ->  (   LabelNode = node(_, atom(_))
        ->  lower_bind(BindNode, Owner, ModuleIdentity, Index, Result)
        ;   LabelNode = node(_, form(_))
        ->  lower_target(TargetNode, Owner, ModuleIdentity, TargetResult),
            finish_compound_edge_bind(
                TargetResult, BindNodeId, Owner, LabelNode, Index, Result)
        ;   node_id(LabelNode, LabelNodeId),
            Result = error(diagnostic(
                               lower, LabelNodeId,
                               edge_label_must_be_atom_or_expression))
        )
    ;   node_id(BindNode, NodeId),
        Result = error(diagnostic(lower, NodeId, expected_bind))
    ).

finish_compound_edge_bind(error(Diagnostic), _, _, _, _,
                          error(Diagnostic)).
finish_compound_edge_bind(
    ok(TargetTerm, _, Nodes, NestedEdges, Relations,
       Origins0, Reservations0),
    BindNodeId, Owner, LabelNode, Index,
    ok(Nodes, [Edge | NestedEdges], Relations, Origins,
       [Reservation | Reservations0])) :-
    Marker = compound_label(BindNodeId),
    Edge = pending_edge(
               Owner, Marker,
               deferred_compound_edge(LabelNode, TargetTerm), Index),
    Reservation = reservation(
                      Owner, Marker,
                      derived_compound_edge(
                          LabelNode, TargetTerm, BindNodeId, Index),
                      compound_edge),
    Origins = [origin(edge(Owner, Marker, Index), BindNodeId) | Origins0].

%% Pass 3: lower facts and rules after all declarations are reserved.
lower_executables(Forms, Owner, Environment, RuleIndex, Result) :-
    lower_executables(Forms, Owner, Environment, 0, RuleIndex, Result).

lower_executables([], _, _, _, _, ok([], [], [])).
lower_executables([Form | Forms], Owner, Environment,
                  SeedIndex, RuleIndex, Result) :-
    (   bind_form(Form, _, _, _)
    ->  lower_executables(Forms, Owner, Environment,
                          SeedIndex, RuleIndex, Result)
    ;   rule_form(Form, RuleNodeId, HeadNode, BodyNodes)
    ->  lower_rule(HeadNode, BodyNodes, Owner, Environment,
                   RuleIndex, RuleNodeId, RuleResult),
        NextRuleIndex is RuleIndex + 1,
        continue_rule(RuleResult, Forms, Owner, Environment,
                      SeedIndex, NextRuleIndex, Result)
    ;   lower_seed(Form, Owner, Environment, SeedIndex,
                   SeedResult),
        NextSeedIndex is SeedIndex + 1,
        continue_seed(SeedResult, Forms, Owner, Environment,
                      NextSeedIndex, RuleIndex, Result)
    ).

continue_rule(error(Diagnostic), _, _, _, _, _, error(Diagnostic)).
continue_rule(ok(Rule, RuleOrigins), Forms, Owner, Environment,
              SeedIndex, RuleIndex, Result) :-
    lower_executables(Forms, Owner, Environment,
                      SeedIndex, RuleIndex, RestResult),
    prepend_rule(RestResult, Rule, RuleOrigins, Result).

prepend_rule(error(Diagnostic), _, _, error(Diagnostic)).
prepend_rule(ok(Seeds, Rules, Origins0), Rule, RuleOrigins,
             ok(Seeds, [Rule | Rules], Origins)) :-
    append(RuleOrigins, Origins0, Origins).

continue_seed(error(Diagnostic), _, _, _, _, _, error(Diagnostic)).
continue_seed(ok(Seed, SeedOrigin), Forms, Owner, Environment,
              SeedIndex, RuleIndex, Result) :-
    lower_executables(Forms, Owner, Environment,
                      SeedIndex, RuleIndex, RestResult),
    prepend_seed(RestResult, Seed, SeedOrigin, Result).

prepend_seed(error(Diagnostic), _, _, error(Diagnostic)).
prepend_seed(ok(Seeds, Rules, Origins), Seed, SeedOrigin,
             ok([Seed | Seeds], Rules, [SeedOrigin | Origins])).

lower_seed(Node, Owner, Environment, SeedIndex, Result) :-
    lower_call(Node, Owner, Environment, CallResult),
    node_id(Node, NodeId),
    (   CallResult = ok(Call, [], [])
    ->  (   call_contains_var(Call)
        ->  Result = error(diagnostic(lower, NodeId, variable_in_seed))
        ;   Result = ok(Call, origin(seed(SeedIndex), NodeId))
        )
    ;   CallResult = ok(_, [_ | _], _)
    ->  Result = error(diagnostic(lower, NodeId,
                                  expression_goals_in_seed))
    ;   Result = CallResult
    ).

lower_rule(HeadNode, BodyNodes, Owner, Environment,
           RuleIndex, RuleNodeId, Result) :-
    lower_head_call(HeadNode, Owner, Environment, HeadResult),
    (   HeadResult = ok(Head, HeadGoals, HeadGoalNodes)
    ->  lower_goals(BodyNodes, Owner, Environment, BodyResult),
        (   BodyResult = ok(BodyGoals, BodyGoalNodes)
        ->  append(BodyGoals, HeadGoals, Body),
            append(BodyGoalNodes, HeadGoalNodes, GoalNodes),
            indexed_goal_origins(GoalNodes, RuleIndex, 0, GoalOrigins),
            Result = ok(rule(Head, Body),
                        [origin(rule(RuleIndex), RuleNodeId) | GoalOrigins])
        ;   Result = BodyResult
        )
    ;   Result = HeadResult
    ).

lower_goals([], _, _, ok([], [])).
lower_goals([Node | Nodes], Owner, Environment, Result) :-
    lower_goal(Node, Owner, Environment, GoalResult),
    (   GoalResult = ok(OwnGoals, OwnGoalNodes)
    ->  lower_goals(Nodes, Owner, Environment, RestResult),
        (   RestResult = ok(RestGoals, RestGoalNodes)
        ->  append(OwnGoals, RestGoals, Goals),
            append(OwnGoalNodes, RestGoalNodes, GoalNodes),
            Result = ok(Goals, GoalNodes)
        ;   Result = RestResult
        )
    ;   Result = GoalResult
    ).

%% Prefix not/1 is erased into explicit pending polarity before checking.
lower_goal(node(NodeId, form([node(_, atom(not)), Inner])),
           Owner, Environment, Result) :-
    !,
    lower_call(Inner, Owner, Environment, CallResult),
    pending_goal_result(CallResult, negative, NodeId, Result).
lower_goal(node(NodeId, form([node(_, atom(not)) | _])),
           _, _, error(diagnostic(lower, NodeId, invalid_negative_goal))) :-
    !.
lower_goal(node(NodeId, form([node(_, atom(count)) | _])),
           _, _,
           error(diagnostic(lower, NodeId,
                            aggregate_outside_rule_head))) :-
    !.
lower_goal(Node, Owner, Environment, Result) :-
    node_id(Node, NodeId),
    lower_call(Node, Owner, Environment, CallResult),
    pending_goal_result(CallResult, positive, NodeId, Result).

pending_goal_result(ok(Call, PrefixGoals, PrefixNodes), Polarity, NodeId,
                    ok(Goals, GoalNodes)) :-
    append(PrefixGoals, [pending_goal(Polarity, Call)], Goals),
    append(PrefixNodes, [NodeId], GoalNodes).
pending_goal_result(error(Diagnostic), _, _, error(Diagnostic)).

lower_head_call(Node, Owner, Environment, Result) :-
    lower_call_mode(head, Node, Owner, Environment, Result).

lower_call(Node, Owner, Environment, Result) :-
    lower_call_mode(plain, Node, Owner, Environment, Result).

lower_call_mode(Mode,
           node(NodeId, form([node(_, atom(Name)) | ArgumentNodes])),
           Owner, Environment, Result) :-
    !,
    expression_callable(Name, Owner, Environment, CallableResult),
    (   CallableResult = ok(_, Arity, _)
    ->
        length(ArgumentNodes, ObservedArity),
        (   ObservedArity =:= Arity
        ->  lower_arguments(Mode, ArgumentNodes, Owner, Environment,
                            ArgumentResult),
            finish_call_arguments(Mode, Name, NodeId, Owner,
                                  ArgumentResult, Result)
        ;   Result = error(diagnostic(
                               lower, NodeId,
                               arity_mismatch(Name, Arity, ObservedArity)))
        )
    ;   CallableResult = error(Reason),
        Result = error(diagnostic(lower, NodeId, Reason))
    ).
lower_call_mode(_, Node, _, _,
                error(diagnostic(lower, NodeId, expected_call))) :-
    node_id(Node, NodeId).

finish_call_arguments(_, _, _, _, error(Diagnostic), error(Diagnostic)).
finish_call_arguments(head, Name, NodeId, Owner,
                      ok(Arguments, Goals, GoalNodes), Result) :-
    !,
    include(count_aggregate, Arguments, Aggregates),
    length(Aggregates, AggregateCount),
    (   AggregateCount =< 1
    ->  Result = ok(call(name(Owner, Name), Arguments), Goals, GoalNodes)
    ;   Result = error(diagnostic(lower, NodeId,
                                  multiple_count_aggregates(Name)))
    ).
finish_call_arguments(_, Name, _, Owner,
                      ok(Arguments, Goals, GoalNodes),
                      ok(call(name(Owner, Name), Arguments),
                         Goals, GoalNodes)).

count_aggregate(aggregate(count, _)).

lower_arguments(_, [], _, _, ok([], [], [])).
lower_arguments(Mode, [Node | Nodes], Owner, Environment, Result) :-
    lower_argument(Mode, Node, Owner, Environment, ArgumentResult),
    (   ArgumentResult = ok(Argument, OwnGoals, OwnGoalNodes)
    ->  lower_arguments(Mode, Nodes, Owner, Environment, RestResult),
        (   RestResult = ok(Arguments, RestGoals, RestGoalNodes)
        ->  append(OwnGoals, RestGoals, Goals),
            append(OwnGoalNodes, RestGoalNodes, GoalNodes),
            Result = ok([Argument | Arguments], Goals, GoalNodes)
        ;   Result = RestResult
        )
    ;   Result = ArgumentResult
    ).

%% lower_expression(+Node, +Owner, +Environment,
%%                  -Value, -Goals, -Origins, -Diagnostics) is det.
%
% Carry one value-position reader node toward flat Datalog. Goals and their
% source nodes stay parallel so a containing rule can assign final goal
% indices after nested applications have been flattened.
lower_expression(node(_, variable(Identity, _)), _, _,
                 var(Identity), [], [], []).
lower_expression(node(_, literal(Value)), _, _,
                 const(Value), [], [], []).
lower_expression(node(NodeId, atom(Name)), Owner,
                 expression_environment(Reservations, _, _),
                 Value, Goals, Origins, []) :-
    scoped_reservation(Owner, Name, Reservations, [],
                       reservation(BindOwner, Name,
                                   deferred_expression(_, _, Index),
                                   expression)),
    !,
    Value = var(derived_lookup(NodeId)),
    Goals = [pending_goal(
                 positive,
                 call(name(Owner, ':'),
                      [ ref(BindOwner), const(Name), Value, const(Index)
                      ]))],
    Origins = [NodeId].
lower_expression(node(_, atom(Name)), Owner, _,
                 name(Owner, Name), [], [], []).
lower_expression(
    node(NodeId, form([node(_, atom(Name)) | ArgumentNodes])),
    Owner, Environment, Value, Goals, Origins, Diagnostics) :-
    !,
    expression_callable(Name, Owner, Environment, CallableResult),
    lower_expression_call(
        CallableResult, Name, NodeId, ArgumentNodes, Owner, Environment,
        Value, Goals, Origins, Diagnostics).
lower_expression(node(NodeId, form([OperatorNode | ArgumentNodes])),
                 Owner, Environment,
                 Value, Goals, Origins, Diagnostics) :-
    !,
    lower_expression(OperatorNode, Owner, Environment,
                     Operator, OperatorGoals, OperatorOrigins,
                     OperatorDiagnostics),
    apply_expression_operator(
        OperatorDiagnostics, Operator, NodeId, ArgumentNodes,
        Owner, Environment, OperatorGoals, OperatorOrigins,
        Value, Goals, Origins, Diagnostics).
lower_expression(node(NodeId, form(_)), _, _,
                 none, [], [],
                 [diagnostic(lower, NodeId, unresolved_expression_form)]).

expression_callable(Name, Owner,
                    expression_environment(Reservations, Relations, _),
                    Result) :-
    (   scoped_reservation(Owner, Name, Reservations, [], Reservation)
    ->  expression_reserved_callable(Reservation, Relations, Name, Result)
    ;   kernel_relation(Name, Arity)
    ->  kernel_relation_keys_for_expression(Name, KeySets),
        Result = ok(kernel(Name), Arity, KeySets)
    ;   Result = error(undeclared_relation(Name))
    ).

scoped_reservation(Owner, Name, Reservations, Visited, Reservation) :-
    \+ memberchk(Owner, Visited),
    (   memberchk(reservation(Owner, Name, Target, Kind), Reservations)
    ->  Reservation = reservation(Owner, Name, Target, Kind)
    ;   reservation_parent(Owner, Reservations, Parent),
        scoped_reservation(Parent, Name, Reservations, [Owner | Visited],
                           Reservation)
    ).

reservation_parent(Owner, Reservations, Parent) :-
    memberchk(reservation(Parent, _, target(Owner), product), Reservations).

expression_reserved_callable(
    reservation(_, _, target(Callable), product), Relations, _, Result) :-
    !,
    (   memberchk(relation(Callable, Arity, KeySets), Relations)
    ->  Result = ok(target(Callable), Arity, KeySets)
    ;   Result = error(undeclared_relation(Callable))
    ).
expression_reserved_callable(_, _, Name, error(not_relation(Name))).

kernel_relation_keys_for_expression(':', [[0, 1], [0, 3]]).
kernel_relation_keys_for_expression(edge_snapshot, [[0, 1], [0, 3]]).
kernel_relation_keys_for_expression(nil, [[]]).
kernel_relation_keys_for_expression(cons, [[0, 1], [2]]).
kernel_relation_keys_for_expression(intern, [[0, 1]]).
kernel_relation_keys_for_expression(intern_snapshot, [[0, 1]]).
kernel_relation_keys_for_expression(predecessor, [[0, 1], [0, 2]]).
kernel_relation_keys_for_expression(_, []).

lower_expression_call(error(Reason), _, NodeId, _, _, _,
                      none, [], [], [diagnostic(lower, NodeId, Reason)]) :-
    !.
lower_expression_call(ok(Callable, Arity, KeySets), Name, NodeId,
                      ArgumentNodes,
                      Owner, Environment,
                      Value, Goals, Origins, Diagnostics) :-
    expression_return_position(Callable, Environment, NodeId,
                               ReturnIndex, ReturnDiagnostics),
    (   ReturnDiagnostics == []
    ->  ExpectedArity is Arity - 1,
        length(ArgumentNodes, ObservedArity),
        (   ObservedArity =:= ExpectedArity
        ->  expression_mode_diagnostics(
                Name, ReturnIndex, Arity, KeySets, NodeId, ModeDiagnostics),
            (   ModeDiagnostics == []
            ->  lower_expression_arguments(
                    ArgumentNodes, Owner, Environment,
                    Arguments, ArgumentGoals, ArgumentOrigins,
                    ArgumentDiagnostics),
                finish_expression_call_arguments(
                    ArgumentDiagnostics, Name, NodeId, ReturnIndex,
                    Arguments, ArgumentGoals, ArgumentOrigins, Owner,
                    Value, Goals, Origins, Diagnostics)
            ;   Value = none,
                Goals = [],
                Origins = [],
                Diagnostics = ModeDiagnostics
            )
        ;   ObservedArity < ExpectedArity
        ->  lower_expression_arguments(
                ArgumentNodes, Owner, Environment,
                Arguments, ArgumentGoals, ArgumentOrigins,
                ArgumentDiagnostics),
            finish_partial_application(
                ArgumentDiagnostics,
                callable(Owner, Name, Callable, Arity, KeySets, ReturnIndex),
                Arguments, ArgumentGoals, ArgumentOrigins,
                Value, Goals, Origins, Diagnostics)
        ;   Value = none,
            Goals = [],
            Origins = [],
            Diagnostics = [diagnostic(
                               lower, NodeId,
                               expression_arity_mismatch(
                                   Name, ExpectedArity, ObservedArity))]
        )
    ;   Value = none,
        Goals = [],
        Origins = [],
        Diagnostics = ReturnDiagnostics
    ).

finish_partial_application(
    [], Callable, Arguments, Goals, Origins,
    partial_application(Callable, Arguments), Goals, Origins, []) :-
    !.
finish_partial_application([Diagnostic | _], _, _, _, _,
                           none, [], [], [Diagnostic]).

apply_expression_operator(
    [Diagnostic | _], _, _, _, _, _, _, _,
    none, [], [], [Diagnostic]) :-
    !.
apply_expression_operator(
    [],
    partial_application(
        callable(CallOwner, Name, Callable, Arity, KeySets, ReturnIndex),
        BoundArguments),
    NodeId, ArgumentNodes, Owner, Environment,
    OperatorGoals, OperatorOrigins,
    Value, Goals, Origins, Diagnostics) :-
    !,
    lower_expression_arguments(
        ArgumentNodes, Owner, Environment,
        Arguments, ArgumentGoals, ArgumentOrigins, ArgumentDiagnostics),
    append(BoundArguments, Arguments, CombinedArguments),
    append(OperatorGoals, ArgumentGoals, CombinedGoals),
    append(OperatorOrigins, ArgumentOrigins, CombinedOrigins),
    finish_partial_application_step(
        ArgumentDiagnostics,
        callable(CallOwner, Name, Callable, Arity, KeySets, ReturnIndex),
        NodeId, CombinedArguments, CombinedGoals, CombinedOrigins,
        Value, Goals, Origins, Diagnostics).
apply_expression_operator([], Operator, NodeId, _, _, _, _, _,
                          none, [], [],
                          [diagnostic(lower, NodeId,
                                      expression_operator_not_callable(
                                          Operator))]).

finish_partial_application_step(
    [Diagnostic | _], _, _, _, _, _,
    none, [], [], [Diagnostic]) :-
    !.
finish_partial_application_step(
    [], Callable,
    NodeId, Arguments, PrefixGoals, PrefixOrigins,
    Value, Goals, Origins, Diagnostics) :-
    Callable = callable(CallOwner, Name, _, Arity, KeySets, ReturnIndex),
    ExpectedArity is Arity - 1,
    length(Arguments, ObservedArity),
    (   ObservedArity < ExpectedArity
    ->  Value = partial_application(Callable, Arguments),
        Goals = PrefixGoals,
        Origins = PrefixOrigins,
        Diagnostics = []
    ;   ObservedArity =:= ExpectedArity
    ->  expression_mode_diagnostics(
            Name, ReturnIndex, Arity, KeySets, NodeId, ModeDiagnostics),
        finish_expression_call_arguments(
            ModeDiagnostics, Name, NodeId, ReturnIndex,
            Arguments, PrefixGoals, PrefixOrigins, CallOwner,
            Value, Goals, Origins, Diagnostics)
    ;   Value = none,
        Goals = [],
        Origins = [],
        Diagnostics = [diagnostic(
                           lower, NodeId,
                           expression_arity_mismatch(
                               Name, ExpectedArity, ObservedArity))]
    ).

expression_mode_diagnostics(Name, ReturnIndex, Arity, KeySets, NodeId,
                            Diagnostics) :-
    positions_except(Arity, ReturnIndex, SuppliedPositions),
    (   member(KeySet, KeySets),
        positions_subset(KeySet, SuppliedPositions)
    ->  Diagnostics = []
    ;   Diagnostics = [diagnostic(
                            lower, NodeId,
                            ambiguous_expression_projection(
                                Name,
                                supplied(SuppliedPositions),
                                keys(KeySets),
                                return(ReturnIndex)))]
    ).

positions_subset([], _).
positions_subset([Position | Positions], Supplied) :-
    memberchk(Position, Supplied),
    positions_subset(Positions, Supplied).

lower_expression_arguments([], _, _, [], [], [], []).
lower_expression_arguments([Node | Nodes], Owner, Environment,
                           [Value | Values], Goals, Origins, Diagnostics) :-
    lower_expression(Node, Owner, Environment,
                     Value, OwnGoals, OwnOrigins, OwnDiagnostics),
    lower_expression_arguments(Nodes, Owner, Environment,
                               Values, RestGoals, RestOrigins,
                               RestDiagnostics),
    append(OwnGoals, RestGoals, Goals),
    append(OwnOrigins, RestOrigins, Origins),
    append(OwnDiagnostics, RestDiagnostics, Diagnostics).

finish_expression_call_arguments([], Name, NodeId, ReturnIndex,
                                 Arguments, ArgumentGoals, ArgumentOrigins,
                                 Owner,
                                 Value, Goals, Origins, []) :-
    !,
    Value = var(expression(NodeId)),
    insert_argument(ReturnIndex, Value, Arguments, FullArguments),
    append(ArgumentGoals,
           [pending_goal(
                positive,
                call(name(Owner, Name), FullArguments))],
           Goals),
    append(ArgumentOrigins, [NodeId], Origins).
finish_expression_call_arguments(Diagnostics, _, _, _, _, _, _, _,
                                 none, [], [], Diagnostics).

insert_argument(0, Value, Arguments, [Value | Arguments]) :- !.
insert_argument(Index, Value, [Argument | Arguments],
                [Argument | FullArguments]) :-
    Index > 0,
    NextIndex is Index - 1,
    insert_argument(NextIndex, Value, Arguments, FullArguments).

%% expression_return_position(+Callable, +Environment, +NodeId,
%%                            -ReturnIndex, -Diagnostics) is det.
%
% `return` selects one tuple position only when a call is embedded as a value.
% Explicit full calls do not pass through this predicate.
expression_return_position(Callable, Environment, NodeId,
                           ReturnIndex, Diagnostics) :-
    callable_return_indices(Callable, Environment, Indices),
    expression_return_indices(Indices, Callable, NodeId,
                              ReturnIndex, Diagnostics).

callable_return_indices(target(Callable),
                        expression_environment(_, _, Edges), Indices) :-
    findall(Index,
            member(pending_edge(Callable, return, _, Index), Edges),
            Indices0),
    sort(Indices0, Indices).
callable_return_indices(kernel(Name), _, Indices) :-
    findall(Index, kernel_return_position(Name, Index), Indices).

expression_return_indices([ReturnIndex], _, _, ReturnIndex, []) :- !.
expression_return_indices([], Callable, NodeId, none,
                          [diagnostic(lower, NodeId,
                                      expression_without_return(Callable))]) :-
    !.
expression_return_indices(Indices, Callable, NodeId, none,
                          [diagnostic(
                               lower, NodeId,
                               expression_multiple_returns(Callable,
                                                           Indices))]).

kernel_return_position(nil, 0).
kernel_return_position(cons, 2).
kernel_return_position(intern, 2).
kernel_return_position(intern_snapshot, 2).

lower_argument(head,
               node(_, form([node(_, atom(count)), Expression])),
               Owner, Environment, Result) :-
    !,
    lower_expression(Expression, Owner, Environment,
                     Value, Goals, GoalNodes, Diagnostics),
    aggregate_argument_result(Value, Goals, GoalNodes, Diagnostics, Result).
lower_argument(head, node(NodeId, form([node(_, atom(count)) | _])), _, _,
               error(diagnostic(lower, NodeId,
                                invalid_count_aggregate))) :-
    !.
lower_argument(plain, node(NodeId, form([node(_, atom(count)) | _])), _, _,
               error(diagnostic(lower, NodeId,
                                aggregate_outside_rule_head))) :-
    !.
lower_argument(_, Node, Owner, Environment, Result) :-
    lower_expression(Node, Owner, Environment,
                     Value, Goals, GoalNodes, Diagnostics),
    expression_argument_result(Value, Goals, GoalNodes, Diagnostics, Result).

expression_argument_result(Value, Goals, GoalNodes, [],
                           ok(Value, Goals, GoalNodes)) :- !.
expression_argument_result(_, _, _, [Diagnostic | _], error(Diagnostic)).

aggregate_argument_result(Value, Goals, GoalNodes, [],
                          ok(aggregate(count, Value), Goals, GoalNodes)) :- !.
aggregate_argument_result(_, _, _, [Diagnostic | _], error(Diagnostic)).

call_contains_var(call(_, Arguments)) :-
    member(Argument, Arguments),
    sub_term(var(_), Argument).

bind_form(node(NodeId,
               form([node(_, atom(':')), node(_, atom(Name)), Target])),
          NodeId, Name, Target).

edge_bind_form(node(NodeId,
                    form([node(_, atom(':')), Label, Target])),
               NodeId, Label, Target).

rule_form(node(NodeId,
               form([node(_, atom('<-')), Head | Body])),
          NodeId, Head, Body).

node_id(node(NodeId, _), NodeId).

kernel_relation(node, 1).
kernel_relation(module, 1).
kernel_relation(product, 1).
kernel_relation(sum, 1).
kernel_relation(':', 4).
kernel_relation(edge_snapshot, 4).
kernel_relation(nil, 1).
kernel_relation(cons, 3).
kernel_relation(intern, 3).
kernel_relation(intern_snapshot, 3).
kernel_relation(predecessor, 3).
kernel_relation(def, 2).
kernel_relation(head, 2).
kernel_relation(head_arg, 4).
kernel_relation(body, 4).
kernel_relation(body_arg, 5).
