% 0_graph.test.pl : receipts for the one graph module.
%
% The interesting unit here is graph_components/2. It is the one predicate in
% 0_graph.pl that library(ugraphs) does not supply, so it gets a DIFFERENTIAL
% oracle rather than hand-written expected answers: warshall_components/2
% below composes the same answer out of ugraphs' transitive_closure/2 on the
% graph and on its transpose. That composition is the obviously-correct
% spelling and was rejected for shipping only on cost (27,082 ms against
% 27 ms on a 1000-node chain, plans/2026-07-30-prolog-graph-buy-verdict.md).
% Slow and clear is exactly what a test oracle wants to be.
%
% SABOTAGE RECEIPTS, each applied to 0_graph.pl, measured, then reverted.
% All 15 tests pass on the real module; the counts below are failing tests:
%   - second Kosaraju pass run over the FORWARD graph instead of the
%     transpose (transpose_ugraph/2 call dropped):      12 red
%   - component_has_internal_edge/2's singleton clause relaxed to succeed
%     unconditionally:                                  10 red
%   - depth-first finish order replaced by plain vertex order:
%                                                        2 red

:- use_module(library(plunit)).
:- use_module(library(ugraphs)).
:- use_module(library(ordsets)).
:- use_module(library(lists)).
:- use_module('../../4_clock_check/0_graph',
              [ graph_from_edges/2, graph_from_edges/3, graph_nodes/2,
                graph_closure/2, graph_reaches/3, graph_components/2,
                graph_cyclic_components/2, graph_component_of/3,
                graph_topological_order/2, graph_has_cycle/1 ]).

% ── the differential oracle ─────────────────────────────────────────────────

% Mutual reachability through two Warshall closures. Note this yields only
% the CYCLIC components, because transitive_closure/2 is strict: a node off
% every cycle is absent from its own target set, so its intersection is
% empty. That is graph_cyclic_components/2's contract.
warshall_components(Graph, Components) :-
    vertices(Graph, Nodes),
    transitive_closure(Graph, Forward),
    transpose_ugraph(Graph, Reversed),
    transitive_closure(Reversed, Backward),
    findall(Component,
            ( member(Node, Nodes),
              memberchk(Node-Ahead, Forward),
              memberchk(Node-Behind, Backward),
              ord_intersection(Ahead, Behind, Component),
              Component \== [] ),
            Components0),
    sort(Components0, Components).

% ── shapes ──────────────────────────────────────────────────────────────────

shape(empty, []).
shape(single_node, []).
shape(chain, [a-b, b-c, c-d]).
shape(self_loop, [a-a]).
shape(mutual_pair, [a-b, b-a]).
shape(three_cycle, [a-b, b-c, c-a]).
shape(diamond, [a-b, a-c, b-d, c-d]).
shape(two_cycles_joined, [a-b, b-a, b-c, c-d, d-c]).
shape(cycle_with_tail, [a-b, b-c, c-b, c-d]).
% The measured flagship shape: one self-recursive rel hanging off a feed.
shape(flagship_shaped, [src-mid, mid-reach, reach-reach, reach-sink]).
shape(disconnected, [a-b, c-d, d-c]).

shape_graph(Name, Graph) :-
    shape(Name, Edges),
    ( Name == single_node
    -> graph_from_edges([lonely], Edges, Graph)
    ;  graph_from_edges(Edges, Graph)
    ).

:- begin_tests(graph_module).

test(components_match_warshall_oracle, [forall(shape(Name, _))]) :-
    shape_graph(Name, Graph),
    graph_cyclic_components(Graph, Components),
    warshall_components(Graph, Expected),
    ( Components == Expected
    -> true
    ;  format(user_error,
              "~w: cyclic components ~q, warshall oracle ~q~n",
              [Name, Components, Expected]),
       fail
    ).

% Every vertex lands in exactly one component, cyclic or not.
test(components_partition_the_vertices, [forall(shape(Name, _))]) :-
    shape_graph(Name, Graph),
    graph_components(Graph, Components),
    graph_nodes(Graph, Nodes),
    append(Components, Members0),
    msort(Members0, Members),
    Members == Nodes.

test(cyclic_components_drop_acyclic_singletons) :-
    shape_graph(cycle_with_tail, Graph),
    graph_components(Graph, All),
    graph_cyclic_components(Graph, Cyclic),
    All == [[a], [b, c], [d]],
    Cyclic == [[b, c]].

test(self_loop_is_a_cyclic_component) :-
    shape_graph(self_loop, Graph),
    graph_cyclic_components(Graph, Components),
    Components == [[a]].

test(acyclic_singleton_is_not_a_cyclic_component) :-
    shape_graph(chain, Graph),
    graph_cyclic_components(Graph, Components),
    Components == [].

test(components_are_ordered_by_smallest_member) :-
    shape_graph(two_cycles_joined, Graph),
    graph_cyclic_components(Graph, Components),
    Components == [[a, b], [c, d]].

test(component_of_finds_the_containing_component) :-
    shape_graph(two_cycles_joined, Graph),
    graph_components(Graph, Components),
    graph_component_of(Components, d, Component),
    Component == [c, d].

test(closure_is_strict_positive_length) :-
    shape_graph(chain, Graph),
    graph_closure(Graph, Closure),
    graph_reaches(Closure, a, d),
    \+ graph_reaches(Closure, a, a),
    \+ graph_reaches(Closure, d, a).

test(closure_holds_a_node_on_a_cycle) :-
    shape_graph(three_cycle, Graph),
    graph_closure(Graph, Closure),
    graph_reaches(Closure, a, a).

test(topological_order_on_a_dag) :-
    shape_graph(chain, Graph),
    graph_topological_order(Graph, Order),
    Order == [a, b, c, d],
    \+ graph_has_cycle(Graph).

test(topological_order_fails_on_a_cycle) :-
    shape_graph(three_cycle, Graph),
    \+ graph_topological_order(Graph, _),
    graph_has_cycle(Graph).

test(topological_order_fails_on_a_self_loop) :-
    shape_graph(self_loop, Graph),
    \+ graph_topological_order(Graph, _),
    graph_has_cycle(Graph).

test(isolated_vertices_survive_construction) :-
    graph_from_edges([lonely], [a-b], Graph),
    graph_nodes(Graph, Nodes),
    Nodes == [a, b, lonely],
    graph_components(Graph, Components),
    memberchk([lonely], Components).

test(duplicate_edges_collapse) :-
    graph_from_edges([a-b, a-b, b-a], Graph),
    graph_from_edges([a-b, b-a], Same),
    Graph == Same.

% ── the count rail ──────────────────────────────────────────────────────────
%
% Standing law: a formerly-quadratic path gets an operation-count assertion,
% never end-state equality alone. The removed algorithm enumerated simple
% paths, so its cost grew with graph CONNECTIVITY, not size. This pins the
% shape directly: adding edges that create one big component to a graph of
% fixed vertex count must not multiply the inference count.
%
% Under the removed all-pairs simple-path search the dense leg does not
% finish at all: it was measured past a 60 second time limit on this exact
% 20-node 60-edge shape while the sparse leg took milliseconds.
test(component_cost_does_not_explode_with_connectivity) :-
    numlist(0, 19, Nodes),
    findall(From-To,
            ( member(From, Nodes), To is From + 1, To =< 19 ),
            SparseEdges),
    findall(From-To,
            ( member(From, Nodes), member(Step, [1, 2, 3]),
              To is (From + Step) mod 20 ),
            DenseEdges),
    inference_count(sparse_components(SparseEdges), SparseCount),
    inference_count(dense_components(DenseEdges), DenseCount),
    Ratio is DenseCount / SparseCount,
    ( Ratio < 10
    -> true
    ;  format(user_error,
              "connectivity blew the count up: sparse ~w, dense ~w, ratio ~2f~n",
              [SparseCount, DenseCount, Ratio]),
       fail
    ).

sparse_components(Edges) :-
    graph_from_edges(Edges, Graph),
    graph_cyclic_components(Graph, Components),
    Components == [].

dense_components(Edges) :-
    graph_from_edges(Edges, Graph),
    graph_cyclic_components(Graph, Components),
    Components = [Single],
    length(Single, 20).

inference_count(Goal, Count) :-
    statistics(inferences, Before),
    ( call(Goal) -> true ; throw(graph_count_probe_failed(Goal)) ),
    statistics(inferences, After),
    Count is After - Before.

:- end_tests(graph_module).
