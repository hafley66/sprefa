% 0_match_expand.pl: shared match-block and enum-declaration sugar expansion.
%
% The retained program term keeps:
%   match(SourceAtom, ((ArmHead <- Guards) ; (EdgeHead <+ Guards)))
%
% Consumers receive one ordinary rule per arm:
%   ArmHead <- SourceAtom, Guards
%   EdgeHead <+ SourceAtom, Guards

:- module(match_expand, [ expand_match_program/2 ]).

:- use_module(library(lists)).
:- use_module('0_enum_expand', [expand_enum_program/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

expand_match_program(prog(SugaredDecls, SugaredRules), ExpandedProgram) :-
    expand_match_rules(SugaredDecls, SugaredRules, MatchExpandedRules),
    expand_enum_program(prog(SugaredDecls, MatchExpandedRules), ExpandedProgram).

expand_match_rules(_, [], []).
expand_match_rules(Decls, [match(SourceAtom, Arms) | Rest],
                   ExpandedRules) :-
    !,
    validate_match_source(SourceAtom),
    match_arms(Arms, ArmList),
    validate_match_coverage(Decls, ArmList),
    maplist(expand_match_arm(SourceAtom), ArmList, ThisRules),
    expand_match_rules(Decls, Rest, RestRules),
    append(ThisRules, RestRules, ExpandedRules).
expand_match_rules(Decls, [Rule | Rest], [Rule | ExpandedRest]) :-
    expand_match_rules(Decls, Rest, ExpandedRest).

validate_match_source(SourceAtom) :-
    ( positive_rel_atom(SourceAtom)
    -> true
    ; throw(unsupported_construct(match_source_not_positive_rel(SourceAtom)))
    ).

positive_rel_atom(Atom) :-
    nonvar(Atom),
    callable(Atom),
    functor(Atom, Name, Arity),
    atom(Name),
    \+ match_language_form(Name/Arity).

match_language_form(','/2).
match_language_form(';' /2).
match_language_form((<-)/2).
match_language_form((<+)/2).
match_language_form((:=)/2).
match_language_form(is/2).
match_language_form((<)/2).
match_language_form((=<)/2).
match_language_form((>)/2).
match_language_form((>=)/2).
match_language_form((==)/2).
match_language_form((\==)/2).
match_language_form(not/1).
match_language_form(pre/1).
match_language_form(now/1).
match_language_form(latest/1).
match_language_form(finalize/1).
match_language_form(departed/1).
match_language_form(decode/2).
match_language_form(json_each/2).
match_language_form(match/2).
match_language_form(true/0).

match_arms((Left ; Right), Arms) :-
    !,
    match_arms(Left, LeftArms),
    match_arms(Right, RightArms),
    append(LeftArms, RightArms, Arms).
match_arms(Arm, [Arm]).

expand_match_arm(SourceAtom, (ArmHead <- Guards), (ArmHead <- Body)) :-
    !,
    validate_match_arm_head(ArmHead),
    conjoin_match_body(SourceAtom, Guards, Body).
expand_match_arm(SourceAtom, (ArmHead <+ Guards), (ArmHead <+ Body)) :-
    !,
    validate_match_arm_head(ArmHead),
    conjoin_match_body(SourceAtom, Guards, Body).
expand_match_arm(_, Arm, _) :-
    throw(unsupported_construct(match_arm_shape(Arm))).

validate_match_arm_head(ArmHead) :-
    ( positive_rel_atom(ArmHead)
    -> true
    ; throw(unsupported_construct(match_arm_head_not_positive_rel(ArmHead)))
    ).

conjoin_match_body(SourceAtom, true, SourceAtom) :- !.
conjoin_match_body(SourceAtom, Guards, (SourceAtom, Guards)).

validate_match_coverage(Decls, Arms) :-
    findall(Enum-Variants, enum_variant_refs(Decls, Enum, Variants),
            EnumVariants),
    findall(HeadRef, (member(Arm, Arms), arm_head_ref(Arm, HeadRef)), HeadRefs),
    ( member(Enum-VariantRefs, EnumVariants),
      HeadRefs \== [],
      forall(member(HeadRef, HeadRefs), memberchk(HeadRef-_, VariantRefs))
    -> forall(member(MissingRef-MissingVariant, VariantRefs),
              ( memberchk(MissingRef, HeadRefs)
              -> true
              ; throw(unsupported_construct(
                    match_nonexhaustive(Enum, MissingVariant)))
              ))
    ; true
    ).

arm_head_ref((Head <- _), Ref) :- !, rel_ref_local(Head, Ref).
arm_head_ref((Head <+ _), Ref) :- !, rel_ref_local(Head, Ref).
arm_head_ref(Arm, _) :-
    throw(unsupported_construct(match_arm_shape(Arm))).

rel_ref_local(Atom, Name/Arity) :-
    positive_rel_atom(Atom),
    functor(Atom, Name, Arity).

enum_variant_refs(Decls, Enum, VariantRefs) :-
    member(enum_decl(Enum, VariantTerms), Decls),
    findall(VariantRef-VariantName,
            ( enum_variant(VariantTerms, VariantTerm),
              variant_name_arity(VariantTerm, VariantName, ContentArity),
              atomic_list_concat([Enum, VariantName], '_', VariantRelName),
              VariantArity is ContentArity + 1,
              VariantRef = VariantRelName/VariantArity
            ),
            VariantRefs).

enum_variant((Left ; Right), VariantTerm) :-
    !,
    ( enum_variant(Left, VariantTerm)
    ; enum_variant(Right, VariantTerm)
    ).
enum_variant(VariantTerm, VariantTerm).

variant_name_arity(VariantTerm, VariantName, ContentArity) :-
    compound(VariantTerm),
    !,
    functor(VariantTerm, VariantName, ContentArity).
variant_name_arity(VariantName, VariantName, 0) :-
    atom(VariantName),
    !.
variant_name_arity(VariantTerm, _, _) :-
    throw(unsupported_construct(enum_variant_shape(VariantTerm))).
