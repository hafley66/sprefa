% 3_clock_check.pl : ring, sign, grade and inferred-clock projection.
%
% The checker reads the expanded program already used by analyze/strat/lower.
% It introduces no source syntax and no runtime storage. Its facts are
% queryable Prolog terms during compilation and in deterministic receipts.

:- module(clock_check,
          [ clock_dependencies/2,
            clock_dependency/8,
            inferred_clock/4,
            clock_fact/5,
            clock_scc/3,
            clock_violation/2,
            clock_boundary/2,
            clock_refusal_reason/1,
            check_clock_program/1
          ]).

:- use_module(library(lists)).
:- use_module(analyze,
              [ conjunction_goals/2, edge_headed_refs/2,
                program_refs/2, rule_head_ref/2,
                rule_is_edge/1, rule_is_level/1 ]).
:- use_module(registry, [ body_surface_for_term/6, clock_role/4 ]).
:- use_module('../0_program_check', [relation_kind/3]).
:- use_module('../conformance/body', [rel_ref/2]).
:- use_module('../0_graph', [ graph_from_edges/3, graph_cyclic_components/2 ]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% clock_dependency(
%   Program, RuleId, FromRef, ToRef, ReadRing, WriteRing, Sign, Grade).
clock_dependency(Program, RuleId, From, To, ReadRing, WriteRing, Sign, Grade) :-
    clock_dependencies(Program, Dependencies),
    member(dependency(RuleId, From, To, ReadRing, WriteRing, Sign, Grade, _),
           Dependencies).

clock_dependencies(prog(Decls, Rules), Dependencies) :-
    edge_headed_refs(Rules, EdgeHeaded),
    findall(Dependency,
            ( nth1(Index, Rules, Rule),
              rule_dependencies(Decls, EdgeHeaded, Index, Rule, RuleDependencies),
              member(Dependency, RuleDependencies)
            ),
            Dependencies0),
    sort(Dependencies0, Dependencies).

rule_dependencies(Decls, _EdgeHeaded, Index, Rule, Dependencies) :-
    rule_is_level(Rule),
    !,
    rule_head_ref(Rule, HeadRef),
    Rule = (_ <- Body),
    conjunction_goals(Body, Goals),
    findall(Dependency,
            ( member(Goal, Goals),
              level_goal_dependency(Decls, Index, HeadRef, Goal, Dependency)
            ),
            Dependencies).
rule_dependencies(Decls, EdgeHeaded, Index, Rule, Dependencies) :-
    rule_is_edge(Rule),
    rule_head_ref(Rule, HeadRef),
    Rule = (_ <+ Body),
    conjunction_goals(Body, Goals),
    edge_goal_dependencies(Decls, EdgeHeaded, Index, HeadRef, Goals,
                           Dependencies).

level_goal_dependency(Decls, Index, HeadRef, not(Atom), Dependency) :-
    !,
    relation_atom(Atom, FromRef),
    dependency_from_role(Decls, rule(Index, level, HeadRef), FromRef, HeadRef,
                         level_absence, Dependency).
level_goal_dependency(Decls, Index, HeadRef, latest(Atom), Dependency) :-
    !,
    relation_atom(Atom, FromRef),
    dependency_from_role(Decls, rule(Index, level, HeadRef), FromRef, HeadRef,
                         edge_sample, Dependency).
level_goal_dependency(Decls, Index, HeadRef, pre(Atom),
                      dependency(rule(Index, level, HeadRef), FromRef, HeadRef,
                                 b, b, previous, -1, pre_in_level)) :-
    !,
    relation_atom(Atom, FromRef),
    relation_plane(Decls, HeadRef, b).
level_goal_dependency(Decls, Index, HeadRef, finalize(Atom),
                      dependency(rule(Index, level, HeadRef), FromRef, HeadRef,
                                 z, b, negative, 1, finalize_in_level)) :-
    !,
    relation_atom(Atom, FromRef),
    relation_plane(Decls, HeadRef, b).
level_goal_dependency(Decls, Index, HeadRef, Atom, Dependency) :-
    relation_atom(Atom, FromRef),
    dependency_from_role(Decls, rule(Index, level, HeadRef), FromRef, HeadRef,
                         level_read, Dependency).

edge_goal_dependencies(Decls, EdgeHeaded, Index, HeadRef, Goals,
                       Dependencies) :-
    ( member(finalize(_), Goals) -> HasDeparture = true ; HasDeparture = false ),
    findall(Dependency,
            ( member(Goal, Goals),
              edge_goal_dependency(Decls, EdgeHeaded, Index, HeadRef,
                                   HasDeparture, Goal, Dependency) ),
            Dependencies).

edge_goal_dependency(Decls, _EdgeHeaded, Index, HeadRef, _,
                     finalize(Atom), Dependency) :-
    !,
    relation_atom(Atom, FromRef),
    dependency_from_role(Decls, rule(Index, edge, HeadRef), FromRef, HeadRef,
                         edge_departure, Dependency).
edge_goal_dependency(Decls, _EdgeHeaded, Index, HeadRef, _, latest(Atom),
                     Dependency) :-
    !,
    relation_atom(Atom, FromRef),
    dependency_from_role(Decls, rule(Index, edge, HeadRef), FromRef, HeadRef,
                         edge_sample, Dependency).
edge_goal_dependency(Decls, _EdgeHeaded, Index, HeadRef, _, pre(Atom),
                     Dependency) :-
    !,
    relation_atom(Atom, FromRef),
    dependency_from_role(Decls, rule(Index, edge, HeadRef), FromRef, HeadRef,
                         edge_pre, Dependency).
edge_goal_dependency(Decls, _EdgeHeaded, Index, HeadRef, _, not(Atom),
                     Dependency) :-
    !,
    relation_atom(Atom, FromRef),
    dependency_from_role(Decls, rule(Index, edge, HeadRef), FromRef, HeadRef,
                         edge_absence, Dependency).
edge_goal_dependency(Decls, _EdgeHeaded, Index, HeadRef, true, Atom,
                     Dependency) :-
    relation_atom(Atom, FromRef),
    !,
    dependency_from_role(Decls, rule(Index, edge, HeadRef), FromRef, HeadRef,
                         edge_sample, Dependency).
edge_goal_dependency(Decls, EdgeHeaded, Index, HeadRef, false, Atom,
                     dependency(rule(Index, edge, HeadRef), FromRef, HeadRef,
                                ReadRing, WriteRing, Sign, Grade, trigger)) :-
    relation_atom(Atom, FromRef),
    clock_role(edge_trigger, ReadRing, Sign, source_delay),
    relation_plane(Decls, HeadRef, WriteRing),
    ( memberchk(FromRef, EdgeHeaded) -> Grade = 1 ; Grade = 0 ).

dependency_from_role(Decls, RuleId, FromRef, HeadRef, Role,
                     dependency(RuleId, FromRef, HeadRef, ReadRing, WriteRing,
                                Sign, Grade, Role)) :-
    clock_role(Role, ReadRing, Sign, Grade),
    relation_plane(Decls, HeadRef, WriteRing).

relation_atom(Atom, Ref) :-
    compound(Atom),
    \+ body_surface_for_term(Atom, _, _, _, _, _),
    rel_ref(Atom, Ref).

relation_plane(Decls, Ref, n) :- relation_kind(Decls, Ref, log), !.
relation_plane(_, _, b).

% Only trigger and level-state dependencies advance a relation's inferred
% occurrence clock. Samples constrain an arm at its trigger clock but do not
% schedule it.
causal_dependency(dependency(_, From, To, _, _, _, Grade, Role),
                  From, To, Grade) :-
    memberchk(Role, [level_read, level_absence, trigger, edge_departure,
                     finalize_in_level]).

inferred_clock(Program, Ref, Origin, Offset) :-
    clock_dependencies(Program, Dependencies),
    program_nodes(Program, Dependencies, Nodes),
    clock_origin(Nodes, Dependencies, Origin),
    clock_path(Origin, Ref, Dependencies, [Origin], 0, Offset, _).

clock_fact(Program, Ref, Ring, clock(Origin, Offset), SccClass) :-
    Program = prog(Decls, _),
    inferred_clock(Program, Ref, Origin, Offset),
    relation_plane(Decls, Ref, Ring),
    ( clock_scc(Program, Component, SccClass), memberchk(Ref, Component)
    -> true
    ; SccClass = acyclic
    ).

program_nodes(prog(_, Rules), Dependencies, Nodes) :-
    program_refs(Rules, RuleRefs),
    findall(Ref,
            ( member(Dependency, Dependencies),
              ( Dependency = dependency(_, Ref, _, _, _, _, _, _)
              ; Dependency = dependency(_, _, Ref, _, _, _, _, _) )
            ),
            DependencyRefs),
    append(RuleRefs, DependencyRefs, All),
    sort(All, Nodes).

clock_origin(Nodes, Dependencies, Origin) :-
    member(Origin, Nodes),
    \+ ( member(Dependency, Dependencies),
         causal_dependency(Dependency, _, Origin, _) ).

clock_path(Origin, Origin, _, _, Offset, Offset, [Origin]).
clock_path(Origin, Target, Dependencies, Visited, Offset0, Offset,
           [Origin | Path]) :-
    member(Dependency, Dependencies),
    causal_dependency(Dependency, Origin, Next, Grade),
    \+ memberchk(Next, Visited),
    Offset1 is Offset0 + Grade,
    clock_path(Next, Target, Dependencies, [Next | Visited], Offset1, Offset,
               Path).

% SCC classification is queryable separately from backend capability.
% A zero-grade positive B cycle is constructive. A delayed recurrence is
% productive when every simple cycle has a positive total grade.
%
% The component set comes from 0_graph.pl. It used to come from an all-pairs
% mutual-reachability search over a private graph_reachable/4 that enumerated
% simple paths with a Visited list, which is what cost the plan phase 255 s on
% a 42-node graph (plans/2026-07-30-prolog-compile-profiling.md). Solution
% order is unchanged: graph_cyclic_components/2 returns sorted components
% sorted, so components still arrive ordered by their smallest member, which
% is the order the old `Node == First` guard produced.
clock_scc(Program, Component, Class) :-
    clock_dependencies(Program, Dependencies),
    clock_components(Program, Dependencies, Components),
    member(Component, Components),
    classify_component(Component, Dependencies, DerivedClass),
    Class = DerivedClass.

% The causal graph: program_nodes/3 supplies the vertices, so a ref that
% takes part in no causal dependency is still a vertex and still falls out as
% an acyclic singleton, exactly as the old `Component \== []` guard dropped
% it. graph_cyclic_components/2 keeps only components carrying an internal
% edge, which is what component_has_cycle/2 used to decide.
clock_components(Program, Dependencies, Components) :-
    program_nodes(Program, Dependencies, Nodes),
    findall(From-To,
            ( member(Dependency, Dependencies),
              causal_dependency(Dependency, From, To, _) ),
            Edges),
    graph_from_edges(Nodes, Edges, Graph),
    graph_cyclic_components(Graph, Components).

classify_component(Component, Dependencies, constructive_b) :-
    component_edges(Component, Dependencies, Edges),
    Edges \== [],
    forall(member(Edge, Edges),
           Edge = dependency(_, _, _, b, b, positive, 0, _)),
    !.
classify_component(Component, Dependencies, productive_delayed) :-
    findall(Sum, component_cycle_sum(Component, Dependencies, Sum), Sums),
    Sums \== [],
    forall(member(Sum, Sums), Sum > 0),
    !.
classify_component(Component, Dependencies, invalid(Reason)) :-
    findall(Sum, component_cycle_sum(Component, Dependencies, Sum), Sums),
    min_list(Sums, Minimum),
    ( Minimum =< 0
    -> Reason = nonpositive_cycle(Minimum)
    ; Reason = nonconstructive_cycle
    ).

component_edges(Component, Dependencies, Edges) :-
    include(edge_inside(Component), Dependencies, Edges).

edge_inside(Component, dependency(_, From, To, _, _, _, _, _)) :-
    memberchk(From, Component),
    memberchk(To, Component).

component_cycle_sum(Component, Dependencies, Sum) :-
    member(Start, Component),
    cycle_from(Start, Start, Component, Dependencies, [Start], 0, Sum).

cycle_from(Start, Current, Component, Dependencies, Visited, Sum0, Sum) :-
    member(Dependency, Dependencies),
    causal_dependency(Dependency, Current, Next, Grade),
    memberchk(Next, Component),
    Sum1 is Sum0 + Grade,
    ( Next == Start
    -> Sum = Sum1
    ; \+ memberchk(Next, Visited),
      cycle_from(Start, Next, Component, Dependencies, [Next | Visited],
                 Sum1, Sum)
    ).

clock_violation(Program, cross_plane(finalize_in_level_rule(Ref))) :-
    clock_dependencies(Program, Dependencies),
    member(dependency(_, Ref, _, z, b, negative, 1, finalize_in_level),
           Dependencies).
clock_violation(Program, cross_plane(pre_in_level_rule(Ref))) :-
    clock_dependencies(Program, Dependencies),
    member(dependency(_, Ref, _, b, b, previous, -1, pre_in_level),
           Dependencies).
clock_violation(prog(Decls, Rules), cross_plane(log_on_level_headed_rel(Ref))) :-
    member(kind(Ref, log), Decls),
    member(Rule, Rules),
    rule_is_level(Rule),
    rule_head_ref(Rule, Ref).
clock_violation(prog(Decls, Rules), cross_plane(keyed_level_head(Ref))) :-
    member(keyed(Ref, _), Decls),
    member(Rule, Rules),
    rule_is_level(Rule),
    rule_head_ref(Rule, Ref).
clock_violation(Program, cross_plane(latest_in_level_rule(Ref))) :-
    Program = prog(_, Rules),
    member((_ <- Body), Rules),
    conjunction_goals(Body, Goals),
    member(latest(Atom), Goals),
    relation_atom(Atom, Ref).
% The dependency set, the node set and the delayed-recurrence node set are all
% functions of Program alone, so they are computed ONCE here rather than per
% clock path. recurrence_free_clock/6 enumerates every simple path from every
% origin, and it used to call clock_scc/3 inside its own negation, which meant
% a full component search per path. That product was the plan phase's cost:
% 58 chain rules times a whole component search each.
clock_violation(Program, clock_path_conflict(Origin, Ref, Left, Right)) :-
    clock_dependencies(Program, Dependencies),
    program_nodes(Program, Dependencies, Nodes),
    delayed_recurrence_nodes(Program, Dependencies, DelayedNodes),
    setof(Offset,
          recurrence_free_clock(Nodes, Dependencies, DelayedNodes, Ref, Origin,
                                Offset),
          Offsets),
    select(Left, Offsets, Rest),
    member(Right, Rest),
    Left < Right,
    !.
clock_violation(Program, unconstructive_clock_cycle(Component, Reason)) :-
    clock_scc(Program, Component, invalid(Reason)).

% Multiple grade-zero trigger sources in one edge arm are intentional
% either-source firing: the firing source is read from Z and every other
% plain atom is sampled from current state. The dependency facts describe
% that execution shape, but cannot establish whether the author requires
% equivalent results under different outside-arrival batchings. Keep that
% limit queryable and non-refusing.
clock_boundary(Program,
               not_provable(
                 multi_trigger_batch_invariance(RuleId, TriggerRefs))) :-
    clock_dependencies(Program, Dependencies),
    findall(Candidate,
            member(dependency(Candidate, _, _, z, _, positive, 0, trigger),
                   Dependencies),
            RuleIds0),
    sort(RuleIds0, RuleIds),
    member(RuleId, RuleIds),
    findall(Ref,
            member(dependency(RuleId, Ref, _, z, _, positive, 0, trigger),
                   Dependencies),
            TriggerRefs0),
    sort(TriggerRefs0, TriggerRefs),
    TriggerRefs = [_, _ | _].

clock_refusal_reason(clock_path_conflict(_, _, _, _)).
clock_refusal_reason(unconstructive_clock_cycle(_, _)).

% The union of every productive_delayed component's members. classify_component
% is called with the class UNBOUND and compared afterwards, matching what
% clock_scc/3 does: its first clause tries constructive_b on its own body, so
% binding the class up front would skip that clause's guard.
delayed_recurrence_nodes(Program, Dependencies, DelayedNodes) :-
    clock_components(Program, Dependencies, Components),
    findall(Node,
            ( member(Component, Components),
              classify_component(Component, Dependencies, Class),
              Class == productive_delayed,
              member(Node, Component) ),
            Nodes0),
    sort(Nodes0, DelayedNodes).

recurrence_free_clock(Nodes, Dependencies, DelayedNodes, Ref, Origin, Offset) :-
    clock_origin(Nodes, Dependencies, Origin),
    clock_path(Origin, Ref, Dependencies, [Origin], 0, Offset, Path),
    \+ ( member(Node, Path),
         memberchk(Node, DelayedNodes) ).

check_clock_program(Program) :-
    ( clock_violation(Program, Violation)
    -> throw(unsupported_construct(Violation))
    ; true
    ).
