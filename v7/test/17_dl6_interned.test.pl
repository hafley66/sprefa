% SABOTAGE RECEIPTS: dropping the intern_snapshot replay rule from the catalog
% leaves the storage layout correct but empties the "interned" companion (0
% rows, not 1). Pointing WrappedTextSelection at text instead of the wrapper
% collapses the selective policy onto the all-text one.
:- begin_tests(dl7_dl6_interned).

:- use_module('../src/2_comptime/2_compiler', [compile_dl7_project/5]).
:- use_module('../src/3_emit/1_artifact_emitter', [emit_compiled/4]).

:- dynamic test_directory/1.
:- prolog_load_context(directory, TestDirectory),
   assertz(test_directory(TestDirectory)).

test(dl6_catalog_derives_one_selective_layout_and_one_wrapper_identity) :-
    garbage_collect,
    catalog(V7Directory, EmitterPath, CatalogPath),
    compile_dl7_project(
        V7Directory, [EmitterPath, CatalogPath],
        Rows, RuntimeProgram, CompileDiagnostics),
    CompileDiagnostics == [],
    named(Rows, 'InternedStorageEmitter', StorageEmitter),
    named(Rows, 'Dl6CatalogEmitter', CatalogEmitter),
    named(Rows, 'Interned', Interned),
    named(Rows, 'Repository', Repository),
    named(Rows, 'SourceFile', SourceFile),
    named(Rows, 'Symbol', Symbol),
    named(Rows, 'SharedTextDictionary', Dictionary),
    named(Rows, 'SelectivePolicy', Selective),
    named(Rows, 'SharedDictionaryPolicy', Shared),
    Wrapper = application(Interned, [primitive(text)]),

    % ONE canonical specialization, reached from two different owners.
    type_edge(Rows, SourceFile, path, 1, ref(Wrapper)),
    type_edge(Rows, Symbol, name, 1, ref(Wrapper)),
    findall(Result,
            member(call(ref(Interned), [_, ref(Result)]), Rows),
            Specializations0),
    sort(Specializations0, Specializations),
    Specializations == [Wrapper],

    closed_runtime(RuntimeProgram, ClosedRuntime),
    emit_compiled(
        dl7(StorageEmitter),
        compiled_unit([], ClosedRuntime, Rows),
        artifacts(ClosedArtifacts), ClosedDiagnostics),
    ClosedDiagnostics == [],

    % The full runtime and the compiler-closed slice hand over the same rows.
    emit_compiled(
        dl7(StorageEmitter),
        compiled_unit([], RuntimeProgram, Rows),
        artifacts(FullArtifacts), FullDiagnostics),
    FullDiagnostics == [],
    same_artifacts(ClosedArtifacts, FullArtifacts),

    artifact(ClosedArtifacts, "types", TypeRows),
    artifact(ClosedArtifacts, "fields", FieldRows),
    artifact(ClosedArtifacts, "dependencies", DependencyRows),
    artifact(ClosedArtifacts, "projections", ProjectionRows),
    artifact(ClosedArtifacts, "dictionaries", DictionaryRows),
    artifact(ClosedArtifacts, "identities", IdentityRows),

    expected_types(Selective, Shared, Repository, SourceFile, Symbol,
                   ExpectedTypes),
    exact_rows(TypeRows, ExpectedTypes),

    expected_fields(
        Selective, Shared, Repository, SourceFile, Symbol, Dictionary,
        Wrapper, ExpectedFields),
    exact_rows(FieldRows, ExpectedFields),

    % The plain neighbour stays scalar under the selective policy while the
    % wrapped columns reach the dictionary.
    memberchk([ref(Selective), ref(SourceFile), const(language), const(2),
               ref(primitive(text)), const("scalar"), ref(primitive(text))],
              FieldRows),
    memberchk([ref(Selective), ref(Repository), const(url), const(0),
               ref(primitive(text)), const("scalar"), ref(primitive(text))],
              FieldRows),
    memberchk([ref(Selective), ref(SourceFile), const(path), const(1),
               ref(Wrapper), const("dictionary-local-id"), ref(Dictionary)],
              FieldRows),
    memberchk([ref(Selective), ref(Symbol), const(name), const(1),
               ref(Wrapper), const("dictionary-local-id"), ref(Dictionary)],
              FieldRows),

    % The reference field keeps its dependency and its projection.
    memberchk([ref(Selective), ref(Symbol), ref(SourceFile), const(file),
               const(0), const("reference")],
              DependencyRows),
    memberchk([ref(Selective), ref(Symbol), const(0), const(file),
               ref(SourceFile), ref(SourceFile), const("reference-local-id")],
              ProjectionRows),

    expected_dependencies(
        Selective, Shared, Repository, SourceFile, Symbol, Dictionary,
        ExpectedDependencies),
    exact_rows(DependencyRows, ExpectedDependencies),

    maplist(field_projection, ExpectedFields, ExpectedProjections),
    exact_rows(ProjectionRows, ExpectedProjections),

    exact_rows(
        DictionaryRows,
        [ [ref(Selective), ref(Wrapper), ref(Dictionary)],
          [ref(Shared), ref(Wrapper), ref(Dictionary)],
          [ref(Shared), ref(primitive(text)), ref(Dictionary)]
        ]),

    % Two selections name one authored dictionary, so the shared policy still
    % carries a single dictionary identity row.
    expected_identities(
        Selective, Shared, Repository, SourceFile, Symbol, Dictionary,
        ExpectedIdentities),
    exact_rows(IdentityRows, ExpectedIdentities),
    include(dictionary_identity(Shared), IdentityRows, SharedDictionaryRows),
    length(SharedDictionaryRows, 1),

    % Exactly one storage row per field, in every policy.
    findall(Policy-Owner-Label,
            member([ref(Policy), ref(Owner), const(Label) | _], FieldRows),
            Claims),
    sort(Claims, DistinctClaims),
    length(FieldRows, 16),
    length(DistinctClaims, 16),
    length(TypeRows, 6),
    length(DependencyRows, 10),
    length(ProjectionRows, 16),
    length(DictionaryRows, 3),
    length(IdentityRows, 8),

    % The companion artifact joins a wrapper back to its underlying type.
    emit_compiled(
        dl7(CatalogEmitter),
        compiled_unit([], RuntimeProgram, Rows),
        artifacts(CompanionArtifacts), CompanionDiagnostics),
    CompanionDiagnostics == [],
    artifact(CompanionArtifacts, "interned", CompanionRows),
    CompanionRows == [[ref(primitive(text)), ref(Wrapper)]],
    !.

catalog(V7Directory, EmitterPath, CatalogPath) :-
    resolve('../emitters/2_interned_storage.dl7', EmitterPath),
    resolve('../applications/dl6/0_catalog.dl7', CatalogPath),
    file_directory_name(EmitterPath, EmittersDirectory),
    file_directory_name(EmittersDirectory, V7Directory).

resolve(Relative, Path) :-
    test_directory(TestDirectory),
    directory_file_path(TestDirectory, Relative, Unresolved),
    absolute_file_name(Unresolved, Path, [access(read)]).

% The sliced runtime keeps the artifact adapter from reifying unrelated
% executable rules; this layout query has no program_relation dependencies.
closed_runtime(
    checked_datalog(Graph, datalog_program(Relations, _, _), _, _),
    checked_datalog(Graph, datalog_program(Relations, [], []), [], [])).

same_artifacts(Closed, Full) :-
    msort(Closed, Sorted),
    msort(Full, Sorted).

artifact(Artifacts, Name, Rows) :-
    memberchk(artifact(Name, _, Rows), Artifacts).

named(Rows, Name, Identity) :-
    member(call(ref(kernel(':')),
                [ref(_), const(Name), ref(Identity), const(_)]),
           Rows),
    !.

type_edge(Rows, Owner, Label, Index, Target) :-
    memberchk(call(ref(kernel(':')),
                   [ref(Owner), const(Label), Target, const(Index)]),
              Rows).

exact_rows(Observed, Expected) :-
    sort(Observed, Sorted),
    sort(Expected, Sorted).

dictionary_identity(Policy, [ref(Policy), _, const("dictionary") | _]).

field_projection(
    [Policy, Owner, Label, Position, LogicalTarget,
     Representation, StorageDomain],
    [Policy, Owner, Position, Label, StorageDomain,
     LogicalTarget, Representation]).

expected_types(Selective, Shared, Repository, SourceFile, Symbol, Rows) :-
    findall(
        [ref(Policy), ref(Type), const("product")],
        ( member(Policy, [Selective, Shared]),
          member(Type, [Repository, SourceFile, Symbol])
        ),
        Rows).

expected_identities(
    Selective, Shared, Repository, SourceFile, Symbol, Dictionary, Rows) :-
    findall(
        Row,
        ( member(Policy, [Selective, Shared]),
          policy_identity(Policy, Repository, SourceFile, Symbol,
                          Dictionary, Row)
        ),
        Rows).

policy_identity(Policy, Repository, SourceFile, Symbol, _,
                [ ref(Policy), ref(Type), const("structural"),
                  const("catalog-local-intern-id"),
                  const("constructor-and-ordered-fields")
                ]) :-
    member(Type, [Repository, SourceFile, Symbol]).
policy_identity(Policy, _, _, _, Dictionary,
                [ ref(Policy), ref(Dictionary), const("dictionary"),
                  const("catalog-local-dictionary-id"),
                  const("scalar-content")
                ]).

% language and url follow the policy: scalar when only the wrapper is mapped,
% dictionary-backed when the primitive selection is present as well.
expected_fields(
    Selective, Shared, Repository, SourceFile, Symbol, Dictionary, Wrapper,
    Rows) :-
    policy_fields(
        Selective, Repository, SourceFile, Symbol, Dictionary, Wrapper,
        "scalar", ref(primitive(text)), SelectiveRows),
    policy_fields(
        Shared, Repository, SourceFile, Symbol, Dictionary, Wrapper,
        "dictionary-local-id", ref(Dictionary), SharedRows),
    append(SelectiveRows, SharedRows, Rows).

policy_fields(
    Policy, Repository, SourceFile, Symbol, Dictionary, Wrapper,
    TextRepresentation, TextDomain,
    [ [ref(Policy), ref(Repository), const(url), const(0),
       ref(primitive(text)), const(TextRepresentation), TextDomain],
      [ref(Policy), ref(SourceFile), const(repository), const(0),
       ref(Repository), const("reference-local-id"), ref(Repository)],
      [ref(Policy), ref(SourceFile), const(path), const(1),
       ref(Wrapper), const("dictionary-local-id"), ref(Dictionary)],
      [ref(Policy), ref(SourceFile), const(language), const(2),
       ref(primitive(text)), const(TextRepresentation), TextDomain],
      [ref(Policy), ref(SourceFile), const(size), const(3),
       ref(primitive(int)), const("scalar"), ref(primitive(int))],
      [ref(Policy), ref(Symbol), const(file), const(0),
       ref(SourceFile), const("reference-local-id"), ref(SourceFile)],
      [ref(Policy), ref(Symbol), const(name), const(1),
       ref(Wrapper), const("dictionary-local-id"), ref(Dictionary)],
      [ref(Policy), ref(Symbol), const(line), const(2),
       ref(primitive(int)), const("scalar"), ref(primitive(int))]
    ]).

expected_dependencies(
    Selective, Shared, Repository, SourceFile, Symbol, Dictionary, Rows) :-
    shared_dependencies(Selective, Repository, SourceFile, Symbol,
                        Dictionary, SelectiveRows),
    shared_dependencies(Shared, Repository, SourceFile, Symbol,
                        Dictionary, SharedBase),
    append(
        SharedBase,
        [ [ref(Shared), ref(Repository), ref(Dictionary), const(url),
           const(0), const("dictionary")],
          [ref(Shared), ref(SourceFile), ref(Dictionary), const(language),
           const(2), const("dictionary")]
        ],
        SharedRows),
    append(SelectiveRows, SharedRows, Rows).

shared_dependencies(
    Policy, Repository, SourceFile, Symbol, Dictionary,
    [ [ref(Policy), ref(SourceFile), ref(Repository), const(repository),
       const(0), const("reference")],
      [ref(Policy), ref(SourceFile), ref(Dictionary), const(path),
       const(1), const("dictionary")],
      [ref(Policy), ref(Symbol), ref(SourceFile), const(file),
       const(0), const("reference")],
      [ref(Policy), ref(Symbol), ref(Dictionary), const(name),
       const(1), const("dictionary")]
    ]).

:- end_tests(dl7_dl6_interned).
