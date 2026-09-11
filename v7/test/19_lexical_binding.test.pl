:- begin_tests(dl7_lexical_binding,
               [ setup(compile_fixtures),
                 cleanup(retractall(compiled_fixture(_, _, _)))
               ]).

:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).

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

test(nearest_ordinary_binding_controls_atom_and_argument_targets) :-
    compiled_fixture('0_nearest_and_chains.dl7', Rows, []),
    named_owner(Rows, 'Base', Base),
    named_owner(Rows, 'Inner', Inner),
    named_owner(Rows, 'Option', Option),
    owner_edges(Rows, Inner, Edges),
    Edges == [ edge(0, const('Name'), ref(Base)),
               edge(1, const(direct), ref(Base)),
               edge(2, const(argument),
                    ref(application(Option, [Base]))),
               edge(3, const('One'), ref(Base)),
               edge(4, const('Two'), ref(Base)),
               edge(5, const('Three'), ref(Base)),
               edge(6, const(same_scope), ref(Base)),
               edge(7, const(across_scope), ref(Base))
             ].

test(nearest_non_callable_blocks_outer_callable) :-
    Reservations = [ reservation(owner(root), 'Name',
                                  target(owner(callable)), product),
                     reservation(owner(root), inner,
                                  target(owner(inner)), product),
                     reservation(owner(inner), 'Name',
                                  target(owner(base)), reference)
                   ],
    Relations = [relation(owner(callable), 2, [[0]])],
    Environment = expression_environment(Reservations, Relations, []),
    dl7_lowerer:expression_callable(
        'Name', owner(inner), Environment, Result),
    Result == error(not_relation('Name')).

test(missing_name_uses_existing_checker_diagnostic) :-
    compiled_fixture('2_missing_name.dl7', _, Diagnostics),
    fixture_path('2_missing_name.dl7', Path),
    Diagnostics == [diagnostic(check, reader_node(Path, 5),
                               unresolved_name(field))].

test(self_cycle_terminates_with_existing_checker_diagnostic) :-
    compiled_fixture('3_self_cycle.dl7', _, Diagnostics),
    fixture_path('3_self_cycle.dl7', Path),
    Diagnostics == [ diagnostic(check, reader_node(Path, 5),
                                unresolved_name('A')),
                     diagnostic(check, reader_node(Path, 9),
                                unresolved_name(field))
                   ].

test(two_name_cycle_terminates_with_existing_checker_diagnostics) :-
    compiled_fixture('4_two_cycle.dl7', _, Diagnostics),
    fixture_path('4_two_cycle.dl7', Path),
    Diagnostics == [ diagnostic(check, reader_node(Path, 5),
                                unresolved_name('A')),
                     diagnostic(check, reader_node(Path, 9),
                                unresolved_name('B')),
                     diagnostic(check, reader_node(Path, 13),
                                unresolved_name(field))
                   ].

test(generated_callable_keeps_final_application_identity) :-
    compiled_fixture('5_generated_callable.dl7', Rows, []),
    named_owner(Rows, 'Holder', Holder),
    named_owner(Rows, 'Option', Option),
    named_owner(Rows, 'User', User),
    named_owner(Rows, 'HistoryOptions', HistoryOptions),
    named_owner(Rows, 'HistoryV1', HistoryV1),
    owner_edges(Rows, Holder, Edges),
    Edges == [edge(0, const(field),
                   ref(application(
                           Option,
                           [application(HistoryV1,
                                        [User, HistoryOptions])])))].

test(same_owner_product_precedence_is_unchanged) :-
    Reservations = [ reservation(owner(scope), 'Name',
                                  target(owner(reference)), reference),
                     reservation(owner(scope), 'Name',
                                  target(owner(product)), product)
                   ],
    dl7_lowerer:scoped_reservation(
        owner(scope), 'Name', Reservations, [], Reservation),
    Reservation == reservation(owner(scope), 'Name',
                               target(owner(product)), product).

test(deferred_expression_aliases_reuse_final_identity) :-
    compiled_fixture('6_deferred_alias_measure.dl7', Rows, []),
    named_owner(Rows, 'Holder', Holder),
    named_owner(Rows, 'Option', Option),
    named_owner(Rows, 'Key', Key),
    named_owner(Rows, 'KeyOptions', KeyOptions),
    Expected = ref(application(Option, [primitive(text)])),
    owner_edges(Rows, Holder, Edges),
    Edges == [ edge(0, const(direct), Expected),
               edge(1, const(alias_a), Expected),
               edge(2, const(alias_b), Expected),
               edge(3, const(chain), Expected),
               edge(4,
                    ref(application(Key, ["compound", KeyOptions])),
                    Expected)
             ].

test(nearest_non_deferred_shadow_blocks_deferred_alias_promotion) :-
    compiled_fixture('7_nearest_shadow.dl7', Rows, []),
    named_owner(Rows, 'Holder', Holder),
    named_owner(Rows, 'Shadow', Shadow),
    owner_edges(Rows, Holder, Edges),
    Edges == [ edge(0, const('Name'), ref(Shadow)),
               edge(1, const(direct), ref(Shadow)),
               edge(2, const(deep), ref(Shadow))
             ].

test(promoted_alias_uses_nearest_owner_index_and_alias_origins) :-
    Alias = reservation(holder, field, name(holder, 'Name'), reference),
    Parent = reservation(module, 'Holder', target(holder), product),
    Terminal = reservation(
                   module, 'Name',
                   deferred_expression(target_node, terminal_bind, 4),
                   expression),
    Local = [Alias, Parent],
    Visible = [Alias, Parent, Terminal],
    AliasEdge = pending_edge(holder, field, name(holder, 'Name'), 7),
    ParentEdge = pending_edge(module, 'Holder', target(holder), 2),
    TerminalEdge = pending_edge(
                       module, 'Name', deferred_expression(target_node), 4),
    Edges = [AliasEdge, ParentEdge],
    VisibleEdges = [AliasEdge, ParentEdge, TerminalEdge],
    Origins = [origin(edge(holder, field, 7), alias_bind)],
    dl7_lowerer:promote_deferred_aliases(
        Local, Visible, Edges, VisibleEdges, Origins,
        Work, Promoted, PromotedEdges, Promotions),
    dl7_lowerer:promoted_alias_rules(
        Promotions, 0, Rules, RuleOrigins, Next),
    Work == [Parent],
    Promoted == [ reservation(
                       holder, field,
                       deferred_expression(
                           node(alias_bind, atom('Name')), alias_bind, 7),
                       expression),
                  Parent
                ],
    PromotedEdges == [ pending_edge(
                           holder, field,
                           deferred_expression(
                               node(alias_bind, atom('Name'))), 7),
                       ParentEdge
                     ],
    Rules == [rule(
                   call(name(holder, ':'),
                        [ref(holder), const(field),
                         var(derived_bind(alias_bind)), const(7)]),
                   [pending_goal(
                        positive,
                        call(name(holder, ':'),
                             [ ref(module), const('Name'),
                               var(derived_bind(alias_bind)), const(4)
                             ]))])],
    RuleOrigins == [ origin(rule(0), alias_bind),
                     origin(goal(0, 0), alias_bind)
                   ],
    Next == 1.

test(reservation_arena_matches_list_for_shadowing_and_aliases) :-
    reservation_arena_fixture(Reservations),
    arena_equivalence(Reservations, ['Name', field, missing, parent_name, other]).

test(reservation_arena_matches_list_for_partial_keys) :-
    reservation_arena_fixture(Reservations),
    arena_equivalence(Reservations, [_, 'Name', field, missing]).

test(reservation_arena_same_owner_product_precedence_is_unchanged) :-
    Reservations = [ reservation(owner(scope), 'Name',
                                  target(owner(reference)), reference),
                     reservation(owner(scope), 'Name',
                                  target(owner(product)), product)
                   ],
    setup_call_cleanup(
        dl7_lowerer:open_reservation_arena(Reservations),
        dl7_lowerer:scoped_reservation(
            owner(scope), 'Name', Reservations, [], Reservation),
        dl7_lowerer:close_reservation_arena),
    Reservation == reservation(owner(scope), 'Name',
                               target(owner(product)), product).

test(reservation_arena_promoted_view_shadows_visible_view) :-
    Visible = [ reservation(owner(scope), name,
                            target(owner(base)), reference),
                reservation(owner(scope), other,
                            target(owner(other_target)), product)
              ],
    Promoted = [ reservation(owner(scope), name,
                             deferred_expression(
                                 node(bind, atom(name)), bind, 3),
                             expression),
                 reservation(owner(scope), other,
                             target(owner(other_target)), product)
               ],
    setup_call_cleanup(
        dl7_lowerer:open_reservation_arena(Visible),
        ( dl7_lowerer:install_promoted_reservation_view(Promoted),
          dl7_lowerer:scoped_reservation(
              owner(scope), name, Visible, [], NameResult),
          NameResult == reservation(
                            owner(scope), name,
                            deferred_expression(
                                node(bind, atom(name)), bind, 3),
                            expression),
          dl7_lowerer:scoped_reservation(
              owner(scope), other, Visible, [], OtherResult),
          OtherResult == reservation(
                              owner(scope), other,
                              target(owner(other_target)), product) ),
        dl7_lowerer:close_reservation_arena).

test(reservation_arena_parent_cycle_terminates_and_fails) :-
    Reservations = [ reservation(owner(cycle_a), other,
                                  target(owner(cycle_b)), product),
                     reservation(owner(cycle_b), other,
                                  target(owner(cycle_a)), product)
                   ],
    setup_call_cleanup(
        dl7_lowerer:open_reservation_arena(Reservations),
        ( dl7_lowerer:scoped_reservation(
              owner(cycle_a), 'Missing', Reservations, [], _) ->
            Cycle = unexpected
        ;   Cycle = terminated
        ),
        dl7_lowerer:close_reservation_arena),
    Cycle == terminated.

test(reservation_arena_unknown_name_fails) :-
    Reservation = reservation(owner(scope), name, target(owner(x)), product),
    setup_call_cleanup(
        dl7_lowerer:open_reservation_arena([Reservation]),
        ( dl7_lowerer:scoped_reservation(
              owner(scope), unknown, [Reservation], [], _) ->
            Outcome = unexpected
        ;   Outcome = absent
        ),
        dl7_lowerer:close_reservation_arena),
    Outcome == absent.

test(reservation_arena_nested_store_isolation) :-
    Inner = [reservation(owner(scope), name, target(owner(inner)), product)],
    Outer = [reservation(owner(scope), name, target(owner(outer)), product)],
    once(
        setup_call_cleanup(
            dl7_lowerer:open_reservation_arena(Outer),
            ( dl7_lowerer:scoped_reservation(
                  owner(scope), name, Outer, [], OuterResult),
              OuterResult == reservation(
                                  owner(scope), name,
                                  target(owner(outer)), product),
              setup_call_cleanup(
                  dl7_lowerer:open_reservation_arena(Inner),
                  ( dl7_lowerer:scoped_reservation(
                        owner(scope), name, Inner, [], InnerResult),
                    InnerResult == reservation(
                                        owner(scope), name,
                                        target(owner(inner)), product) ),
                  dl7_lowerer:close_reservation_arena),
              dl7_lowerer:scoped_reservation(
                  owner(scope), name, Outer, [], Restored),
              Restored == reservation(
                              owner(scope), name,
                              target(owner(outer)), product) ),
            dl7_lowerer:close_reservation_arena)).

test(reservation_arena_simultaneous_thread_isolation) :-
    First = [reservation(owner(scope), name, target(owner(first)), product)],
    Second = [reservation(owner(scope), name, target(owner(second)), product)],
    message_queue_create(Queue),
    thread_create(
        reservation_arena_thread(First, first, Queue),
        FirstThread, []),
    thread_create(
        reservation_arena_thread(Second, second, Queue),
        SecondThread, []),
    thread_join(FirstThread, _),
    thread_join(SecondThread, _),
    thread_get_message(Queue, first-FirstResult),
    thread_get_message(Queue, second-SecondResult),
    message_queue_destroy(Queue),
    FirstResult == reservation(owner(scope), name, target(owner(first)), product),
    SecondResult == reservation(owner(scope), name,
                                target(owner(second)), product).

test(reservation_arena_cleanup_after_success_failure_exception) :-
    Reservations = [reservation(owner(scope), name, target(owner(x)), product)],
    setup_call_cleanup(
        dl7_lowerer:open_reservation_arena(Reservations),
        true,
        dl7_lowerer:close_reservation_arena),
    reservation_arena_residue(0),
    (   setup_call_cleanup(
            dl7_lowerer:open_reservation_arena(Reservations),
            fail,
            dl7_lowerer:close_reservation_arena)
    ->  Failed = unexpected
    ;   Failed = failed
    ),
    Failed == failed,
    reservation_arena_residue(0),
    catch(
        setup_call_cleanup(
            dl7_lowerer:open_reservation_arena(Reservations),
            throw(arena_test_exception),
            dl7_lowerer:close_reservation_arena),
        arena_test_exception,
        true),
    reservation_arena_residue(0).

%% reservation_arena_fixture(-Reservations) is det.
%
% A reservation table exercising direct shadowing, a chained parent alias,
% product-over-reference precedence, and a two-owner parent cycle.
reservation_arena_fixture(Reservations) :-
    Reservations = [
        reservation(owner(root), 'Name', target(owner(base)), product),
        reservation(owner(root), 'Name', target(owner(shadow)), reference),
        reservation(owner(inner), 'Name', target(owner(inner_name)), reference),
        reservation(owner(inner), field, target(owner(field_target)), product),
        reservation(owner(parent), parent_name, target(owner(inner)), product),
        reservation(owner(cycle_a), other, target(owner(cycle_b)), product),
        reservation(owner(cycle_b), other, target(owner(cycle_a)), product)
    ].

%% arena_equivalence(+Reservations, +Names) is det.
%
% Every (Owner, Name) query with each owner in the table and each name in
% Names must return the identical reservation from the list accessor and the
% JITI arena. Names may hold a variable to exercise partial keys.
arena_equivalence(Reservations, Names) :-
    list_owners(Reservations, Owners),
    findall(Result,
            arena_query(Owners, Names, Reservations, Result),
            ListResults),
    setup_call_cleanup(
        dl7_lowerer:open_reservation_arena(Reservations),
        findall(Result,
                arena_query(Owners, Names, Reservations, Result),
                ArenaResults),
        dl7_lowerer:close_reservation_arena),
    ListResults == ArenaResults.

arena_query(Owners, Names, Reservations, Result) :-
    member(Owner, Owners),
    member(Name, Names),
    dl7_lowerer:scoped_reservation(Owner, Name, Reservations, [], Result).

list_owners(Reservations, Owners) :-
    findall(Owner,
            member(reservation(Owner, _, _, _), Reservations),
            OwnerList0),
    sort(OwnerList0, Owners).

reservation_arena_thread(Reservations, Tag, Queue) :-
    setup_call_cleanup(
        dl7_lowerer:open_reservation_arena(Reservations),
        dl7_lowerer:scoped_reservation(
            owner(scope), name, Reservations, [], Result),
        dl7_lowerer:close_reservation_arena),
    thread_send_message(Queue, Tag-Result).

reservation_arena_residue(Facts) :-
    findall(_, dl7_lowerer:arena_reservation(_, _, _, _, _, _), FactList),
    length(FactList, Facts),
    findall(_, dl7_lowerer:reservation_arena_scope(_), ScopeList),
    length(ScopeList, 0).

fixture_path(Name, Path) :-
    test_directory(TestDirectory),
    atomic_list_concat(
        [TestDirectory, '/fixtures/lexical_binding/', Name], Path).

named_owner(Rows, Name, Owner) :-
    member(call(ref(kernel(':')),
                [ref(_), const(Name), ref(Owner), const(_)]),
           Rows),
    !.

owner_edges(Rows, Owner, Edges) :-
    findall(Index-edge(Index, Label, Target),
            member(call(ref(kernel(':')),
                        [ref(Owner), Label, Target, const(Index)]),
                   Rows),
            Indexed),
    keysort(Indexed, Sorted),
    pairs_values(Sorted, Edges).

:- end_tests(dl7_lexical_binding).
