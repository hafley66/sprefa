:- begin_tests(dl7_entrypoints).

:- use_module(library(aggregate), [aggregate_all/3]).
:- use_module(library(process), [process_create/3, process_wait/2]).
:- use_module('../src/0_reader/2_embedder', [dl7_text_unit/5]).
:- use_module('../src/0_reader/3_file_loader', [load_dl7/3]).
:- use_module('../src/2_comptime/2_compiler',
              [ compile_dl7/4,
                compile_unit/3
              ]).
:- use_module('../src/2_comptime/0_lowerer', [lower_datalog/4]).
:- use_module('../src/2_comptime/1_checker',
              [ check_datalog/4,
                check_goal_sequence/4
              ]).
:- use_module('../src/1_libtime/0_evaluator',
              [ evaluate/4,
                stratify_rules/3,
                validate_functional_rows/3
              ]).
:- use_module('fixtures/1_embedded', []).

test(file_and_bare_quasi_share_reader_and_expansion_pipeline) :-
    load_dl7('v7/test/fixtures/0_minimal.dl7',
             FileUnit, FileDiagnostics),
    FileUnit = dl7_unit(FileOrigin, content_sha256(FileDigest),
                        FileForms, FileRows, FileExpansions),
    once(dl7_embedded_fixture:dl7_unit(
             EmbeddedOrigin, content_sha256(EmbeddedDigest),
             EmbeddedForms, EmbeddedRows, EmbeddedExpansions)),
    content_snapshot(FileForms, FileRows, FileContent),
    content_snapshot(EmbeddedForms, EmbeddedRows, EmbeddedContent),
    origin_kinds(FileOrigin, EmbeddedOrigin, OriginKinds),
    equality(FileDigest, EmbeddedDigest, DigestEqual),
    equality(FileContent, EmbeddedContent, ContentEqual),
    Observed = entrypoint_result(
                   OriginKinds, DigestEqual, ContentEqual,
                   FileDiagnostics, FileExpansions, EmbeddedExpansions),
    Observed == entrypoint_result(true, true, true, [], [], []).

test(driver_is_canonical_on_two_consecutive_runs) :-
    load_dl7('v7/test/fixtures/0_minimal.dl7',
             ExpectedUnit, []),
    driver_run(Status1, Stdout1, Stderr1),
    driver_run(Status2, Stdout2, Stderr2),
    term_string(Unit1, Stdout1),
    term_string(Unit2, Stdout2),
    equality(Stdout1, Stdout2, OutputEqual),
    equality(Unit1, ExpectedUnit, FirstUnitEqual),
    equality(Unit2, ExpectedUnit, SecondUnitEqual),
    Observed = driver_result(Status1, Status2, OutputEqual,
                             FirstUnitEqual, SecondUnitEqual,
                             Stderr1, Stderr2),
    Observed == driver_result(exit(0), exit(0), true,
                              true, true, "", "").

test(userland_partial_maps_type_edges_deterministically) :-
    compile_dl7('v7/test/fixtures/2_partial.dl7',
                Rows1, Runtime1, Diagnostics1),
    compile_dl7('v7/test/fixtures/2_partial.dl7',
                Rows2, Runtime2, Diagnostics2),
    once(partial_snapshot(Rows1, Snapshot)),
    runtime_snapshot(Runtime1, RuntimeSnapshot),
    runtime_key_snapshot(Runtime1, KeySnapshot),
    evaluator_snapshot(EvaluatorSnapshot),
    equality(Rows1, Rows2, RowsEqual),
    equality(Runtime1, Runtime2, RuntimeEqual),
    length(Rows1, CompilerRowCount),
    Observed = partial_result(Diagnostics1, Diagnostics2,
                              CompilerRowCount, Snapshot,
                              RuntimeSnapshot, KeySnapshot, EvaluatorSnapshot,
                              RowsEqual, RuntimeEqual),
    Observed == partial_result(
                    [], [], 89,
                    partial(user,
                            [mapped(id, option(int), 0),
                             mapped(name, option(text), 1)]),
                    runtime(counts(32, 32, 13, 19, 5, 10, 13),
                            normalized(true)),
                    keys(colon([[0, 1], [0, 3]]),
                         edge_snapshot([[0, 1], [0, 3]]),
                         cons([[0, 1], [2]]),
                         intern([[0, 1]]),
                         predecessor([[0, 1], [0, 2]])),
                    evaluator(temporary_rules(0), temporary_seeds(0),
                              temporary_lower_rows(0)),
                    true, true),
    !.

test(final_closure_rejects_declared_functional_key_conflicts) :-
    Relation = ref(kernel(':')),
    Relations = [relation(Relation, 4, [[0, 1], [0, 3]])],
    Rows = [ call(Relation,
                  [ref(owner), const(name), ref(first), const(0)]),
             call(Relation,
                  [ref(owner), const(name), ref(second), const(1)])
           ],
    validate_functional_rows(Relations, Rows, Diagnostics),
    Diagnostics ==
        [diagnostic(evaluate, none,
                    functional_key_conflict(
                        Relation, [0, 1], [ref(owner), const(name)],
                        call(Relation,
                             [ref(owner), const(name), ref(first), const(0)]),
                        call(Relation,
                             [ref(owner), const(name), ref(second), const(1)]))
                   )].

test(authored_order_kernel_modes_are_checked_left_to_right) :-
    Construct = checked_goal(
                    positive,
                    call(ref(kernel(cons)),
                         [var(element), const(symbol(nil)), var(arguments)])),
    Intern = checked_goal(
                 positive,
                 call(ref(kernel(intern)),
                      [ref(option), var(arguments), var(result)])),
    check_goal_sequence([Construct, Intern], [element], Bound,
                        AcceptedDiagnostics),
    check_goal_sequence([Construct], [], _, RejectedDiagnostics),
    sort(Bound, SortedBound),
    Observed = authored_order(
                   accepted(SortedBound, AcceptedDiagnostics),
                   rejected(RejectedDiagnostics)),
    Observed == authored_order(
                    accepted([arguments, element, result], []),
                    rejected(
                        [diagnostic(
                             check, none,
                             underconstrained_kernel_goal(
                                 cons, [[2], [0, 1]]))])).

test(stratification_is_pure_deterministic_and_strict_cycle_checked) :-
    Source = ref(source),
    Left = ref(left),
    Right = ref(right),
    AcyclicRules =
        [rule(call(Left, [var(value)]),
              [checked_goal(negative,
                            call(Source, [var(value)]))])],
    CycleRules =
        [ rule(call(Left, [var(value)]),
               [checked_goal(negative,
                             call(Right, [var(value)]))]),
          rule(call(Right, [var(value)]),
               [checked_goal(positive,
                             call(Left, [var(value)]))])
        ],
    stratify_rules(AcyclicRules, AcyclicStrata, AcyclicDiagnostics),
    stratify_rules(CycleRules, CycleStrata, CycleDiagnostics),
    evaluate(CycleRules, [], CycleClosure, EvaluationDiagnostics),
    evaluator_snapshot(EvaluatorSnapshot),
    Observed = stratification(
                   acyclic(AcyclicStrata, AcyclicDiagnostics),
                   strict_cycle(CycleStrata, CycleDiagnostics,
                                evaluation(CycleClosure,
                                           EvaluationDiagnostics,
                                           EvaluatorSnapshot))),
    Observed == stratification(
                    acyclic([stratum(Left, 1)], []),
                    strict_cycle(
                        [],
                        [diagnostic(
                             stratify, none,
                             strict_dependency_cycle([Left, Right]))],
                        evaluation(
                            [],
                            [diagnostic(
                                 stratify, none,
                                 strict_dependency_cycle([Left, Right]))],
                            evaluator(temporary_rules(0), temporary_seeds(0),
                                      temporary_lower_rows(0))))).

test(cons_constructs_deconstructs_and_stops_at_the_nil_tail) :-
    Rules =
        [ rule(call(ref(singleton), [var(list)]),
               [checked_goal(
                    positive,
                    call(ref(kernel(cons)),
                         [const(one), const(symbol(nil)), var(list)]))]),
          rule(call(ref(pair), [var(list)]),
               [ checked_goal(
                     positive,
                     call(ref(kernel(cons)),
                          [const(two), const(symbol(nil)), var(tail)])),
                 checked_goal(
                     positive,
                     call(ref(kernel(cons)),
                          [const(one), var(tail), var(list)]))
               ]),
          rule(call(ref(suffix), [var(list)]),
               [checked_goal(positive,
                             call(ref(source), [var(list)]))]),
          rule(call(ref(suffix), [var(tail)]),
               [ checked_goal(positive,
                              call(ref(suffix), [var(list)])),
                 checked_goal(
                     positive,
                     call(ref(kernel(cons)),
                          [var(head), var(tail), var(list)]))
               ]),
          rule(call(ref(item), [var(head)]),
               [ checked_goal(positive,
                              call(ref(suffix), [var(list)])),
                 checked_goal(
                     positive,
                     call(ref(kernel(cons)),
                          [var(head), var(tail), var(list)]))
               ]),
          rule(call(ref(empty_witness), []),
               [checked_goal(
                    positive,
                    call(ref(kernel(cons)),
                         [var(head), var(tail), const([])]))]),
          rule(call(ref(improper_witness), []),
               [checked_goal(
                    positive,
                    call(ref(kernel(cons)),
                         [var(head), var(tail), const([one | improper])]))])
        ],
    Seeds = [call(ref(source),
                  [const([const(one), const(two), const(three)])])],
    evaluate(Rules, Seeds, Closure, EvaluationDiagnostics),
    evaluator_snapshot(EvaluatorSnapshot),
    findall(Item,
            member(call(ref(item), [const(Item)]), Closure),
            Items),
    findall(List,
            member(call(ref(suffix), [const(List)]), Closure),
            Suffixes0),
    sort(Suffixes0, Suffixes),
    findall(List,
            member(call(ref(singleton), [const(List)]), Closure),
            Singletons),
    findall(List,
            member(call(ref(pair), [const(List)]), Closure),
            Pairs),
    witness_presence(Closure, empty_witness, EmptyWitness),
    witness_presence(Closure, improper_witness, ImproperWitness),
    underconstrained_cons_diagnostic(SourceDiagnostics),
    Observed = cons_result(
                   evaluation(EvaluationDiagnostics, EvaluatorSnapshot),
                   traversal(Items, Suffixes),
                   construction(Singletons, Pairs),
                   absent(EmptyWitness, ImproperWitness),
                   source_check(SourceDiagnostics)),
    Observed == cons_result(
                    evaluation(
                        [],
                        evaluator(temporary_rules(0), temporary_seeds(0),
                                  temporary_lower_rows(0))),
                    traversal(
                        [one, three, two],
                        [ symbol(nil),
                          [const(one), const(two), const(three)],
                          [const(three)],
                          [const(two), const(three)]
                        ]),
                    construction([[const(one)]],
                                 [[const(one), const(two)]]),
                    absent(false, false),
                    source_check(
                        [diagnostic(
                             check, reader_node(cons_mode_source, 26),
                             underconstrained_kernel_goal(
                                 cons, [[2], [0, 1]]))])),
    !.

test(checked_edge_indices_expose_adjacent_and_strict_order) :-
    Text = "(: Empty (*))\n(: Singleton (* (: only int)))\n(: Triple (* (: first int) (: second int) (: third int)))\n(: before (* (: owner type) (: earlier int) (: later int)))\n(<- (before ?Owner ?Earlier ?Later)\n    (predecessor ?Owner ?Earlier ?Later))\n(<- (before ?Owner ?Earlier ?Later)\n    (predecessor ?Owner ?Earlier ?Middle)\n    (before ?Owner ?Middle ?Later))\n",
    dl7_text_unit(ordered_index, ordered_index_source, Text, Unit,
                  ReaderDiagnostics),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows),
                 CompilerDiagnostics),
    named_owner(CompilerRows, 'Empty', Empty),
    named_owner(CompilerRows, 'Singleton', Singleton),
    named_owner(CompilerRows, 'Triple', Triple),
    named_owner(CompilerRows, before, Before),
    relation_pairs(CompilerRows, ref(kernel(predecessor)), Empty,
                   EmptyPairs),
    relation_pairs(CompilerRows, ref(kernel(predecessor)), Singleton,
                   SingletonPairs),
    relation_pairs(CompilerRows, ref(kernel(predecessor)), Triple,
                   AdjacentPairs),
    relation_pairs(CompilerRows, ref(Before), Triple, StrictPairs),
    runtime_predecessor_snapshot(RuntimeProgram, Triple, RuntimeSnapshot),
    Observed = ordered_index_result(
                   diagnostics(ReaderDiagnostics, CompilerDiagnostics),
                   empty(EmptyPairs),
                   singleton(SingletonPairs),
                   triple(adjacent(AdjacentPairs), strict(StrictPairs)),
                   RuntimeSnapshot),
    Observed == ordered_index_result(
                    diagnostics([], []),
                    empty([]),
                    singleton([]),
                    triple(
                        adjacent([0-1, 1-2]),
                        strict([0-1, 0-2, 1-2])),
                    runtime(
                        keys([[0, 1], [0, 2]]),
                        ordered_seeds([0-1, 1-2]))),
    !.

test(prefix_negation_is_safe_stratified_and_cleanup_scoped) :-
    anti_join_receipt(AntiJoin),
    unsafe_negation_receipt(Unsafe),
    negative_cycle_receipt(Cycle),
    negative_kernel_receipt(Kernel),
    evaluator_exception_receipt(Exception),
    Observed = negation_result(AntiJoin, Unsafe, Cycle, Kernel, Exception),
    Observed == negation_result(
                    anti_join(
                        values(["a"]),
                        body([positive(candidate), negative(blocked)]),
                        dependencies(positive, negative),
                        strata(candidate(0), blocked(0), allowed(1)),
                        evaluator(temporary_rules(0), temporary_seeds(0),
                                  temporary_lower_rows(0))),
                    unsafe(
                        goal_node(32),
                        variable_node(27)),
                    cycle(
                        goal_node(38),
                        relations([left, right])),
                    kernel(
                        goal_node(29),
                        negative_constructive_kernel_goal(cons)),
                    exception(
                        caught,
                        evaluator(temporary_rules(0), temporary_seeds(0),
                                  temporary_lower_rows(0)))),
    !.

test(count_groups_completed_lower_proofs_and_rejects_bad_placement) :-
    grouped_count_receipt(Grouped),
    multiple_count_receipt(Multiple),
    misplaced_count_receipt(Misplaced),
    nested_head_receipt(Nested),
    aggregate_cycle_receipt(Cycle),
    Observed = count_result(Grouped, Multiple, Misplaced, Nested, Cycle),
    Observed == count_result(
                    grouped(
                        rows(["east"-2, "west"-1]),
                        checked_head(
                            [plain(region), aggregate(count, region)]),
                        dependency(positive),
                        strata(source(0), count(1)),
                        evaluator(temporary_rules(0), temporary_seeds(0),
                                  temporary_lower_rows(0))),
                    multiple(
                        node(27),
                        multiple_count_aggregates(region_count)),
                    misplaced(
                        node(26),
                        aggregate_outside_rule_head),
                    nested(
                        node(25),
                        nested_call_argument),
                    cycle(
                        node(13),
                        aggregate_dependency_cycle([loop]))),
    !.

grouped_count_receipt(Receipt) :-
    Text = "(: sale (* (: region text) (: item text)))\n(: region_count (* (: region text) (: total int)))\n(sale \"east\" \"one\")\n(sale \"east\" \"two\")\n(sale \"west\" \"three\")\n(<- (region_count ?Region (count ?Region))\n    (sale ?Region ?Item))\n",
    dl7_text_unit(grouped_count, grouped_count_source, Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows), []),
    named_owner(CompilerRows, sale, Sale),
    named_owner(CompilerRows, region_count, RegionCount),
    findall(Region-Count,
            member(call(ref(RegionCount), [const(Region), const(Count)]),
                   CompilerRows),
            Rows),
    grouped_count_runtime_snapshot(RuntimeProgram, Sale, RegionCount,
                                   RuntimeSnapshot),
    evaluator_snapshot(EvaluatorSnapshot),
    RuntimeSnapshot = runtime(CheckedHead, Dependency, Strata),
    Receipt = grouped(rows(Rows), CheckedHead, Dependency, Strata,
                      EvaluatorSnapshot).

grouped_count_runtime_snapshot(
    checked_datalog(_, datalog_program(_, _, Rules), Depends, Strata),
    Sale, RegionCount,
    runtime(checked_head(
                [plain(region), aggregate(count, region)]),
            dependency(positive),
            strata(source(SaleLevel), count(CountLevel)))) :-
    memberchk(rule(
                  call(ref(RegionCount),
                       [var(Region), aggregate(count, var(Region))]),
                  [checked_goal(
                       positive,
                       call(ref(Sale), [var(Region), var(_)]))]),
              Rules),
    memberchk(depends(ref(RegionCount), ref(Sale), positive), Depends),
    memberchk(stratum(ref(Sale), SaleLevel), Strata),
    memberchk(stratum(ref(RegionCount), CountLevel), Strata).

multiple_count_receipt(multiple(node(NodeIndex), Reason)) :-
    Text = "(: source (* (: value text)))\n(: region_count (* (: first int) (: second int)))\n(source \"x\")\n(<- (region_count (count ?Value) (count ?Value))\n    (source ?Value))\n",
    dl7_text_unit(multiple_count, multiple_count_source, Text, Unit, []),
    compile_unit(Unit, [],
                 [diagnostic(lower,
                             reader_node(multiple_count_source, NodeIndex),
                             Reason)]).

misplaced_count_receipt(misplaced(node(NodeIndex), Reason)) :-
    Text = "(: source (* (: value text)))\n(: bad (* (: value text)))\n(source \"x\")\n(<- (bad ?Value)\n    (count ?Value)\n    (source ?Value))\n",
    dl7_text_unit(misplaced_count, misplaced_count_source, Text, Unit, []),
    compile_unit(Unit, [],
                 [diagnostic(lower,
                             reader_node(misplaced_count_source, NodeIndex),
                             Reason)]).

nested_head_receipt(nested(node(NodeIndex), Reason)) :-
    Text = "(: source (* (: value text)))\n(: bad (* (: value text)))\n(source \"x\")\n(<- (bad (wrapper ?Value))\n    (source ?Value))\n",
    dl7_text_unit(nested_head, nested_head_source, Text, Unit, []),
    compile_unit(Unit, [],
                 [diagnostic(lower,
                             reader_node(nested_head_source, NodeIndex),
                             Reason)]).

aggregate_cycle_receipt(
    cycle(node(NodeIndex), aggregate_dependency_cycle([loop]))) :-
    Text = "(: loop (* (: value text) (: total int)))\n(<- (loop ?Value (count ?Value))\n    (loop ?Value ?Total))\n",
    dl7_text_unit(aggregate_cycle, aggregate_cycle_source, Text, Unit, []),
    lower_datalog(Unit, Basement, Origins, []),
    Basement = basement_program(root_graph(_, PendingEdges), _),
    memberchk(pending_edge(_, loop, target(Loop), _), PendingEdges),
    check_datalog(
        Basement, Origins, [],
        [diagnostic(stratify,
                    reader_node(aggregate_cycle_source, NodeIndex),
                    aggregate_dependency_cycle([ref(Loop)]))]).

anti_join_receipt(Receipt) :-
    Text = "(: candidate (* (: value text)))\n(: blocked (* (: value text)))\n(: allowed (* (: value text)))\n(candidate \"a\")\n(candidate \"b\")\n(blocked \"b\")\n(<- (allowed ?Value)\n    (candidate ?Value)\n    (not (blocked ?Value)))\n",
    dl7_text_unit(anti_join, anti_join_source, Text, Unit, []),
    compile_unit(Unit,
                 compiled_unit(_, RuntimeProgram, CompilerRows), []),
    named_owner(CompilerRows, candidate, Candidate),
    named_owner(CompilerRows, blocked, Blocked),
    named_owner(CompilerRows, allowed, Allowed),
    findall(Value,
            member(call(ref(Allowed), [const(Value)]), CompilerRows),
            Values),
    anti_join_runtime_snapshot(RuntimeProgram, Candidate, Blocked, Allowed,
                               RuntimeSnapshot),
    evaluator_snapshot(EvaluatorSnapshot),
    RuntimeSnapshot = runtime(Body, Dependencies, Strata),
    Receipt = anti_join(values(Values), Body, Dependencies, Strata,
                        EvaluatorSnapshot).

anti_join_runtime_snapshot(
    checked_datalog(_, datalog_program(_, _, Rules), Depends, Strata),
    Candidate, Blocked, Allowed,
    runtime(body(BodySnapshot), dependencies(Positive, Negative),
            strata(candidate(CandidateLevel), blocked(BlockedLevel),
                    allowed(AllowedLevel)))) :-
    memberchk(rule(call(ref(Allowed), [_]), Body), Rules),
    maplist(label_checked_goal(Candidate, Blocked), Body, BodySnapshot),
    dependency_presence(Depends, ref(Allowed), ref(Candidate), positive,
                        Positive),
    dependency_presence(Depends, ref(Allowed), ref(Blocked), negative,
                        Negative),
    memberchk(stratum(ref(Candidate), CandidateLevel), Strata),
    memberchk(stratum(ref(Blocked), BlockedLevel), Strata),
    memberchk(stratum(ref(Allowed), AllowedLevel), Strata).

label_checked_goal(Candidate, _,
                   checked_goal(positive, call(ref(Candidate), [_])),
                   positive(candidate)).
label_checked_goal(_, Blocked,
                   checked_goal(negative, call(ref(Blocked), [_])),
                   negative(blocked)).

dependency_presence(Depends, Head, Body, Polarity, Polarity) :-
    memberchk(depends(Head, Body, Polarity), Depends).

unsafe_negation_receipt(unsafe(goal_node(GoalIndex),
                               variable_node(VariableIndex))) :-
    Text = "(: candidate (* (: value text)))\n(: blocked (* (: value text)))\n(: allowed (* (: value text)))\n(<- (allowed ?Value)\n    (not (blocked ?Value))\n    (candidate ?Value))\n",
    dl7_text_unit(unsafe_negation, unsafe_negation_source, Text, Unit, []),
    compile_unit(
        Unit, [],
        [diagnostic(
             check, reader_node(unsafe_negation_source, GoalIndex),
             unbound_negative_goal(
                 [variable(reader_node(unsafe_negation_source, VariableIndex),
                           'Value')]))]).

negative_cycle_receipt(cycle(goal_node(GoalIndex),
                             relations([left, right]))) :-
    Text = "(: domain (* (: value text)))\n(: left (* (: value text)))\n(: right (* (: value text)))\n(domain \"a\")\n(<- (left ?Value)\n    (domain ?Value)\n    (not (right ?Value)))\n(<- (right ?Value)\n    (domain ?Value)\n    (not (left ?Value)))\n",
    dl7_text_unit(negative_cycle, negative_cycle_source, Text, Unit, []),
    lower_datalog(Unit, Basement, Origins, []),
    Basement = basement_program(root_graph(_, PendingEdges), _),
    memberchk(pending_edge(_, left, target(Left), _), PendingEdges),
    memberchk(pending_edge(_, right, target(Right), _), PendingEdges),
    sort([ref(Left), ref(Right)], ExpectedRelations),
    check_datalog(
        Basement, Origins, [],
        [diagnostic(stratify,
                    reader_node(negative_cycle_source, GoalIndex),
                    strict_dependency_cycle(ExpectedRelations))]).

negative_kernel_receipt(
    kernel(goal_node(GoalIndex), negative_constructive_kernel_goal(cons))) :-
    Text = "(: source (* (: value any)))\n(: bad (* (: value any)))\n(source \"x\")\n(<- (bad ?Value)\n    (source ?Value)\n    (not (cons ?Value 'nil ?List)))\n",
    dl7_text_unit(negative_kernel, negative_kernel_source, Text, Unit, []),
    compile_unit(
        Unit, [],
        [diagnostic(
             check, reader_node(negative_kernel_source, GoalIndex),
             negative_constructive_kernel_goal(cons))]).

evaluator_exception_receipt(exception(caught, EvaluatorSnapshot)) :-
    catch(evaluate([], [call(ref(seed), [_])], _, _),
          error(instantiation_error, _),
          true),
    evaluator_snapshot(EvaluatorSnapshot).

witness_presence(Closure, Relation, true) :-
    memberchk(call(ref(Relation), []), Closure),
    !.
witness_presence(_, _, false).

underconstrained_cons_diagnostic(Diagnostics) :-
    Text = "(: Source (* (: value any)))\n(: Bad (* (: value any)))\n(Source \"ok\")\n(<- (Bad ?Value)\n    (cons ?Head ?Tail ?List)\n    (Source ?Value))\n",
    dl7_text_unit(cons_mode, cons_mode_source, Text, Unit, []),
    compile_unit(Unit, [], Diagnostics).

partial_snapshot(Rows, Snapshot) :-
    member(call(ref(kernel(':')),
                [ref(Module), const('User'), ref(User), const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('Partial'), ref(PartialConstructor),
                 const(_)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Module), const('Option'), ref(OptionConstructor),
                 const(_)]), Rows),
    Partial = application(PartialConstructor, [User]),
    member(call(ref(kernel(node)), [ref(Partial)]), Rows),
    member(call(ref(kernel(product)), [ref(Partial)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Partial), const(id),
                 ref(application(OptionConstructor, [primitive(int)])),
                 const(0)]), Rows),
    member(call(ref(kernel(':')),
                [ref(Partial), const(name),
                 ref(application(OptionConstructor, [primitive(text)])),
                 const(1)]), Rows),
    Snapshot = partial(user,
                       [mapped(id, option(int), 0),
                        mapped(name, option(text), 1)]).

runtime_snapshot(
    checked_datalog(root_graph(Nodes, Edges),
                    datalog_program(Relations, Seeds, Rules),
                    Depends, Strata),
    runtime(counts(NodeCount, EdgeCount, RelationCount, SeedCount,
                   RuleCount, DependsCount, StrataCount),
            normalized(Normalized))) :-
    maplist(length,
            [Nodes, Edges, Relations, Seeds, Rules, Depends, Strata],
            [NodeCount, EdgeCount, RelationCount, SeedCount,
             RuleCount, DependsCount, StrataCount]),
    (   normalized_program(Relations, Seeds, Rules, Depends, Strata)
    ->  Normalized = true
    ;   Normalized = false
    ).

runtime_key_snapshot(
    checked_datalog(_, datalog_program(Relations, _, _), _, _),
    keys(colon(ColonKeys), edge_snapshot(SnapshotKeys),
         cons(ConsKeys), intern(InternKeys),
         predecessor(PredecessorKeys))) :-
    memberchk(relation(ref(kernel(':')), 4, ColonKeys), Relations),
    memberchk(relation(ref(kernel(edge_snapshot)), 4, SnapshotKeys),
              Relations),
    memberchk(relation(ref(kernel(cons)), 3, ConsKeys), Relations),
    memberchk(relation(ref(kernel(intern)), 3, InternKeys), Relations),
    memberchk(relation(ref(kernel(predecessor)), 3, PredecessorKeys),
              Relations).

named_owner(Rows, Name, Owner) :-
    member(call(ref(kernel(':')),
                [ref(_), const(Name), ref(Owner), const(_)]),
           Rows),
    !.

relation_pairs(Rows, Relation, Owner, Pairs) :-
    findall(Earlier-Later,
            member(call(Relation,
                        [ref(Owner), const(Earlier), const(Later)]),
                   Rows),
            Pairs).

runtime_predecessor_snapshot(
    checked_datalog(_, datalog_program(Relations, Seeds, _), _, _),
    Owner,
    runtime(keys(Keys), ordered_seeds(Pairs))) :-
    memberchk(relation(ref(kernel(predecessor)), 3, Keys), Relations),
    relation_pairs(Seeds, ref(kernel(predecessor)), Owner, Pairs).

normalized_program(Relations, Seeds, Rules, Depends, Strata) :-
    maplist(normalized_relation, Relations),
    maplist(normalized_call, Seeds),
    maplist(normalized_rule, Rules),
    maplist(normalized_depends, Depends),
    maplist(normalized_stratum, Strata).

normalized_relation(relation(ref(_), Arity, KeySets)) :-
    integer(Arity),
    is_list(KeySets).
normalized_call(call(ref(_), Arguments)) :- is_list(Arguments).
normalized_rule(rule(Head, Body)) :-
    normalized_call(Head),
    maplist(normalized_goal, Body).
normalized_goal(checked_goal(Polarity, Call)) :-
    memberchk(Polarity, [positive, negative]),
    normalized_call(Call).
normalized_depends(depends(ref(_), ref(_), Polarity)) :-
    memberchk(Polarity, [positive, negative]).
normalized_stratum(stratum(ref(_), Level)) :-
    integer(Level),
    Level >= 0.

evaluator_snapshot(
    evaluator(temporary_rules(RuleFacts), temporary_seeds(SeedFacts),
              temporary_lower_rows(LowerFacts))) :-
    aggregate_all(count, dl7_evaluator:evaluation_rule(_, _), RuleFacts),
    aggregate_all(count, dl7_evaluator:evaluation_seed(_, _), SeedFacts),
    aggregate_all(count, dl7_evaluator:evaluation_lower(_, _), LowerFacts).

origin_kinds(file(_),
             embedded(_, position(_, _, _)),
             true) :-
    !.
origin_kinds(_, _, false).

equality(Left, Right, true) :-
    Left == Right,
    !.
equality(_, _, false).

content_snapshot(Forms, SourceRows,
                 content(FormSnapshot, SourceSnapshot)) :-
    maplist(content_node, Forms, FormSnapshot),
    maplist(content_source, SourceRows, SourceSnapshot).

content_node(node(reader_node(_, Index), Payload),
             node(Index, Snapshot)) :-
    content_payload(Payload, Snapshot).

content_payload(atom(Name), atom(Name)).
content_payload(literal(Value), literal(Value)).
content_payload(variable(VariableId, Name),
                variable(SnapshotId, Name)) :-
    content_variable_id(VariableId, SnapshotId).
content_payload(form(Nodes), form(Snapshots)) :-
    maplist(content_node, Nodes, Snapshots).

content_variable_id(variable(reader_node(_, Index), Name),
                    variable(Index, Name)).

content_source(
    source(reader_node(_, Index), _, StartOffset, EndOffset,
           StartLine, StartColumn, EndLine, EndColumn),
    source(Index, StartOffset, EndOffset,
           StartLine, StartColumn, EndLine, EndColumn)).

driver_run(Status, Stdout, Stderr) :-
    process_create(
        path(swipl),
        [ '-q',
          '-s', 'v7/src/0_reader/4_cli_mainer.pl',
          '--', 'v7/test/fixtures/0_minimal.dl7'
        ],
        [ stdout(pipe(StdoutStream)),
          stderr(pipe(StderrStream)),
          process(Process)
        ]),
    read_string(StdoutStream, _, Stdout),
    close(StdoutStream),
    read_string(StderrStream, _, Stderr),
    close(StderrStream),
    process_wait(Process, Status),
    !.

:- end_tests(dl7_entrypoints).
