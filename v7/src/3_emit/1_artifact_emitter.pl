:- module(dl7_artifact_emitter,
          [ emit_compiled/4,
            compiler_view/2
          ]).

:- use_module('0_logical_program_reifier',
              [ logical_program_rows/2,
                logical_program_rows_calls/5
              ]).
:- use_module('../1_libtime/0_evaluator',
              [ evaluate/4,
                validate_functional_rows/3
              ]).

%% compiler_view(+CompiledUnit, -View) is det.
%
% Expose one immutable input shape to host-Prolog emitters. The type graph and
% compiler rows carry semantic facts; the checked Datalog program carries the
% statically resolved executable relation graph.
compiler_view(
    compiled_unit(TypeGraphFacts, RuntimeProgram, CompilerFacts),
    compiler_view(TypeGraphFacts, CompilerFacts,
                  LogicalProgramRows, RuntimeProgram)) :-
    logical_program_rows(RuntimeProgram, LogicalProgramRows).

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
emit_compiled(
    relational_program,
    CompiledUnit,
    artifact(relational_program, LogicalProgramRows),
    []) :-
    !,
    compiler_view(
        CompiledUnit,
        compiler_view(_, _, LogicalProgramRows, _)).
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
    CompiledUnit,
    Artifact,
    Diagnostics) :-
    !,
    compiler_view(CompiledUnit, View),
    dl7_emitter_artifact(Emitter, View, Artifact, Diagnostics).
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

dl7_emitter_artifact(
    Emitter, View,
    Artifact, Diagnostics) :-
    View = compiler_view(_, CompilerFacts, _, _),
    named_relation_id(CompilerFacts, emits, EmitsResult),
    continue_dl7_emitter_protocol(
        EmitsResult, Emitter, CompilerFacts, View, Artifact, Diagnostics).

continue_dl7_emitter_protocol(
    ok(Emits), Emitter, CompilerFacts, View,
    Artifact, Diagnostics) :-
    !,
    findall(
        artifact_ref(Name, Output),
        member(call(ref(Emits),
                    [ref(Emitter), const(Name), ref(Output)]),
               CompilerFacts),
        ArtifactRefs0),
    sort(ArtifactRefs0, ArtifactRefs),
    artifact_ref_diagnostics(Emitter, ArtifactRefs, RefDiagnostics),
    continue_dl7_emitter_refs(
        RefDiagnostics, ArtifactRefs, View, Artifact, Diagnostics).
continue_dl7_emitter_protocol(
    error(Reason), _, _, _, [],
    [diagnostic(emit, none, Reason)]).

continue_dl7_emitter_refs(
    [], ArtifactRefs, View, artifacts(Artifacts), Diagnostics) :-
    !,
    findall(Output,
            member(artifact_ref(_, Output), ArtifactRefs),
            OutputRelations0),
    sort(OutputRelations0, OutputRelations),
    dl7_emitter_rows(OutputRelations, View, Rows, ViewDiagnostics),
    materialize_artifact_refs(
        ViewDiagnostics, ArtifactRefs, Rows, Artifacts, Diagnostics).
continue_dl7_emitter_refs(Reasons, _, _, [], Diagnostics) :-
    maplist(emitter_diagnostic, Reasons, Diagnostics).

dl7_emitter_rows(
    OutputRelations,
    compiler_view(_, CompilerFacts, LogicalProgramRows, RuntimeProgram),
    Rows, Diagnostics) :-
    RuntimeProgram = checked_datalog(
                         _, datalog_program(_, _, Rules), _, _),
    rules_for_outputs(
        Rules, OutputRelations, DependencyRelations, EmitterRules),
    logical_program_rows_calls(
        CompilerFacts, LogicalProgramRows, DependencyRelations,
        LogicalCalls, ProtocolDiagnostics),
    evaluate_dl7_emitter_rows(
        ProtocolDiagnostics, CompilerFacts, LogicalCalls, RuntimeProgram,
        DependencyRelations, EmitterRules, Rows, Diagnostics).

evaluate_dl7_emitter_rows(
    [], CompilerFacts, LogicalCalls,
    checked_datalog(_, datalog_program(Relations, RuntimeSeeds, _), _, _),
    DependencyRelations, EmitterRules, Rows, Diagnostics) :-
    !,
    include(call_has_relation(DependencyRelations),
            CompilerFacts, RelevantCompilerFacts),
    include(call_has_relation(DependencyRelations),
            RuntimeSeeds, RelevantRuntimeSeeds),
    include(call_has_relation(DependencyRelations),
            LogicalCalls, RelevantLogicalCalls),
    include(declaration_has_relation(DependencyRelations),
            Relations, RelevantRelations),
    append([ RelevantCompilerFacts, RelevantRuntimeSeeds,
             RelevantLogicalCalls
           ], Seeds0),
    sort(Seeds0, Seeds),
    evaluate(EmitterRules, Seeds, Closure, EvaluationDiagnostics),
    validate_dl7_emitter_rows(
        EvaluationDiagnostics, RelevantRelations, Closure, Rows, Diagnostics).
evaluate_dl7_emitter_rows(Diagnostics, _, _, _, _, _, [], Diagnostics).

rules_for_outputs(Rules, OutputRelations, Dependencies, EmitterRules) :-
    output_dependency_closure(Rules, OutputRelations, Dependencies),
    include(rule_heads_relation(Dependencies), Rules, EmitterRules).

call_has_relation(Relations, call(ref(Relation), _)) :-
    memberchk(Relation, Relations).

declaration_has_relation(Relations, relation(ref(Relation), _, _)) :-
    memberchk(Relation, Relations).

output_dependency_closure(Rules, Relations, Closure) :-
    findall(
        BodyRelation,
        ( member(rule(call(ref(HeadRelation), _), Goals), Rules),
          memberchk(HeadRelation, Relations),
          member(checked_goal(_, call(ref(BodyRelation), _)), Goals)
        ),
        BodyRelations),
    append(Relations, BodyRelations, Next0),
    sort(Next0, Next),
    (   Next == Relations
    ->  Closure = Next
    ;   output_dependency_closure(Rules, Next, Closure)
    ).

rule_heads_relation(Relations, rule(call(ref(Relation), _), _)) :-
    memberchk(Relation, Relations).

validate_dl7_emitter_rows([], Relations, Closure, Rows, Diagnostics) :-
    !,
    validate_functional_rows(Relations, Closure, Diagnostics),
    (   Diagnostics == []
    ->  Rows = Closure
    ;   Rows = []
    ).
validate_dl7_emitter_rows(Diagnostics, _, _, [], Diagnostics).

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
materialize_artifact_refs(Diagnostics, _, _, [], Diagnostics).

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
