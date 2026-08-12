:- module(emit_rust_types, [ rust_types_text/3, emit_rust_types/3 ]).

:- use_module(library(lists)).

rust_types_text(_Name, Rows, Text) :-
    findall(RelRow, renderable_rel(Rows, RelRow), RelRows),
    collision_type_names(RelRows, CollisionTypeNames),
    maplist(rust_rel_text(Rows, CollisionTypeNames), RelRows, Parts),
    atomic_list_concat(Parts, '\n', Atom),
    atom_string(Atom, Text).

emit_rust_types(Name, Rows, Path) :-
    rust_types_text(Name, Rows, Text),
    setup_call_cleanup(open(Path, write, Stream), format(Stream, '~s', [Text]), close(Stream)).

renderable_rel(Rows, RelRow) :-
    member(RelRow, Rows),
    RelRow = row(_, _, _, Name, rel, _, _, _ModuleId, _, _, _),
    \+ compiler_helper_rel(Name),
    rel_columns(Rows, RelRow, Columns),
    maplist(rust_column_type(Rows), Columns, _).

compiler_helper_rel(Name) :- sub_atom(Name, _, _, _, '__').

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
    findall(Ord-Name-TypeId,
            member(row(_, RelId, Ord, Name, column, TypeId, _, _, _, _, _), Rows),
            Unsorted),
    keysort(Unsorted, Columns).

rust_rel_text(Rows, CollisionTypeNames, RelRow, Text) :-
    emitted_type_name(Rows, CollisionTypeNames, RelRow, TypeName),
    rel_columns(Rows, RelRow, Columns),
    maplist(rust_property_text(Rows, CollisionTypeNames), Columns, Properties),
    atomic_list_concat(Properties, '', Body),
    format(string(Text), '#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub struct ~w {\n~s}\n', [TypeName, Body]).

rust_property_text(Rows, CollisionTypeNames, _Ord-Name-TypeId, Text) :-
    rust_type(Rows, CollisionTypeNames, TypeId, Type),
    format(string(Text), '    pub ~w: ~w,\n', [Name, Type]).

rust_column_type(Rows, _Ord-_Name-TypeId, Type) :- rust_type(Rows, TypeId, Type).

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
rust_kind(Rows, CollisionTypeNames, _TypeRow, _Name, json_list, ElementId, Type) :-
    rust_type(Rows, CollisionTypeNames, ElementId, Element),
    format(string(Type), 'Vec<~w>', [Element]).
rust_kind(Rows, CollisionTypeNames, _TypeRow, _Name, option, ElementId, Type) :-
    rust_type(Rows, CollisionTypeNames, ElementId, Element),
    format(string(Type), 'Option<~w>', [Element]).
rust_kind(Rows, CollisionTypeNames, TypeRow, _Name, rel, _ElementId, Type) :-
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

type_name(Name, Type) :-
    atomic_list_concat(Parts, '_', Name),
    maplist(capitalized, Parts, Capitals),
    atomic_list_concat(Capitals, '', Type).

capitalized(Part, Capital) :-
    atom_chars(Part, [First | Rest]),
    upcase_atom(First, Upper),
    atom_chars(Capital, [Upper | Rest]).
