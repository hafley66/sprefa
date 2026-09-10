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
    '6_deferred_alias_measure.dl7'
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
