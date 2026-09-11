:- begin_tests(dl7_graph_lookup,
               [ setup(compile_fixtures),
                 cleanup(retractall(compiled_fixture(_, _, _)))
               ]).

:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).
:- use_module('../src/2_comptime/0_graph_lookup',
              [ open_checker_graph_store/2,
                open_lowerer_graph_store/1,
                close_graph_store/0
              ]).

:- dynamic test_directory/1.
:- dynamic compiled_fixture/3.
:- prolog_load_context(directory, TestDirectory),
   assertz(test_directory(TestDirectory)).

fixture_names([
    '0_nearest_and_chains.dl7',
    '2_missing_name.dl7',
    '3_self_cycle.dl7',
    '4_two_cycle.dl7',
    '5_generated_callable.dl7',
    '6_deferred_alias_measure.dl7',
    '7_nearest_shadow.dl7'
]).

compile_fixtures :-
    retractall(compiled_fixture(_, _, _)),
    fixture_names(Names),
    maplist(compile_fixture, Names).

compile_fixture(Name) :-
    fixture_path(Name, Path),
    compile_dl7(Path, Rows, _RuntimeProgram, Diagnostics),
    assertz(compiled_fixture(Name, Rows, Diagnostics)).

%% Forward lookup: bound keys, partial keys, and duplicate-key first match.

test(graph_forward_bound_and_partial_key_equivalence) :-
    graph_edges(Edges),
    graph_scope_residue(0),
    owners(Owners),
    names(Names),
    findall(Result,
            ( member(Owner, Owners),
              member(Name, Names),
              forward_result(Edges, Owner, Name, Result) ),
            ListBound),
    findall(Result,
            ( member(Owner, Owners),
              forward_result(Edges, Owner, _, Result) ),
            ListPartial),
    with_lowerer_store(
        Edges,
        ( findall(Result,
                  ( member(Owner, Owners),
                    member(Name, Names),
                    forward_result(Edges, Owner, Name, Result) ),
                  ArenaBound),
          findall(Result,
                  ( member(Owner, Owners),
                    forward_result(Edges, Owner, _, Result) ),
                  ArenaPartial) )),
    ListBound == ArenaBound,
    ListPartial == ArenaPartial,
    graph_scope_residue(0).

test(graph_forward_duplicate_key_keeps_first_match) :-
    graph_edges(Edges),
    forward_result(Edges, a, 'Name', Result),
    Result == forward(a, 'Name', target(base)).

%% Reverse parent lookup: first-match and owner chain termination.

test(graph_parent_first_match_equivalence) :-
    graph_edges(Edges),
    graph_scope_residue(0),
    owners(Owners),
    findall(Result,
            ( member(Owner, Owners),
              parent_result(Edges, Owner, Result) ),
            ListResults),
    with_lowerer_store(
        Edges,
        findall(Result,
                ( member(Owner, Owners),
                  parent_result(Edges, Owner, Result) ),
                ArenaResults)),
    ListResults == ArenaResults,
    parent_result(Edges, a, FirstParent),
    FirstParent == parent(a, parent),
    graph_scope_residue(0).

test(graph_parent_cycle_terminates_and_fails) :-
    Edges = [ pending_edge(cycle_a, other, target(cycle_b), 0),
              pending_edge(cycle_b, other, target(cycle_a), 1)
            ],
    with_lowerer_store(
        Edges,
        (   graph_parent_walk(Edges, cycle_a, 'Missing', [], _)
        ->  Outcome = unexpected
        ;   Outcome = terminated
        )),
    Outcome == terminated.

%% Callable slot: label, index, non-atom first match, and missing fallback.

test(graph_callable_slot_equivalence) :-
    graph_edges(Edges),
    graph_scope_residue(0),
    callables(Callables),
    indices(Indices),
    findall(Result,
            ( member(Callable, Callables),
              member(Index, Indices),
              slot_result(Edges, Callable, Index, Result) ),
            ListResults),
    with_lowerer_store(
        Edges,
        findall(Result,
                ( member(Callable, Callables),
                  member(Index, Indices),
                  slot_result(Edges, Callable, Index, Result) ),
                ArenaResults)),
    ListResults == ArenaResults,
    graph_scope_residue(0).

test(graph_callable_slot_non_atom_first_match_is_none) :-
    graph_edges(Edges),
    slot_result(Edges, callable, 3, NonAtomResult),
    NonAtomResult == slot(callable, 3, none),
    slot_result(Edges, callable, 2, AtomResult),
    AtomResult == slot(callable, 2, label_atom),
    slot_result(Edges, callable, 9, MissingResult),
    MissingResult == slot(callable, 9, none).

%% Module-node membership and primitive/kernel fallback.

test(graph_module_member_equivalence) :-
    graph_nodes(Nodes),
    graph_scope_residue(0),
    owners(Owners),
    findall(Result,
            ( member(Owner, Owners),
              member_result(Nodes, Owner, Result) ),
            ListResults),
    with_checker_store([pending_edge(a, n, target(x), 0)], Nodes,
        findall(Result,
                ( member(Owner, Owners),
                  member_result(Nodes, Owner, Result) ),
                ArenaResults)),
    ListResults == ArenaResults,
    graph_scope_residue(0).

test(graph_checker_primitive_and_kernel_fallback) :-
    Edges = [],
    Nodes = [module(a)],
    with_checker_store(
        Edges, Nodes,
        ( dl7_checker_resolve_name(a, int, Edges, Nodes, [], Primitive),
          Primitive == ref(primitive(int)),
          dl7_checker_resolve_name(a, ':', Edges, Nodes, [], Kernel),
          Kernel == ref(kernel(':'))
        )),
    graph_scope_residue(0).

%% Compiler fixtures: aliases, unknown names, cycles, generated callable.

test(graph_compiler_nearest_and_chains_stays_diagnostic_free) :-
    compiled_fixture('0_nearest_and_chains.dl7', Rows, Diagnostics),
    Rows \== [],
    Diagnostics == [].

test(graph_compiler_missing_name_keeps_existing_diagnostic) :-
    compiled_fixture('2_missing_name.dl7', _, Diagnostics),
    fixture_path('2_missing_name.dl7', Path),
    Diagnostics == [diagnostic(check, reader_node(Path, 5),
                               unresolved_name(field))].

test(graph_compiler_self_cycle_keeps_existing_diagnostics) :-
    compiled_fixture('3_self_cycle.dl7', _, Diagnostics),
    fixture_path('3_self_cycle.dl7', Path),
    Diagnostics == [ diagnostic(check, reader_node(Path, 5),
                                unresolved_name('A')),
                     diagnostic(check, reader_node(Path, 9),
                                unresolved_name(field))
                   ].

test(graph_compiler_generated_callable_stays_diagnostic_free) :-
    compiled_fixture('5_generated_callable.dl7', Rows, Diagnostics),
    Rows \== [],
    Diagnostics == [].

test(graph_compiler_two_name_cycle_keeps_existing_diagnostics) :-
    compiled_fixture('4_two_cycle.dl7', _, Diagnostics),
    fixture_path('4_two_cycle.dl7', Path),
    Diagnostics == [ diagnostic(check, reader_node(Path, 5),
                                unresolved_name('A')),
                     diagnostic(check, reader_node(Path, 9),
                                unresolved_name('B')),
                     diagnostic(check, reader_node(Path, 13),
                                unresolved_name(field))
                   ].

test(graph_compiler_compound_label_stays_diagnostic_free) :-
    compiled_fixture('6_deferred_alias_measure.dl7', Rows, Diagnostics),
    Rows \== [],
    Diagnostics == [].

test(graph_compiler_nearest_shadow_row_count) :-
    compiled_fixture('7_nearest_shadow.dl7', Rows, Diagnostics),
    length(Rows, 810),
    Diagnostics == [].

%% Lifecycle: nesting, threads, cleanup, partial install, stale reuse.

test(graph_nested_store_isolation) :-
    Inner = [pending_edge(scope, name, target(inner), 0)],
    Outer = [pending_edge(scope, name, target(outer), 0)],
    once(
        setup_call_cleanup(
            open_lowerer_graph_store(Outer),
            ( forward_result(Outer, scope, name, OuterResult),
              OuterResult == forward(scope, name, target(outer)),
              setup_call_cleanup(
                  open_lowerer_graph_store(Inner),
                  ( forward_result(Inner, scope, name, InnerResult),
                    InnerResult == forward(scope, name, target(inner)) ),
                  close_graph_store),
              forward_result(Outer, scope, name, Restored),
              Restored == forward(scope, name, target(outer)) ),
            close_graph_store)),
    graph_scope_residue(0).

test(graph_simultaneous_thread_isolation) :-
    First = [pending_edge(scope, name, target(first), 0)],
    Second = [pending_edge(scope, name, target(second), 0)],
    message_queue_create(Queue),
    thread_create(graph_store_thread(First, first, Queue), FirstThread, []),
    thread_create(graph_store_thread(Second, second, Queue), SecondThread, []),
    thread_join(FirstThread, _),
    thread_join(SecondThread, _),
    thread_get_message(Queue, first-FirstResult),
    thread_get_message(Queue, second-SecondResult),
    message_queue_destroy(Queue),
    FirstResult == forward(scope, name, target(first)),
    SecondResult == forward(scope, name, target(second)).

test(graph_cleanup_after_success_failure_exception) :-
    Edges = [pending_edge(scope, name, target(x), 0)],
    setup_call_cleanup(open_lowerer_graph_store(Edges), true, close_graph_store),
    graph_residue(0, 0, 0),
    (   setup_call_cleanup(open_lowerer_graph_store(Edges), fail,
                           close_graph_store)
    ->  Failed = unexpected
    ;   Failed = failed
    ),
    Failed == failed,
    graph_residue(0, 0, 0),
    catch(
        setup_call_cleanup(
            open_lowerer_graph_store(Edges),
            throw(graph_store_test_exception),
            close_graph_store),
        graph_store_test_exception,
        true),
    graph_residue(0, 0, 0).

test(graph_partial_install_failure_cleans_up) :-
    BadEdges = [pending_edge(scope, name, target(x), 0) | broken_tail],
    (   setup_call_cleanup(
            open_lowerer_graph_store(BadEdges),
            true,
            close_graph_store)
    ->  Outcome = unexpected
    ;   Outcome = clean_failure
    ),
    Outcome == clean_failure,
    graph_residue(0, 0, 0).

test(graph_checker_stores_are_not_reused_across_edge_lists) :-
    EdgesA = [pending_edge(a, n, target(x), 0)],
    EdgesB = [pending_edge(a, n, target(y), 0)],
    with_checker_store(EdgesA, [module(a)],
        ( forward_result(EdgesA, a, n, First),
          First == forward(a, n, target(x)) )),
    with_checker_store(EdgesB, [module(a)],
        ( forward_result(EdgesB, a, n, Second),
          Second == forward(a, n, target(y)) )),
    graph_residue(0, 0, 0).

%% Helpers.

graph_edges(Edges) :-
    Edges = [
        pending_edge(a, 'Name', target(base), 0),
        pending_edge(a, 'Name', target(shadow), 1),
        pending_edge(a, field, target(field_target), 2),
        pending_edge(inner, 'Name', target(inner_name), 3),
        pending_edge(parent, parent_name, target(a), 0),
        pending_edge(grand, grand_name, target(parent), 1),
        pending_edge(callable, label_atom, target(x), 2),
        pending_edge(callable, name(x, y), target(y), 3)
    ].

graph_nodes(Nodes) :-
    Nodes = [node(a), module(a), product(a), module(inner), node(inner),
             node(kernel(node)), module(module_root)].

owners([a, inner, parent, grand, callable, absent]).

names(['Name', field, parent_name, missing]).

callables([callable, absent]).

indices([0, 2, 3, 9]).

forward_result(Edges, Owner, Name, Result) :-
    (   dl7_graph_lookup:graph_forward(Edges, Owner, Name, Target)
    ->  Result = forward(Owner, Name, Target)
    ;   (   var(Name)
        ->  Result = forward(Owner, none)
        ;   Result = forward(Owner, Name, none)
        )
    ).

parent_result(Edges, Owner, Result) :-
    (   dl7_graph_lookup:graph_parent(Edges, Owner, Parent)
    ->  Result = parent(Owner, Parent)
    ;   Result = parent(Owner, none)
    ).

slot_result(Edges, Callable, Index, Result) :-
    dl7_graph_lookup:graph_callable_slot(Edges, Callable, Index, Label),
    Result = slot(Callable, Index, Label).

member_result(Nodes, Owner, Result) :-
    (   dl7_graph_lookup:graph_module_member(Nodes, Owner)
    ->  Result = member(Owner)
    ;   Result = absent(Owner)
    ).

graph_parent_walk(Edges, Owner, Name, Visited, Resolved) :-
    \+ memberchk(Owner-Name, Visited),
    (   dl7_graph_lookup:graph_forward(Edges, Owner, Name, _)
    ->  Resolved = ref(Owner)
    ;   dl7_graph_lookup:graph_parent(Edges, Owner, Parent)
    ->  graph_parent_walk(Edges, Parent, Name, [Owner-Name | Visited], Resolved)
    ).

with_lowerer_store(Edges, Goal) :-
    setup_call_cleanup(
        open_lowerer_graph_store(Edges),
        once(Goal),
        close_graph_store).

with_checker_store(Edges, Nodes, Goal) :-
    setup_call_cleanup(
        open_checker_graph_store(Edges, Nodes),
        once(Goal),
        close_graph_store).

graph_store_thread(Edges, Tag, Queue) :-
    setup_call_cleanup(
        open_lowerer_graph_store(Edges),
        forward_result(Edges, scope, name, Result),
        close_graph_store),
    thread_send_message(Queue, Tag-Result).

graph_scope_residue(0) :-
    findall(_, dl7_graph_lookup:graph_store_scope(_), Scopes),
    Scopes == [].

graph_residue(EdgeCount, NodeCount, ScopeCount) :-
    findall(_, dl7_graph_lookup:arena_pending_edge(_, _, _, _, _, _), Edges),
    length(Edges, EdgeCount),
    findall(_, dl7_graph_lookup:arena_module_node(_, _), Nodes),
    length(Nodes, NodeCount),
    findall(_, dl7_graph_lookup:graph_store_scope(_), Scopes),
    length(Scopes, ScopeCount).

dl7_checker_resolve_name(Owner, Name, Edges, Nodes, Visited, Resolved) :-
    dl7_checker:resolve_name(Owner, Name, Edges, Nodes, Visited, Resolved).

fixture_path(Name, Path) :-
    test_directory(TestDirectory),
    atomic_list_concat(
        [TestDirectory, '/fixtures/lexical_binding/', Name], Path).

:- end_tests(dl7_graph_lookup).
