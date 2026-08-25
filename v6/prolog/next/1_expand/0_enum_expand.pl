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
            tag_rel_name/2,
            enum_type_rows/2,
            merge_enum_type_rows/3,
            merge_option_type_rows/2,
            drop_minted_keyed_on_derived/3 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).
:- use_module('../../0_program_check', [level_headed/2]).
:- use_module('0_option_expand', [companion_rel_name/3, option_enum_name/2,
                                  option_value_element/2, scalar_element/1]).
:- use_module('../../0_type_ids', [decl_id/4, member_id/4]).

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

% Runs after every rule-producing phase in either door: a minted keyed on a
% derived rel is dropped; author keyed stays under the keyed_level_head guard.
drop_minted_keyed_on_derived(EnumContext, prog(Decls, Rules),
                             prog(Kept, Rules)) :-
    findall(Ref,
            ( member(_-VariantPairs, EnumContext),
              member(Ref-_, VariantPairs) ),
            VariantRefs),
    findall(CompanionName/2,
            ( member(option_column(ParentName/_, Column, _), Decls),
              companion_rel_name(ParentName, Column, CompanionName) ),
            CompanionRefs),
    append(VariantRefs, CompanionRefs, MintedRefs),
    exclude(minted_keyed_on_derived(MintedRefs, Rules), Decls, Kept).

minted_keyed_on_derived(MintedRefs, Rules, keyed(Ref, _)) :-
    memberchk(Ref, MintedRefs),
    level_headed(Rules, Ref).

% Runs on the completed program in either door, from the SURFACE declarations
% expansion erased. The rows are additive; nothing above reads them back.
merge_enum_type_rows(SurfaceDecls, prog(Decls0, Rules), prog(Decls, Rules)) :-
    enum_type_rows(SurfaceDecls, EnumRows),
    (   EnumRows == []
    ->  Decls = Decls0
    ;   memberchk(semantic_type_rows(_), Decls0)
    ->  maplist(merge_one_enum_type_rows(EnumRows), Decls0, Decls)
    ;   append(Decls0, [semantic_type_rows(EnumRows)], Decls)
    ).

merge_one_enum_type_rows(EnumRows, semantic_type_rows(Rows0),
                         semantic_type_rows(Rows)) :-
    !,
    append(Rows0, EnumRows, Unsorted),
    sort(Unsorted, Rows).
merge_one_enum_type_rows(_, Decl, Decl).

% Runs after merge_enum_type_rows in either door, from the option_column/3
% markers desugar leaves in the COMPLETED declarations. Rows are additive.
merge_option_type_rows(prog(Decls0, Rules), prog(Decls, Rules)) :-
    option_type_rows(Decls0, OptionRows),
    (   OptionRows == []
    ->  Decls = Decls0
    ;   memberchk(semantic_type_rows(_), Decls0)
    ->  maplist(merge_one_enum_type_rows(OptionRows), Decls0, Decls)
    ;   append(Decls0, [semantic_type_rows(OptionRows)], Decls)
    ).

%! option_type_rows(+Decls, -Rows) is det.
option_type_rows(Decls, Rows) :-
    findall(Row, option_type_row(Decls, Row), Unsorted),
    sort(Unsorted, Rows).

option_type_row(Decls, Row) :-
    member(option_column(_/_, _, Element), Decls),
    option_catalog_value_element(Decls, Element),
    option_enum_type_rows(Element, EnumRows),
    member(Row, EnumRows).
option_type_row(Decls,
                origin(EnumId, option_column(ParentName, Column, Element))) :-
    member(option_column(ParentName/_, Column, Element), Decls),
    option_catalog_value_element(Decls, Element),
    option_enum_name(Element, EnumName),
    semantic_decl_id(Decls, enum, EnumName, EnumId).
option_type_row(Decls, Row) :-
    member(option_column(ParentName/_, Column, Element), Decls),
    \+ option_catalog_value_element(Decls, Element),
    companion_rel_name(ParentName, Column, CompanionName),
    semantic_decl_id(Decls, relation, ParentName, ParentId),
    semantic_decl_id(Decls, relation, CompanionName, CompanionId),
    member(Row,
           [ declaration(ParentId, root, ParentName, relation, materialized),
             declaration(CompanionId, root, CompanionName, relation,
                         materialized),
             derived_from(CompanionId, ParentId),
             origin(CompanionId, option_column(ParentName, Column, Element))
           ]).

% enum expansion has erased enum_decl/2 by the time merge_option_type_rows/2
% runs. Its semantic declaration row is therefore the post-expansion witness
% that an atom is an enum value rather than a relation reference.
option_catalog_value_element(_, Element) :- scalar_element(Element), !.
option_catalog_value_element(_, option(_)) :- !.
option_catalog_value_element(Decls, Element) :-
    atom(Element),
    member(semantic_type_rows(Rows), Decls),
    member(declaration(_, root, Element, enum, compile_time), Rows).

% Metadata only needs the enum and generated-relation names. The payload is
% intentionally `int`: runtime expansion has already normalized a nested
% option payload to its inner enum name, while enum_type_rows/2 reads only the
% variant shape and declaration identity.
option_enum_type_rows(Element, Rows) :-
    option_enum_type_decls(Element, Decls),
    enum_type_rows(Decls, Rows).

option_enum_type_decls(Element, Decls) :-
    option_enum_name(Element, EnumName),
    ( Element = option(Inner)
    -> option_enum_type_decls(Inner, InnerDecls)
    ; InnerDecls = []
    ),
    append(InnerDecls, [enum_decl(EnumName, (none ; some(value:int)))], Decls).

% An enum's members are its variants; each variant rel edges back to the enum.
%! enum_type_rows(+SurfaceDecls, -Rows) is det.
enum_type_rows(SurfaceDecls, Rows) :-
    findall(Row, enum_type_row(SurfaceDecls, Row), Unsorted),
    sort(Unsorted, Rows).

enum_type_row(SurfaceDecls, declaration(EnumId, root, EnumName, enum, compile_time)) :-
    member(enum_decl(EnumName, _), SurfaceDecls),
    semantic_decl_id(SurfaceDecls, enum, EnumName, EnumId).
enum_type_row(SurfaceDecls,
              declaration(VariantRelId, root, VariantRelName, relation, materialized)) :-
    enum_variant_position(SurfaceDecls, _, _, _, VariantRelName, VariantRelId).
enum_type_row(SurfaceDecls, derived_from(VariantRelId, EnumId)) :-
    enum_variant_position(SurfaceDecls, EnumId, _, _, _, VariantRelId).
enum_type_row(SurfaceDecls,
              member(MemberId, EnumId, Ordinal, VariantName,
                     type_ref(declaration(VariantRelId)))) :-
    enum_variant_position(SurfaceDecls, EnumId, Ordinal, VariantName, _,
                          VariantRelId),
    member_id(EnumId, Ordinal, VariantName, MemberId).

enum_variant_position(SurfaceDecls, EnumId, Ordinal, VariantName, VariantRelName,
                      VariantRelId) :-
    member(enum_decl(EnumName, VariantTerms), SurfaceDecls),
    semantic_decl_id(SurfaceDecls, enum, EnumName, EnumId),
    findall(Name,
            ( enum_variant(VariantTerms, VariantTerm),
              variant_spec(VariantTerm, Name, _) ),
            VariantNames),
    nth1(Ordinal, VariantNames, VariantName),
    variant_rel_name(EnumName, VariantName, VariantRelName),
    semantic_decl_id(SurfaceDecls, relation, VariantRelName, VariantRelId).

semantic_decl_id(Decls, Kind, Name, Id) :-
    (   member(semantic_decl_module(Kind, Name, ModuleHash), Decls)
    ->  true
    ;   enum_generated_module(Decls, Kind, Name, ModuleHash)
    ->  true
    ;   ModuleHash = local
    ),
    decl_id(ModuleHash, Kind, Name, Id).

enum_generated_module(Decls, relation, VariantRelName, ModuleHash) :-
    member(enum_decl(EnumName, VariantTerms), Decls),
    enum_variant(VariantTerms, VariantTerm),
    variant_spec(VariantTerm, VariantName, _),
    variant_rel_name(EnumName, VariantName, VariantRelName),
    member(semantic_decl_module(enum, EnumName, ModuleHash), Decls),
    !.
enum_generated_module(Decls, relation, CompanionName, ModuleHash) :-
    member(option_column(ParentName/_, Column, _), Decls),
    companion_rel_name(ParentName, Column, CompanionName),
    member(semantic_decl_module(relation, ParentName, ModuleHash), Decls),
    !.

enum_tag_names(SugaredDecls, EnumToTag) :-
    findall(EnumName-TagName,
            ( member(enum_decl(EnumName, _), SugaredDecls),
              tag_rel_name(EnumName, TagName) ),
            EnumToTag).

% An enum column holds the instance id, so reading a variant is an ordinary join
% on the tag rel. A ref would make the DERIVED tag rel an arrival target too.
% The enum_column/3 marker survives so catalog-backed emitters can recover the
% enum's declared type from the retargeted int column (mirrors option_column).
retarget_enum_column_types([], Decls, Decls) :- !.
retarget_enum_column_types(EnumToTag, Decls0, Decls) :-
    enum_columns(EnumToTag, Decls0, Markers),
    maplist(retarget_enum_column_type(EnumToTag), Decls0, Retargeted),
    append(Retargeted, Markers, Decls).

enum_columns(EnumToTag, Decls0, Markers) :-
    findall(enum_column(Ref, Column, EnumName),
            ( member(col_type(Ref, Column, EnumName), Decls0),
              memberchk(EnumName-_, EnumToTag),
              \+ memberchk(option_column(Ref, Column, _), Decls0) ),
            Markers).

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
    maplist(expand_variant(RelName), Variants, VariantDeclLists,
            VariantRules),
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
