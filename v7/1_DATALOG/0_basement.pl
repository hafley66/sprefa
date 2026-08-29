:- module(dl7_basement, [lower_datalog/4, check_datalog/4]).

:- use_module(library(error), [must_be/2]).

%% lower_datalog(+Unit, -Program, -Origins, -Diagnostics) is det.
%
% Lower the ground reader tree in three passes: mint constructor owners,
% reserve every bind name, then lower top-level facts and rules against the
% complete file-owner reservation table. The result remains ground compiler
% data; reference resolution and Datalog checks run later in this module.
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
    ok(Nodes0, Edges, Relations, DeclarationOrigins, Reservations),
    Forms, ModuleOwner, Program, Origins, Diagnostics) :-
    lower_executables(Forms, ModuleOwner, Reservations, Relations,
                      ExecutableResult),
    (   ExecutableResult = ok(Seeds, Rules, ExecutableOrigins)
    ->          Nodes = [module(ModuleOwner) | Nodes0],
        Program = basement_program(
                      root_graph(Nodes, Edges),
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

lower_declarations([], _, _, _, ok([], [], [], [], [])).
lower_declarations([Form | Forms], Owner, UnitIdentity, Index, Result) :-
    (   bind_form(Form, _, _, _)
    ->  lower_bind(Form, Owner, UnitIdentity, Index, BindResult),
        NextIndex is Index + 1,
        continue_declarations(BindResult, Forms, Owner, UnitIdentity,
                              NextIndex, Result)
    ;   lower_declarations(Forms, Owner, UnitIdentity, Index, Result)
    ).

continue_declarations(error(Diagnostic), _, _, _, _, error(Diagnostic)).
continue_declarations(ok(Nodes0, Edges0, Relations0, Origins0,
                         Reservations0),
                      Forms, Owner, UnitIdentity, Index, Result) :-
    lower_declarations(Forms, Owner, UnitIdentity, Index, RestResult),
    (   RestResult = ok(Nodes1, Edges1, Relations1, Origins1, Reservations1)
    ->  append(Nodes0, Nodes1, Nodes),
        append(Edges0, Edges1, Edges),
        append(Relations0, Relations1, Relations),
        append(Origins0, Origins1, Origins),
        append(Reservations0, Reservations1, Reservations),
        Result = ok(Nodes, Edges, Relations, Origins, Reservations)
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
             _ParentOwner, UnitIdentity, Result) :-
    !,
    Owner = owner(UnitIdentity, NodeId),
    lower_bind_list(Bindings, Owner, UnitIdentity, 0, BindResult),
    finish_constructor_target(BindResult, NodeId, Owner, product, Result).
lower_target(node(NodeId, form([node(_, atom('+')) | Bindings])),
             _ParentOwner, UnitIdentity, Result) :-
    !,
    Owner = owner(UnitIdentity, NodeId),
    lower_bind_list(Bindings, Owner, UnitIdentity, 0, BindResult),
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
    Nodes = [Row | NestedNodes],
    constructor_relations(Kind, Owner, Edges, OwnRelations),
    append(OwnRelations, NestedRelations, Relations),
    Origins = [origin(node(Owner), NodeId) | NestedOrigins].

%% node/1 is the identity carrier; Kind is an ordinary classifier row.
classifier_row(product, Owner, product(Owner)).
classifier_row(sum, Owner, sum(Owner)).

constructor_relations(product, Owner, Edges, [relation(Owner, Arity)]) :-
    include(edge_owned_by(Owner), Edges, OwnEdges),
    length(OwnEdges, Arity).
constructor_relations(sum, _, _, []).

edge_owned_by(Owner, pending_edge(Owner, _, _, _)).

lower_bind_list([], _, _, _, ok([], [], [], [], [])).
lower_bind_list([Bind | Binds], Owner, UnitIdentity, Index, Result) :-
    lower_bind(Bind, Owner, UnitIdentity, Index, BindResult),
    NextIndex is Index + 1,
    continue_bind_list(BindResult, Binds, Owner, UnitIdentity, NextIndex,
                       Result).

continue_bind_list(error(Diagnostic), _, _, _, _, error(Diagnostic)).
continue_bind_list(ok(Nodes0, Edges0, Relations0, Origins0, Reservations0),
                   Binds, Owner, UnitIdentity, Index, Result) :-
    lower_bind_list(Binds, Owner, UnitIdentity, Index, RestResult),
    (   RestResult = ok(Nodes1, Edges1, Relations1, Origins1, Reservations1)
    ->  append(Nodes0, Nodes1, Nodes),
        append(Edges0, Edges1, Edges),
        append(Relations0, Relations1, Relations),
        append(Origins0, Origins1, Origins),
        append(Reservations0, Reservations1, Reservations),
        Result = ok(Nodes, Edges, Relations, Origins, Reservations)
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

%% check_datalog(+BasementProgram, +Origins, -Checked, -Diagnostics) is det.
%
% Resolve every pending name through owner edges and reverse binding edges,
% check binds, indices, relation use, arities, ground seeds, and positive-rule
% safety, then emit canonical colon edges, the positive dependency graph, and
% SCC strata. Diagnostics are sorted by origin; no Checked value survives a
% diagnostic.
check_datalog(basement_program(root_graph(Nodes, PendingEdges),
                               datalog_program(Relations0, Seeds0, Rules0)),
              Origins, Checked, Diagnostics) :-
    !,
    must_be(ground, Origins),
    relations_refs(Relations0, Relations),
    bind_diagnostics(PendingEdges, Origins, BindDiags),
    resolve_edges(PendingEdges, PendingEdges, Nodes, Origins, ColonEdges,
                  EdgeDiags),
    resolve_seeds(Seeds0, 0, PendingEdges, Nodes, Relations, Origins,
                  Seeds, SeedDiags),
    resolve_rules(Rules0, 0, PendingEdges, Nodes, Relations, Origins,
                  Rules, RuleDiags),
    append([BindDiags, EdgeDiags, SeedDiags, RuleDiags], Diags),
    (   Diags == []
    ->  depends_rows(Rules, Depends0),
        sort(Depends0, Depends),
        strata_rows(Relations, Strata),
        msort(ColonEdges, SortedEdges),
        msort(Relations, SortedRelations),
        Checked = checked_datalog(root_graph(Nodes, SortedEdges),
                                  datalog_program(SortedRelations, Seeds, Rules),
                                  Depends, Strata),
        Diagnostics = []
    ;   Checked = [],
        sort(Diags, Diagnostics)
    ).
check_datalog(Program, _, [], Diagnostics) :-
    must_be(ground, Program),
    Diagnostics = [diagnostic(check, none, invalid_basement_program)].

%% Bind checks: one unique name per owner and dense zero-based indices.
bind_diagnostics(Edges, Origins, Diags) :-
    duplicate_bind_diagnostics(Edges, Origins, [], Diags0),
    dense_index_diagnostics(Edges, Edges, Origins, Diags1),
    append(Diags0, Diags1, Diags).

duplicate_bind_diagnostics([], _, _, []).
duplicate_bind_diagnostics([pending_edge(Owner, Name, _, Index) | Rest],
                           Origins, Seen, Diags) :-
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   memberchk(seen(Owner, Name), Seen)
    ->  Diags = [diagnostic(check, NodeId, duplicate_bind(Owner, Name))
                 | RestDiags]
    ;   Diags = RestDiags
    ),
    duplicate_bind_diagnostics(Rest, Origins, [seen(Owner, Name) | Seen],
                               RestDiags).

dense_index_diagnostics([], _, _, []).
dense_index_diagnostics([pending_edge(Owner, Name, _, Index) | Rest], All,
                        Origins, Diags) :-
    count_owner_edges(All, Owner, Count),
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   Index >= Count
    ->  Diags = [diagnostic(check, NodeId, non_dense_index(Owner, Index))
                 | RestDiags]
    ;   Diags = RestDiags
    ),
    dense_index_diagnostics(Rest, All, Origins, RestDiags).

count_owner_edges([], _, 0).
count_owner_edges([pending_edge(Owner, _, _, _) | Rest], Owner, Count) :-
    !,
    count_owner_edges(Rest, Owner, RestCount),
    Count is RestCount + 1.
count_owner_edges([_|Rest], Owner, Count) :-
    count_owner_edges(Rest, Owner, Count).

%% Pending edges become canonical ':'(Owner, Name, Target, Index) edges.
resolve_edges([], _, _, _, [], []).
resolve_edges([pending_edge(Owner, Name, Target, Index) | Rest], All, Nodes,
              Origins, [':'(Owner, Name, Resolved, Index) | Edges], Diags) :-
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   resolve_target(Target, All, Nodes, [], Resolved)
    ->  Diags = RestDiags
    ;   Resolved = Target,
        Diags = [diagnostic(check, NodeId, unresolved_name(Name)) | RestDiags]
    ),
    resolve_edges(Rest, All, Nodes, Origins, Edges, RestDiags).

%% resolve_target(+TargetTerm, +Edges, +Nodes, +Visited, -Resolved) is semidet.
resolve_target(target(Target), _, _, _, ref(Target)).
resolve_target(const(Value), _, _, _, const(Value)).
resolve_target(name(Owner, Name), Edges, Nodes, Visited, Resolved) :-
    resolve_name(Owner, Name, Edges, Nodes, Visited, Resolved).

%% resolve_name(+Owner, +Name, +Edges, +Nodes, +Visited, -Resolved) is semidet.
%
% Local owner edge first, then the reverse binding edge to the containing
% owner; a module owner resolves the four pinned primitive names.
resolve_name(Owner, Name, Edges, Nodes, Visited, Resolved) :-
    \+ memberchk(Owner, Visited),
    (   memberchk(pending_edge(Owner, Name, Target, _), Edges)
    ->  resolve_target(Target, Edges, Nodes, [Owner | Visited], Resolved)
    ;   parent_owner(Owner, Edges, Parent),
        resolve_name(Parent, Name, Edges, Nodes, [Owner | Visited], Resolved)
    ;   memberchk(module(Owner), Nodes),
        primitive_name(Name),
        Resolved = ref(primitive(Name))
    ).

parent_owner(Owner, Edges, Parent) :-
    memberchk(pending_edge(Parent, _, target(Owner), _), Edges).

primitive_name(int).
primitive_name(text).
primitive_name(any).
primitive_name(type).

%% Seeds resolve to ground calls over declared product relations.
resolve_seeds([], _, _, _, _, _, [], []).
resolve_seeds([Seed | Rest], SeedIndex, Edges, Nodes, Relations, Origins,
              [ResolvedSeed | Seeds], Diags) :-
    seed_origin(Origins, SeedIndex, NodeId),
    resolve_call(Seed, Edges, Nodes, Relations, Result),
    (   Result = ok(call(Target, Args)),
        \+ member(var(_), Args)
    ->  ResolvedSeed = call(Target, Args),
        Diags = RestDiags
    ;   Result = ok(call(_, _))
    ->  ResolvedSeed = Seed,
        Diags = [diagnostic(check, NodeId, non_ground_seed) | RestDiags]
    ;   Result = error(Reason)
    ->  ResolvedSeed = Seed,
        Diags = [diagnostic(check, NodeId, Reason) | RestDiags]
    ),
    NextSeedIndex is SeedIndex + 1,
    resolve_seeds(Rest, NextSeedIndex, Edges, Nodes, Relations, Origins,
                  Seeds, RestDiags).

%% Rules resolve head and body calls and check positive-rule safety.
resolve_rules([], _, _, _, _, _, [], []).
resolve_rules([Rule | Rest], RuleIndex, Edges, Nodes, Relations, Origins,
              [ResolvedRule | Rules], Diags) :-
    Rule = rule(Head, Body),
    rule_origin(Origins, RuleIndex, NodeId),
    resolve_call(Head, Edges, Nodes, Relations, HeadResult),
    resolve_goals(Body, RuleIndex, 0, Edges, Nodes, Relations, Origins,
                  BodyResult, GoalDiags),
    (   HeadResult = ok(ResolvedHead),
        BodyResult = ok(ResolvedBody)
    ->  ResolvedRule = rule(ResolvedHead, ResolvedBody),
        head_safety_diagnostics(ResolvedHead, ResolvedBody, Origins,
                                RuleIndex, SafetyDiags),
        append(GoalDiags, SafetyDiags, OwnDiags)
    ;   ResolvedRule = Rule,
        (   HeadResult = error(Reason)
        ->  OwnDiags = [diagnostic(check, NodeId, Reason) | GoalDiags]
        ;   OwnDiags = GoalDiags
        )
    ),
    NextRuleIndex is RuleIndex + 1,
    resolve_rules(Rest, NextRuleIndex, Edges, Nodes, Relations, Origins,
                  Rules, RestDiags),
    append(OwnDiags, RestDiags, Diags).

resolve_goals([], _, _, _, _, _, _, ok([]), []).
resolve_goals([Goal | Rest], RuleIndex, GoalIndex, Edges, Nodes, Relations,
              Origins, Result, Diags) :-
    goal_origin(Origins, RuleIndex, GoalIndex, NodeId),
    resolve_call(Goal, Edges, Nodes, Relations, GoalResult),
    NextGoalIndex is GoalIndex + 1,
    resolve_goals(Rest, RuleIndex, NextGoalIndex, Edges, Nodes, Relations,
                  Origins, RestResult, RestDiags),
    (   GoalResult = ok(ResolvedGoal),
        RestResult = ok(RestGoals)
    ->  Result = ok([ResolvedGoal | RestGoals])
    ;   Result = error(rule)
    ),
    (   GoalResult = error(Reason)
    ->  Diags = [diagnostic(check, NodeId, Reason) | RestDiags]
    ;   Diags = RestDiags
    ).

%% resolve_call(+Call, +Edges, +Nodes, +Relations, -Result) is det.
%
% Result is ok(call(ref(Relation), ResolvedArgs)) or error(Reason).
resolve_call(call(name(Owner, Name), Args), Edges, Nodes, Relations, Result) :-
    (   resolve_name(Owner, Name, Edges, Nodes, [], Target)
    ->  (   Target = ref(_)
        ->  (   memberchk(relation(Target, Arity), Relations)
            ->  length(Args, ObservedArity),
                (   ObservedArity =:= Arity
                ->  resolve_args(Args, Edges, Nodes, ArgsResult),
                    (   ArgsResult = ok(ResolvedArgs)
                    ->  Result = ok(call(Target, ResolvedArgs))
                    ;   Result = ArgsResult
                    )
                ;   Result = error(arity_mismatch(Name, Arity, ObservedArity))
                )
            ;   Result = error(undeclared_relation(Name))
            )
        ;   Result = error(not_relation(Name))
        )
    ;   Result = error(unresolved_name(Name))
    ).

resolve_args([], _, _, ok([])).
resolve_args([Arg | Rest], Edges, Nodes, Result) :-
    (   Arg = name(ArgOwner, ArgName)
    ->  (   resolve_name(ArgOwner, ArgName, Edges, Nodes, [], Resolved)
        ->  resolve_args(Rest, Edges, Nodes, RestResult),
            (   RestResult = ok(RestResolved)
            ->  Result = ok([Resolved | RestResolved])
            ;   Result = RestResult
            )
        ;   Result = error(unresolved_name(ArgName))
        )
    ;   resolve_args(Rest, Edges, Nodes, RestResult),
        (   RestResult = ok(RestResolved)
        ->  Result = ok([Arg | RestResolved])
        ;   Result = RestResult
        )
    ).

%% Every head var(Identity) must occur in a positive body call.
head_safety_diagnostics(call(_, HeadArgs), Body, Origins, RuleIndex, Diags) :-
    rule_origin(Origins, RuleIndex, NodeId),
    findall(Var, member(var(Var), HeadArgs), HeadVars),
    findall(Var, (member(call(_, BodyArgs), Body),
                  member(var(Var), BodyArgs)), BodyVars),
    unsafe_vars(HeadVars, BodyVars, NodeId, Diags).

unsafe_vars([], _, _, []).
unsafe_vars([Var | Rest], BodyVars, NodeId, Diags) :-
    (   memberchk(Var, BodyVars)
    ->  Diags = RestDiags
    ;   Diags = [diagnostic(check, NodeId, unsafe_head_var(Var)) | RestDiags]
    ),
    unsafe_vars(Rest, BodyVars, NodeId, RestDiags).

%% One dependency row per rule; sort/2 keeps distinct tuples.
depends_rows([], []).
depends_rows([rule(call(HeadRef, _), Body) | Rest], Depends) :-
    body_refs(Body, HeadRef, OwnDeps),
    depends_rows(Rest, RestDeps),
    append(OwnDeps, RestDeps, Depends).

body_refs([], _, []).
body_refs([call(BodyRef, _) | Rest], HeadRef,
          [depends(HeadRef, BodyRef, positive) | More]) :-
    body_refs(Rest, HeadRef, More).

%% One stratum row per declared relation; positive-only graphs sit at zero.
strata_rows(Relations, Strata) :-
    strata_rows(Relations, [], Strata).

strata_rows([], Strata, Strata).
strata_rows([relation(Relation, _) | Rest], Acc, Strata) :-
    strata_rows(Rest, [stratum(Relation, 0) | Acc], Strata).

relations_refs([], []).
relations_refs([relation(Target, Arity) | Rest],
               [relation(ref(Target), Arity) | Refs]) :-
    relations_refs(Rest, Refs).

edge_origin(Origins, Owner, Name, Index, NodeId) :-
    memberchk(origin(edge(Owner, Name, Index), NodeId), Origins),
    !.
edge_origin(_, _, _, _, none).

seed_origin(Origins, SeedIndex, NodeId) :-
    memberchk(origin(seed(SeedIndex), NodeId), Origins),
    !.
seed_origin(_, _, none).

rule_origin(Origins, RuleIndex, NodeId) :-
    memberchk(origin(rule(RuleIndex), NodeId), Origins),
    !.
rule_origin(_, _, none).

goal_origin(Origins, RuleIndex, GoalIndex, NodeId) :-
    memberchk(origin(goal(RuleIndex, GoalIndex), NodeId), Origins),
    !.
goal_origin(_, _, _, none).
