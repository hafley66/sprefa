% Print the DL6 catalog storage layout, one readable row per field per policy.
% Run through `just dl6-demo`, which supplies the v7 directory.

:- initialization(main, main).

main([V7Directory]) :-
    directory_file_path(
        V7Directory, 'emitters/2_interned_storage.dl7', EmitterPath),
    directory_file_path(
        V7Directory, 'applications/dl6/0_catalog.dl7', CatalogPath),
    compiler(V7Directory),
    compile_dl7_project(
        V7Directory, [EmitterPath, CatalogPath],
        Rows, RuntimeProgram, Diagnostics),
    report_diagnostics(compile, Diagnostics),
    named(Rows, 'InternedStorageEmitter', StorageEmitter),
    named(Rows, 'Dl6CatalogEmitter', CatalogEmitter),
    labels(Rows, Labels),
    emit_artifacts(StorageEmitter, RuntimeProgram, Rows, StorageArtifacts),
    emit_artifacts(CatalogEmitter, RuntimeProgram, Rows, CompanionArtifacts),
    memberchk(artifact("fields", _, FieldRows), StorageArtifacts),
    memberchk(artifact("interned", _, CompanionRows), CompanionArtifacts),
    format("~w~t~24|~w~t~48|~w~t~70|~w~n",
           ['policy', 'owner.field', 'representation', 'storage domain']),
    forall(member(Row, FieldRows), print_field(Labels, Row)),
    nl,
    format("wrapper joins~n", []),
    forall(member([Source, Result], CompanionRows),
           ( label(Labels, Source, SourceLabel),
             label(Labels, Result, ResultLabel),
             format("  ~w~t~24|underlying type ~w~n",
                    [ResultLabel, SourceLabel]) )),
    halt(0).

compiler(V7Directory) :-
    directory_file_path(
        V7Directory, 'src/2_comptime/2_compiler.pl', Compiler),
    directory_file_path(
        V7Directory, 'src/3_emit/1_artifact_emitter.pl', Emitter),
    use_module(Compiler, [compile_dl7_project/5]),
    use_module(Emitter, [emit_compiled/4]).

emit_artifacts(Emitter, RuntimeProgram, Rows, Artifacts) :-
    emit_compiled(
        dl7(Emitter), compiled_unit([], RuntimeProgram, Rows),
        Result, Diagnostics),
    report_diagnostics(emit, Diagnostics),
    (   Result = artifacts(Artifacts)
    ->  true
    ;   format(user_error, "emit produced ~q~n", [Result]),
        halt(1)
    ).

report_diagnostics(_, []) :- !.
report_diagnostics(Phase, Diagnostics) :-
    forall(member(Diagnostic, Diagnostics),
           format(user_error, "~w diagnostic ~q~n", [Phase, Diagnostic])),
    halt(1).

print_field(Labels, [Policy, Owner, const(Label), _, _,
                     const(Representation), Domain]) :-
    label(Labels, Policy, PolicyLabel),
    label(Labels, Owner, OwnerLabel),
    label(Labels, Domain, DomainLabel),
    format(atom(Field), '~w.~w', [OwnerLabel, Label]),
    format("~w~t~24|~w~t~48|~w~t~70|~w~n",
           [PolicyLabel, Field, Representation, DomainLabel]).

labels(Rows, Labels) :-
    findall(Identity-Name,
            ( member(Name, ['Repository', 'SourceFile', 'Symbol',
                            'SharedTextDictionary', 'SelectivePolicy',
                            'SharedDictionaryPolicy']),
              named(Rows, Name, Identity)
            ),
            Named),
    (   named(Rows, 'Interned', Interned)
    ->  findall(application(Interned, [Argument])-Wrapper,
                ( member(call(ref(Interned),
                              [ref(Source), ref(application(Interned,
                                                            [Argument]))]),
                         Rows),
                  wrapper_label(Source, Wrapper)
                ),
                Wrappers)
    ;   Wrappers = []
    ),
    append(Named, Wrappers, Labels).

wrapper_label(primitive(Primitive), Label) :-
    !,
    format(atom(Label), 'Interned(~w)', [Primitive]).
wrapper_label(Other, Other).

label(Labels, ref(Identity), Label) :-
    !,
    label_term(Labels, Identity, Label).
label(Labels, Identity, Label) :-
    label_term(Labels, Identity, Label).

label_term(Labels, Identity, Label) :-
    memberchk(Identity-Label, Labels),
    !.
label_term(_, primitive(Primitive), Primitive) :- !.
label_term(_, Other, Other).

named(Rows, Name, Identity) :-
    member(call(ref(kernel(':')),
                [ref(_), const(Name), ref(Identity), const(_)]),
           Rows),
    !.
