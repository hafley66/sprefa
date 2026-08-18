:- module(emit_ts_types, [ ts_types_text/3, emit_ts_types/3 ]).

:- use_module(library(lists)).
:- use_module(library(pairs)).

ts_types_text(_Name, Rows, Text) :-
    relation_id_alias_text(Rows, RelationIdAlias),
    relation_id_alias_parts(RelationIdAlias, RelationIdParts),
    option_alias_text(Rows, OptionAlias),
    option_alias_parts(OptionAlias, OptionParts),
    findall(InterfaceText, ts_interface_text(Rows, InterfaceText), InterfaceParts),
    findall(GenericText, ts_generic_text(Rows, GenericText), GenericParts),
    findall(EnumText, ts_enum_text(Rows, EnumText), EnumParts),
    findall(RelRow, renderable_rel(Rows, RelRow), RelRows),
    collision_type_names(RelRows, CollisionTypeNames),
    maplist(ts_rel_text(Rows, CollisionTypeNames), RelRows, RelParts),
    append([RelationIdParts, OptionParts, InterfaceParts, GenericParts, EnumParts, RelParts], Parts),
    atomic_list_concat(Parts, '\n', Atom),
    atom_string(Atom, Text).

% TypeScript needs a tagged carrier rather than `T | undefined`: the latter
% collapses option(option(T)). The two tags preserve outer-none versus
% outer-some/inner-none at the host boundary.
option_alias_text(Rows,
                  'export type Option<T> = { tag: \'none\' } | { tag: \'some\'; value: T };\n') :-
    member(row(_, _, _, _, option, _, _, _, _, _, _), Rows), !.
option_alias_text(_, '').

option_alias_parts('', []) :- !.
option_alias_parts(Text, [Text]).

% A relation endpoint is an integer on SQLite's wire, with a target-specific
% phantom member in the authored TypeScript surface. The target name comes
% from the column type_id, while the storage child says this is an endpoint
% rather than a followed relation value.
relation_id_alias_text(Rows,
                       'declare const __dl6RelationId: unique symbol;\nexport type RelationId<T extends string> = number & { readonly [__dl6RelationId]: T };\n') :-
    relation_id_storage(Rows, _), !.
relation_id_alias_text(_, '').

relation_id_alias_parts('', []) :- !.
relation_id_alias_parts(Text, [Text]).

ts_enum_text(Rows, Text) :-
    member(row(EnumId, _, _, Name, enum, _, _, _, _, _, _), Rows),
    \+ compiler_helper_rel(Name),
    type_name(Name, TypeName),
    findall(Ordinal-VariantText,
            ( member(row(_, EnumId, Ordinal, VariantName, enum_variant,
                         VariantRelId, _, _, _, _, _), Rows),
              ts_enum_variant_text(Rows, VariantName, VariantRelId, VariantText) ),
            Unsorted),
    keysort(Unsorted, Ordered),
    pairs_values(Ordered, Variants),
    atomic_list_concat(Variants, '', Body),
    format(string(Text), 'export type ~w =\n~s;\n', [TypeName, Body]).

ts_enum_variant_text(Rows, VariantName, VariantRelId, Text) :-
    findall(Name-Type,
            ( member(row(ColumnId, VariantRelId, _, Name, column, TypeId,
                         _, _, _, _, _), Rows),
              Name \== id,
              ts_column_value_type(Rows, [], ColumnId, TypeId, Type) ), Fields),
    ( Fields == []
    -> format(string(Text), '  | { tag: \'~w\' }\n', [VariantName])
    ; maplist(ts_enum_field_text, Fields, FieldTexts),
      atomic_list_concat(FieldTexts, '', Body),
      format(string(Text), '  | { tag: \'~w\'; ~s}\n', [VariantName, Body])
    ).

ts_enum_field_text(Name-Type, Text) :- format(string(Text), '~w: ~w; ', [Name, Type]).

emit_ts_types(Name, Rows, Path) :-
    ts_types_text(Name, Rows, Text),
    setup_call_cleanup(open(Path, write, Stream), format(Stream, '~s', [Text]), close(Stream)).

renderable_rel(Rows, RelRow) :-
    member(RelRow, Rows),
    RelRow = row(RelId, _, _, Name, rel, _, _, _ModuleId, _, _, _),
    ( \+ compiler_helper_rel(Name) ; concrete_rel(Rows, RelId) ),
    rel_columns(Rows, RelRow, Columns),
    maplist(ts_column_type(Rows), Columns, _).

compiler_helper_rel(Name) :- sub_atom(Name, _, _, _, '__').
concrete_rel(Rows, RelId) :-
    memberchk(row(_, RelId, _, _, concrete_type, _, _, _, _, _, _), Rows).

ts_interface_text(Rows, Text) :-
    member(row(_, _, _, Name, interface, _, _, _, _, _, _), Rows),
    type_name(Name, TypeName),
    format(string(Text), 'export interface ~w {}\n', [TypeName]).

ts_generic_text(Rows, Text) :-
    member(row(GenericId, _, _, Name, generic_rel, _, _, _, _, _, _), Rows),
    type_name(Name, TypeName),
    generic_parameters_text(Rows, GenericId, ParametersText),
    findall(Ord-Column-TypeId,
            member(row(_, GenericId, Ord, Column, generic_column, TypeId,
                       _, _, _, _, _), Rows), Columns0),
    keysort(Columns0, Columns),
    maplist(ts_generic_property_text(Rows), Columns, Properties),
    atomic_list_concat(Properties, '', Body),
    format(string(Text), 'export interface ~w~w {\n~s}\n',
           [TypeName, ParametersText, Body]).

generic_parameters_text(Rows, GenericId, Text) :-
    findall(Ord-Name-ParameterId,
            member(row(ParameterId, GenericId, Ord, Name, type_parameter,
                       _, _, _, _, _, _), Rows), Parameters0),
    keysort(Parameters0, Parameters),
    maplist(ts_parameter_text(Rows), Parameters, ParameterTexts),
    ( ParameterTexts == [] -> Text = ''
    ; atomic_list_concat(ParameterTexts, ', ', Joined),
      format(atom(Text), '<~w>', [Joined]) ).

ts_parameter_text(Rows, _Ord-Name-ParameterId, Text) :-
    findall(Constraint,
            ( member(row(_, ParameterId, _, Interface, constraint, _, _, _,
                         _, _, _), Rows),
              type_name(Interface, Constraint) ), Constraints),
    ( Constraints == [] -> Text = Name
    ; atomic_list_concat(Constraints, ' & ', Bound),
      format(atom(Text), '~w extends ~w', [Name, Bound]) ).

ts_generic_property_text(Rows, _Ord-Name-TypeId, Text) :-
    ts_type(Rows, TypeId, Type),
    format(string(Text), '  ~w: ~w;\n', [Name, Type]).

collision_type_names(RelRows, CollisionTypeNames) :-
    findall(TypeName,
            ( member(row(_, _, _, Name, rel, _, _, _, _, _, _), RelRows),
              type_name(Name, TypeName) ),
            TypeNames),
    msort(TypeNames, SortedTypeNames),
    duplicate_type_names(SortedTypeNames, CollisionTypeNames).

duplicate_type_names([], []).
duplicate_type_names([_], []).
duplicate_type_names([TypeName, TypeName | Rest], [TypeName | Collisions]) :-
    !,
    drop_type_name(TypeName, Rest, Remaining),
    duplicate_type_names(Remaining, Collisions).
duplicate_type_names([_ | Rest], Collisions) :- duplicate_type_names(Rest, Collisions).

drop_type_name(TypeName, [TypeName | Rest], Remaining) :-
    !,
    drop_type_name(TypeName, Rest, Remaining).
drop_type_name(_TypeName, Remaining, Remaining).

rel_columns(Rows, row(RelId, _, _, _, rel, _, _, _, _, _, _), Columns) :-
    findall(Ord-ColumnId-Name-TypeId,
            member(row(ColumnId, RelId, Ord, Name, column, TypeId,
                       _, _, _, _, _), Rows),
            Unsorted),
    keysort(Unsorted, Columns).

ts_rel_text(Rows, CollisionTypeNames, RelRow, Text) :-
    emitted_type_name(Rows, CollisionTypeNames, RelRow, TypeName),
    rel_columns(Rows, RelRow, Columns),
    maplist(ts_property_text(Rows, CollisionTypeNames), Columns, Properties),
    atomic_list_concat(Properties, '', Body),
    format(string(Text), 'export interface ~w {\n~s}\n', [TypeName, Body]).

ts_property_text(Rows, CollisionTypeNames, _Ord-ColumnId-Name-TypeId, Text) :-
    ts_column_value_type(Rows, CollisionTypeNames, ColumnId, TypeId, Type),
    format(string(Text), '  ~w: ~w;\n', [Name, Type]).

ts_column_type(Rows, _Ord-ColumnId-_Name-TypeId, Type) :-
    ts_column_value_type(Rows, [], ColumnId, TypeId, Type).

ts_column_value_type(Rows, CollisionTypeNames, ColumnId, TypeId, Type) :-
    ( relation_id_storage(Rows, ColumnId)
    -> ts_relation_id_type(Rows, CollisionTypeNames, TypeId, Type)
    ; ts_type(Rows, CollisionTypeNames, TypeId, Type)
    ).

relation_id_storage(Rows, ColumnId) :-
    member(row(_, ColumnId, _, relation_id, storage, _, _, _, _, _, _), Rows).

ts_relation_id_type(Rows, CollisionTypeNames, TargetId, Type) :-
    member(TargetRow, Rows),
    TargetRow = row(TargetId, _, _, _, rel, _, _, _, _, _, _),
    emitted_type_name(Rows, CollisionTypeNames, TargetRow, Target),
    format(string(Type), 'RelationId<\'~w\'>', [Target]).

ts_type(Rows, TypeId, Type) :- ts_type(Rows, [], TypeId, Type).

ts_type(Rows, CollisionTypeNames, TypeId, Type) :-
    member(TypeRow, Rows),
    TypeRow = row(TypeId, _, _, Name, Kind, ElementId, _, _, _, _, _),
    ts_kind(Rows, CollisionTypeNames, TypeRow, Name, Kind, ElementId, Type).

ts_kind(_Rows, _CollisionTypeNames, _TypeRow, int, primitive, _ElementId, 'number').
ts_kind(_Rows, _CollisionTypeNames, _TypeRow, float, primitive, _ElementId, 'number').
ts_kind(_Rows, _CollisionTypeNames, _TypeRow, text, primitive, _ElementId, 'string').
ts_kind(_Rows, _CollisionTypeNames, _TypeRow, bool, primitive, _ElementId, 'boolean').
ts_kind(_Rows, _CollisionTypeNames, _TypeRow, json, primitive, _ElementId, 'unknown').
ts_kind(_Rows, _CollisionTypeNames, _TypeRow, Name, type_parameter, _ElementId, Name).
ts_kind(Rows, CollisionTypeNames, _TypeRow, _Name, json_list, ElementId, Type) :-
    ts_type(Rows, CollisionTypeNames, ElementId, Element),
    format(string(Type), 'Array<~w>', [Element]).
% The relational list crosses the boundary as its elements, so the two list
% spellings render one TS type.
ts_kind(Rows, CollisionTypeNames, _TypeRow, _Name, list, ElementId, Type) :-
    ts_type(Rows, CollisionTypeNames, ElementId, Element),
    format(string(Type), 'Array<~w>', [Element]).
ts_kind(Rows, CollisionTypeNames, _TypeRow, _Name, option, ElementId, Type) :-
    ts_type(Rows, CollisionTypeNames, ElementId, Element),
    format(string(Type), 'Option<~w>', [Element]).
ts_kind(_Rows, _CollisionTypeNames, _TypeRow, Name, enum, _ElementId, Type) :-
    type_name(Name, Type).
ts_kind(Rows, CollisionTypeNames, TypeRow, _Name, rel, _ElementId, Type) :-
    emitted_type_name(Rows, CollisionTypeNames, TypeRow, Type).

emitted_type_name(Rows, CollisionTypeNames,
                  row(_, _, _, Name, rel, _, _, ModuleId, _, _, _), Type) :-
    type_name(Name, BareType),
    (   memberchk(BareType, CollisionTypeNames)
    ->  memberchk(row(ModuleId, _, _, ModuleName, module, _, _, _, _, _, _), Rows),
        module_type_name(ModuleName, ModuleType),
        atom_concat(ModuleType, BareType, Type)
    ;   Type = BareType
    ).

module_type_name(ModuleName, Type) :-
    atom_codes(ModuleName, Codes),
    maplist(module_identifier_code, Codes, IdentifierCodes),
    atom_codes(IdentifierName, IdentifierCodes),
    atomic_list_concat(Parts0, '_', IdentifierName),
    exclude(empty_atom, Parts0, Parts),
    maplist(capitalized, Parts, Capitals),
    atomic_list_concat(Capitals, '', Type0),
    prefix_digit_type(Type0, Type).

module_identifier_code(Code, Code) :- code_type(Code, alnum), !.
module_identifier_code(_Code, 0'_).

empty_atom('').

prefix_digit_type(Type0, Type) :-
    atom_codes(Type0, [First | _]),
    code_type(First, digit),
    !,
    atom_concat('M', Type0, Type).
prefix_digit_type(Type, Type).

% A double underscore splits to an empty part; capitalized/2 cannot
% destructure '', so an unfiltered split dropped the rel with no solution.
type_name(Name, Type) :-
    atomic_list_concat(Parts0, '_', Name),
    exclude(empty_atom, Parts0, Parts),
    maplist(capitalized, Parts, Capitals),
    atomic_list_concat(Capitals, '', Type).

capitalized(Part, Capital) :-
    atom_chars(Part, [First | Rest]),
    upcase_atom(First, Upper),
    atom_chars(Capital, [Upper | Rest]).
