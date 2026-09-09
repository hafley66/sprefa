:- begin_tests(dl7_interned_storage).

:- use_module('../src/2_comptime/2_compiler', [compile_dl7_project/5]).
:- use_module('../src/3_emit/1_artifact_emitter', [emit_compiled/4]).

:- dynamic test_directory/1.
:- prolog_load_context(directory, TestDirectory),
   assertz(test_directory(TestDirectory)).

test(selected_type_graph_closes_to_exact_target_neutral_layout_rows) :-
    garbage_collect,
    fixture_paths(EmitterPath, FixturePath),
    file_directory_name(EmitterPath, EmittersDirectory),
    file_directory_name(EmittersDirectory, V7Directory),
    compile_dl7_project(
        V7Directory, [EmitterPath, FixturePath],
        CompilerRows, RuntimeProgram, CompileDiagnostics),
    CompileDiagnostics == [],
    named_owner(CompilerRows, 'InternedStorageEmitter', Emitter),
    named_owners(
        CompilerRows,
        [ 'Repository'-Repository,
          'WatchedRef'-WatchedRef,
          'SourceCoordinate'-SourceCoordinate,
          'SourceRevision'-SourceRevision,
          'RefTransition'-RefTransition,
          'Span'-Span,
          'Node'-Node,
          'Unrelated'-Unrelated,
          'SpanChoice'-SpanChoice,
          'ChoiceRoot'-ChoiceRoot,
          'SharedTextDictionary'-TextDictionary,
          'InternedPolicy'-InternedPolicy,
          'PlainKindPolicy'-PlainKindPolicy,
          'RefTransitionPolicy'-RefTransitionPolicy,
          'SumPolicy'-SumPolicy
        ]),
    type_edges(CompilerRows, Node, NodeEdges),
    NodeEdges ==
        [ edge(0, primary, ref(Span)),
          edge(1, secondary, ref(Span)),
          edge(2, kind, ref(primitive(text)))
        ],
    type_edges(CompilerRows, SourceRevision, SourceRevisionEdges),
    SourceRevisionEdges ==
        [ edge(0, coordinate, ref(SourceCoordinate)),
          edge(1, content_hash, ref(primitive(text)))
        ],
    type_edges(CompilerRows, RefTransition, RefTransitionEdges),
    RefTransitionEdges ==
        [ edge(0, watched_ref, ref(WatchedRef)),
          edge(1, old_sha, ref(primitive(text))),
          edge(2, new_sha, ref(primitive(text)))
        ],
    compiler_closed_runtime(RuntimeProgram, ClosedRuntime),
    emit_compiled(
        dl7(Emitter),
        compiled_unit([], ClosedRuntime, CompilerRows),
        artifacts(Artifacts), EmitDiagnostics),
    EmitDiagnostics == [],
    artifact_rows(Artifacts, "types", TypeRows),
    artifact_rows(Artifacts, "fields", FieldRows),
    artifact_rows(Artifacts, "dependencies", DependencyRows),
    artifact_rows(Artifacts, "projections", ProjectionRows),
    artifact_rows(Artifacts, "dictionaries", DictionaryRows),
    artifact_rows(Artifacts, "identities", IdentityRows),
    expected_selected_types(
        Repository, WatchedRef, SourceCoordinate, SourceRevision,
        RefTransition, Span, Node, SpanChoice, ChoiceRoot,
        InternedPolicy, PlainKindPolicy, RefTransitionPolicy, SumPolicy,
        ExpectedTypes),
    exact_rows(TypeRows, ExpectedTypes),
    policy_rows(FieldRows, InternedPolicy, InternedFields),
    expected_node_fields(
        InternedPolicy, Repository, WatchedRef, SourceCoordinate,
        SourceRevision, Span, Node, TextDictionary,
        "dictionary-local-id", InternedFieldsExpected),
    exact_rows(InternedFields, InternedFieldsExpected),
    policy_rows(FieldRows, PlainKindPolicy, PlainFields),
    expected_node_fields(
        PlainKindPolicy, Repository, WatchedRef, SourceCoordinate,
        SourceRevision, Span, Node, TextDictionary,
        "inline", PlainFieldsExpected),
    exact_rows(PlainFields, PlainFieldsExpected),
    policy_rows(FieldRows, RefTransitionPolicy, TransitionFields),
    expected_transition_fields(
        RefTransitionPolicy, Repository, WatchedRef, RefTransition,
        TextDictionary, TransitionFieldsExpected),
    exact_rows(TransitionFields, TransitionFieldsExpected),
    policy_rows(FieldRows, SumPolicy, SumFields),
    expected_sum_fields(
        SumPolicy, Repository, WatchedRef, SourceCoordinate,
        SourceRevision, Span, SpanChoice, ChoiceRoot, TextDictionary,
        SumFieldsExpected),
    exact_rows(SumFields, SumFieldsExpected),
    \+ row_mentions(Unrelated, TypeRows),
    \+ row_mentions(Unrelated, FieldRows),
    expected_dictionaries(
        [InternedPolicy, PlainKindPolicy, RefTransitionPolicy, SumPolicy],
        TextDictionary, ExpectedDictionaries),
    exact_rows(DictionaryRows, ExpectedDictionaries),
    policy_rows(DependencyRows, InternedPolicy, InternedDependencies),
    expected_node_dependencies(
        InternedPolicy, Repository, WatchedRef, SourceCoordinate,
        SourceRevision, Span, Node, TextDictionary,
        InternedDependenciesExpected),
    exact_rows(InternedDependencies, InternedDependenciesExpected),
    maplist(field_projection, InternedFieldsExpected,
            InternedProjectionsExpected),
    policy_rows(ProjectionRows, InternedPolicy, InternedProjections),
    exact_rows(InternedProjections, InternedProjectionsExpected),
    policy_rows(IdentityRows, InternedPolicy, InternedIdentities),
    expected_node_identities(
        InternedPolicy, Repository, WatchedRef, SourceCoordinate,
        SourceRevision, Span, Node, TextDictionary,
        InternedIdentitiesExpected),
    exact_rows(InternedIdentities, InternedIdentitiesExpected),
    length(TypeRows, 22),
    length(FieldRows, 48),
    length(DependencyRows, 41),
    length(ProjectionRows, 48),
    length(DictionaryRows, 4),
    length(IdentityRows, 26),
    !.

fixture_paths(EmitterPath, FixturePath) :-
    test_directory(TestDirectory),
    directory_file_path(
        TestDirectory, '../emitters/2_interned_storage.dl7', Emitter0),
    directory_file_path(
        TestDirectory, 'fixtures/16_interned_storage.dl7', Fixture0),
    absolute_file_name(Emitter0, EmitterPath, [access(read)]),
    absolute_file_name(Fixture0, FixturePath, [access(read)]).

% Layout rows are already closed by the compiler's real DL7 comptime pass.
% The sliced runtime prevents the artifact adapter from reifying unrelated
% executable rules; this layout query has no program_relation dependencies.
compiler_closed_runtime(
    checked_datalog(Graph, datalog_program(Relations, _, _), _, _),
    checked_datalog(Graph, datalog_program(Relations, [], []), [], [])).

named_owners(_, []).
named_owners(Rows, [Name-Owner | Named]) :-
    named_owner(Rows, Name, Owner),
    named_owners(Rows, Named).

named_owner(Rows, Name, Owner) :-
    member(call(ref(kernel(':')),
                [ref(_), const(Name), ref(Owner), const(_)]),
           Rows),
    !.

type_edges(Rows, Owner, Edges) :-
    findall(
        edge(Index, Label, Target),
        member(call(ref(kernel(':')),
                    [ref(Owner), const(Label), Target, const(Index)]),
               Rows),
        Edges0),
    sort(Edges0, Edges).

artifact_rows(Artifacts, Name, Rows) :-
    memberchk(artifact(Name, _, Rows), Artifacts).

policy_rows(Rows, Policy, Selected) :-
    include(has_policy(Policy), Rows, Selected).

has_policy(Policy, [ref(Policy) | _]).

row_mentions(Identity, RowSets) :-
    member(Row, RowSets),
    memberchk(ref(Identity), Row).

exact_rows(Observed, Expected) :-
    sort(Observed, ObservedSorted),
    sort(Expected, ExpectedSorted),
    ObservedSorted == ExpectedSorted.

expected_selected_types(
    Repository, WatchedRef, SourceCoordinate, SourceRevision,
    RefTransition, Span, Node, SpanChoice, ChoiceRoot,
    InternedPolicy, PlainKindPolicy, RefTransitionPolicy, SumPolicy,
    Rows) :-
    findall(
        [ref(Policy), ref(Type), const(Kind)],
        selected_type(Policy, Type, Kind,
                      Repository, WatchedRef, SourceCoordinate,
                      SourceRevision, RefTransition, Span, Node,
                      SpanChoice, ChoiceRoot,
                      InternedPolicy, PlainKindPolicy,
                      RefTransitionPolicy, SumPolicy),
        Rows).

selected_type(Policy, Type, "product",
              Repository, WatchedRef, SourceCoordinate, SourceRevision,
              _, Span, Node, _, _, InternedPolicy, PlainKindPolicy, _, _) :-
    member(Policy, [InternedPolicy, PlainKindPolicy]),
    member(Type, [Repository, WatchedRef, SourceCoordinate,
                  SourceRevision, Span, Node]).
selected_type(RefTransitionPolicy, Type, "product",
              Repository, WatchedRef, _, _, RefTransition, _, _, _, _,
              _, _, RefTransitionPolicy, _) :-
    member(Type, [Repository, WatchedRef, RefTransition]).
selected_type(SumPolicy, Type, "product",
              Repository, WatchedRef, SourceCoordinate, SourceRevision,
              _, Span, _, _, ChoiceRoot, _, _, _, SumPolicy) :-
    member(Type, [Repository, WatchedRef, SourceCoordinate,
                  SourceRevision, Span, ChoiceRoot]).
selected_type(SumPolicy, SpanChoice, "sum",
              _, _, _, _, _, _, _, SpanChoice, _, _, _, _, SumPolicy).

expected_node_fields(
    Policy, Repository, WatchedRef, SourceCoordinate, SourceRevision,
    Span, Node, Dictionary, KindRepresentation,
    [ [ref(Policy), ref(Repository), const(url), const(0),
       ref(primitive(text)), const("dictionary-local-id"), ref(Dictionary)],
      [ref(Policy), ref(WatchedRef), const(repository), const(0),
       ref(Repository), const("reference-local-id"), ref(Repository)],
      [ref(Policy), ref(WatchedRef), const(name), const(1),
       ref(primitive(text)), const("dictionary-local-id"), ref(Dictionary)],
      [ref(Policy), ref(SourceCoordinate), const(repository), const(0),
       ref(Repository), const("reference-local-id"), ref(Repository)],
      [ref(Policy), ref(SourceCoordinate), const(watched_ref), const(1),
       ref(WatchedRef), const("reference-local-id"), ref(WatchedRef)],
      [ref(Policy), ref(SourceCoordinate), const(path), const(2),
       ref(primitive(text)), const("dictionary-local-id"), ref(Dictionary)],
      [ref(Policy), ref(SourceRevision), const(coordinate), const(0),
       ref(SourceCoordinate), const("reference-local-id"),
       ref(SourceCoordinate)],
      [ref(Policy), ref(SourceRevision), const(content_hash), const(1),
       ref(primitive(text)), const("dictionary-local-id"), ref(Dictionary)],
      [ref(Policy), ref(Span), const(source), const(0),
       ref(SourceRevision), const("reference-local-id"), ref(SourceRevision)],
      [ref(Policy), ref(Span), const(start), const(1),
       ref(primitive(int)), const("scalar"), ref(primitive(int))],
      [ref(Policy), ref(Span), const(end), const(2),
       ref(primitive(int)), const("scalar"), ref(primitive(int))],
      [ref(Policy), ref(Node), const(primary), const(0),
       ref(Span), const("reference-local-id"), ref(Span)],
      [ref(Policy), ref(Node), const(secondary), const(1),
       ref(Span), const("reference-local-id"), ref(Span)],
      [ref(Policy), ref(Node), const(kind), const(2),
       ref(primitive(text)), const(KindRepresentation),
       ref(KindDomain)]
    ]) :-
    (   KindRepresentation == "inline"
    ->  KindDomain = primitive(text)
    ;   KindDomain = Dictionary
    ).

expected_transition_fields(
    Policy, Repository, WatchedRef, RefTransition, Dictionary,
    [ [ref(Policy), ref(Repository), const(url), const(0),
       ref(primitive(text)), const("dictionary-local-id"), ref(Dictionary)],
      [ref(Policy), ref(WatchedRef), const(repository), const(0),
       ref(Repository), const("reference-local-id"), ref(Repository)],
      [ref(Policy), ref(WatchedRef), const(name), const(1),
       ref(primitive(text)), const("dictionary-local-id"), ref(Dictionary)],
      [ref(Policy), ref(RefTransition), const(watched_ref), const(0),
       ref(WatchedRef), const("reference-local-id"), ref(WatchedRef)],
      [ref(Policy), ref(RefTransition), const(old_sha), const(1),
       ref(primitive(text)), const("dictionary-local-id"), ref(Dictionary)],
      [ref(Policy), ref(RefTransition), const(new_sha), const(2),
       ref(primitive(text)), const("dictionary-local-id"), ref(Dictionary)]
    ]).

expected_sum_fields(
    Policy, Repository, WatchedRef, SourceCoordinate, SourceRevision,
    Span, SpanChoice, ChoiceRoot, Dictionary, Rows) :-
    expected_node_fields(
        Policy, Repository, WatchedRef, SourceCoordinate, SourceRevision,
        Span, ignored_node, Dictionary, "dictionary-local-id", NodeRows),
    exclude(has_owner(ignored_node), NodeRows, BaseRows),
    append(
        BaseRows,
        [ [ref(Policy), ref(SpanChoice), const(first), const(0),
           ref(Span), const("reference-local-id"), ref(Span)],
          [ref(Policy), ref(SpanChoice), const(second), const(1),
           ref(Span), const("reference-local-id"), ref(Span)],
          [ref(Policy), ref(ChoiceRoot), const(choice), const(0),
           ref(SpanChoice), const("reference-local-id"), ref(SpanChoice)]
        ],
        Rows).

has_owner(Owner, [_, ref(Owner) | _]).

expected_dictionaries(Policies, Dictionary, Rows) :-
    findall(
        [ref(Policy), ref(primitive(text)), ref(Dictionary)],
        member(Policy, Policies),
        Rows).

expected_node_dependencies(
    Policy, Repository, WatchedRef, SourceCoordinate, SourceRevision,
    Span, Node, Dictionary,
    [ [ref(Policy), ref(Repository), ref(Dictionary), const(url), const(0),
       const("dictionary")],
      [ref(Policy), ref(WatchedRef), ref(Repository), const(repository),
       const(0), const("reference")],
      [ref(Policy), ref(WatchedRef), ref(Dictionary), const(name), const(1),
       const("dictionary")],
      [ref(Policy), ref(SourceCoordinate), ref(Repository), const(repository),
       const(0), const("reference")],
      [ref(Policy), ref(SourceCoordinate), ref(WatchedRef), const(watched_ref),
       const(1), const("reference")],
      [ref(Policy), ref(SourceCoordinate), ref(Dictionary), const(path),
       const(2), const("dictionary")],
      [ref(Policy), ref(SourceRevision), ref(SourceCoordinate),
       const(coordinate), const(0), const("reference")],
      [ref(Policy), ref(SourceRevision), ref(Dictionary), const(content_hash),
       const(1), const("dictionary")],
      [ref(Policy), ref(Span), ref(SourceRevision), const(source), const(0),
       const("reference")],
      [ref(Policy), ref(Node), ref(Span), const(primary), const(0),
       const("reference")],
      [ref(Policy), ref(Node), ref(Span), const(secondary), const(1),
       const("reference")],
      [ref(Policy), ref(Node), ref(Dictionary), const(kind), const(2),
       const("dictionary")]
    ]).

field_projection(
    [Policy, Owner, Label, Position, LogicalTarget,
     Representation, StorageDomain],
    [Policy, Owner, Position, Label, StorageDomain,
     LogicalTarget, Representation]).

expected_node_identities(
    Policy, Repository, WatchedRef, SourceCoordinate, SourceRevision,
    Span, Node, Dictionary, Rows) :-
    findall(
        [ ref(Policy), ref(Type), const("structural"),
          const("catalog-local-intern-id"),
          const("constructor-and-ordered-fields")
        ],
        member(Type, [Repository, WatchedRef, SourceCoordinate,
                      SourceRevision, Span, Node]),
        StructuralRows),
    append(
        StructuralRows,
        [[ ref(Policy), ref(Dictionary), const("dictionary"),
           const("catalog-local-dictionary-id"), const("scalar-content")
        ]],
        Rows).

:- end_tests(dl7_interned_storage).
