% 0_match_expand.pl: shared match-block and enum-declaration sugar expansion.
%
% The retained program term keeps:
%   match(SourceAtom, ((ArmHead <- Guards) ; (EdgeHead <+ Guards)))
%
% Consumers receive one ordinary rule per arm:
%   ArmHead <- SourceAtom, Guards
%   EdgeHead <+ SourceAtom, Guards

:- module(match_expand,
          [ expand_match_program/2,
            expand_match_program_in_context/3 ]).

:- use_module(library(lists)).
:- use_module('next/1_expand/0_enum_expand', [expand_enum_program/2, enum_context/2,
                                merge_enum_type_rows/3,
                                merge_option_type_rows/2,
                                drop_minted_keyed_on_derived/3]).
:- use_module('0_generic_expand', [freeze_type_rows/2]).

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% One-shot entry that expands match arms before enum declarations.
expand_match_program(prog(SugaredDecls, SugaredRules), ExpandedProgram) :-
    enum_context(SugaredDecls, Enums),
    expand_match_rules(Enums, SugaredRules, MatchExpandedRules),
    expand_enum_program(prog(SugaredDecls, MatchExpandedRules),
                        EnumExpandedProgram),
    drop_minted_keyed_on_derived(Enums, EnumExpandedProgram, DroppedProgram),
    merge_enum_type_rows(SugaredDecls, DroppedProgram, EnumRowedProgram),
    merge_option_type_rows(EnumRowedProgram, OptionRowedProgram),
    OptionRowedProgram = prog(OptionDecls, OptionRules),
    freeze_type_rows(OptionDecls, FrozenDecls),
    ExpandedProgram = prog(FrozenDecls, OptionRules).

% Driver entry: arms only, no enum pass, coverage checked against a context
% built from the SURFACE declarations. Enum expansion has already run and
% erased the enum_decl/2 entries by the time this is called, which is exactly
% why the context is a parameter and not something to re-derive here.
expand_match_program_in_context(Enums, prog(Decls, Rules),
                                prog(Decls, ExpandedRules)) :-
    expand_match_rules(Enums, Rules, ExpandedRules).

expand_match_rules(_, [], []).
expand_match_rules(Enums, [match(SourceAtom, Arms) | Rest],
                   ExpandedRules) :-
    !,
    validate_match_source(SourceAtom),
    match_arms(Arms, ArmList),
    validate_match_coverage(Enums, ArmList),
    maplist(expand_match_arm(SourceAtom), ArmList, ThisRules),
    expand_match_rules(Enums, Rest, RestRules),
    append(ThisRules, RestRules, ExpandedRules).
expand_match_rules(Enums, [Rule | Rest], [Rule | ExpandedRest]) :-
    expand_match_rules(Enums, Rest, ExpandedRest).

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

validate_match_coverage(EnumVariants, Arms) :-
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

% enum_variant_refs/3, a second enum_variant/2 semicolon walker and
% variant_name_arity/3 used to sit here, duplicating 0_enum_expand.pl's naming
% rule. They are gone: the enum metadata arrives as the context parameter, from
% the one file that owns variant naming.
