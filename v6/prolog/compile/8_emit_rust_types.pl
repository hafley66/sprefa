:- module(emit_rust_types, [ rust_types_text/3, emit_rust_types/3 ]).

:- use_module(library(lists)).

rust_types_text(_Name, Rows, Text) :-
    findall(RelRow, renderable_rel(Rows, RelRow), RelRows),
    maplist(rust_rel_text(Rows), RelRows, Parts),
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

rel_columns(Rows, row(RelId, _, _, _, rel, _, _, _, _, _, _), Columns) :-
    findall(Ord-Name-TypeId,
            member(row(_, RelId, Ord, Name, column, TypeId, _, _, _, _, _), Rows),
            Unsorted),
    keysort(Unsorted, Columns).

rust_rel_text(Rows, RelRow, Text) :-
    RelRow = row(_, _, _, Name, rel, _, _, _, _, _, _),
    type_name(Name, TypeName),
    rel_columns(Rows, RelRow, Columns),
    maplist(rust_property_text(Rows), Columns, Properties),
    atomic_list_concat(Properties, '', Body),
    format(string(Text), '#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]\npub struct ~w {\n~s}\n', [TypeName, Body]).

rust_property_text(Rows, _Ord-Name-TypeId, Text) :-
    rust_type(Rows, TypeId, Type),
    format(string(Text), '    pub ~w: ~w,\n', [Name, Type]).

rust_column_type(Rows, _Ord-_Name-TypeId, Type) :- rust_type(Rows, TypeId, Type).

rust_type(Rows, TypeId, Type) :-
    member(row(TypeId, _, _, Name, Kind, ElementId, _, _, _, _, _), Rows),
    rust_kind(Rows, Name, Kind, ElementId, Type).

rust_kind(_Rows, int, primitive, _ElementId, 'i64').
rust_kind(_Rows, float, primitive, _ElementId, 'f64').
rust_kind(_Rows, text, primitive, _ElementId, 'String').
rust_kind(_Rows, bool, primitive, _ElementId, 'bool').
rust_kind(_Rows, json, primitive, _ElementId, 'serde_json::Value').
rust_kind(Rows, _Name, json_list, ElementId, Type) :-
    rust_type(Rows, ElementId, Element),
    format(string(Type), 'Vec<~w>', [Element]).
rust_kind(Rows, _Name, option, ElementId, Type) :-
    rust_type(Rows, ElementId, Element),
    format(string(Type), 'Option<~w>', [Element]).
rust_kind(_Rows, Name, rel, _ElementId, Type) :- type_name(Name, Type).

type_name(Name, Type) :-
    atomic_list_concat(Parts, '_', Name),
    maplist(capitalized, Parts, Capitals),
    atomic_list_concat(Capitals, '', Type).

capitalized(Part, Capital) :-
    atom_chars(Part, [First | Rest]),
    upcase_atom(First, Upper),
    atom_chars(Capital, [Upper | Rest]).
