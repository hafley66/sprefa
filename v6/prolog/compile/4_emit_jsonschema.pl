:- module(emit_jsonschema,
          [ jsonschema_text/3,
            emit_jsonschema/3,
            jsonschema_document/3,
            module_defs/4,
            entry_module_details/4,
            option_rows/3
          ]).

:- use_module(library(http/json)).
:- use_module(library(lists)).
:- use_module(library(pairs)).

jsonschema_text(Name, Rows, Text) :-
    jsonschema_document(Name, Rows, Doc),
    with_output_to(string(Body),
                   json_write_dict(current_output, Doc, [width(78), step(2), tab(200)])),
    format(string(Text), '~w\n', [Body]).

emit_jsonschema(Name, Rows, Path) :-
    jsonschema_text(Name, Rows, Text),
    setup_call_cleanup(open(Path, write, Stream),
                       format(Stream, '~s', [Text]),
                       close(Stream)).

jsonschema_document(Name, Rows, Doc) :-
    entry_module_details(Name, Rows, ModuleId, Hash),
    format(atom(Id), '~w#~w', [Name, Hash]),
    module_defs(ModuleId, Rows, '#/$defs/', Pairs),
    dict_pairs(Defs, defs, Pairs),
    Doc = _{ '$schema': 'https://json-schema.org/draft/2020-12/schema',
             '$id': Id,
             '$defs': Defs }.

entry_module_details(Name, Rows, ModuleId, Hash) :-
    entry_module_row(Rows, Name, ModuleRow),
    ModuleRow = row(ModuleId, _, _, _, module, _, _, _, Hash, _, _).

module_defs(ModuleId, Rows, RefPrefix, Pairs) :-
    findall(RelRow,
            ( member(RelRow, Rows),
              RelRow = row(_, _, _, LocalName, rel, _, _, ModuleId, _, _, _),
              \+ compiler_helper_rel(LocalName) ),
            RelRows),
    maplist(rel_def_pair(Rows, RefPrefix), RelRows, Pairs).

% The `__` namespace is compiler-owned (option enums, list companions,
% ref-option split rels); authors cannot spell it, so the marker identifies a minted helper.
compiler_helper_rel(LocalName) :-
    sub_atom(LocalName, _, _, _, '__').

% option_rows(+Decls, +Rows0, -Rows): the catalog lost option(T) when expansion
% erased it, so fold the author's option columns back into `option` kind rows.
option_rows(Decls, Rows0, Rows) :-
    findall(Element,
            ( member(option_column(_, _, Element), Decls),
              scalar_option_element(Element) ),
            Elems0),
    sort(Elems0, Elems),
    max_row_id(Rows0, MaxId),
    option_row_ids(Elems, MaxId, ElementToId, OptionRows),
    maplist(rewrite_option_column(Decls, Rows0, ElementToId), Rows0, Rewritten),
    append(Rewritten, OptionRows, Rows).

scalar_option_element(text).
scalar_option_element(int).
scalar_option_element(float).
scalar_option_element(bool).
scalar_option_element(json).

max_row_id(Rows, MaxId) :-
    findall(Id, member(row(Id, _, _, _, _, _, _, _, _, _, _), Rows), Ids),
    max_list(Ids, MaxId).

option_row_ids([], _Id, [], []).
option_row_ids([Element | Rest], Id0, [Element-Id | Map], [Row | More]) :-
    Id is Id0 + 1,
    option_element_id(Element, ElementId),
    Row = row(Id, 0, 0, Element, option, ElementId, 0, 0, '', '', ''),
    option_row_ids(Rest, Id, Map, More).

option_element_id(text, 1).
option_element_id(int, 2).
option_element_id(float, 3).
option_element_id(bool, 4).
option_element_id(json, 5).

rewrite_option_column(Decls, Rows0, ElementToId, Row0, Row) :-
    Row0 = row(Id, RelId, Ord, Name, column, TypeId, Arity, ModuleId, HId, HS, HR),
    option_column_element(Decls, Rows0, RelId, Name, Element),
    memberchk(Element-OptId, ElementToId),
    !,
    Row = row(Id, RelId, Ord, Name, column, OptId, Arity, ModuleId, HId, HS, HR).
rewrite_option_column(_Decls, _Rows0, _ElementToId, Row, Row).

option_column_element(Decls, Rows0, RelId, ColumnName, Element) :-
    member(option_column(RelName/_, ColumnName, Element), Decls),
    scalar_option_element(Element),
    member(row(RelId, _, _, RelName, rel, _, _, _, _, _, _), Rows0).


entry_module_row(Rows, Name, Row) :-
    member(Row, Rows),
    Row = row(_, _, _, Name, module, _, _, _, _, _, _).
entry_module_row(Rows, _Name, Row) :-
    member(Row, Rows),
    Row = row(_, 0, _, _, module, _, _, _, _, _, _).

rel_def_pair(Rows, RefPrefix, RelRow, Key-Schema) :-
    rel_path(Rows, RelRow, Path),
    atomic_list_concat(Path, '.', Key),
    rel_object(Rows, RefPrefix, RelRow, Schema).

rel_object(Rows, RefPrefix, RelRow, Schema) :-
    RelRow = row(RelId, _, _, _, _, _, _, _, _, _, _),
    findall(Ord-ColumnName-ColumnTypeId,
            member(row(_, RelId, Ord, ColumnName, column, ColumnTypeId, _, _, _, _, _), Rows),
            Triples0),
    keysort(Triples0, Triples),
    maplist(column_property(Rows, RefPrefix), Triples, Pairs),
    exclude(nullable_pair, Pairs, RequiredPairs),
    pairs_keys(RequiredPairs, Required),
    dict_pairs(Properties, properties, Pairs),
    Schema = _{ type: object,
                properties: Properties,
                required: Required,
                additionalProperties: false }.

column_property(Rows, RefPrefix, _Ord-ColumnName-ColumnTypeId, ColumnName-Schema) :-
    column_schema(Rows, RefPrefix, ColumnTypeId, Schema).

nullable_pair(_Name-_{ anyOf: _ }).

column_schema(Rows, RefPrefix, ColumnTypeId, Schema) :-
    row_at(Rows, ColumnTypeId, TargetRow),
    TargetRow = row(_, _, _, Name, Kind, ElementTypeId, _, _, _, _, _),
    kind_schema(Rows, RefPrefix, TargetRow, Name, Kind, ElementTypeId, Schema).

kind_schema(_Rows, _Prefix, _Target, Name, primitive, _Element, Schema) :-
    primitive_schema(Name, Schema).
kind_schema(Rows, Prefix, _Target, _Name, list, ElementTypeId, Schema) :-
    column_schema(Rows, Prefix, ElementTypeId, ItemSchema),
    Schema = _{ type: array, items: ItemSchema }.
kind_schema(Rows, Prefix, _Target, _Name, json_list, ElementTypeId, Schema) :-
    column_schema(Rows, Prefix, ElementTypeId, ItemSchema),
    Schema = _{ type: array, items: ItemSchema }.
kind_schema(Rows, Prefix, _Target, _Name, option, ElementTypeId, Schema) :-
    column_schema(Rows, Prefix, ElementTypeId, Inner),
    Schema = _{ anyOf: [ Inner, _{ type: null } ] }.
kind_schema(Rows, RefPrefix, TargetRow, _Name, rel, _Element, Schema) :-
    rel_path(Rows, TargetRow, Path),
    atomic_list_concat(Path, '.', Pointer),
    atomic_list_concat([RefPrefix, Pointer], Ref),
    Schema = _{ '$ref': Ref }.

primitive_schema(int,    _{ type: integer }).
primitive_schema(float,  _{ type: number }).
primitive_schema(text,   _{ type: string }).
primitive_schema(bool,   _{ type: boolean }).
primitive_schema(json,   _{}).

row_at(Rows, RelId, Row) :-
    member(Row, Rows),
    Row = row(RelId, _, _, _, _, _, _, _, _, _, _).

rel_path(Rows, RelRow, Path) :-
    RelRow = row(_, ParentId, _, Name, rel, _, _, _, _, _, _),
    (   parent_rel_row(Rows, ParentId, ParentRow)
    ->  rel_path(Rows, ParentRow, ParentPath),
        append(ParentPath, [Name], Path)
    ;   Path = [Name]
    ).

parent_rel_row(Rows, ParentId, ParentRow) :-
    member(ParentRow, Rows),
    ParentRow = row(ParentId, _, _, _, rel, _, _, _, _, _, _).
