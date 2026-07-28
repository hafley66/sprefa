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

:- module(enum_expand, [ expand_enum_program/2 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).

:- op(1150, xfx, <-).

expand_enum_program(prog(SugaredDecls, OriginalRules),
                    prog(ExpandedDecls, ExpandedRules)) :-
    validate_enum_names(SugaredDecls),
    expand_enum_decls(SugaredDecls, ExpandedDecls, TagRules),
    append(OriginalRules, TagRules, ExpandedRules).

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
                 col_type(VariantRef, ColumnName, StorageType)) :-
    storage_type(TypeName, StorageType).

storage_type(int, int) :- !.
storage_type(text, text) :- !.
storage_type(_, int).

content_key_positions(0, []) :- !.
content_key_positions(ContentArity, Positions) :-
    LastPosition is ContentArity + 1,
    numlist(2, LastPosition, Positions).

variant_rel_name(RelName, VariantName, GeneratedName) :-
    atomic_list_concat([RelName, VariantName], '_', GeneratedName).

tag_rel_name(RelName, TagName) :-
    atomic_list_concat([RelName, tag], '_', TagName).
