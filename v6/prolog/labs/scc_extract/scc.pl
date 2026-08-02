% Strongly connected components in a directed graph via Tarjan's algorithm,
% extracted from the SWI-Prolog clpfd library.
%
% scc/2 and its helpers below are copied verbatim from clpfd.pl, written by
% Markus Triska (https://www.metalevel.at/). Source: clpfd.pl in the SWI
% distribution at /opt/homebrew/Cellar/swi-prolog/10.0.2/lib/swipl/library/
% clp/clpfd.pl, lines 5892 through 5962, where all_distinct/1 and
% global_cardinality/3 use it. Module-private there, unexported, undocumented.
%
% This file keeps Triska's names and algorithm. The only additions are the
% attribution above, module/export scaffolding, and the scc_components/2
% wrapper that translates between atoms and the attributed unbound variables
% the algorithm works on.

:- module(scc,
          [ scc_components/2   % +Graph, -Components
          ]).

:- use_module(library(ugraphs)).
:- use_module(library(pairs)).

scc(Vs, Succ) :- phrase(scc(Vs), [s(0,[],Succ)], _).

scc([])     --> [].
scc([V|Vs]) -->
        (   vindex_defined(V) -> scc(Vs)
        ;   scc_(V), scc(Vs)
        ).

vindex_defined(V) --> { get_attr(V, index, _) }.

vindex_is_index(V) -->
        state(s(Index,_,_)),
        { put_attr(V, index, Index) }.

vlowlink_is_index(V) -->
        state(s(Index,_,_)),
        { put_attr(V, lowlink, Index) }.

index_plus_one -->
        state(s(I,Stack,Succ), s(I1,Stack,Succ)),
        { I1 is I+1 }.

s_push(V)  -->
        state(s(I,Stack,Succ), s(I,[V|Stack],Succ)),
        { put_attr(V, in_stack, true) }.

vlowlink_min_lowlink(V, VP) -->
        { get_attr(V, lowlink, VL),
          get_attr(VP, lowlink, VPL),
          VL1 is min(VL, VPL),
          put_attr(V, lowlink, VL1) }.

successors(V, Tos) --> state(s(_,_,Succ)), { call(Succ, V, Tos) }.

scc_(V) -->
        vindex_is_index(V),
        vlowlink_is_index(V),
        index_plus_one,
        s_push(V),
        successors(V, Tos),
        each_edge(Tos, V),
        (   { get_attr(V, index, VI),
              get_attr(V, lowlink, VI) } -> pop_stack_to(V, VI)
        ;   []
        ).

pop_stack_to(V, N) -->
        state(s(I,[First|Stack],Succ), s(I,Stack,Succ)),
        { del_attr(First, in_stack) },
        (   { First == V } -> []
        ;   { put_attr(First, lowlink, N) },
            pop_stack_to(V, N)
        ).

each_edge([], _) --> [].
each_edge([VP|VPs], V) -->
        (   vindex_defined(VP) ->
            (   v_in_stack(VP) ->
                vlowlink_min_lowlink(V, VP)
            ;   []
            )
        ;   scc_(VP),
            vlowlink_min_lowlink(V, VP)
        ),
        each_edge(VPs, V).

state(S), [S] --> [S].

state(S0, S), [S] --> [S0].

v_in_stack(V) --> { get_attr(V, in_stack, true) }.

% The wrapper: translate a ugraph of atoms into attributed variables, run
% scc/2, then read the shared lowlink back into grouped components. Contract
% identical to graph_components/2 in 0_graph.pl: every vertex lands in exactly
% one component, a vertex on no cycle is its own singleton, each component is
% sorted, and the component list is sorted.
scc_components(Graph, Components) :-
    vertices(Graph, Nodes),
    map_nodes_to_vars(Nodes, AtomToVar, Vars),
    successors_closure(Graph, AtomToVar, Vars, VarToNeighbours),
    scc(Vars, scc:successor_lookup(VarToNeighbours)),
    group_by_lowlink(Vars, AtomToVar, Components).

map_nodes_to_vars([], [], []).
map_nodes_to_vars([Node | Rest], [Node-Var | Pairs], [Var | Vars]) :-
    map_nodes_to_vars(Rest, Pairs, Vars).

successors_closure(Graph, AtomToVar, Vars, VarToNeighbours) :-
    neighbours_by_var(Vars, Graph, AtomToVar, VarToNeighbours).

neighbours_by_var([], _, _, []).
neighbours_by_var([V | Vs], Graph, AtomToVar, [V-Neighbours | Rest]) :-
    vertex_name(V, AtomToVar, Name),
    ( memberchk(Name-Atoms, Graph)
    -> map_atoms_to_vars(Atoms, AtomToVar, Neighbours)
    ;  Neighbours = []
    ),
    neighbours_by_var(Vs, Graph, AtomToVar, Rest).

% Identity scan again: finds a vertex's atom without unifying the (eventually
% attributed) vertex variable.
vertex_name(V, [Name-Stored | _], Name) :- V == Stored.
vertex_name(V, [_ | Rest], Name) :- vertex_name(V, Rest, Name).

map_atoms_to_vars([], _, []).
map_atoms_to_vars([Atom | Rest], AtomToVar, [Var | Vars]) :-
    memberchk(Atom-Var, AtomToVar),
    map_atoms_to_vars(Rest, AtomToVar, Vars).

% The successor closure. scc/2 calls call(Succ, V, Tos), so Succ must be a
% callable; a bare mapping list would not be callable. The module qualifier
% pins resolution to this module regardless of where scc/2 is invoked from.
successor_lookup(VarToNeighbours, V, Tos) :-
    succ_identical(V, VarToNeighbours, Tos0),
    ( Tos0 = [_|_] -> Tos = Tos0 ; Tos = [] ).

% Identity scan. scc/2 never binds the vertex variables, so a lookup by
% unification would tie the current vertex (already carrying index, lowlink
% and in_stack attributes) to every stored key it skips, unifying attributed
% variables and tripping undefined hook modules. Identity keeps it free.
succ_identical(V, [Key-Neighbours|_], Neighbours) :- V == Key.
succ_identical(V, [_|Rest], Tos) :- succ_identical(V, Rest, Tos).

group_by_lowlink(Vars, AtomToVar, Components) :-
    findall(Lowlink-Atom,
            ( member(Var, Vars),
              get_attr(Var, lowlink, Lowlink),
              member(Atom-Var2, AtomToVar),
              Var2 == Var ),
            Pairs),
    sort(Pairs, SortedPairs),
    group_pairs_by_key(SortedPairs, Groups),
    pairs_values(Groups, AtomLists),
    maplist(sort, AtomLists, Components0),
    sort(Components0, Components).
