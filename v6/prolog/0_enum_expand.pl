% 0_enum_expand.pl: shared enum-declaration sugar expansion.
%
% The retained program term keeps:
%   enum_decl(body, (page(view:view) ; redirect(to:text)))
%
% Consumers receive ordinary declaration entries and level rules:
%   body_page(Id, View)
%   body_redirect(Id, To)
%   body_tag(Id, page) <- body_page(Id, View)
%   body_tag(Id, redirect) <- body_redirect(Id, To)

% enum_context/2 exists because expansion ERASES its own input: expand_enum_
% program/2 removes every enum_decl/2 entry. Anything that has to reason about
% enums AFTER that point (match exhaustiveness is the live case) must be handed
% the metadata rather than re-reading declarations that are gone. It is
% computed from the SURFACE declarations, before any phase runs.
%
% This file owns variant naming and the enum context.
:- module(enum_expand,
          [ expand_enum_program/2,
            expand_enum_in_context/3,
            enum_context/2,
            tag_rel_name/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).

:- op(1150, xfx, <-).

% enum_context(+SurfaceDecls, -Enums) with Enums a list of
% EnumName-VariantRefs, and VariantRefs a list of GeneratedRef-VariantName in
% declaration order. The generated names come from the same variant_rel_name/3
% the expansion itself uses, so the context cannot drift from what expansion
% produces.
enum_context(SurfaceDecls, Enums) :-
    findall(EnumName-VariantRefs,
            ( member(enum_decl(EnumName, VariantTerms), SurfaceDecls),
              findall(VariantRef-VariantName,
                      ( enum_variant(VariantTerms, VariantTerm),
                        variant_spec(VariantTerm, VariantName, Columns),
                        variant_rel_name(EnumName, VariantName,
                                         VariantRelName),
                        length(Columns, ContentArity),
                        VariantArity is ContentArity + 1,
                        VariantRef = VariantRelName/VariantArity ),
                      VariantRefs) ),
            Enums).

% The expansion-driver arity. Enum runs first, so it needs nothing from the
% context; the argument is there because 1_expansion.pl calls every wired
% phase the same way.
expand_enum_in_context(_Context, Program, Expanded) :-
    expand_enum_program(Program, Expanded).

expand_enum_program(prog(SugaredDecls, OriginalRules),
                    prog(ExpandedDecls, ExpandedRules)) :-
    validate_enum_names(SugaredDecls),
    expand_enum_decls(SugaredDecls, ExpandedDecls0, TagRules),
    enum_tag_names(SugaredDecls, EnumToTag),
    retarget_enum_column_types(EnumToTag, ExpandedDecls0, ExpandedDecls),
    append(OriginalRules, TagRules, ExpandedRules).

enum_tag_names(SugaredDecls, EnumToTag) :-
    findall(EnumName-TagName,
            ( member(enum_decl(EnumName, _), SugaredDecls),
              tag_rel_name(EnumName, TagName) ),
            EnumToTag).

% An enum column holds the instance id, so reading a variant is an ordinary join
% on the tag rel. A ref would make the DERIVED tag rel an arrival target too.
retarget_enum_column_types([], Decls, Decls) :- !.
retarget_enum_column_types(EnumToTag, Decls0, Decls) :-
    maplist(retarget_enum_column_type(EnumToTag), Decls0, Decls).

retarget_enum_column_type(EnumToTag,
                          col_type(Ref, Column, EnumName),
                          col_type(Ref, Column, int)) :-
    memberchk(EnumName-_, EnumToTag),
    !.
retarget_enum_column_type(_, Decl, Decl).

validate_enum_names(Decls) :-
    plain_decl_names(Decls, PlainNames),
    findall(GeneratedName,
            ( member(enum_decl(RelName, VariantTerms), Decls),
              enum_variant(VariantTerms, VariantTerm),
              variant_spec(VariantTerm, VariantName, _),
              ( memberchk(VariantName, PlainNames)
              -> throw(unsupported_construct(
                           enum_variant_name_collision(VariantName)))
              ; true
              ),
              variant_rel_name(RelName, VariantName, GeneratedName)
            ),
            GeneratedNames),
    validate_generated_names(GeneratedNames, PlainNames).

plain_decl_names(Decls, Names) :-
    findall(Name,
            ( member(Decl, Decls),
              plain_decl_ref(Decl, Name/_)
            ),
            Names0),
    sort(Names0, Names).

plain_decl_ref(kind(Ref, _), Ref).
plain_decl_ref(keyed(Ref, _), Ref).
plain_decl_ref(keep(Ref, _), Ref).
plain_decl_ref(col_type(Ref, _, _), Ref).

validate_generated_names([], _).
validate_generated_names([GeneratedName | Rest], PlainNames) :-
    ( memberchk(GeneratedName, PlainNames)
    -> throw(unsupported_construct(
                 enum_variant_rel_collision(GeneratedName)))
    ; memberchk(GeneratedName, Rest)
    -> throw(unsupported_construct(
                 enum_variant_rel_collision(GeneratedName)))
    ; validate_generated_names(Rest, PlainNames)
    ).

expand_enum_decls([], [], []).
expand_enum_decls([enum_decl(RelName, VariantTerms) | Rest],
                  ExpandedDecls, TagRules) :-
    !,
    findall(VariantTerm, enum_variant(VariantTerms, VariantTerm), Variants),
    maplist(expand_variant(RelName), Variants, VariantDeclLists, VariantRules),
    append(VariantDeclLists, VariantDecls),
    tag_rel_name(RelName, TagName),
    TagRef = TagName/2,
    append(VariantDecls,
           [col_type(TagRef, id, int), col_type(TagRef, tag, text)],
           ThisEnumDecls),
    expand_enum_decls(Rest, RestDecls, RestRules),
    append(ThisEnumDecls, RestDecls, ExpandedDecls),
    append(VariantRules, RestRules, TagRules).
expand_enum_decls([Decl | Rest], [Decl | ExpandedRest], TagRules) :-
    expand_enum_decls(Rest, ExpandedRest, TagRules).

enum_variant((Left ; Right), VariantTerm) :-
    !,
    ( enum_variant(Left, VariantTerm)
    ; enum_variant(Right, VariantTerm)
    ).
enum_variant(VariantTerm, VariantTerm).

variant_spec(VariantTerm, VariantName, Columns) :-
    compound(VariantTerm),
    !,
    VariantTerm =.. [VariantName | FieldTerms],
    maplist(variant_column, FieldTerms, Columns).
variant_spec(VariantName, VariantName, []) :-
    atom(VariantName),
    !.
variant_spec(VariantTerm, _, _) :-
    throw(unsupported_construct(enum_variant_shape(VariantTerm))).

variant_column(FieldTerm, column(ColumnName, TypeName)) :-
    nonvar(FieldTerm),
    FieldTerm =.. [':', ColumnName, TypeName],
    atom(ColumnName),
    atom(TypeName),
    !.
variant_column(FieldTerm, _) :-
    throw(unsupported_construct(enum_variant_column_shape(FieldTerm))).

expand_variant(RelName, VariantTerm, Decls, Rule) :-
    variant_spec(VariantTerm, VariantName, Columns),
    variant_rel_name(RelName, VariantName, VariantRelName),
    length(Columns, ContentArity),
    VariantArity is ContentArity + 1,
    VariantRef = VariantRelName/VariantArity,
    maplist(variant_col_type(VariantRef), Columns, ColumnDecls),
    content_key_positions(ContentArity, KeyPositions),
    append([col_type(VariantRef, id, int) | ColumnDecls],
           [keyed(VariantRef, KeyPositions)],
           Decls),
    length(Args, VariantArity),
    Args = [Id | _],
    VariantRow =.. [VariantRelName | Args],
    tag_rel_name(RelName, TagName),
    TagRow =.. [TagName, Id, VariantName],
    Rule = (TagRow <- VariantRow).

variant_col_type(VariantRef, column(ColumnName, TypeName),
                 col_type(VariantRef, ColumnName, TypeName)).

% Identity is the CONTENT, so the key skips position 1. A fieldless variant has
% no content, and `PRIMARY KEY ()` is a syntax error, so its id carries it.
content_key_positions(0, [1]) :- !.
content_key_positions(ContentArity, Positions) :-
    LastPosition is ContentArity + 1,
    numlist(2, LastPosition, Positions).

variant_rel_name(RelName, VariantName, GeneratedName) :-
    atomic_list_concat([RelName, VariantName], '_', GeneratedName).

tag_rel_name(RelName, TagName) :-
    atomic_list_concat([RelName, tag], '_', TagName).
