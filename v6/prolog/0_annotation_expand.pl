% 0_annotation_expand.pl : structural elaboration for applicative type syntax.
%
% This phase only makes the implicit Target sequence explicit.  Relation
% lookup, compiler-plane execution, cardinality checks, and key evidence
% consumption belong to later phases.

:- module(annotation_expand,
          [ elaborate_annotation/3,
            elaborate_annotation_applications/3
          ]).

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
