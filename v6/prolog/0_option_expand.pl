% option(T) decl sugar (plans/2026-08-08-option-type-design.md, ruling
% option_surface): value -> '__opt_<t>' enum id, rel ref -> companion split rel.
:- module(option_expand,
          [ expand_option_in_context/3,
            expand_option_program/2,
            expand_option_decls/2,
            option_enum_name/2,
            option_enum_decl/2,
            option_value_element/2,
            companion_rel_name/3,
            acyclic_companion/5,
            scalar_element/1 ]).

:- use_module(library(lists)).
:- use_module(library(apply)).

expand_option_in_context(_Context, Program, Expanded) :-
    expand_option_program(Program, Expanded).

% Rules untouched: authors write bodies against the desugared rels, the
% same consumption shape enums already have.
expand_option_program(prog(Decls0, Rules), prog(Decls, Rules)) :-
    expand_option_decls(Decls0, Decls).

expand_option_decls(Decls0, Decls) :-
    strip_acyclic_wrappers(Decls0, Decls1),
    desugar_enum_payload_options(Decls1, Decls2),
    desugar_option_columns(Decls2, Decls).

% Enum expansion turns every payload field into an ordinary col_type/3 after
% this phase.  Normalize option payloads first, while their complete source
% type is still attached to the declaring variant.  The marker carries that
% source type forward for the catalog-backed TS/Rust/JSON emitters; SQLite
% still sees the generated option enum endpoint as an integer column.
desugar_enum_payload_options(Decls0, Decls) :-
    enum_payload_option_elements(Decls0, Elements),
    ensure_option_enum_decls(Elements, Decls0, WithEnums),
    rewrite_enum_payload_option_decls(WithEnums, Decls).

enum_payload_option_elements(Decls, Elements) :-
    findall(Element,
            ( member(enum_decl(_, Variants), Decls),
              enum_variant_payload_type(Variants, option(Element)) ),
            Found),
    sort(Found, Elements).

enum_variant_payload_type((Left ; Right), Type) :-
    !,
    ( enum_variant_payload_type(Left, Type)
    ; enum_variant_payload_type(Right, Type) ).
enum_variant_payload_type(Variant, Type) :-
    Variant =.. [_ | Fields],
    member(_:Type, Fields).

ensure_option_enum_decls([], Decls, Decls).
ensure_option_enum_decls([Element | Rest], Decls0, Decls) :-
    ensure_option_enum_decls(Decls0, Element, _EnumName, Decls1),
    ensure_option_enum_decls(Rest, Decls1, Decls).

rewrite_enum_payload_option_decls([], []).
rewrite_enum_payload_option_decls([enum_decl(EnumName, Variants0) | Rest],
                                  [enum_decl(EnumName, Variants) | Output]) :-
    !,
    rewrite_enum_variant_options(EnumName, Variants0, Variants, Markers),
    rewrite_enum_payload_option_decls(Rest, RestOutput),
    append(Markers, RestOutput, Output).
rewrite_enum_payload_option_decls([Decl | Rest], [Decl | Output]) :-
    rewrite_enum_payload_option_decls(Rest, Output).

rewrite_enum_variant_options(EnumName, (Left0 ; Right0), (Left ; Right),
                             Markers) :-
    !,
    rewrite_enum_variant_options(EnumName, Left0, Left, LeftMarkers),
    rewrite_enum_variant_options(EnumName, Right0, Right, RightMarkers),
    append(LeftMarkers, RightMarkers, Markers).
rewrite_enum_variant_options(EnumName, Variant0, Variant, Markers) :-
    Variant0 =.. [VariantName | Fields0],
    rewrite_enum_variant_fields(EnumName, VariantName, Fields0, Fields,
                                Markers),
    Variant =.. [VariantName | Fields].

rewrite_enum_variant_fields(_, _, [], [], []).
rewrite_enum_variant_fields(EnumName, VariantName,
                            [FieldName:option(Element) | Rest],
                            [FieldName:OptionEnum | Fields],
                            [enum_option_payload(EnumName, VariantName,
                                                 FieldName, Element) | Markers]) :-
    !,
    option_enum_name(Element, OptionEnum),
    rewrite_enum_variant_fields(EnumName, VariantName, Rest, Fields, Markers).
rewrite_enum_variant_fields(EnumName, VariantName, [Field | Rest],
                            [Field | Fields], Markers) :-
    rewrite_enum_variant_fields(EnumName, VariantName, Rest, Fields, Markers).

desugar_option_columns(Decls0, Decls) :-
    ( member(col_type(Ref, Column, option(Element)), Decls0)
    -> desugar_option_column(Decls0, Ref, Column, Element, Decls1),
       desugar_option_columns(Decls1, Decls)
    ; Decls = Decls0
    ).

% acyclic(...) is a constraint the storage plane never sees: the inner type
% stays, and acyclic_column/2 records that the author spelled the guard out.
strip_acyclic_wrappers(Decls0, Decls) :-
    (   selectchk(col_type(Ref, Column, acyclic(Inner)), Decls0,
                  col_type(Ref, Column, Inner), Decls1)
    ->  check_acyclic_target(Ref, Column, Inner),
        append(Decls1, [acyclic_column(Ref, Column)], Decls2),
        strip_acyclic_wrappers(Decls2, Decls)
    ;   Decls = Decls0
    ).

% The guard walks the chain the column itself forms, so the only type it can
% wrap is an option of the rel that declares it.
check_acyclic_target(Name/_, _, option(Name)) :- !.
check_acyclic_target(Ref, Column, Inner) :-
    throw(unsupported_construct(acyclic_not_a_self_option(Ref, Column,
                                                          Inner))).

% Wrapper composition contract
%
%   option_value_element(+Decls, +SurfaceElement) is semidet.
%   ensure_option_enum_decls(+Decls0, +SurfaceElement, -EnumName, -Decls) is det.
%
% State timeline for option(option(T)):
%   source option(option(T)) -> __opt_T + __opt_option_T -> INTEGER enum ids.
%   The outer `some` payload is an inner option id. option_column/3 retains
%   the full surface tree for the type catalog, so outer-none, outer-some /
%   inner-none, and outer-some / inner-some remain distinct values.
desugar_option_column(Decls0, Ref, Column, Element, Decls) :-
    ( option_column_position(Decls0, Ref, Column, Position)
    -> true
    ; throw(unsupported_construct(option_column_untyped_siblings(Ref)))
    ),
    % option_column/3 survives so catalog-backed schema emitters can recover
    % the recursive tagged option tree from the desugared enum-id column.
    ( option_value_element(Decls0, Element)
    -> desugar_value_option(Decls0, Ref, Column, Element, Decls1),
       append(Decls1, [option_column(Ref, Column, Element)], Decls)
    ; memberchk(keyed(Ref, KeyPositions), Decls0),
      memberchk(Position, KeyPositions),
      declared_rel_element(Decls0, Element)
    % A keyed option must stay in its owner row: `none` and `some(Target)`
    % are enum ids, so SQLite key equality never observes NULL/3VL.
    -> desugar_value_option(Decls0, Ref, Column, Element, Decls1),
       append(Decls1, [option_column(Ref, Column, Element)], Decls)
    ; declared_rel_element(Decls0, Element)
    -> desugar_reference_option(Decls0, Ref, Column, Element, Position,
                                Decls1),
       append(Decls1, [option_column(Ref, Column, Element)], Decls)
    ; throw(unsupported_construct(option_element_type_unknown(Element)))
    ).

scalar_element(int).
scalar_element(text).
scalar_element(bool).
scalar_element(float).
scalar_element(json).

% This recursive branch descends through a strict subterm of a parsed finite
% type tree. A named relation intentionally does not match this predicate.
option_value_element(_, Element) :- scalar_element(Element), !.
option_value_element(Decls, Element) :-
    atom(Element), memberchk(enum_decl(Element, _), Decls), !.
option_value_element(Decls, option(Element)) :-
    option_value_element(Decls, Element).

declared_rel_element(Decls, Element) :-
    atom(Element),
    ( memberchk(col_type(Element/_, _, _), Decls)
    ; memberchk(type_decl(Element, _), Decls)
    ).

% Positions are meaningful only fully typed; the length check is that
% requirement.
option_column_position(Decls, Name/Arity, Column, Position) :-
    findall(EachColumn,
            member(col_type(Name/Arity, EachColumn, _), Decls),
            Columns),
    length(Columns, Arity),
    nth1(Position, Columns, Column).

desugar_value_option(Decls0, Ref, Column, Element, Decls) :-
    ensure_option_enum_decls(Decls0, Element, EnumName, WithEnums),
    selectchk(col_type(Ref, Column, option(Element)),
              WithEnums,
              col_type(Ref, Column, EnumName),
              Decls).

ensure_option_enum_decls(Decls0, Element, EnumName, Decls) :-
    option_enum_name(Element, EnumName),
    option_enum_payload(Decls0, Element, Payload, Decls1),
    ( memberchk(enum_decl(EnumName, _), Decls1)
    -> Decls = Decls1
    ; Decls = [enum_decl(EnumName, (none ; some(value:Payload))) | Decls1]
    ).

option_enum_payload(Decls, Element, Element, Decls) :- scalar_element(Element), !.
option_enum_payload(Decls, Element, Element, Decls) :-
    atom(Element), memberchk(enum_decl(Element, _), Decls), !.
option_enum_payload(Decls, Element, Element, Decls) :-
    declared_rel_element(Decls, Element), !.
option_enum_payload(Decls0, option(Inner), InnerEnumName, Decls) :-
    ensure_option_enum_decls(Decls0, Inner, InnerEnumName, Decls).

option_enum_name(Element, EnumName) :-
    option_type_stem(Element, Stem),
    atomic_list_concat(['__opt_', Stem], EnumName).

option_type_stem(Element, Element) :- atom(Element), !.
option_type_stem(option(Inner), Stem) :-
    option_type_stem(Inner, InnerStem),
    atomic_list_concat([option, InnerStem], '_', Stem).

% The one spelling of the minted enum, shared with the row merge so the
% graph cannot drift from what expansion mints.
option_enum_decl(Element, enum_decl(EnumName, (none ; some(value:Element)))) :-
    option_enum_name(Element, EnumName).

desugar_reference_option(Decls0, ParentName/Arity, Column, Element, Position,
                         Decls) :-
    NewArity is Arity - 1,
    check_companion_name_free(Decls0, ParentName/Arity, Column),
    exclude(option_column_entry(ParentName/Arity, Column), Decls0, Kept),
    maplist(shrink_parent_ref(ParentName/Arity, ParentName/NewArity,
                              Position),
            Kept, RewrittenDecls),
    companion_rel_decls(Decls0, ParentName, Column, Element, CompanionDecls),
    append(RewrittenDecls, CompanionDecls, Decls).

option_column_entry(Ref, Column, col_type(Ref, Column, option(_))).

% The companion split rel lands in the author namespace, the hazard
% validate_generated_name_collisions/3 covers for the minted generic names.
check_companion_name_free(Decls, ParentName/Arity, Column) :-
    companion_rel_name(ParentName, Column, CompanionName),
    (   member(col_type(CompanionName/DeclaredArity, _, _), Decls)
    ->  throw(unsupported_construct(
                option_companion_name_collision(
                    CompanionName/DeclaredArity, ParentName/Arity, Column)))
    ;   true
    ).

shrink_parent_ref(OldRef, NewRef, _, col_type(OldRef, Column, Type),
                  col_type(NewRef, Column, Type)) :- !.
shrink_parent_ref(OldRef, NewRef, DroppedPosition,
                  keyed(OldRef, Positions), keyed(NewRef, Renumbered)) :-
    !,
    maplist(renumber_key_position(DroppedPosition), Positions, Renumbered).
shrink_parent_ref(OldRef, NewRef, _, kind(OldRef, Kind),
                  kind(NewRef, Kind)) :- !.
shrink_parent_ref(OldRef, NewRef, _, keep(OldRef, Policy),
                  keep(NewRef, Policy)) :- !.
% The path carrier is keyed on the ref, so a shrink that skips it leaves the
% dot phase reading an arity no other decl carries.
shrink_parent_ref(OldRef, NewRef, _, rel_path_decl(OldRef, Segments),
                  rel_path_decl(NewRef, Segments)) :- !.
shrink_parent_ref(_, _, _, Decl, Decl).

% DroppedPosition itself cannot appear: the key-column ban threw first.
renumber_key_position(DroppedPosition, Position, Renumbered) :-
    ( Position > DroppedPosition
    -> Renumbered is Position - 1
    ; Renumbered = Position
    ).

companion_rel_name(ParentName, Column, CompanionName) :-
    atomic_list_concat([ParentName, '__', Column], CompanionName).

% Default-on: a column typed option(<its own rel>) is a parent chain and its
% companion split rel carries the guard whether or not acyclic was spelled.
acyclic_companion(Decls, CompanionName/2, declared_at(ParentName, Column),
                  OwnerColumn, TargetColumn) :-
    member(option_column(ParentName/_, Column, ParentName), Decls),
    companion_rel_name(ParentName, Column, CompanionName),
    atom_concat(ParentName, '_id', OwnerColumn),
    companion_element_column(ParentName, Column, ParentName, TargetColumn).

% The companion's element column stores the element rel's id, so it is typed
% as the element (resolving to ref(Element) storage) when the element is a
% declared type. A col_type-only element rel has no type_decl and stays an
% ordinary integer id, matching the list member's value column behavior.
companion_rel_decls(Decls, ParentName, Column, Element,
                    [ col_type(CompanionRef, ParentIdColumn, int),
                      col_type(CompanionRef, ElementIdColumn, ElementType),
                      keyed(CompanionRef, [1]) ]) :-
    companion_rel_name(ParentName, Column, CompanionName),
    CompanionRef = CompanionName/2,
    atom_concat(ParentName, '_id', ParentIdColumn),
    companion_element_column(ParentName, Column, Element, ElementIdColumn),
    companion_element_type(Decls, Element, ElementType).

companion_element_type(Decls, Element, Element) :-
    memberchk(type_decl(Element, _), Decls), !.
companion_element_type(_, _, int).

% A self-typed column names both endpoints after the same rel and one CREATE
% TABLE cannot carry the atom twice, so the column name qualifies the target.
companion_element_column(ParentName, Column, ParentName, ElementIdColumn) :-
    !,
    atomic_list_concat([Column, '_', ParentName, '_id'], ElementIdColumn).
companion_element_column(_, _, Element, ElementIdColumn) :-
    atom_concat(Element, '_id', ElementIdColumn).
