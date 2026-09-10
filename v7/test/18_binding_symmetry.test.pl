:- begin_tests(dl7_binding_symmetry).

:- use_module('../src/0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../src/2_comptime/2_compiler', [compile_dl7/4]).
:- use_module('../src/2_comptime/0_lowerer', [lower_datalog_deferred/5]).

:- dynamic test_directory/1.
:- prolog_load_context(directory, TestDirectory),
   assertz(test_directory(TestDirectory)).

test(both_label_forms_resolve_one_expression_target) :-
    garbage_collect,
    fixture_rows('0_atom_label_expression_target.dl7', AtomRows),
    fixture_rows('1_compound_label_expression_target.dl7', CompoundRows),
    named_owner(AtomRows, 'Holder', Holder),
    named_owner(CompoundRows, 'Ledger', Ledger),
    owner_edges(AtomRows, Holder, AtomEdges),
    owner_edges(CompoundRows, Ledger, CompoundEdges),
    AtomEdges = [edge(0, const(direct), AtomTarget)],
    CompoundEdges = [edge(0, _, CompoundTarget)],
    AtomTarget == CompoundTarget,
    named_owner(AtomRows, 'Option', Option),
    AtomTarget == ref(application(Option, [primitive(text)])).

test(compound_label_keeps_its_generated_key_identity) :-
    garbage_collect,
    fixture_rows('1_compound_label_expression_target.dl7', Rows),
    named_owner(Rows, 'Ledger', Ledger),
    named_owner(Rows, 'Key', Key),
    named_owner(Rows, 'KeyOptions', KeyOptions),
    named_owner(Rows, 'Option', Option),
    owner_edges(Rows, Ledger, Edges),
    Edges == [edge(0,
                   ref(application(Key, ["direct", KeyOptions])),
                   ref(application(Option, [primitive(text)])))].

test(nested_applications_survive_both_label_forms) :-
    garbage_collect,
    fixture_rows('2_nested_application_targets.dl7', Rows),
    named_owner(Rows, 'Nested', Nested),
    named_owner(Rows, 'Base', Base),
    named_owner(Rows, 'Option', Option),
    named_owner(Rows, 'Partial', Partial),
    named_owner(Rows, 'Key', Key),
    named_owner(Rows, 'KeyOptions', KeyOptions),
    owner_edges(Rows, Nested, Edges),
    Expected = ref(application(Option, [application(Partial, [Base])])),
    Edges == [ edge(0, const(atom_label), Expected),
               edge(1,
                    ref(application(Key, ["compound_label", KeyOptions])),
                    Expected)
             ].

test(compound_label_edge_keeps_its_authored_index) :-
    garbage_collect,
    fixture_rows('3_declaration_order.dl7', Rows),
    named_owner(Rows, 'Ordered', Ordered),
    named_owner(Rows, 'Key', Key),
    named_owner(Rows, 'KeyOptions', KeyOptions),
    named_owner(Rows, 'Option', Option),
    owner_edges(Rows, Ordered, Edges),
    Edges == [ edge(0, const(first), ref(primitive(text))),
               edge(1,
                    ref(application(Key, ["second", KeyOptions])),
                    ref(application(Option, [primitive(text)]))),
               edge(2, const(third), ref(primitive(int)))
             ].

test(structural_targets_under_compound_labels_are_unchanged) :-
    garbage_collect,
    fixture_rows('4_structural_targets.dl7', Rows),
    named_owner(Rows, 'Structural', Structural),
    named_owner(Rows, 'Base', Base),
    named_owner(Rows, 'Key', Key),
    named_owner(Rows, 'KeyOptions', KeyOptions),
    owner_edges(Rows, Structural, Edges),
    Edges == [ edge(0,
                    ref(application(Key, ["primitive", KeyOptions])),
                    ref(primitive(text))),
               edge(1,
                    ref(application(Key, ["reference", KeyOptions])),
                    ref(Base)),
               edge(2,
                    ref(application(Key, ["literal", KeyOptions])),
                    const("pinned"))
             ].

test(label_form_does_not_mint_a_second_specialization) :-
    garbage_collect,
    fixture_rows('5_shared_specialization.dl7', Rows),
    named_owner(Rows, 'Left', Left),
    named_owner(Rows, 'Right', Right),
    owner_edges(Rows, Left, [edge(0, const(field), LeftTarget)]),
    owner_edges(Rows, Right, [edge(0, _, RightTarget)]),
    LeftTarget == RightTarget,
    findall(Target,
            ( member(call(ref(kernel(':')),
                          [ref(_), _, Target, const(_)]), Rows),
              Target = ref(application(_, [primitive(text)]))
            ),
            Targets),
    sort(Targets, Distinct),
    Distinct == [LeftTarget].

%% A partial application is an ordinary target under either label form, and both
%% forms reach it through the same Curry rows.
test(partial_target_is_accepted_under_an_atom_label) :-
    garbage_collect,
    fixture_diagnostics('6_partial_atom_label_target.dl7', Diagnostics),
    Diagnostics == [].

test(partial_target_is_accepted_under_a_compound_label) :-
    garbage_collect,
    fixture_rows('7_partial_compound_label_target.dl7', Rows),
    named_owner(Rows, 'Ledger', Ledger),
    named_owner(Rows, 'Key', Key),
    named_owner(Rows, 'KeyOptions', KeyOptions),
    named_owner(Rows, 'Curry', Curry),
    named_owner(Rows, 'Pair', Pair),
    named_owner(Rows, 'User', User),
    Partial = application(Curry,
                          [Pair, [bound(0, "reference", ref(User))]]),
    ground(Partial),
    owner_edges(Rows, Ledger, Edges),
    Edges == [edge(0,
                   ref(application(Key, ["pair", KeyOptions])),
                   ref(Partial))].

%% Deferral reads the whole diagnostic list. A deferrable label beside a real
%% target error must still raise that target error under either policy.
test(a_real_target_error_survives_a_deferrable_label) :-
    garbage_collect,
    fixture_diagnostics('11_mixed_deferrable_and_real_error.dl7',
                        Diagnostics),
    Diagnostics = [diagnostic(lower, NodeId,
                              expression_arity_mismatch('Option', 1, 3))],
    NodeId \== none.

%% Deferral is reachable only through lower_datalog_deferred/5, the entry
%% 0a_module_lowerer.pl uses; no .dl7 program reproduces it through compile_dl7.
test(a_wholly_deferrable_compound_bind_defers) :-
    garbage_collect,
    deferred_diagnostics(
        "(: KeyOptions (*))\n(: Base (*))\n(: Ledger (* (: (NotDeclaredLabel \"n\" KeyOptions) (NotDeclaredTarget Base))))\n",
        Diagnostics),
    Diagnostics == [].

test(a_mixed_compound_bind_does_not_defer) :-
    garbage_collect,
    deferred_diagnostics(
        "(: KeyOptions (*))\n(: Base (*))\n(: Wrap (* (: source type) (: return type)))\n(: Ledger (* (: (NotDeclaredLabel \"n\" KeyOptions) (Wrap Base Base Base))))\n",
        CompoundDiagnostics),
    CompoundDiagnostics = [diagnostic(lower, _, CompoundReason)],
    deferred_diagnostics(
        "(: KeyOptions (*))\n(: Base (*))\n(: Wrap (* (: source type) (: return type)))\n(: Ledger (* (: field (Wrap Base Base Base))))\n",
        AtomDiagnostics),
    AtomDiagnostics = [diagnostic(lower, _, AtomReason)],
    CompoundReason == AtomReason,
    CompoundReason == expression_arity_mismatch('Wrap', 1, 3).

%% HistoryV1 resolves at the deferred pass, so these two pin target parity for a
%% fixpoint-minted callable and exercise no deferral.
test(late_generated_callable_reads_the_same_under_both_labels) :-
    garbage_collect,
    fixture_rows('8_generated_callable_targets.dl7', Rows),
    named_owner(Rows, 'Both', Both),
    named_owner(Rows, 'Key', Key),
    named_owner(Rows, 'KeyOptions', KeyOptions),
    named_owner(Rows, 'Option', Option),
    named_owner(Rows, 'User', User),
    named_owner(Rows, 'HistoryOptions', HistoryOptions),
    named_owner(Rows, 'HistoryV1', HistoryV1),
    owner_edges(Rows, Both, Edges),
    Generated = application(HistoryV1, [User, HistoryOptions]),
    Expected = ref(application(Option, [Generated])),
    Edges == [ edge(0, const(atom_label), Expected),
               edge(1,
                    ref(application(Key, ["compound_label", KeyOptions])),
                    Expected)
             ].

test(saturated_generated_application_stands_as_a_target) :-
    garbage_collect,
    fixture_rows('9_saturated_generated_targets.dl7', Rows),
    named_owner(Rows, 'Both', Both),
    named_owner(Rows, 'Key', Key),
    named_owner(Rows, 'KeyOptions', KeyOptions),
    named_owner(Rows, 'User', User),
    named_owner(Rows, 'HistoryOptions', HistoryOptions),
    named_owner(Rows, 'HistoryV1', HistoryV1),
    owner_edges(Rows, Both, Edges),
    Expected = ref(application(HistoryV1, [User, HistoryOptions])),
    Edges == [ edge(0, const(atom_label), Expected),
               edge(1,
                    ref(application(Key, ["compound_label", KeyOptions])),
                    Expected)
             ].

%% The colon is the only infix form and it rotates before lowering, so both
%% spellings must reach the same edges an explicit prefix bind reaches.
test(infix_colon_carries_both_label_forms) :-
    garbage_collect,
    fixture_rows('10_infix_colon_spelling.dl7', Rows),
    named_owner(Rows, 'Separate', Separate),
    named_owner(Rows, 'Trailing', Trailing),
    named_owner(Rows, 'Key', Key),
    named_owner(Rows, 'KeyOptions', KeyOptions),
    named_owner(Rows, 'Option', Option),
    Expected = ref(application(Option, [primitive(text)])),
    owner_edges(Rows, Separate, SeparateEdges),
    SeparateEdges == [ edge(0, const(atom_label), Expected),
                       edge(1,
                            ref(application(Key,
                                            ["compound_label", KeyOptions])),
                            Expected)
                     ],
    owner_edges(Rows, Trailing, TrailingEdges),
    TrailingEdges == [edge(0, const(atom_label), Expected)].

%% Both label forms emit the same five Curry rules with the same rule origins;
%% only the final edge rule adds goal origins, and only for a derived label.
test(partial_rule_origins_differ_only_on_the_final_edge_rule) :-
    partial_environment(Owner, Callable, Bound, Environment),
    dl7_lowerer:partial_bind_rules(
        Owner, const_label(field), bind_node, 7,
        Callable, Bound, Environment, 5, AtomResult),
    AtomResult = ok(AtomRules, AtomOrigins, AtomNext),
    length(AtomRules, 5),
    AtomNext == 10,
    CommonOrigins = [ origin(rule(5), bind_node), origin(rule(6), bind_node),
                      origin(rule(7), bind_node), origin(rule(8), bind_node),
                      origin(rule(9), bind_node)
                    ],
    AtomOrigins == CommonOrigins,
    LabelGoals = [ pending_goal(positive, call(name(Owner, probe_a), [])),
                   pending_goal(positive, call(name(Owner, probe_b), [])) ],
    dl7_lowerer:partial_bind_rules(
        Owner,
        derived_label(var(derived_label(bind_node)), LabelGoals,
                      [label_node_a, label_node_b],
                      compound_label(bind_node)),
        bind_node, 7, Callable, Bound, Environment, 5, DerivedResult),
    DerivedResult = ok(DerivedRules, DerivedOrigins, DerivedNext),
    length(DerivedRules, 5),
    DerivedNext == 10,
    append(CommonOrigins,
           [ origin(goal(9, 0), label_node_a),
             origin(goal(9, 1), label_node_b)
           ],
           ExpectedDerivedOrigins),
    DerivedOrigins == ExpectedDerivedOrigins,
    once(append(AtomCommon, [AtomEdge], AtomRules)),
    once(append(DerivedCommon, [DerivedEdge], DerivedRules)),
    AtomCommon == DerivedCommon,
    Partial = application(owner(origins_curry),
                          [ owner(origins_callable),
                            [bound(0, "reference", ref(owner(origins_bound)))]
                          ]),
    AtomEdge == rule(call(name(Owner, ':'),
                          [ ref(Owner), const(field), ref(Partial),
                            const(7)
                          ]),
                     []),
    DerivedEdge == rule(call(name(Owner, ':'),
                             [ ref(Owner), var(derived_label(bind_node)),
                               ref(Partial), const(7)
                             ]),
                        LabelGoals).

%% Without the Curry reservation no Curry row can be built. The atom form keeps
%% its authored name in the payload; the compound form names its bind instead.
test(a_partial_label_description_names_the_bind_for_either_form) :-
    partial_environment(Owner, Callable, Bound, _),
    curryless_environment(Owner, Environment),
    dl7_lowerer:partial_bind_rules(
        Owner, const_label(field), bind_node, 7,
        Callable, Bound, Environment, 5, AtomResult),
    AtomResult == error(diagnostic(
                            lower, bind_node,
                            partial_application_requires_more_arguments(
                                field))),
    dl7_lowerer:partial_bind_rules(
        Owner,
        derived_label(var(derived_label(bind_node)), [], [],
                      compound_label(bind_node)),
        bind_node, 7, Callable, Bound, Environment, 5, DerivedResult),
    DerivedResult == error(diagnostic(
                               lower, bind_node,
                               partial_application_requires_more_arguments(
                                   compound_label(bind_node)))).

curryless_environment(Owner, expression_environment([], [], Edges)) :-
    partial_environment(Owner, _, _, expression_environment(_, _, Edges)).

partial_environment(Owner, Callable, Bound, Environment) :-
    Owner = owner(origins_fixture),
    CallableOwner = owner(origins_callable),
    Environment = expression_environment(
                      [ reservation(Owner, 'Curry',
                                    target(owner(origins_curry)), product),
                        reservation(Owner, 'Literal',
                                    target(owner(origins_literal)), product)
                      ],
                      [],
                      [ pending_edge(CallableOwner, left,
                                     name(CallableOwner, type), 0),
                        pending_edge(CallableOwner, return,
                                     name(CallableOwner, type), 1)
                      ]),
    Callable = callable(Owner, 'Pair', target(CallableOwner), 2, [], 1),
    Bound = [bound(0, ref(owner(origins_bound)))].

fixture_path(Name, Path) :-
    test_directory(TestDirectory),
    atomic_list_concat(
        [TestDirectory, '/fixtures/binding_symmetry/', Name], Path).

fixture_rows(Name, Rows) :-
    fixture_path(Name, Path),
    compile_dl7(Path, Rows, _RuntimeProgram, Diagnostics),
    Diagnostics == [].

fixture_diagnostics(Name, Diagnostics) :-
    fixture_path(Name, Path),
    compile_dl7(Path, _Rows, _RuntimeProgram, Diagnostics).

deferred_diagnostics(Text, Diagnostics) :-
    dl7_text_unit(binding_symmetry, binding_symmetry_source, Text, Unit, []),
    lower_datalog_deferred(
        Unit, expression_environment([], [], []),
        _Program, _Origins, Diagnostics).

named_owner(Rows, Name, Owner) :-
    member(call(ref(kernel(':')),
                [ref(_), const(Name), ref(Owner), const(_)]),
           Rows),
    !.

%% Compound labels are application terms, so the label stays a whole term and
%% the edges stay in authored index order rather than sorted by that term.
owner_edges(Rows, Owner, Edges) :-
    findall(Index-edge(Index, Label, Target),
            member(call(ref(kernel(':')),
                        [ref(Owner), Label, Target, const(Index)]),
                   Rows),
            Indexed),
    keysort(Indexed, Sorted),
    pairs_values(Sorted, Edges).

:- end_tests(dl7_binding_symmetry).
