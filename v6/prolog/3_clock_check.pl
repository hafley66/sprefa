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
            clock_unsupported_reason/1,
            check_clock_program/1
          ]).

:- use_module(library(lists)).
:- use_module(library(assoc)).
:- use_module(library(pairs)).
:- use_module(analyze,
              [ conjunction_goals/2, edge_headed_refs/2,
                declared_refs/2, program_refs/2, rule_head_ref/2,
                rule_is_edge/1, rule_is_level/1 ]).
:- use_module('compile/registry', [ body_surface_for_term/6, clock_role/4 ]).
:- use_module('0_program_check', [relation_kind/3]).
:- use_module('conformance/body', [rel_ref/2]).
:- use_module('0_graph', [ graph_from_edges/3, graph_cyclic_components/2 ]).

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
level_goal_dependency(Decls, Index, HeadRef, pre(Atom, _), Dependency) :-
    !,
    level_goal_dependency(Decls, Index, HeadRef, pre(Atom), Dependency).
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
edge_goal_dependency(Decls, EdgeHeaded, Index, HeadRef, HasDeparture,
                     pre(Atom, _), Dependency) :-
    !,
    edge_goal_dependency(Decls, EdgeHeaded, Index, HeadRef, HasDeparture,
                         pre(Atom), Dependency).
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
% The proof-payoff plan's rank 3 also proposed rejecting zero-grade cycles
% that carry an "occurrence-sensitive" edge, meaning edge_sample or
% edge_absence. That extension is REFUTED by measurement, not skipped.
% causal_dependency/4 excludes both roles, so neither closes a cycle here,
% and both shapes were run through the oracle under two arrival orders:
%
%   out(Item) <+ req(Item), not(seen(Item)).  seen(Item) <- out(Item).
%   out(Item) <+ req(Item), latest(seen(Item)). seen(Item) <- out(Item).
%
% Both are zero-grade cycles through the arm plane and both are
% order-independent, because the level plane freezes after arrivals and
% before edges run (tick_phase_alignment). Adding them to the causal graph
% would refuse two correct programs. The one shape that IS order-dependent
% closes through the arm's own EDGE-headed head, and it is named by the
% arm_absence_batch_invariance boundary below rather than refused, because a
% ruled fixture rides it.
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

% PINNED OFF (rulings.pl clock_path_check_pinned_off): the clock path walk,
% clock_path_conflict and unconstructive_clock_cycle, does not run on the
% compile path. The code stays as the seed of a later calculus: edge reference
% counting (a full retraction invalidating edges and refCounts, auto-drop as in
% Rust), relational cardinality over time, and det modes in the Mercury sense
% with clocks on the pipeline. The prolog flag dl6_clock_path_walk (false by
% default; the checker's own test battery sets it true) brings it back.
:- create_prolog_flag(dl6_clock_path_walk, false, [type(boolean), keep(true)]).
clock_path_walk_enabled :- current_prolog_flag(dl6_clock_path_walk, true).

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
    clock_path_walk_enabled,
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
    clock_path_walk_enabled,
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

clock_boundary(Program, not_provable(externally_fed(Ref))) :-
    Program = prog(Decls, Rules),
    declared_refs(Decls, DeclaredRefs),
    memberchk(Ref, DeclaredRefs),
    clock_dependencies(Program, Dependencies),
    member(dependency(_, Ref, _, _, _, _, _, _), Dependencies),
    \+ ( member(Rule, Rules), rule_head_ref(Rule, Ref) ).

% An arm's not(Atom) is a zero-test against a plane, and WHICH plane decides
% whether the arm is a function of tick-start state or of arrival order.
%
%   negated rel is LEVEL-headed : the level plane freezes after arrivals and
%       before edges run, so every occurrence in the batch tests the same
%       frozen extent. Measured order-independent (the match-frontier lab's
%       d2, re-measured here on the post-tick-alignment engine).
%   negated rel is EDGE-headed  : edge writes land as the batch is consumed,
%       so a later occurrence tests a plane that an earlier occurrence in the
%       SAME batch already wrote. Measured order-DEPENDENT: `out(Item) <+
%       req(Item), not(out(_))` over one batch of req(a), req(b) yields
%       out(a) in one order and out(b) in the other (the lab's d1).
%
% This is stated, not refused. `json_typed_capture_folds_into_a_keyed_int_total`
% is a live graded fixture on exactly this shape: its keyed first-write arm
% reads not(total(Repo, _)) over its own edge-headed head, and its schedule
% never puts two rows of one key in one batch, so the order sensitivity is
% real but unexercised. Refusing the shape would reject a ruled program;
% naming the boundary is what the checker can honestly own, the same call
% already made for multi_trigger_batch_invariance above.
clock_boundary(Program,
               not_provable(arm_absence_batch_invariance(RuleId, Ref))) :-
    Program = prog(_, Rules),
    edge_headed_refs(Rules, EdgeHeaded),
    clock_dependencies(Program, Dependencies),
    member(dependency(RuleId, Ref, _, b, _, negative, 0, edge_absence),
           Dependencies),
    memberchk(Ref, EdgeHeaded).

clock_unsupported_reason(clock_path_conflict(_, _, _, _)).
clock_unsupported_reason(unconstructive_clock_cycle(_, _)).

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

% ── offsets without paths ───────────────────────────────────────────────────
%
% recurrence_free_clock/6 answers ONE question: which offsets can reach Ref
% from Origin along a path that touches no delayed-recurrence node. It used to
% answer it by enumerating every such simple path, which is exponential in the
% number of parallel mid-chain routes and not in the size of the program.
% Measured on the shape below (k diamonds in series, so 2^k routes end to end,
% every route the same weight and therefore no violation to find):
%
%   | k  |   old inferences | old ms |  new inferences | new ms |
%   |----|-----------------:|-------:|----------------:|-------:|
%   |  4 |           15,030 |    1.5 |          12,191 |    1.1 |
%   |  8 |          140,478 |    6.9 |          21,178 |    1.4 |
%   | 12 |        2,888,484 |    153 |          32,616 |    2.1 |
%   | 16 |       60,464,486 |  3,283 |          44,557 |    2.9 |
%   | 20 |    1,201,719,860 | 51,103 |          57,188 |    3.5 |
%
% The old column doubles per k; the new one adds ~2,800 inferences per
% diamond. The 2026-07-31 filesystem-fold program was that shape in the wild:
% a sixteen-rel filesystem fold that four different file depths enter, and its
% compile went 30 s to 9 m 40 s at an 8 GB stack, dying with `Stack limit
% (1.0Gb) exceeded` inside this predicate's setof at the served compiler's
% default (ARCH clock_check_path_blowup). Measured end to end on that file,
% same rig: 284.80 s at an 8 GB stack becomes 14.66 s at the DEFAULT 1 GB
% stack, emitted module byte-identical (sha256 32b868c6).
%
% What replaces it is Lustre's own shape: propagate one offset per edge and
% read the answer off the NODES. A node holding two offsets IS the conflict,
% and clock_violation/2 reports it with the same reason functor and the same
% two numbers, because the offset SET per node is what the path enumeration
% was computing all along.
%
% WHY THE SETS AGREE, and it is not obvious. Propagation follows WALKS; the
% old code followed SIMPLE PATHS. The two give the same weights exactly when
% every cycle in the searched graph weighs zero: a closed walk decomposes into
% simple cycles, so a zero-weight cycle can be excised from any walk without
% moving its weight, and repeated excision leaves a simple path. That
% condition is CHECKED, never assumed, by zero_weight_cycles_only/2 --
%
%   every causal grade is >= 0 (registry clock_role/4 gives 0 or 1, and
%   causal_dependency/4 admits no other role), and
%   every causal edge inside a cyclic component weighs 0.
%
% Together those two make every cycle weigh zero: a grade-1 edge inside a
% strongly connected component would sit on a cycle of weight >= 1. When
% either half fails the old enumeration runs unchanged, so no program can
% change verdict on a graph this reasoning does not cover. The one shape that
% reaches the fallback is a component holding both a zero cycle and a positive
% one, which classify_component/3 already calls invalid and clock_violation/2
% already refuses one clause further down.
recurrence_free_clock(Nodes, Dependencies, DelayedNodes, Ref, Origin, Offset) :-
    live_causal_edges(Dependencies, DelayedNodes, Edges),
    (   zero_weight_cycles_only(Nodes, DelayedNodes, Edges)
    ->  successor_index(Edges, Successors),
        clock_origin(Nodes, Dependencies, Origin),
        propagated_offsets(Origin, Successors, Reached),
        member(Ref-Offsets, Reached),
        member(Offset, Offsets)
    ;   clock_origin(Nodes, Dependencies, Origin),
        clock_path(Origin, Ref, Dependencies, [Origin], 0, Offset, Path),
        \+ ( member(Node, Path),
             memberchk(Node, DelayedNodes) )
    ).

exclude_delayed(Nodes, DelayedNodes, LiveNodes) :-
    exclude(delayed_node(DelayedNodes), Nodes, LiveNodes).

delayed_node(DelayedNodes, Node) :-
    memberchk(Node, DelayedNodes).

% From-To-Grade triples, delayed nodes dropped at both ends. Duplicate
% triples collapse; two rules producing the SAME grade between the same pair
% are one edge, two rules producing different grades stay two edges, which is
% the parallel-route disagreement this checker exists to find.
live_causal_edges(Dependencies, DelayedNodes, Edges) :-
    findall(From-To-Grade,
            ( member(Dependency, Dependencies),
              causal_dependency(Dependency, From, To, Grade),
              \+ memberchk(From, DelayedNodes),
              \+ memberchk(To, DelayedNodes) ),
            Edges0),
    sort(Edges0, Edges).

% Two halves, and the second one only runs when it can fail. Every grade being
% zero already makes every cycle weigh zero, whatever shape the graph has, so
% the component search is skipped outright on the common program -- and that
% short-circuit is not a special case, it is the same theorem with an empty
% delaying set.
zero_weight_cycles_only(Nodes, DelayedNodes, Edges) :-
    forall(member(_-_-Grade, Edges), Grade >= 0),
    include(delaying_edge, Edges, DelayingEdges),
    (   DelayingEdges == []
    ->  true
    ;   exclude_delayed(Nodes, DelayedNodes, LiveNodes),
        findall(From-To, member(From-To-_, Edges), PlainEdges),
        graph_from_edges(LiveNodes, PlainEdges, Graph),
        graph_cyclic_components(Graph, Components),
        \+ ( member(Component, Components),
             member(From-To-_, DelayingEdges),
             memberchk(From, Component),
             memberchk(To, Component) )
    ).

delaying_edge(_-_-Grade) :-
    Grade =\= 0.

successor_index(Edges, Successors) :-
    findall(From-(To-Grade), member(From-To-Grade, Edges), Pairs0),
    keysort(Pairs0, Pairs),
    group_pairs_by_key(Pairs, Grouped),
    list_to_assoc(Grouped, Successors).

% One worklist pass. A node is re-expanded only when it gains an offset it
% did not already hold, so the work is bounded by the number of DISTINCT
% offsets in the graph rather than by the number of routes: with every cycle
% at weight zero, every reachable offset is the weight of some simple path,
% so the value domain is bounded by the count of grade-1 edges.
propagated_offsets(Origin, Successors, Reached) :-
    empty_assoc(Empty),
    put_assoc(Origin, Empty, [0], Seeded),
    propagate([Origin-0], Successors, Seeded, Final),
    assoc_to_list(Final, Reached).

propagate([], _, Assoc, Assoc).
propagate([Node-Offset | Queue0], Successors, Assoc0, Assoc) :-
    (   get_assoc(Node, Successors, Targets)
    ->  true
    ;   Targets = []
    ),
    relax(Targets, Offset, Assoc0, Assoc1, Queue0, Queue),
    propagate(Queue, Successors, Assoc1, Assoc).

relax([], _, Assoc, Assoc, Queue, Queue).
relax([To-Grade | Rest], Offset, Assoc0, Assoc, Queue0, Queue) :-
    Next is Offset + Grade,
    (   get_assoc(To, Assoc0, Known)
    ->  true
    ;   Known = []
    ),
    (   memberchk(Next, Known)
    ->  Assoc1 = Assoc0,
        Queue1 = Queue0
    ;   put_assoc(To, Assoc0, [Next | Known], Assoc1),
        Queue1 = [To-Next | Queue0]
    ),
    relax(Rest, Offset, Assoc1, Assoc, Queue1, Queue).

check_clock_program(Program) :-
    ( clock_violation(Program, Violation)
    -> throw(unsupported_construct(Violation))
    ; true
    ).
