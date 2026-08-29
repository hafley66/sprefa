:- module(dl7_checker,
          [ check_datalog/4,
            check_goal_sequence/4
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module('../1_libtime/0_evaluator', [stratify_rules/3]).
:- use_module('0_lowerer', [kernel_relation/2]).

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
    relations_refs(Relations0, SourceRelations),
    kernel_relation_rows(KernelRelations),
    append(SourceRelations, KernelRelations, AllRelations),
    sort(AllRelations, Relations),
    bind_diagnostics(PendingEdges, Origins, BindDiags),
    resolve_edges(PendingEdges, PendingEdges, Nodes, Origins, ColonEdges,
                  EdgeDiags),
    resolve_seeds(Seeds0, 0, PendingEdges, Nodes, Relations, Origins,
                  Seeds, SeedDiags),
    resolve_rules(Rules0, 0, PendingEdges, Nodes, Relations, Origins,
                  Rules, RuleDiags),
    append([BindDiags, EdgeDiags, SeedDiags, RuleDiags], Diags),
    (   Diags == []
    ->  stratify_rules(Rules, DerivedStrata, StrataDiagnostics),
        finish_checked(StrataDiagnostics, DerivedStrata, Nodes, ColonEdges,
                       Relations, Seeds, Rules, Checked, Diagnostics)
    ;   Checked = [],
        sort(Diags, Diagnostics)
    ).
check_datalog(Program, _, [], Diagnostics) :-
    must_be(ground, Program),
    Diagnostics = [diagnostic(check, none, invalid_basement_program)].

finish_checked([], DerivedStrata, Nodes, ColonEdges, Relations, Seeds, Rules,
               Checked, []) :-
    !,
    depends_rows(Rules, Depends0),
    sort(Depends0, Depends),
    strata_rows(Relations, DerivedStrata, Strata),
    kernel_graph(KernelNodes, KernelEdges),
    append(Nodes, KernelNodes, CheckedNodes),
    append(ColonEdges, KernelEdges, AllEdges),
    msort(AllEdges, SortedEdges),
    predecessor_seeds(SortedEdges, PredecessorSeeds),
    append(Seeds, PredecessorSeeds, CheckedSeeds),
    msort(Relations, SortedRelations),
    Checked = checked_datalog(root_graph(CheckedNodes, SortedEdges),
                              datalog_program(SortedRelations, CheckedSeeds,
                                              Rules),
                              Depends, Strata).
finish_checked(Diagnostics, _, _, _, _, _, _, [], Diagnostics).

%% Bind checks: one unique name per owner and dense zero-based indices.
bind_diagnostics(Edges, Origins, Diags) :-
    duplicate_bind_diagnostics(Edges, Origins, [], Diags0),
    duplicate_index_diagnostics(Edges, Origins, [], Diags1),
    dense_index_diagnostics(Edges, Edges, Origins, Diags2),
    append([Diags0, Diags1, Diags2], Diags).

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

duplicate_index_diagnostics([], _, _, []).
duplicate_index_diagnostics([pending_edge(Owner, Name, _, Index) | Rest],
                            Origins, Seen, Diags) :-
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   memberchk(seen(Owner, Index), Seen)
    ->  Diags = [diagnostic(check, NodeId,
                            duplicate_bind_index(Owner, Index)) | RestDiags]
    ;   Diags = RestDiags
    ),
    duplicate_index_diagnostics(Rest, Origins,
                                [seen(Owner, Index) | Seen], RestDiags).

dense_index_diagnostics([], _, _, []).
dense_index_diagnostics([pending_edge(Owner, Name, _, Index) | Rest], All,
                        Origins, Diags) :-
    count_owner_edges(All, Owner, Count),
    edge_origin(Origins, Owner, Name, Index, NodeId),
    (   (   Index < 0
        ;   Index >= Count
        )
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
        kernel_relation(Name, _),
        Resolved = ref(kernel(Name))
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

kernel_relation_rows(Relations) :-
    findall(relation(ref(kernel(Name)), Arity, KeySets),
            ( kernel_relation(Name, Arity),
              kernel_relation_keys(Name, KeySets)
            ),
            Relations).

kernel_relation_keys(':', [[0, 1], [0, 3]]).
kernel_relation_keys(cons, [[0, 1], [2]]).
kernel_relation_keys(intern, [[0, 1]]).
kernel_relation_keys(predecessor, [[0, 1], [0, 2]]).
kernel_relation_keys(node, []).
kernel_relation_keys(module, []).
kernel_relation_keys(product, []).
kernel_relation_keys(sum, []).

%% Every checked dense owner-index sequence contributes its adjacent pairs.
predecessor_seeds(Edges, Seeds) :-
    findall(call(ref(kernel(predecessor)),
                 [ref(Owner), const(EarlierIndex), const(LaterIndex)]),
            ( member(':'(Owner, _, _, LaterIndex), Edges),
              LaterIndex > 0,
              EarlierIndex is LaterIndex - 1
            ),
            Seeds0),
    sort(Seeds0, Seeds).

kernel_graph(
    [ node(primitive(int)),
      node(primitive(text)),
      node(primitive(any)),
      node(primitive(type)),
      node(kernel(node)), product(kernel(node)),
      node(kernel(module)), product(kernel(module)),
      node(kernel(product)), product(kernel(product)),
      node(kernel(sum)), product(kernel(sum)),
      node(kernel(':')), product(kernel(':')),
      node(kernel(cons)), product(kernel(cons)),
      node(kernel(intern)), product(kernel(intern)),
      node(kernel(predecessor)), product(kernel(predecessor))
    ],
    [ ':'(kernel(node), id, ref(primitive(type)), 0),
      ':'(kernel(module), id, ref(primitive(type)), 0),
      ':'(kernel(product), id, ref(primitive(type)), 0),
      ':'(kernel(sum), id, ref(primitive(type)), 0),
      ':'(kernel(':'), owner, ref(primitive(type)), 0),
      ':'(kernel(':'), name, ref(primitive(text)), 1),
      ':'(kernel(':'), target, ref(primitive(any)), 2),
      ':'(kernel(':'), index, ref(primitive(int)), 3),
      ':'(kernel(cons), head, ref(primitive(any)), 0),
      ':'(kernel(cons), tail, ref(primitive(any)), 1),
      ':'(kernel(cons), return, ref(primitive(any)), 2),
      ':'(kernel(intern), constructor, ref(primitive(type)), 0),
      ':'(kernel(intern), arguments, ref(primitive(any)), 1),
      ':'(kernel(intern), return, ref(primitive(type)), 2),
      ':'(kernel(predecessor), owner, ref(primitive(type)), 0),
      ':'(kernel(predecessor), earlier, ref(primitive(int)), 1),
      ':'(kernel(predecessor), later, ref(primitive(int)), 2)
    ]).

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
        head_variables(ResolvedHead, HeadVariables),
        check_goal_sequence_failures(ResolvedBody, 0, HeadVariables, _,
                                     ModeFailures),
        mode_failure_diagnostics(ModeFailures, RuleIndex, Origins, ModeDiags),
        head_safety_diagnostics(ResolvedHead, ResolvedBody, Origins,
                                RuleIndex, SafetyDiags),
        append([GoalDiags, ModeDiags, SafetyDiags], OwnDiags)
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
resolve_goals([pending_goal(Polarity, Goal) | Rest], RuleIndex, GoalIndex,
              Edges, Nodes, Relations,
              Origins, Result, Diags) :-
    goal_origin(Origins, RuleIndex, GoalIndex, NodeId),
    resolve_call(Goal, Edges, Nodes, Relations, GoalResult),
    NextGoalIndex is GoalIndex + 1,
    resolve_goals(Rest, RuleIndex, NextGoalIndex, Edges, Nodes, Relations,
                  Origins, RestResult, RestDiags),
    (   GoalResult = ok(ResolvedGoal),
        RestResult = ok(RestGoals)
    ->  Result = ok([checked_goal(Polarity, ResolvedGoal) | RestGoals])
    ;   Result = error(rule)
    ),
    (   GoalResult = error(Reason)
    ->  Diags = [diagnostic(check, NodeId, Reason) | RestDiags]
    ;   Diags = RestDiags
    ).

%% check_goal_sequence(+Goals, +Bound0, -Bound, -Diagnostics) is det.
%
% Fold checked goals in authored order. Bound0 represents variables supplied
% by the relation call context. Ordinary positive calls bind every variable;
% constructive kernel calls first require one of their declared input modes.
check_goal_sequence(Goals, Bound0, Bound, Diagnostics) :-
    check_goal_sequence_failures(Goals, 0, Bound0, Bound, Failures),
    maplist(unlocated_mode_diagnostic, Failures, Diagnostics).

check_goal_sequence_failures([], _, Bound, Bound, []).
check_goal_sequence_failures([Goal | Goals], GoalIndex, Bound0, Bound,
                             Failures) :-
    check_goal(Goal, Bound0, Bound1, Reason),
    NextGoalIndex is GoalIndex + 1,
    check_goal_sequence_failures(Goals, NextGoalIndex, Bound1, Bound,
                                 RestFailures),
    (   Reason == none
    ->  Failures = RestFailures
    ;   Failures = [goal_failure(GoalIndex, Reason) | RestFailures]
    ).

check_goal(Goal, Bound0, Bound, Reason) :-
    goal_call(Goal, positive,
              call(ref(kernel(cons)), [Head, Tail, List])),
    !,
    (   argument_is_bound(List, Bound0)
    ;   argument_is_bound(Head, Bound0),
        argument_is_bound(Tail, Bound0)
    ->  goal_variables(Goal, Variables),
        add_variables(Variables, Bound0, Bound),
        Reason = none
    ;   Bound = Bound0,
        Reason = underconstrained_kernel_goal(cons, [[2], [0, 1]])
    ).
check_goal(Goal, Bound0, Bound, Reason) :-
    goal_call(Goal, positive,
              call(ref(kernel(intern)), [Constructor, Arguments, _])),
    !,
    (   argument_is_bound(Constructor, Bound0),
        argument_is_bound(Arguments, Bound0)
    ->  goal_variables(Goal, Variables),
        add_variables(Variables, Bound0, Bound),
        Reason = none
    ;   Bound = Bound0,
        Reason = underconstrained_kernel_goal(intern, [[0, 1]])
    ).
check_goal(Goal, Bound, Bound, negative_constructive_kernel_goal(Name)) :-
    goal_call(Goal, negative, call(ref(kernel(Name)), _)),
    memberchk(Name, [cons, intern]),
    !.
check_goal(Goal, Bound, Bound, Reason) :-
    goal_call(Goal, negative, _),
    !,
    goal_variables(Goal, Variables),
    (   variables_are_bound(Variables, Bound)
    ->  Reason = none
    ;   Reason = unbound_negative_goal(Variables)
    ).
check_goal(Goal, Bound0, Bound, none) :-
    goal_call(Goal, positive, _),
    goal_variables(Goal, Variables),
    add_variables(Variables, Bound0, Bound).

argument_is_bound(Argument, Bound) :-
    argument_variables(Argument, Variables),
    variables_are_bound(Variables, Bound).

argument_variables(Argument, Variables) :-
    findall(Identity,
            ( sub_term(Subterm, Argument),
              Subterm = var(Identity)
            ),
            Variables0),
    sort(Variables0, Variables).

variables_are_bound([], _).
variables_are_bound([Variable | Variables], Bound) :-
    memberchk(Variable, Bound),
    variables_are_bound(Variables, Bound).

add_variables([], Bound, Bound).
add_variables([Variable | Variables], Bound0, Bound) :-
    (   memberchk(Variable, Bound0)
    ->  Bound1 = Bound0
    ;   Bound1 = [Variable | Bound0]
    ),
    add_variables(Variables, Bound1, Bound).

unlocated_mode_diagnostic(goal_failure(_, Reason),
                          diagnostic(check, none, Reason)).

mode_failure_diagnostics([], _, _, []).
mode_failure_diagnostics([goal_failure(GoalIndex, Reason) | Failures],
                         RuleIndex, Origins,
                         [diagnostic(check, NodeId, Reason) | Diagnostics]) :-
    goal_origin(Origins, RuleIndex, GoalIndex, NodeId),
    mode_failure_diagnostics(Failures, RuleIndex, Origins, Diagnostics).

head_variables(call(_, Arguments), Variables) :-
    findall(Identity, member(var(Identity), Arguments), Variables0),
    sort(Variables0, Variables).

%% resolve_call(+Call, +Edges, +Nodes, +Relations, -Result) is det.
%
% Result is ok(call(ref(Relation), ResolvedArgs)) or error(Reason).
resolve_call(call(name(Owner, Name), Args), Edges, Nodes, Relations, Result) :-
    (   resolve_name(Owner, Name, Edges, Nodes, [], Target)
    ->  (   Target = ref(_)
        ->  (   memberchk(relation(Target, Arity, _), Relations)
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
    findall(Var, (member(Goal, Body),
                  goal_variables(Goal, GoalVars),
                  member(Var, GoalVars)), BodyVars),
    unsafe_vars(HeadVars, BodyVars, NodeId, Diags).

%% goal_call(+CheckedGoal, -Polarity, -Call) is det.
goal_call(checked_goal(Polarity, Call), Polarity, Call).

%% goal_variables(+CheckedGoal, -VariableIdentities) is det.
goal_variables(Goal, Variables) :-
    goal_call(Goal, _, call(_, Arguments)),
    findall(Identity, member(var(Identity), Arguments), Variables).

%% goal_dependency(+HeadRef, +CheckedGoal, -Dependency) is det.
goal_dependency(HeadRef, Goal, depends(HeadRef, BodyRef, Polarity)) :-
    goal_call(Goal, Polarity, call(BodyRef, _)).

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
body_refs([Goal | Rest], HeadRef, [Dependency | More]) :-
    goal_dependency(HeadRef, Goal, Dependency),
    body_refs(Rest, HeadRef, More).

%% One deterministic stratum row per declared relation. Relations without a
%% derived head remain at zero; the shared stratifier owns derived levels.
strata_rows(Relations, DerivedStrata, Strata) :-
    maplist(relation_stratum(DerivedStrata), Relations, Strata0),
    sort(Strata0, Strata).

relation_stratum(DerivedStrata, relation(Relation, _, _),
                 stratum(Relation, Level)) :-
    (   memberchk(stratum(Relation, DerivedLevel), DerivedStrata)
    ->  Level = DerivedLevel
    ;   Level = 0
    ).

relations_refs([], []).
relations_refs([relation(Target, Arity, KeySets) | Rest],
               [relation(ref(Target), Arity, KeySets) | Refs]) :-
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
