% Graph reachability, closure, strongly connected components, topological
% order, and cycle detection. Components use Kosaraju over ugraphs data.

:- module(graph,
          [ graph_from_edges/2,          % +Edges, -Graph
            graph_from_edges/3,          % +Vertices, +Edges, -Graph
            graph_nodes/2,               % +Graph, -Nodes
            graph_closure/2,             % +Graph, -Closure
            graph_reaches/3,             % +Closure, +From, +To
            graph_components/2,          % +Graph, -Components
            graph_cyclic_components/2,   % +Graph, -Components
            graph_component_of/3,        % +Components, +Node, -Component
            graph_topological_order/2,   % +Graph, -Order
            graph_has_cycle/1            % +Graph
          ]).

:- use_module(library(ugraphs)).
:- use_module(library(assoc)).
:- use_module(library(lists)).

% ═══ construction ════════════════════════════════════════════════════════════

%!  graph_from_edges(+Edges, -Graph) is det.
%
%   Edges is a list of From-To pairs. Vertices are the endpoints. Duplicate
%   edges collapse.

graph_from_edges(Edges, Graph) :-
    findall(Node,
            ( member(From-To, Edges),
              ( Node = From ; Node = To ) ),
            Nodes0),
    sort(Nodes0, Nodes),
    graph_from_edges(Nodes, Edges, Graph).

%!  graph_from_edges(+Vertices, +Edges, -Graph) is det.
%
%   As above, but Vertices seeds isolated nodes that no edge mentions. An
%   endpoint absent from Vertices is still added.

graph_from_edges(Vertices, Edges, Graph) :-
    sort(Vertices, SortedVertices),
    sort(Edges, SortedEdges),
    vertices_edges_to_ugraph(SortedVertices, SortedEdges, Graph).

graph_nodes(Graph, Nodes) :-
    vertices(Graph, Nodes).

% ═══ reachability ════════════════════════════════════════════════════════════

%!  graph_closure(+Graph, -Closure) is det.
%
%   Closure maps each node to the nodes reachable from it along a path of
%   AT LEAST ONE edge. A node appears in its own target set exactly when it
%   sits on a cycle. This is library(ugraphs)' transitive_closure/2, whose
%   strictness was confirmed by measurement, not by reading:
%
%     transitive_closure([1-[2],2-[3],3-[]], C)  gives  [1-[2,3],2-[3],3-[]]
%     transitive_closure([1-[2],2-[1]],     C)  gives  [1-[1,2],2-[1,2]]
%     transitive_closure([1-[1]],           C)  gives  [1-[1]]
%
%   Note the cost: Warshall materializes the whole reachability relation.
%   Use graph_components/2 rather than this when the question is component
%   membership.

graph_closure(Graph, Closure) :-
    transitive_closure(Graph, Closure).

%!  graph_reaches(+Closure, +From, +To) is semidet.

graph_reaches(Closure, From, To) :-
    memberchk(From-Targets, Closure),
    memberchk(To, Targets).

% ═══ strongly connected components ═══════════════════════════════════════════

%!  graph_components(+Graph, -Components) is det.
%
%   Every vertex lands in exactly one component. A vertex on no cycle forms
%   its own singleton. Each component is sorted, and Components is sorted,
%   so components come out ordered by their smallest member.
%
%   Kosaraju: one depth-first pass over Graph collecting finish order, then
%   one pass over the transposed graph in decreasing finish order, where
%   each tree is a component.

graph_components(Graph, Components) :-
    vertices(Graph, Nodes),
    neighbourhood(Graph, Neighbourhood),
    empty_assoc(NoneVisited),
    finish_order(Nodes, Neighbourhood, NoneVisited, _, [], FinishOrder),
    transpose_ugraph(Graph, Reversed),
    neighbourhood(Reversed, ReversedNeighbourhood),
    empty_assoc(NoneAssigned),
    collect_components(FinishOrder, ReversedNeighbourhood, NoneAssigned, _,
                       [], Components0),
    sort(Components0, Components).

%!  graph_cyclic_components(+Graph, -Components) is det.
%
%   graph_components/2 restricted to components that contain at least one
%   edge: every multi-node component, plus singletons carrying a self-loop.
%   This is the set a dependency checker calls "the recursive ones".

graph_cyclic_components(Graph, Components) :-
    graph_components(Graph, AllComponents),
    neighbourhood(Graph, Neighbourhood),
    include(component_has_internal_edge(Neighbourhood), AllComponents,
            Components).

component_has_internal_edge(_, [_, _ | _]).
component_has_internal_edge(Neighbourhood, [Only]) :-
    neighbours_of(Neighbourhood, Only, Neighbours),
    memberchk(Only, Neighbours).

%!  graph_component_of(+Components, +Node, -Component) is semidet.

graph_component_of(Components, Node, Component) :-
    member(Candidate, Components),
    memberchk(Node, Candidate),
    !,
    Component = Candidate.

% A ugraph is already a sorted Node-Neighbours pair list with unique keys,
% which is exactly list_to_assoc/2's input contract. Looking neighbours up
% through the assoc keeps both Kosaraju passes off a linear scan per node;
% memberchk/2 over the pair list would put a quadratic term back into the
% one path this whole module exists to take out.
neighbourhood(Graph, Neighbourhood) :-
    list_to_assoc(Graph, Neighbourhood).

neighbours_of(Neighbourhood, Node, Neighbours) :-
    ( get_assoc(Node, Neighbourhood, Found)
    -> Neighbours = Found
    ;  Neighbours = []
    ).

% Pass one. FinishOrder accumulates latest-finished at the head, which is
% the order pass two consumes.
finish_order([], _, Visited, Visited, Order, Order).
finish_order([Node | Rest], Neighbourhood, Visited0, Visited, Order0, Order) :-
    depth_first_finish(Node, Neighbourhood, Visited0, Visited1, Order0, Order1),
    finish_order(Rest, Neighbourhood, Visited1, Visited, Order1, Order).

depth_first_finish(Node, Neighbourhood, Visited0, Visited, Order0, Order) :-
    ( get_assoc(Node, Visited0, true)
    -> Visited = Visited0,
       Order = Order0
    ;  put_assoc(Node, Visited0, true, Visited1),
       neighbours_of(Neighbourhood, Node, Neighbours),
       finish_order(Neighbours, Neighbourhood, Visited1, Visited, Order0,
                    Order1),
       Order = [Node | Order1]
    ).

% Pass two, over the transposed graph.
collect_components([], _, Assigned, Assigned, Components, Components).
collect_components([Node | Rest], Neighbourhood, Assigned0, Assigned,
                   Components0, Components) :-
    ( get_assoc(Node, Assigned0, true)
    -> collect_components(Rest, Neighbourhood, Assigned0, Assigned,
                          Components0, Components)
    ;  collect_reachable([Node], Neighbourhood, Assigned0, Assigned1, [],
                         Members),
       sort(Members, Component),
       collect_components(Rest, Neighbourhood, Assigned1, Assigned,
                          [Component | Components0], Components)
    ).

collect_reachable([], _, Assigned, Assigned, Members, Members).
collect_reachable([Node | Rest], Neighbourhood, Assigned0, Assigned, Members0,
                  Members) :-
    ( get_assoc(Node, Assigned0, true)
    -> collect_reachable(Rest, Neighbourhood, Assigned0, Assigned, Members0,
                         Members)
    ;  put_assoc(Node, Assigned0, true, Assigned1),
       neighbours_of(Neighbourhood, Node, Neighbours),
       append(Neighbours, Rest, Pending),
       collect_reachable(Pending, Neighbourhood, Assigned1, Assigned,
                         [Node | Members0], Members)
    ).

% ═══ order and cycles ════════════════════════════════════════════════════════

%!  graph_topological_order(+Graph, -Order) is semidet.
%
%   Fails when Graph has a cycle, self-loops included. library(ugraphs)'
%   top_sort/2, which is Kahn's algorithm over an in-degree count.

graph_topological_order(Graph, Order) :-
    top_sort(Graph, Order).

%!  graph_has_cycle(+Graph) is semidet.

graph_has_cycle(Graph) :-
    \+ top_sort(Graph, _).
