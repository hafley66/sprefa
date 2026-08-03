% scc.test.pl : differential receipts for the extracted Tarjan (scc.pl).
%
% For each of the 11 shapes in the 0_graph test suite, three answers must
% agree: the extracted Tarjan (scc_components/2), Kosaraju
% (graph_components/2), and the Warshall oracle where its contract applies.
% The Warshall oracle yields only the CYCLIC components (see the comment at
% the top of 0_graph.test.pl), so it is compared against the cyclic
% restriction of Tarjan's result.

:- use_module(library(plunit)).
:- use_module(library(ugraphs)).
:- use_module(library(assoc)).
:- use_module(library(ordsets)).
:- use_module(library(lists)).
:- use_module('../../0_graph',
              [ graph_from_edges/2, graph_from_edges/3,
                graph_components/2, graph_cyclic_components/2 ]).
:- use_module('../scc_extract/scc', [ scc_components/2 ]).

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

shape(empty, []).
shape(single_node, []).
shape(chain, [a-b, b-c, c-d]).
shape(self_loop, [a-a]).
shape(mutual_pair, [a-b, b-a]).
shape(three_cycle, [a-b, b-c, c-a]).
shape(diamond, [a-b, a-c, b-d, c-d]).
shape(two_cycles_joined, [a-b, b-a, b-c, c-d, d-c]).
shape(cycle_with_tail, [a-b, b-c, c-b, c-d]).
shape(flagship_shaped, [src-mid, mid-reach, reach-reach, reach-sink]).
shape(disconnected, [a-b, c-d, d-c]).

shape_graph(Name, Graph) :-
    shape(Name, Edges),
    ( Name == single_node
    -> graph_from_edges([lonely], Edges, Graph)
    ;  graph_from_edges(Edges, Graph)
    ).

:- begin_tests(scc_extract).

% Tarjan agrees with Kosaraju on all 11 shapes, cyclic or not.
test(tarjan_matches_kosaraju, [forall(shape(Name, _))]) :-
    shape_graph(Name, Graph),
    scc_components(Graph, Tarjan),
    graph_components(Graph, Kosaraju),
    ( Tarjan == Kosaraju
    -> true
    ;  format(user_error, "~w: tarjan ~q, kosaraju ~q~n",
              [Name, Tarjan, Kosaraju]),
       fail
    ).

% Tarjan's cyclic restriction agrees with the Warshall oracle. This also
% cross-checks against graph_cyclic_components/2, which 0_graph already
% proves against the same oracle.
test(tarjan_cyclic_matches_warshall, [forall(shape(Name, _))]) :-
    shape_graph(Name, Graph),
    scc_components(Graph, All),
    graph_cyclic_components(Graph, KosarajuCyclic),
    restrict_to_cyclic(Graph, All, TarjanCyclic),
    warshall_components(Graph, Expected),
    ( TarjanCyclic == Expected, TarjanCyclic == KosarajuCyclic
    -> true
    ;  format(user_error,
              "~w: tarjan-cyclic ~q, kosaraju-cyclic ~q, warshall ~q~n",
              [Name, TarjanCyclic, KosarajuCyclic, Expected]),
       fail
    ).

restrict_to_cyclic(_, [], []).
restrict_to_cyclic(Graph, [Comp | Rest], Out) :-
    neighbourhood(Graph, Neigh),
    ( component_is_cyclic(Neigh, Comp)
    -> Out = [Comp | OutRest]
    ;  Out = OutRest
    ),
    restrict_to_cyclic(Graph, Rest, OutRest).

neighbourhood(Graph, Neigh) :- list_to_assoc(Graph, Neigh).

component_is_cyclic(_, [_ , _ | _]).
component_is_cyclic(Neigh, [Only]) :-
    ( get_assoc(Only, Neigh, Neighbours)
    -> memberchk(Only, Neighbours)
    ;  fail
    ).

% Every vertex lands in exactly one component.
test(tarjan_partitions_vertices, [forall(shape(Name, _))]) :-
    shape_graph(Name, Graph),
    scc_components(Graph, Components),
    vertices(Graph, Nodes),
    append(Components, Members0),
    msort(Members0, Members),
    ( Members == Nodes -> true ; fail ).

:- end_tests(scc_extract).
