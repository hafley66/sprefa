:- module(emit_rust_types, [ rust_types_text/3, emit_rust_types/3 ]).

:- use_module(library(lists)).
:- use_module(library(pairs)).

rust_types_text(_Name, Rows, Text) :-
    rust_relation_id_alias_text(Rows, RelationIdAlias),
    rust_relation_id_alias_parts(RelationIdAlias, RelationIdParts),
    rust_option_alias_text(Rows, OptionAlias),
    rust_option_alias_parts(OptionAlias, OptionParts),
    findall(InterfaceText, rust_interface_text(Rows, InterfaceText), InterfaceParts),
    findall(GenericText, rust_generic_text(Rows, GenericText), GenericParts),
    findall(EnumText, rust_enum_text(Rows, EnumText), EnumParts),
    findall(RelRow, renderable_rel(Rows, RelRow), RelRows),
    collision_type_names(RelRows, CollisionTypeNames),
    maplist(rust_rel_text(Rows, CollisionTypeNames), RelRows, RelParts),
    append([RelationIdParts, OptionParts, InterfaceParts, GenericParts, EnumParts, RelParts], Parts),
    atomic_list_concat(Parts, '\n', Atom),
    atom_string(Atom, Text).

% serde's built-in Option serializes None and Some(None) identically as null.
% This tagged carrier preserves every recursive option state on the wire.
rust_option_alias_text(Rows,
                       '#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\n#[serde(tag = "tag", content = "value", rename_all = "snake_case")]\npub enum DlOption<T> {\n    None,\n    Some(T),\n}\n') :-
    member(row(_, _, _, _, option, _, _, _, _, _, _), Rows), !.
rust_option_alias_text(_, '').

rust_option_alias_parts('', []) :- !.
rust_option_alias_parts(Text, [Text]).

% The marker carries target identity in Rust's type system while serde keeps
% the public wire representation as the one SQLite INTEGER endpoint.
rust_relation_id_alias_text(Rows,
                            '#[repr(transparent)]\n#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\npub struct RelationId<T>(pub i64, pub std::marker::PhantomData<fn() -> T>);\n\nimpl<T> serde::Serialize for RelationId<T> {\n    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {\n        serde::Serialize::serialize(&self.0, serializer)\n    }\n}\n\nimpl<''de, T> serde::Deserialize<''de> for RelationId<T> {\n    fn deserialize<D: serde::Deserializer<''de>>(deserializer: D) -> Result<Self, D::Error> {\n        let value = <i64 as serde::Deserialize>::deserialize(deserializer)?;\n        Ok(Self(value, std::marker::PhantomData))\n    }\n}\n') :-
    rust_relation_id_used(Rows), !.
rust_relation_id_alias_text(_, '').

rust_relation_id_used(Rows) :- rust_relation_id_storage(Rows, _), !.
rust_relation_id_used(Rows) :-
    member(row(_, _, _, _, relation_id_list, _, _, _, _, _, _), Rows).

rust_relation_id_alias_parts('', []) :- !.
rust_relation_id_alias_parts(Text, [Text]).

rust_enum_text(Rows, Text) :-
    member(row(EnumId, _, _, Name, enum, _, _, _, _, _, _), Rows),
    \+ compiler_helper_rel(Name),
    type_name(Name, TypeName),
    findall(Ordinal-VariantText,
            ( member(row(_, EnumId, Ordinal, VariantName, enum_variant,
                         VariantRelId, _, _, _, _, _), Rows),
              rust_enum_variant_text(Rows, VariantName, VariantRelId, VariantText) ),
            Unsorted),
    keysort(Unsorted, Ordered),
    pairs_values(Ordered, Variants),
    atomic_list_concat(Variants, '', Body),
    format(string(Text), '#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub enum ~w {\n~s}\n', [TypeName, Body]).

rust_enum_variant_text(Rows, VariantName, VariantRelId, Text) :-
    type_name(VariantName, VariantNameText),
    findall(Name-Type,
            ( member(row(ColumnId, VariantRelId, _, Name, column, TypeId,
                         _, _, _, _, _), Rows),
              Name \== id,
              rust_column_value_type(Rows, [], ColumnId, TypeId, Type) ), Fields),
    ( Fields == []
    -> format(string(Text), '    ~w,\n', [VariantNameText])
    ; maplist(rust_enum_field_text, Fields, FieldTexts),
      atomic_list_concat(FieldTexts, ', ', Body),
      format(string(Text), '    ~w { ~s },\n', [VariantNameText, Body])
    ).

rust_enum_field_text(Name-Type, Text) :- format(string(Text), '~w: ~w', [Name, Type]).

emit_rust_types(Name, Rows, Path) :-
    rust_types_text(Name, Rows, Text),
    setup_call_cleanup(open(Path, write, Stream), format(Stream, '~s', [Text]), close(Stream)).

renderable_rel(Rows, RelRow) :-
    member(RelRow, Rows),
    RelRow = row(RelId, _, _, Name, rel, _, _, _ModuleId, _, _, _),
    ( \+ compiler_helper_rel(Name) ; concrete_rel(Rows, RelId) ),
    rel_columns(Rows, RelRow, Columns),
    maplist(rust_column_type(Rows), Columns, _).

compiler_helper_rel(Name) :- sub_atom(Name, _, _, _, '__').
concrete_rel(Rows, RelId) :-
    memberchk(row(_, RelId, _, _, concrete_type, _, _, _, _, _, _), Rows).

rust_interface_text(Rows, Text) :-
    member(row(_, _, _, Name, interface, _, _, _, _, _, _), Rows),
    type_name(Name, TypeName),
    format(string(Text), 'pub trait ~w {}\n', [TypeName]).

rust_generic_text(Rows, Text) :-
    member(row(GenericId, _, _, Name, generic_rel, _, _, _, _, _, _), Rows),
    type_name(Name, TypeName),
    rust_generic_parameters_text(Rows, GenericId, ParametersText),
    findall(Ord-Column-TypeId,
            member(row(_, GenericId, Ord, Column, generic_column, TypeId,
                       _, _, _, _, _), Rows), Columns0),
    keysort(Columns0, Columns),
    maplist(rust_generic_property_text(Rows), Columns, Properties),
    rust_phantom_property_text(Rows, GenericId, Columns, PhantomText),
    atomic_list_concat(Properties, '', PropertyBody),
    atomic_list_concat([PropertyBody, PhantomText], Body),
    format(string(Text), '#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub struct ~w~w {\n~s}\n',
           [TypeName, ParametersText, Body]).

% rustc stops at E0392 on a type parameter no field mentions, and a template
% declaring a parameter it never spends in a column is legal dl6.
rust_phantom_property_text(Rows, GenericId, Columns, Text) :-
    findall(ParameterId,
            member(row(ParameterId, GenericId, _, _, type_parameter,
                       _, _, _, _, _, _), Rows), Declared),
    findall(UsedId,
            ( member(_Ord-_Column-TypeId, Columns),
              rust_parameter_reference(Rows, TypeId, UsedId) ), Used),
    findall(Name,
            ( member(ParameterId, Declared),
              \+ memberchk(ParameterId, Used),
              memberchk(row(ParameterId, _, _, Name, type_parameter,
                            _, _, _, _, _, _), Rows) ), Unused),
    (   Unused == []
    ->  Text = ''
    ;   rust_phantom_tuple_text(Unused, Tuple),
        format(string(Text),
               '    #[serde(skip)]\n    pub phantom: std::marker::PhantomData<fn() -> ~w>,\n',
               [Tuple])
    ).

% A one-element Rust tuple keeps the trailing comma; a wider one drops it.
rust_phantom_tuple_text([Name], Tuple) :- !, format(atom(Tuple), '(~w,)', [Name]).
rust_phantom_tuple_text(Names, Tuple) :-
    atomic_list_concat(Names, ', ', Joined),
    format(atom(Tuple), '(~w)', [Joined]).

rust_parameter_reference(Rows, TypeId, ParameterId) :-
    member(row(TypeId, _, _, _, Kind, ElementId, _, _, _, _, _), Rows),
    (   Kind == type_parameter
    ->  ParameterId = TypeId
    ;   memberchk(Kind, [json_list, list, option]),
        rust_parameter_reference(Rows, ElementId, ParameterId)
    ).

rust_generic_parameters_text(Rows, GenericId, Text) :-
    findall(Ord-Name-ParameterId,
            member(row(ParameterId, GenericId, Ord, Name, type_parameter,
                       _, _, _, _, _, _), Rows), Parameters0),
    keysort(Parameters0, Parameters),
    maplist(rust_parameter_text(Rows), Parameters, ParameterTexts),
    ( ParameterTexts == [] -> Text = ''
    ; atomic_list_concat(ParameterTexts, ', ', Joined),
      format(atom(Text), '<~w>', [Joined]) ).

rust_parameter_text(Rows, _Ord-Name-ParameterId, Text) :-
    findall(Constraint,
            ( member(row(_, ParameterId, _, Interface, constraint, _, _, _,
                         _, _, _), Rows),
              type_name(Interface, Constraint) ), Constraints),
    ( Constraints == [] -> Text = Name
    ; atomic_list_concat(Constraints, ' + ', Bound),
      format(atom(Text), '~w: ~w', [Name, Bound]) ).

rust_generic_property_text(Rows, _Ord-Name-TypeId, Text) :-
    rust_type(Rows, TypeId, Type),
    rust_field_name(Name, Field),
    format(string(Text), '    pub ~w: ~w,\n', [Field, Type]).

% A column named for a Rust keyword stops rustc at the declaration; the raw
% identifier escape is the spelling that keeps the wire name.
rust_field_name(Name, Field) :-
    (   rust_keyword(Name)
    ->  atom_concat('r#', Name, Field)
    ;   Field = Name
    ).

% `crate`, `self`, `Self` and `super` are absent: Rust bars them from the raw
% form, so no escape of this shape exists for them.
rust_keyword(abstract). rust_keyword(as).       rust_keyword(async).
rust_keyword(await).    rust_keyword(become).   rust_keyword(box).
rust_keyword(break).    rust_keyword(const).    rust_keyword(continue).
rust_keyword(do).       rust_keyword(dyn).      rust_keyword(else).
rust_keyword(enum).     rust_keyword(extern).   rust_keyword(false).
rust_keyword(final).    rust_keyword(fn).       rust_keyword(for).
rust_keyword(if).       rust_keyword(impl).     rust_keyword(in).
rust_keyword(let).      rust_keyword(loop).     rust_keyword(macro).
rust_keyword(match).    rust_keyword(mod).      rust_keyword(move).
rust_keyword(mut).      rust_keyword(override). rust_keyword(priv).
rust_keyword(pub).      rust_keyword(ref).      rust_keyword(return).
rust_keyword(static).   rust_keyword(struct).   rust_keyword(trait).
rust_keyword(true).     rust_keyword(try).      rust_keyword(type).
rust_keyword(typeof).   rust_keyword(unsafe).   rust_keyword(unsized).
rust_keyword(use).      rust_keyword(virtual).  rust_keyword(where).
rust_keyword(while).    rust_keyword(yield).

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

rust_rel_text(Rows, CollisionTypeNames, RelRow, Text) :-
    emitted_type_name(Rows, CollisionTypeNames, RelRow, TypeName),
    rel_columns(Rows, RelRow, Columns),
    maplist(rust_property_text(Rows, CollisionTypeNames), Columns, Properties),
    atomic_list_concat(Properties, '', Body),
    format(string(Text), '#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub struct ~w {\n~s}\n', [TypeName, Body]).

rust_property_text(Rows, CollisionTypeNames, _Ord-ColumnId-Name-TypeId, Text) :-
    rust_column_value_type(Rows, CollisionTypeNames, ColumnId, TypeId, Type),
    rust_field_name(Name, Field),
    format(string(Text), '    pub ~w: ~w,\n', [Field, Type]).

rust_column_type(Rows, _Ord-ColumnId-_Name-TypeId, Type) :-
    rust_column_value_type(Rows, [], ColumnId, TypeId, Type).

rust_column_value_type(Rows, CollisionTypeNames, ColumnId, TypeId, Type) :-
    ( rust_relation_id_storage(Rows, ColumnId)
    -> rust_relation_id_type(Rows, CollisionTypeNames, TypeId, Type)
    ; rust_type(Rows, CollisionTypeNames, TypeId, Type)
    ).

rust_relation_id_storage(Rows, ColumnId) :-
    member(row(_, ColumnId, _, relation_id, storage, _, _, _, _, _, _), Rows).

rust_relation_id_type(Rows, CollisionTypeNames, TargetId, Type) :-
    member(TargetRow, Rows),
    TargetRow = row(TargetId, _, _, _, rel, _, _, _, _, _, _),
    emitted_type_name(Rows, CollisionTypeNames, TargetRow, Target),
    format(string(Type), 'RelationId<~w>', [Target]).

rust_type(Rows, TypeId, Type) :- rust_type(Rows, [], TypeId, Type).

rust_type(Rows, CollisionTypeNames, TypeId, Type) :-
    member(TypeRow, Rows),
    TypeRow = row(TypeId, _, _, Name, Kind, ElementId, _, _, _, _, _),
    rust_kind(Rows, CollisionTypeNames, TypeRow, Name, Kind, ElementId, Type).

rust_kind(_Rows, _CollisionTypeNames, _TypeRow, int, primitive, _ElementId, 'i64').
rust_kind(_Rows, _CollisionTypeNames, _TypeRow, float, primitive, _ElementId, 'f64').
rust_kind(_Rows, _CollisionTypeNames, _TypeRow, text, primitive, _ElementId, 'String').
rust_kind(_Rows, _CollisionTypeNames, _TypeRow, bool, primitive, _ElementId, 'bool').
rust_kind(_Rows, _CollisionTypeNames, _TypeRow, json, primitive, _ElementId, 'serde_json::Value').
rust_kind(_Rows, _CollisionTypeNames, _TypeRow, Name, type_parameter, _ElementId, Name).
rust_kind(Rows, CollisionTypeNames, _TypeRow, _Name, json_list, ElementId, Type) :-
    rust_type(Rows, CollisionTypeNames, ElementId, Element),
    format(string(Type), 'Vec<~w>', [Element]).
% The relational list crosses the boundary as its elements, so the two list
% spellings render one Rust type.
rust_kind(Rows, CollisionTypeNames, _TypeRow, _Name, list, ElementId, Type) :-
    rust_type(Rows, CollisionTypeNames, ElementId, Element),
    format(string(Type), 'Vec<~w>', [Element]).
rust_kind(Rows, CollisionTypeNames, _TypeRow, _Name, relation_id_list,
          ElementId, Type) :-
    member(TargetRow, Rows),
    TargetRow = row(ElementId, _, _, _, rel, _, _, _, _, _, _),
    emitted_type_name(Rows, CollisionTypeNames, TargetRow, Target),
    format(string(Type), 'Vec<RelationId<~w>>', [Target]).
rust_kind(Rows, CollisionTypeNames, _TypeRow, _Name, option, ElementId, Type) :-
    rust_type(Rows, CollisionTypeNames, ElementId, Element),
    format(string(Type), 'DlOption<~w>', [Element]).
rust_kind(_Rows, _CollisionTypeNames, _TypeRow, Name, enum, _ElementId, Type) :-
    type_name(Name, Type).
rust_kind(Rows, CollisionTypeNames, TypeRow, _Name, rel, _ElementId, Type) :-
    emitted_type_name(Rows, CollisionTypeNames, TypeRow, Type).

emitted_type_name(Rows, CollisionTypeNames,
                  row(_, ParentId, _, Name, rel, _, _, ModuleId, _, _, _), Type) :-
    type_name(Name, BareType),
    (   memberchk(BareType, CollisionTypeNames)
    ->  rel_qualifier(Rows, ParentId, ModuleId, Qualifier),
        atom_concat(Qualifier, BareType, Type)
    ;   Type = BareType
    ).

% A dotted rel parents on its path rel, so a module-only prefix leaves two rels
% of one module colliding. The walk reaches the module row at the top only.
rel_qualifier(Rows, ParentId, _ModuleId, Qualifier) :-
    parent_rel_row(Rows, ParentId, ParentRow),
    !,
    ParentRow = row(_, GrandParentId, _, ParentName, rel, _, _,
                    ParentModuleId, _, _, _),
    type_name(ParentName, ParentType),
    rel_qualifier(Rows, GrandParentId, ParentModuleId, Above),
    atom_concat(Above, ParentType, Qualifier).
rel_qualifier(Rows, _ParentId, ModuleId, Qualifier) :-
    memberchk(row(ModuleId, _, _, ModuleName, module, _, _, _, _, _, _), Rows),
    module_type_name(ModuleName, Qualifier).

parent_rel_row(Rows, ParentId, ParentRow) :-
    ParentRow = row(ParentId, _, _, _, rel, _, _, _, _, _, _),
    memberchk(ParentRow, Rows).

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
