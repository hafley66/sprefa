:- module(dl7_artifact_emitter,
          [ emit_compiled/4,
            compiler_view/2
          ]).

%% compiler_view(+CompiledUnit, -View) is det.
%
% Expose one immutable input shape to host-Prolog emitters. The type graph and
% compiler rows carry semantic facts; the checked Datalog program carries the
% statically resolved executable relation graph.
compiler_view(
    compiled_unit(TypeGraphFacts, RuntimeProgram, CompilerFacts),
    compiler_view(TypeGraphFacts, CompilerFacts, RuntimeProgram)).

%% emit_compiled(+Emitter, +CompiledUnit, -Artifact, -Diagnostics) is det.
%
% The built-in Datalog arm returns the checked monomorphic program. A Prolog
% emitter is a callable with signature call(+CompilerView, -Artifact). A DL7
% emitter is a type identity connected to one or more output relations by
% emits(Emitter, ArtifactName, OutputRelation) rows.
emit_compiled(
    monomorphic_datalog,
    compiled_unit(_, RuntimeProgram, _),
    artifact(monomorphic_datalog, RuntimeProgram),
    []) :-
    !.
emit_compiled(prolog(Callable), CompiledUnit, Artifact, Diagnostics) :-
    !,
    compiler_view(CompiledUnit, View),
    catch(
        call_prolog_emitter(Callable, View, Artifact, Diagnostics),
        Error,
        ( Artifact = [],
          Diagnostics = [diagnostic(
                             emit, none,
                             prolog_emitter_exception(Callable, Error))]
        )).
emit_compiled(
    dl7(Emitter),
    compiled_unit(_, _, CompilerFacts),
    Artifact,
    Diagnostics) :-
    !,
    dl7_emitter_artifact(
        Emitter, CompilerFacts, Artifact, Diagnostics).
emit_compiled(Emitter, _, [],
              [diagnostic(emit, none, unknown_emitter(Emitter))]).

call_prolog_emitter(Callable, View, Artifact, Diagnostics) :-
    (   once(call(Callable, View, Artifact))
    ->  Diagnostics = []
    ;   Artifact = [],
        Diagnostics = [diagnostic(
                           emit, none,
                           prolog_emitter_failed(Callable))]
    ).

dl7_emitter_artifact(Emitter, CompilerFacts, Artifact, Diagnostics) :-
    named_relation_id(CompilerFacts, emits, EmitsResult),
    continue_dl7_emitter_artifact(
        EmitsResult, Emitter, CompilerFacts, Artifact, Diagnostics).

continue_dl7_emitter_artifact(
    ok(Emits), Emitter, CompilerFacts,
    artifacts(Artifacts), Diagnostics) :-
    !,
    findall(
        artifact_ref(Name, Output),
        member(call(ref(Emits),
                    [ref(Emitter), const(Name), ref(Output)]),
               CompilerFacts),
        ArtifactRefs0),
    sort(ArtifactRefs0, ArtifactRefs),
    artifact_ref_diagnostics(Emitter, ArtifactRefs, RefDiagnostics),
    materialize_artifact_refs(
        RefDiagnostics, ArtifactRefs, CompilerFacts,
        Artifacts, Diagnostics).
continue_dl7_emitter_artifact(
    error(Reason), _, _, [],
    [diagnostic(emit, none, Reason)]).

named_relation_id(CompilerFacts, Name, Result) :-
    findall(
        Relation,
        member(call(ref(kernel(':')),
                    [ref(_), const(Name), ref(Relation), const(_)]),
               CompilerFacts),
        Relations0),
    sort(Relations0, Relations),
    named_relation_result(Name, Relations, Result).

named_relation_result(_, [Relation], ok(Relation)) :- !.
named_relation_result(Name, [], error(emitter_protocol_missing(Name))) :- !.
named_relation_result(Name, Relations,
                      error(emitter_protocol_ambiguous(Name, Relations))).

artifact_ref_diagnostics(Emitter, [],
                         [emitter_has_no_outputs(Emitter)]) :-
    !.
artifact_ref_diagnostics(_, ArtifactRefs, Diagnostics) :-
    findall(Name,
            conflicting_artifact_name(ArtifactRefs, Name),
            Names0),
    sort(Names0, Names),
    artifact_name_diagnostics(Names, Diagnostics).

conflicting_artifact_name(ArtifactRefs, Name) :-
    member(artifact_ref(Name, First), ArtifactRefs),
    member(artifact_ref(Name, Second), ArtifactRefs),
    First \== Second.

artifact_name_diagnostics([], []).
artifact_name_diagnostics([Name | Names],
                          [duplicate_artifact_name(Name) | Diagnostics]) :-
    artifact_name_diagnostics(Names, Diagnostics).

materialize_artifact_refs([], ArtifactRefs, CompilerFacts,
                          Artifacts, []) :-
    !,
    maplist(materialize_artifact_ref(CompilerFacts),
            ArtifactRefs, Artifacts).
materialize_artifact_refs(Reasons, _, _, [], Diagnostics) :-
    maplist(emitter_diagnostic, Reasons, Diagnostics).

emitter_diagnostic(Reason, diagnostic(emit, none, Reason)).

materialize_artifact_ref(
    CompilerFacts,
    artifact_ref(Name, Output),
    artifact(Name, Output, Rows)) :-
    findall(
        Arguments,
        member(call(ref(Output), Arguments), CompilerFacts),
        Rows0),
    sort(Rows0, Rows).
