% 0_annotation_expand.pl : structural elaboration for applicative type syntax.
%
% This phase only makes the implicit Target sequence explicit.  Relation
% lookup, compiler-plane execution, cardinality checks, and key evidence
% consumption belong to later phases.

:- module(annotation_expand,
          [ elaborate_annotation/3,
            elaborate_annotation_applications/3,
            prepare_annotations/2,
            evaluate_annotation_requests/4
          ]).

:- use_module('0_generic_expand', [semantic_type_id/3]).

% elaborate_annotation(+InputType, +Applications, -Elaborated)
%
% The first application receives the parsed input type.  Each later
% application receives the result placeholder produced by its predecessor.
% The placeholders are site-local and ordered, so a later execution phase can
% replace them with concrete type ids without reparsing the surface term.
elaborate_annotation(InputType, Applications, Elaborated) :-
    elaborate_annotation_applications(InputType, Applications, Steps),
    Elaborated = annotation_steps(InputType, Steps).

elaborate_annotation_applications(InputType, Applications, Steps) :-
    elaborate_steps(Applications, InputType, 1, Steps).

elaborate_steps([], _, _, []).
elaborate_steps([Application | Rest], Current, Ordinal,
                [annotation_step(Ordinal, Current, ElaboratedApplication,
                                 annotation_result(Ordinal)) | Steps]) :-
    add_implicit_target(Application, Current, ElaboratedApplication),
    NextOrdinal is Ordinal + 1,
    elaborate_steps(Rest, annotation_result(Ordinal), NextOrdinal, Steps).

add_implicit_target(Application, Target, ElaboratedApplication) :-
    Application =.. [Name | Arguments],
    ElaboratedApplication =.. [Name, named('Target', Target) | Arguments].

% This is the compiler-path boundary for the retained surface carrier.  It is
% after generic substitution and anonymous minting, and before key and option
% consumers.  Requests remain in the declaration stream until compiler-plane
% closure evaluation has produced the rows needed by the applications.
prepare_annotations(Decls0, Decls) :-
    maplist(prepare_annotation_decl(Decls0), Decls0, Prepared),
    append(Prepared, Decls).

prepare_annotation_decl(Decls, col_type(Ref, Column, Type0), Rows) :-
    !,
    rewrite_annotation_type(Decls, Ref-Column, [], Type0, Type, Requests),
    append([[col_type(Ref, Column, Type)], Requests], Rows).
prepare_annotation_decl(Decls, type_decl(Name, Specs0), Rows) :-
    !,
    maplist(prepare_annotation_spec(Decls, Name), Specs0, Specs, RequestLists),
    append(RequestLists, Requests),
    append([[type_decl(Name, Specs)], Requests], Rows).
prepare_annotation_decl(_, Decl, [Decl]).

prepare_annotation_spec(Decls, Owner, col(Column, Type0), col(Column, Type),
                        Requests) :-
    rewrite_annotation_type(Decls, Owner-Column, [], Type0, Type, Requests).

rewrite_annotation_type(Decls, Site, Path, annotated_type(Type0, Apps), Type,
                       [annotation_request(Site, Path, Base, RewrittenApps) | Requests]) :-
    !,
    rewrite_annotation_type(Decls, Site, Path, Type0, Base, Nested),
    maplist(rewrite_annotation_application(Decls, Site, Path), Apps, RewrittenApps),
    annotation_wrapped_type(Base, RewrittenApps, Type),
    append(Nested, Requests).
rewrite_annotation_type(Decls, Site, Path, Type0, Type, Requests) :-
    compound(Type0),
    Type0 =.. [Functor | Args0],
    rewrite_annotation_arguments(Decls, Site, Path, 1, Args0, Args, Requests),
    Type =.. [Functor | Args],
    !.
rewrite_annotation_type(_, _, _, Type, Type, []).

rewrite_annotation_arguments(_, _, _, _, [], [], []).
rewrite_annotation_arguments(Decls, Site, Path, Ordinal, [Arg0 | Rest],
                             [Arg | More], Requests) :-
    append(Path, [Ordinal], ArgPath),
    rewrite_annotation_type(Decls, Site, ArgPath, Arg0, Arg, ArgRequests),
    Next is Ordinal + 1,
    rewrite_annotation_arguments(Decls, Site, Path, Next, Rest, More,
                                 RestRequests),
    append(ArgRequests, RestRequests, Requests).

rewrite_annotation_application(Decls, Site, Path, Application0, Application) :-
    Application0 =.. [Name | Arguments0],
    maplist(rewrite_annotation_argument(Decls, Site, Path), Arguments0,
            Arguments),
    Application =.. [Name | Arguments].

rewrite_annotation_argument(Decls, Site, Path, named(Name, Value0),
                            named(Name, Value)) :-
    rewrite_annotation_type(Decls, Site, Path, Value0, Value, _), !.
rewrite_annotation_argument(Decls, Site, Path, pos(Value0), pos(Value)) :-
    rewrite_annotation_type(Decls, Site, Path, Value0, Value, _), !.

annotation_wrapped_type(Type, Applications, Wrapped) :-
    ( memberchk(key, Applications) -> Wrapped = key(Type) ; Wrapped = Type ).

% Resolve each retained application against the already evaluated compiler
% closure.  The returned rows are compiler metadata, so they never become
% runtime declarations or facts.
evaluate_annotation_requests(Decls, Requests, Closure, Evidence) :-
    findall(Row,
            ( member(annotation_request(Site, Path, InputType, Applications),
                     Requests),
              evaluate_annotation_steps(Decls, Site, Path, InputType,
                                         Applications, Closure, Row) ),
            Unsorted),
    sort(Unsorted, Evidence).

evaluate_annotation_steps(_, _, _, _, [], _, _) :- fail.
evaluate_annotation_steps(Decls, Site, Path, InputType, Applications, Closure,
                          Row) :-
    evaluate_annotation_step_list(Decls, Site, Path, InputType, Applications,
                                  Closure, 1, Row).

evaluate_annotation_step_list(_, _, _, _, [], _, _, _) :- fail.
evaluate_annotation_step_list(Decls, Site, Path, Current, [Application | Rest],
                              Closure, Ordinal, Row) :-
    Application =.. [Name | Arguments],
    semantic_type_id(Decls, Current, CurrentId),
    application_arguments(Arguments, Values),
    findall(OutputId,
            ( member(Fact, Closure), Fact =.. [Name, CurrentId | Tail],
              append(Values, [OutputId], Tail) ),
            Outputs0),
    sort(Outputs0, Outputs),
    ( Outputs = []
    -> throw(unsupported_construct(annotation_application_no_result(Name)))
    ; Outputs = [OutputId]
    -> annotation_type_for_id(Decls, OutputId, OutputType),
       ( Rest == []
       -> Row = type_annotation_evidence(Site, Path, Ordinal, Current,
                                         Application, OutputType)
       ;  Next is Ordinal + 1,
          evaluate_annotation_step_list(Decls, Site, Path, OutputType, Rest,
                                        Closure, Next, Row)
       )
    ; throw(unsupported_construct(annotation_application_multiple_results(Name)))
    ).

application_arguments([], []).
application_arguments([named(_, Value) | Rest], [Value | Values]) :-
    application_arguments(Rest, Values).
application_arguments([pos(Value) | Rest], [Value | Values]) :-
    application_arguments(Rest, Values).

annotation_type_for_id(Decls, Id, Type) :-
    annotation_type_candidate(Decls, Type),
    semantic_type_id(Decls, Type, Id), !.

annotation_type_candidate(_, Type) :- member(Type, [int, text, bytes, bool, float]).
annotation_type_candidate(Decls, Type) :- member(col_type(_, _, Type), Decls).
annotation_type_candidate(Decls, Type) :- member(type_decl(Type, _), Decls).
