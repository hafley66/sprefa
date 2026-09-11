:- module(dl7_graph_lookup,
          [ open_checker_graph_store/2,
            open_lowerer_graph_store/1,
            close_graph_store/0,
            graph_forward/4,
            graph_parent/3,
            graph_callable_slot/4,
            graph_module_member/2
          ]).

:- use_module(library(gensym), [gensym/2]).

:- dynamic arena_pending_edge/6.
:- dynamic arena_module_node/2.

:- thread_local graph_store_scope/1.

%% open_checker_graph_store(+Edges, +Nodes) is det.
%
% Materialize one check_datalog/4 invocation's current pending edges and module
% nodes as flattened dynamic facts in source-list order. The store is built
% fresh for each checker call, because compiler fixpoint rounds can change the
% edge and node lists, so a previous store is never reused. The caller pairs
% this with close_graph_store/0 under setup_call_cleanup/3.
open_checker_graph_store(Edges, Nodes) :-
    open_graph_store(Edges, Nodes).

%% open_lowerer_graph_store(+Edges) is det.
%
% Materialize one lower_datalog/5 invocation's current environment edges as
% flattened dynamic facts in list order. The store is built fresh for each
% lowering boundary and no module-node facts are installed.
open_lowerer_graph_store(Edges) :-
    open_graph_store(Edges, []).

open_graph_store(Edges, Nodes) :-
    gensym(dl7_graph_store_, StoreId),
    asserta(graph_store_scope(StoreId)),
    catch(
        (   install_pending_edges(Edges, StoreId, 0, _),
            install_module_nodes(Nodes, StoreId, 0, _)
        ->  true
        ;   close_graph_store,
            fail
        ),
        Error,
        ( close_graph_store,
          throw(Error)
        )).

%% install_pending_edges(+Edges, +StoreId, +Sequence0, -Sequence) is det.
%
% assertz/1 keeps duplicate keys in source-list order, so the first asserted
% matching fact reproduces the first memberchk/2 match. The stored sequence
% preserves the authored ordinal for order-sensitive checks.
install_pending_edges([], _, Sequence, Sequence).
install_pending_edges(
    [pending_edge(Owner, Name, Target, Index) | Edges], StoreId,
    Sequence0, Sequence) :-
    assertz(arena_pending_edge(
                StoreId, Owner, Name, Target, Index, Sequence0)),
    Sequence1 is Sequence0 + 1,
    install_pending_edges(Edges, StoreId, Sequence1, Sequence).

install_module_nodes([], _, Sequence, Sequence).
install_module_nodes([Node | Nodes], StoreId, Sequence0, Sequence) :-
    (   Node = module(Owner)
    ->  assertz(arena_module_node(StoreId, Owner)),
        Sequence1 is Sequence0 + 1
    ;   Sequence1 = Sequence0
    ),
    install_module_nodes(Nodes, StoreId, Sequence1, Sequence).

%% close_graph_store is det.
%
% Pop the innermost boundary and retract exactly its clauses. Runs on success,
% failure, exception, and partial-installation failure through the owning
% setup_call_cleanup/3 and open_graph_store/2's catch.
close_graph_store :-
    (   retract(graph_store_scope(StoreId))
    ->  retractall(arena_pending_edge(StoreId, _, _, _, _, _)),
        retractall(arena_module_node(StoreId, _))
    ;   true
    ).

%% graph_forward(?Edges, +Owner, +Name, -Target) is semidet.
%
% First pending edge for (Owner, Name), as memberchk/2 over Edges. JITI narrows
% the candidates; the stored full terms are still unified. Falls back to the
% list when no graph store is active.
graph_forward(Edges, Owner, Name, Target) :-
    (   graph_store_scope(StoreId)
    ->  once(arena_pending_edge(StoreId, Owner, Name, Target, _, _))
    ;   memberchk(pending_edge(Owner, Name, Target, _), Edges)
    ).

%% graph_parent(?Edges, +Owner, -Parent) is semidet.
%
% First edge whose target is target(Owner), as memberchk/2 over Edges.
graph_parent(Edges, Owner, Parent) :-
    (   graph_store_scope(StoreId)
    ->  once(arena_pending_edge(StoreId, Parent, _, target(Owner), _, _))
    ;   memberchk(pending_edge(Parent, _, target(Owner), _), Edges)
    ).

%% graph_callable_slot(?Edges, +Callable, +Index, -Label) is det.
%
% Label is the first matching (Callable, Index) edge's name when that name is an
% atom, otherwise none. This reproduces memberchk/2 followed by atom/1 and the
% clause cut: a first non-atom candidate does not backtrack to a later edge.
graph_callable_slot(Edges, Callable, Index, Label) :-
    (   graph_store_scope(StoreId)
    ->  (   once(arena_pending_edge(StoreId, Callable, Candidate, _, Index, _)),
            atom(Candidate)
        ->  Label = Candidate
        ;   Label = none
        )
    ;   (   memberchk(pending_edge(Callable, Candidate, _, Index), Edges),
            atom(Candidate)
        ->  Label = Candidate
        ;   Label = none
        )
    ).

%% graph_module_member(?Nodes, +Owner) is semidet.
%
% Membership of module(Owner) in Nodes, as memberchk/2 over Nodes.
graph_module_member(Nodes, Owner) :-
    (   graph_store_scope(StoreId)
    ->  once(arena_module_node(StoreId, Owner))
    ;   memberchk(module(Owner), Nodes)
    ).
