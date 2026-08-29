:- module(dl7_datalog_lower, [lower_datalog/4]).

:- use_module(library(error), [must_be/2]).

%% lower_datalog(+Unit, -Program, -Origins, -Diagnostics) is det.
%
% Lower the ground reader tree in three passes: mint constructor owners,
% reserve every bind name, then lower top-level facts and rules against the
% complete file-owner reservation table. The result remains ground compiler
% data; reference resolution and Datalog checks belong to 0_check.pl.
lower_datalog(Unit, Program, Origins, Diagnostics) :-
    must_be(ground, Unit),
    (   Unit = dl7_unit(Origin, ContentIdentity, Forms, _, _)
    ->  UnitIdentity = unit(Origin, ContentIdentity),
        ModuleOwner = module(UnitIdentity),
        lower_declarations(Forms, ModuleOwner, UnitIdentity,
                           DeclarationResult),
        lower_after_declarations(DeclarationResult, Forms, ModuleOwner,
                                 Program, Origins, Diagnostics)
    ;   Program = [],
        Origins = [],
        Diagnostics = [diagnostic(lower, unit, invalid_dl7_unit(Unit))]
    ).

lower_after_declarations(error(Diagnostic), _, _, [], [], [Diagnostic]).
lower_after_declarations(
    ok(Nodes0, Parents, Edges, Relations, DeclarationOrigins, Reservations),
    Forms, ModuleOwner, Program, Origins, Diagnostics) :-
    lower_executables(Forms, ModuleOwner, Reservations, Relations,
                      ExecutableResult),
    (   ExecutableResult = ok(Seeds, Rules, ExecutableOrigins)
    ->  Nodes = [node(ModuleOwner, module) | Nodes0],
        Program = basement_program(
                      root_graph(Nodes, Parents, Edges),
                      datalog_program(Relations, Seeds, Rules)),
        append(DeclarationOrigins, ExecutableOrigins, Origins),
        Diagnostics = []
    ;   ExecutableResult = error(Diagnostic),
        Program = [],
        Origins = [],
        Diagnostics = [Diagnostic]
    ).

%% Pass 1 and 2: mint every nested owner and reserve every bind in that owner.
lower_declarations(Forms, Owner, UnitIdentity, Result) :-
    lower_declarations(Forms, Owner, UnitIdentity, 0, Result).

lower_declarations([], _, _, _, ok([], [], [], [], [], [])).
lower_declarations([Form | Forms], Owner, UnitIdentity, Index, Result) :-
    (   bind_form(Form, _, _, _)
    ->  lower_bind(Form, Owner, UnitIdentity, Index, BindResult),
        NextIndex is Index + 1,
        continue_declarations(BindResult, Forms, Owner, UnitIdentity,
                              NextIndex, Result)
    ;   lower_declarations(Forms, Owner, UnitIdentity, Index, Result)
    ).

continue_declarations(error(Diagnostic), _, _, _, _, error(Diagnostic)).
continue_declarations(ok(Nodes0, Parents0, Edges0, Relations0, Origins0,
                         Reservations0),
                      Forms, Owner, UnitIdentity, Index, Result) :-
    lower_declarations(Forms, Owner, UnitIdentity, Index, RestResult),
    (   RestResult = ok(Nodes1, Parents1, Edges1, Relations1, Origins1,
                        Reservations1)
    ->  append(Nodes0, Nodes1, Nodes),
        append(Parents0, Parents1, Parents),
        append(Edges0, Edges1, Edges),
        append(Relations0, Relations1, Relations),
        append(Origins0, Origins1, Origins),
        append(Reservations0, Reservations1, Reservations),
        Result = ok(Nodes, Parents, Edges, Relations, Origins, Reservations)
    ;   Result = RestResult
    ).

lower_bind(BindNode, Owner, UnitIdentity, Index, Result) :-
    (   bind_form(BindNode, BindNodeId, Name, TargetNode)
    ->  lower_target(TargetNode, Owner, UnitIdentity, TargetResult),
        finish_bind(TargetResult, BindNodeId, Owner, Name, Index, Result)
    ;   node_id(BindNode, NodeId),
        Result = error(diagnostic(lower, NodeId, expected_bind))
    ).

finish_bind(error(Diagnostic), _, _, _, _, error(Diagnostic)).
finish_bind(ok(TargetTerm, Kind, Nodes, Parents, NestedEdges, Relations,
               Origins0, Reservations0),
            BindNodeId, Owner, Name, Index,
            ok(Nodes, Parents, [Edge | NestedEdges], Relations, Origins,
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
             ParentOwner, UnitIdentity, Result) :-
    !,
    Owner = owner(UnitIdentity, NodeId),
    lower_bind_list(Bindings, Owner, UnitIdentity, 0, BindResult),
    finish_constructor_target(BindResult, NodeId, Owner, ParentOwner,
                              product, Result).
lower_target(node(NodeId, form([node(_, atom('+')) | Bindings])),
             ParentOwner, UnitIdentity, Result) :-
    !,
    Owner = owner(UnitIdentity, NodeId),
    lower_bind_list(Bindings, Owner, UnitIdentity, 0, BindResult),
    finish_constructor_target(BindResult, NodeId, Owner, ParentOwner,
                              sum, Result).
lower_target(node(_, atom(Name)), Owner, _,
             ok(name(Owner, Name), reference, [], [], [], [], [], [])).
lower_target(node(_, literal(Value)), _, _,
             ok(const(Value), literal, [], [], [], [], [], [])).
lower_target(node(NodeId, variable(_, _)), _, _,
             error(diagnostic(lower, NodeId, variable_bind_target))).
lower_target(node(NodeId, form(_)), _, _,
             error(diagnostic(lower, NodeId, unsupported_bind_target))).

finish_constructor_target(error(Diagnostic), _, _, _, _, error(Diagnostic)).
finish_constructor_target(
    ok(NestedNodes, NestedParents, Edges, NestedRelations, NestedOrigins,
       Reservations),
    NodeId, Owner, ParentOwner, Kind,
    ok(target(Owner), Kind, Nodes, Parents, Edges, Relations, Origins,
       Reservations)) :-
    Nodes = [node(Owner, Kind) | NestedNodes],
    Parents = [scope_parent(Owner, ParentOwner) | NestedParents],
    constructor_relations(Kind, Owner, Edges, OwnRelations),
    append(OwnRelations, NestedRelations, Relations),
    Origins = [origin(node(Owner), NodeId) | NestedOrigins].

constructor_relations(product, Owner, Edges, [relation(Owner, Arity)]) :-
    include(edge_owned_by(Owner), Edges, OwnEdges),
    length(OwnEdges, Arity).
constructor_relations(sum, _, _, []).

edge_owned_by(Owner, pending_edge(Owner, _, _, _)).

lower_bind_list([], _, _, _, ok([], [], [], [], [], [])).
lower_bind_list([Bind | Binds], Owner, UnitIdentity, Index, Result) :-
    lower_bind(Bind, Owner, UnitIdentity, Index, BindResult),
    NextIndex is Index + 1,
    continue_bind_list(BindResult, Binds, Owner, UnitIdentity, NextIndex,
                       Result).

continue_bind_list(error(Diagnostic), _, _, _, _, error(Diagnostic)).
continue_bind_list(ok(Nodes0, Parents0, Edges0, Relations0, Origins0,
                      Reservations0),
                   Binds, Owner, UnitIdentity, Index, Result) :-
    lower_bind_list(Binds, Owner, UnitIdentity, Index, RestResult),
    (   RestResult = ok(Nodes1, Parents1, Edges1, Relations1, Origins1,
                        Reservations1)
    ->  append(Nodes0, Nodes1, Nodes),
        append(Parents0, Parents1, Parents),
        append(Edges0, Edges1, Edges),
        append(Relations0, Relations1, Relations),
        append(Origins0, Origins1, Origins),
        append(Reservations0, Reservations1, Reservations),
        Result = ok(Nodes, Parents, Edges, Relations, Origins, Reservations)
    ;   Result = RestResult
    ).

%% Pass 3: lower facts and rules after all declarations are reserved.
lower_executables(Forms, Owner, Reservations, Relations, Result) :-
    lower_executables(Forms, Owner, Reservations, Relations, 0, 0, Result).

lower_executables([], _, _, _, _, _, ok([], [], [])).
lower_executables([Form | Forms], Owner, Reservations, Relations,
                  SeedIndex, RuleIndex, Result) :-
    (   bind_form(Form, _, _, _)
    ->  lower_executables(Forms, Owner, Reservations, Relations,
                          SeedIndex, RuleIndex, Result)
    ;   rule_form(Form, RuleNodeId, HeadNode, BodyNodes)
    ->  lower_rule(HeadNode, BodyNodes, Owner, Reservations, Relations,
                   RuleIndex, RuleNodeId, RuleResult),
        NextRuleIndex is RuleIndex + 1,
        continue_rule(RuleResult, Forms, Owner, Reservations, Relations,
                      SeedIndex, NextRuleIndex, Result)
    ;   lower_seed(Form, Owner, Reservations, Relations, SeedIndex,
                   SeedResult),
        NextSeedIndex is SeedIndex + 1,
        continue_seed(SeedResult, Forms, Owner, Reservations, Relations,
                      NextSeedIndex, RuleIndex, Result)
    ).

continue_rule(error(Diagnostic), _, _, _, _, _, _, error(Diagnostic)).
continue_rule(ok(Rule, RuleOrigins), Forms, Owner, Reservations, Relations,
              SeedIndex, RuleIndex, Result) :-
    lower_executables(Forms, Owner, Reservations, Relations,
                      SeedIndex, RuleIndex, RestResult),
    prepend_rule(RestResult, Rule, RuleOrigins, Result).

prepend_rule(error(Diagnostic), _, _, error(Diagnostic)).
prepend_rule(ok(Seeds, Rules, Origins0), Rule, RuleOrigins,
             ok(Seeds, [Rule | Rules], Origins)) :-
    append(RuleOrigins, Origins0, Origins).

continue_seed(error(Diagnostic), _, _, _, _, _, _, error(Diagnostic)).
continue_seed(ok(Seed, SeedOrigin), Forms, Owner, Reservations, Relations,
              SeedIndex, RuleIndex, Result) :-
    lower_executables(Forms, Owner, Reservations, Relations,
                      SeedIndex, RuleIndex, RestResult),
    prepend_seed(RestResult, Seed, SeedOrigin, Result).

prepend_seed(error(Diagnostic), _, _, error(Diagnostic)).
prepend_seed(ok(Seeds, Rules, Origins), Seed, SeedOrigin,
             ok([Seed | Seeds], Rules, [SeedOrigin | Origins])).

lower_seed(Node, Owner, Reservations, Relations, SeedIndex, Result) :-
    lower_call(Node, Owner, Reservations, Relations, CallResult),
    node_id(Node, NodeId),
    (   CallResult = ok(Call)
    ->  (   call_contains_var(Call)
        ->  Result = error(diagnostic(lower, NodeId, variable_in_seed))
        ;   Result = ok(Call, origin(seed(SeedIndex), NodeId))
        )
    ;   Result = CallResult
    ).

lower_rule(HeadNode, BodyNodes, Owner, Reservations, Relations,
           RuleIndex, RuleNodeId, Result) :-
    lower_call(HeadNode, Owner, Reservations, Relations, HeadResult),
    (   HeadResult = ok(Head)
    ->  lower_goals(BodyNodes, Owner, Reservations, Relations, RuleIndex, 0,
                    BodyResult),
        (   BodyResult = ok(Body, GoalOrigins)
        ->  Result = ok(rule(Head, Body),
                        [origin(rule(RuleIndex), RuleNodeId) | GoalOrigins])
        ;   Result = BodyResult
        )
    ;   Result = HeadResult
    ).

lower_goals([], _, _, _, _, _, ok([], [])).
lower_goals([Node | Nodes], Owner, Reservations, Relations, RuleIndex,
            GoalIndex, Result) :-
    lower_call(Node, Owner, Reservations, Relations, CallResult),
    (   CallResult = ok(Call)
    ->  node_id(Node, NodeId),
        NextGoalIndex is GoalIndex + 1,
        lower_goals(Nodes, Owner, Reservations, Relations, RuleIndex,
                    NextGoalIndex, RestResult),
        (   RestResult = ok(Calls, Origins)
        ->  Result = ok([Call | Calls],
                        [origin(goal(RuleIndex, GoalIndex), NodeId) | Origins])
        ;   Result = RestResult
        )
    ;   Result = CallResult
    ).

lower_call(node(NodeId, form([node(_, atom(Name)) | ArgumentNodes])),
           Owner, Reservations, Relations, Result) :-
    !,
    (   memberchk(reservation(Owner, Name, target(Target), product),
                  Reservations)
    ->  memberchk(relation(Target, Arity), Relations),
        length(ArgumentNodes, ObservedArity),
        (   ObservedArity =:= Arity
        ->  lower_arguments(ArgumentNodes, Owner, ArgumentResult),
            (   ArgumentResult = ok(Arguments)
            ->  Result = ok(call(name(Owner, Name), Arguments))
            ;   Result = ArgumentResult
            )
        ;   Result = error(diagnostic(
                               lower, NodeId,
                               arity_mismatch(Name, Arity, ObservedArity)))
        )
    ;   memberchk(reservation(Owner, Name, _, _), Reservations)
    ->  Result = error(diagnostic(lower, NodeId, not_relation(Name)))
    ;   Result = error(diagnostic(lower, NodeId, undeclared_relation(Name)))
    ).
lower_call(Node, _, _, _, error(diagnostic(lower, NodeId, expected_call))) :-
    node_id(Node, NodeId).

lower_arguments([], _, ok([])).
lower_arguments([Node | Nodes], Owner, Result) :-
    lower_argument(Node, Owner, ArgumentResult),
    (   ArgumentResult = ok(Argument)
    ->  lower_arguments(Nodes, Owner, RestResult),
        (   RestResult = ok(Arguments)
        ->  Result = ok([Argument | Arguments])
        ;   Result = RestResult
        )
    ;   Result = ArgumentResult
    ).

lower_argument(node(_, variable(Identity, _)), _, ok(var(Identity))).
lower_argument(node(_, literal(Value)), _, ok(const(Value))).
lower_argument(node(_, atom(Name)), Owner, ok(name(Owner, Name))).
lower_argument(node(NodeId, form(_)), _,
               error(diagnostic(lower, NodeId, nested_call_argument))).

call_contains_var(call(_, Arguments)) :- member(var(_), Arguments).

bind_form(node(NodeId,
               form([node(_, atom(':')), node(_, atom(Name)), Target])),
          NodeId, Name, Target).

rule_form(node(NodeId,
               form([node(_, atom('<-')), Head | Body])),
          NodeId, Head, Body).

node_id(node(NodeId, _), NodeId).
