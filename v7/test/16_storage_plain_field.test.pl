% SABOTAGE RECEIPT: dropping the (not (storage_plain_field ...)) guard from the
% scalar arm of emitters/2_interned_storage.dl7 gives 14 field rows, not 12;
% Blob.size then carries both "inline" and "scalar" under PlainIntPolicy and
% NoDictionaryPolicy.
:- begin_tests(dl7_storage_plain_field).

:- use_module('../src/2_comptime/2_compiler', [compile_dl7_project/5]).
:- use_module('../src/3_emit/1_artifact_emitter', [emit_compiled/4]).

:- dynamic test_directory/1.
:- prolog_load_context(directory, TestDirectory),
   assertz(test_directory(TestDirectory)).

test(plain_selected_field_claims_exactly_one_row_without_a_dictionary) :-
    garbage_collect,
    storage_layout('fixtures/17_storage_plain_field.dl7', Rows, Names),
    named(Names, 'Blob', Blob),
    named(Names, 'Repository', Repository),
    named(Names, 'SharedTextDictionary', Dictionary),
    named(Names, 'PlainIntPolicy', PlainInt),
    named(Names, 'PlainTextPolicy', PlainText),
    named(Names, 'NoDictionaryPolicy', NoDictionary),
    policy_rows(Rows, PlainInt, PlainIntRows),
    exact_rows(
        PlainIntRows,
        [ [ref(PlainInt), ref(Repository), const(url), const(0),
           ref(primitive(text)), const("dictionary-local-id"),
           ref(Dictionary)],
          [ref(PlainInt), ref(Blob), const(repository), const(0),
           ref(Repository), const("reference-local-id"), ref(Repository)],
          [ref(PlainInt), ref(Blob), const(ordinary), const(1),
           ref(primitive(text)), const("dictionary-local-id"),
           ref(Dictionary)],
          [ref(PlainInt), ref(Blob), const(size), const(2),
           ref(primitive(int)), const("inline"), ref(primitive(int))]
        ]),
    policy_rows(Rows, NoDictionary, NoDictionaryRows),
    exact_rows(
        NoDictionaryRows,
        [ [ref(NoDictionary), ref(Repository), const(url), const(0),
           ref(primitive(text)), const("scalar"), ref(primitive(text))],
          [ref(NoDictionary), ref(Blob), const(repository), const(0),
           ref(Repository), const("reference-local-id"), ref(Repository)],
          [ref(NoDictionary), ref(Blob), const(ordinary), const(1),
           ref(primitive(text)), const("scalar"), ref(primitive(text))],
          [ref(NoDictionary), ref(Blob), const(size), const(2),
           ref(primitive(int)), const("inline"), ref(primitive(int))]
        ]),
    policy_rows(Rows, PlainText, PlainTextRows),
    exact_rows(
        PlainTextRows,
        [ [ref(PlainText), ref(Repository), const(url), const(0),
           ref(primitive(text)), const("dictionary-local-id"),
           ref(Dictionary)],
          [ref(PlainText), ref(Blob), const(repository), const(0),
           ref(Repository), const("reference-local-id"), ref(Repository)],
          [ref(PlainText), ref(Blob), const(ordinary), const(1),
           ref(primitive(text)), const("inline"), ref(primitive(text))],
          [ref(PlainText), ref(Blob), const(size), const(2),
           ref(primitive(int)), const("scalar"), ref(primitive(int))]
        ]),
    findall(Policy-Owner-Label,
            member([ref(Policy), ref(Owner), const(Label) | _], Rows),
            Fields),
    sort(Fields, DistinctFields),
    length(Rows, 12),
    length(DistinctFields, 12),
    !.

storage_layout(Relative, Rows, Names) :-
    fixture(Relative, FixturePath),
    fixture('../emitters/2_interned_storage.dl7', EmitterPath),
    file_directory_name(EmitterPath, EmittersDirectory),
    file_directory_name(EmittersDirectory, V7Directory),
    compile_dl7_project(
        V7Directory, [EmitterPath, FixturePath],
        CompilerRows, RuntimeProgram, CompileDiagnostics),
    CompileDiagnostics == [],
    named_owner(CompilerRows, 'InternedStorageEmitter', Emitter),
    compiler_closed_runtime(RuntimeProgram, ClosedRuntime),
    emit_compiled(
        dl7(Emitter),
        compiled_unit([], ClosedRuntime, CompilerRows),
        artifacts(Artifacts), EmitDiagnostics),
    EmitDiagnostics == [],
    memberchk(artifact("fields", _, Rows), Artifacts),
    findall(Name-Identity,
            ( member(Name, ['Blob', 'Repository', 'SharedTextDictionary',
                            'PlainIntPolicy', 'PlainTextPolicy',
                            'NoDictionaryPolicy']),
              named_owner(CompilerRows, Name, Identity)
            ),
            Names).

fixture(Relative, Path) :-
    test_directory(TestDirectory),
    directory_file_path(TestDirectory, Relative, Unresolved),
    absolute_file_name(Unresolved, Path, [access(read)]).

% The sliced runtime keeps the artifact adapter from reifying unrelated
% executable rules; this layout query has no program_relation dependencies.
compiler_closed_runtime(
    checked_datalog(Graph, datalog_program(Relations, _, _), _, _),
    checked_datalog(Graph, datalog_program(Relations, [], []), [], [])).

named_owner(Rows, Name, Owner) :-
    member(call(ref(kernel(':')),
                [ref(_), const(Name), ref(Owner), const(_)]),
           Rows),
    !.

named(Names, Name, Identity) :-
    memberchk(Name-Identity, Names).

policy_rows(Rows, Policy, Selected) :-
    include(has_policy(Policy), Rows, Selected0),
    sort(Selected0, Selected).

has_policy(Policy, [ref(Policy) | _]).

exact_rows(Observed, Expected) :-
    sort(Observed, Sorted),
    sort(Expected, Sorted).

:- end_tests(dl7_storage_plain_field).
