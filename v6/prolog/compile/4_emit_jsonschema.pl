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
    referenced_helper_rows(Rows, RelRows, HelperRows),
    append(RelRows, HelperRows, DefRows),
    maplist(rel_def_pair(Rows, RefPrefix), DefRows, Pairs).

% A minted rel an author rel points at still receives a `$ref`, so the `__`
% filter alone leaves that pointer without a target.
referenced_helper_rows(Rows, SeedRows, HelperRows) :-
    maplist(rel_row_id, SeedRows, SeedIds0),
    sort(SeedIds0, SeedIds),
    closure_rel_ids(Rows, SeedIds, SeedIds, AllIds),
    subtract(AllIds, SeedIds, HelperIds),
    findall(HelperRow,
            ( member(HelperId, HelperIds),
              once(( member(HelperRow, Rows),
                     HelperRow = row(HelperId, _, _, _, rel, _, _, _, _, _, _) )) ),
            HelperRows).

rel_row_id(row(RelId, _, _, _, rel, _, _, _, _, _, _), RelId).

closure_rel_ids(_Rows, [], Seen, Seen).
closure_rel_ids(Rows, [RelId | Queue0], Seen0, Seen) :-
    findall(TargetId, referenced_rel_id(Rows, RelId, TargetId), TargetIds0),
    sort(TargetIds0, TargetIds),
    subtract(TargetIds, Seen0, Fresh),
    append(Seen0, Fresh, Seen1),
    append(Queue0, Fresh, Queue),
    closure_rel_ids(Rows, Queue, Seen1, Seen).

referenced_rel_id(Rows, RelId, TargetId) :-
    member(row(_, RelId, _, _, column, ColumnTypeId, _, _, _, _, _), Rows),
    rel_type_id(Rows, ColumnTypeId, TargetId).

rel_type_id(Rows, TypeId, TargetId) :-
    once(row_at(Rows, TypeId, TypeRow)),
    TypeRow = row(_, _, _, _, Kind, ElementTypeId, _, _, _, _, _),
    (   Kind == rel
    ->  TargetId = TypeId
    ;   wrapper_kind(Kind),
        rel_type_id(Rows, ElementTypeId, TargetId)
    ).

wrapper_kind(list).
wrapper_kind(relation_id_list).
wrapper_kind(json_list).
wrapper_kind(option).

% The `__` namespace is compiler-owned (option enums, list companions,
% ref-option split rels); authors cannot spell it, so the marker identifies a minted helper.
compiler_helper_rel(LocalName) :-
    sub_atom(LocalName, _, _, _, '__').

% option_rows(+Decls, +Rows0, -Rows): the catalog lost option(T) when expansion
% erased it, so fold the author's option columns back into `option` kind rows.
%
% Type signature and timeline:
%   option_rows(+ExpandedDecls, +CatalogRows, -CatalogRowsWithWrappers) is det.
%   option(option(T)) receives two synthetic rows, inner then outer. The parent
%   column points at the outer row. This preserves the storage timeline
%   none / some(none) / some(some(T)) for all target emitters.
option_rows(Decls, Rows0, Rows) :-
    option_surface_types(Decls, Elements),
    wrapper_type_closure(Elements, OptionTypes),
    validate_option_render_depth(OptionTypes),
    option_enum_names(Decls, Elements, OptionEnumNames),
    direct_enum_names(Decls, DirectEnumNames),
    list_to_set(OptionEnumNames, OptionEnumSet),
    list_to_set(DirectEnumNames, DirectEnumSet),
    subtract(DirectEnumSet, OptionEnumSet, OnlyDirect),
    append(OptionEnumNames, OnlyDirect, EnumNames),
    max_row_id(Rows0, MaxId),
    enum_row_ids(EnumNames, MaxId, EnumToId, EnumRows, AfterEnums),
    enum_variant_rows(Decls, Rows0, EnumToId, AfterEnums, EnumVariantRows,
                      AfterVariants),
    option_row_ids(OptionTypes, AfterVariants, EnumToId, OptionToId, OptionRows, _),
    append([EnumRows, EnumVariantRows, OptionRows], WrapperRows),
    maplist(rewrite_option_column(Decls, Rows0, OptionToId), Rows0, Rewritten0),
    maplist(rewrite_enum_column(Decls, Rows0, EnumToId), Rewritten0, Rewritten),
    append(Rewritten, WrapperRows, Rows).

% Direct enum columns (a col_type typed as an enum, not wrapped in option) are
% collected from the enum_column/3 markers enum expansion records. This is what
% lets a concrete generic sum, minted and lowered like a nominal enum, emit as a
% tagged union.
direct_enum_names(Decls, Names) :-
    findall(Name,
            ( member(enum_column(_, _, Name), Decls),
              semantic_enum_name(Decls, Name) ),
            Found),
    sort(Found, Names).

% The column's declared type was the enum; point it at the enum row so the
% emitter renders `state: Status` rather than `state: number`.
rewrite_enum_column(Decls, Rows0, EnumToId, Row0, Row) :-
    Row0 = row(Id, RelId, Ord, Name, column, _TypeId, Arity, ModuleId, HId, HS, HR),
    enum_column_element(Decls, Rows0, RelId, Name, EnumName),
    memberchk(EnumName-EnumId, EnumToId),
    !,
    Row = row(Id, RelId, Ord, Name, column, EnumId, Arity, ModuleId, HId, HS, HR).
rewrite_enum_column(_Decls, _Rows0, _EnumToId, Row, Row).

enum_column_element(Decls, Rows0, RelId, ColumnName, EnumName) :-
    member(enum_column(RelName/_, ColumnName, EnumName), Decls),
    semantic_enum_name(Decls, EnumName),
    member(row(RelId, _, _, RelName, rel, _, _, _, _, _, _), Rows0).

scalar_option_element(text).
scalar_option_element(int).
scalar_option_element(float).
scalar_option_element(bool).
scalar_option_element(json).

max_row_id(Rows, MaxId) :-
    findall(Id, member(row(Id, _, _, _, _, _, _, _, _, _, _), Rows), Ids),
    max_list(Ids, MaxId).

option_surface_types(Decls, Elements) :-
    findall(Element,
            ( member(option_column(_, _, Element), Decls),
              option_surface_value(Decls, Element) ),
            Found),
    sort(Found, Elements).

option_surface_value(_, Element) :- scalar_option_element(Element), !.
option_surface_value(_, option(_)) :- !.
option_surface_value(Decls, Element) :-
    atom(Element), semantic_enum_name(Decls, Element).

wrapper_type_closure(Elements, Types) :-
    findall(Type,
            ( member(Element, Elements), option_type_member(Element, Type) ),
            Found),
    sort(Found, Unordered),
    order_option_types(Unordered, Types).

option_type_member(Element, option(Element)).
option_type_member(option(Inner), Type) :- option_type_member(Inner, Type).

% Every inner option precedes its direct parent. Sorting inside a depth makes
% ids deterministic when several columns introduce independent wrapper trees.
order_option_types(Types, Ordered) :-
    findall(Depth-Type, (member(Type, Types), option_depth(Type, Depth)), Pairs),
    keysort(Pairs, Sorted),
    pairs_values(Sorted, Ordered).

option_depth(option(Inner), Depth) :-
    ( Inner = option(_) -> option_depth(Inner, InnerDepth), Depth is InnerDepth + 1
    ; Depth = 1 ).

% Both served DL6 renderers unroll this finite type-only recurrence. The
% catalog rejects a deeper source type before either emitter can omit a field.
% The bound applies only to generated target declarations; SQLite's enum-id
% storage and option expansion recurse over the complete finite source term.
option_type_render_depth_limit(5).

validate_option_render_depth(Types) :-
    option_type_render_depth_limit(Limit),
    ( member(Type, Types), option_depth(Type, Depth), Depth > Limit
    -> throw(unsupported_construct(type_emitter_option_depth(Type, Limit)))
    ; true ).

option_enum_names(Decls, Elements, Names) :-
    findall(Name,
            ( member(Element, Elements), atom(Element),
              semantic_enum_name(Decls, Element), Name = Element ),
            Found),
    sort(Found, Names).

semantic_enum_name(Decls, Name) :-
    member(semantic_type_rows(SemanticRows), Decls),
    member(declaration(_, root, Name, enum, compile_time), SemanticRows).

enum_row_ids([], Id, [], [], Id).
enum_row_ids([Name | Rest], Id0, [Name-Id | Map],
             [row(Id, 0, 0, Name, enum, 0, 0, 0, '', '', '') | Rows], IdFinal) :-
    Id1 is Id0 + 1,
    Id = Id1,
    enum_row_ids(Rest, Id1, Map, Rows, IdFinal).

enum_variant_rows(Decls, Rows0, EnumToId, Id0, Rows, IdFinal) :-
    findall(EnumName-Ordinal-VariantName-VariantRelId,
            semantic_enum_variant(Decls, Rows0, EnumToId, EnumName, Ordinal,
                                  VariantName, VariantRelId),
            Unordered),
    keysort(Unordered, Ordered),
    enum_variant_rows_(Ordered, EnumToId, Id0, Rows, IdFinal).

semantic_enum_variant(Decls, Rows0, EnumToId, EnumName, Ordinal, VariantName,
                      VariantRelId) :-
    member(EnumName-_, EnumToId),
    member(semantic_type_rows(SemanticRows), Decls),
    member(declaration(EnumSemanticId, root, EnumName, enum, compile_time),
           SemanticRows),
    member(member(_, EnumSemanticId, Ordinal, VariantName,
                  type_ref(declaration(VariantSemanticId))), SemanticRows),
    member(declaration(VariantSemanticId, _, VariantRelName, relation, _),
           SemanticRows),
    member(row(VariantRelId, _, _, VariantRelName, rel, _, _, _, _, _, _), Rows0).

enum_variant_rows_([], _, Id, [], Id).
enum_variant_rows_([EnumName-Ordinal-VariantName-VariantRelId | Rest], EnumToId,
                   Id0,
                   [row(Id, EnumId, Ordinal, VariantName, enum_variant,
                        VariantRelId, 0, 0, '', '', '') | Rows], IdFinal) :-
    Id is Id0 + 1,
    memberchk(EnumName-EnumId, EnumToId),
    enum_variant_rows_(Rest, EnumToId, Id, Rows, IdFinal).

option_row_ids(Types, Id0, EnumToId, Map, Rows, IdFinal) :-
    option_id_map(Types, Id0, Map, IdFinal),
    option_rows_from_map(Types, EnumToId, Map, Rows).

option_id_map([], Id, [], Id).
option_id_map([Type | Rest], Id0, [Type-Id | Map], IdFinal) :-
    Id is Id0 + 1,
    option_id_map(Rest, Id, Map, IdFinal).

option_rows_from_map([], _, _, []).
option_rows_from_map([Type | Rest], EnumToId, OptionToId, [Row | More]) :-
    memberchk(Type-Id, OptionToId),
    Type = option(Element),
    option_element_type_id(Element, EnumToId, OptionToId, ElementId),
    term_string(Type, Name),
    Row = row(Id, 0, 0, Name, option, ElementId, 0, 0, '', '', ''),
    option_rows_from_map(Rest, EnumToId, OptionToId, More).

option_element_type_id(Element, _, _, ElementId) :- option_element_id(Element, ElementId), !.
option_element_type_id(Element, EnumToId, _, ElementId) :-
    atom(Element), memberchk(Element-ElementId, EnumToId), !.
option_element_type_id(Element, _, OptionToId, ElementId) :-
    memberchk(Element-ElementId, OptionToId).

option_element_id(text, 1).
option_element_id(int, 2).
option_element_id(float, 3).
option_element_id(bool, 4).
option_element_id(json, 5).

rewrite_option_column(Decls, Rows0, OptionToId, Row0, Row) :-
    Row0 = row(Id, RelId, Ord, Name, column, _TypeId, Arity, ModuleId, HId, HS, HR),
    option_column_element(Decls, Rows0, RelId, Name, Element),
    memberchk(option(Element)-OptId, OptionToId),
    !,
    Row = row(Id, RelId, Ord, Name, column, OptId, Arity, ModuleId, HId, HS, HR).
rewrite_option_column(_Decls, _Rows0, _ElementToId, Row, Row).

option_column_element(Decls, Rows0, RelId, ColumnName, Element) :-
    member(option_column(RelName/_, ColumnName, Element), Decls),
    option_surface_value(Decls, Element),
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
    findall(Ord-ColumnId-ColumnName-ColumnTypeId,
            member(row(ColumnId, RelId, Ord, ColumnName, column, ColumnTypeId,
                       _, _, _, _, _), Rows),
            Triples0),
    keysort(Triples0, Triples),
    maplist(column_property(Rows, RefPrefix), Triples, Pairs),
    pairs_keys(Pairs, Required),
    dict_pairs(Properties, properties, Pairs),
    Schema = _{ type: object,
                properties: Properties,
                required: Required,
                additionalProperties: false }.

column_property(Rows, RefPrefix, _Ord-ColumnId-ColumnName-ColumnTypeId,
                ColumnName-Schema) :-
    ( relation_id_storage(Rows, ColumnId)
    -> relation_id_schema(Rows, ColumnTypeId, Schema)
    ; column_schema(Rows, RefPrefix, ColumnTypeId, Schema)
    ).

relation_id_storage(Rows, ColumnId) :-
    member(row(_, ColumnId, _, relation_id, storage, _, _, _, _, _, _), Rows).

relation_id_schema(Rows, TargetId,
                   _{ type: integer, '$comment': Comment }) :-
    row_at(Rows, TargetId, row(_, _, _, TargetName, rel, _, _, _, _, _, _)),
    format(atom(Comment), 'DL6 relation identity for ~w', [TargetName]).

column_schema(Rows, RefPrefix, ColumnTypeId, Schema) :-
    row_at(Rows, ColumnTypeId, TargetRow),
    TargetRow = row(_, _, _, Name, Kind, ElementTypeId, _, _, _, _, _),
    kind_schema(Rows, RefPrefix, TargetRow, Name, Kind, ElementTypeId, Schema).

kind_schema(_Rows, _Prefix, _Target, Name, primitive, _Element, Schema) :-
    primitive_schema(Name, Schema).
kind_schema(Rows, Prefix, _Target, _Name, list, ElementTypeId, Schema) :-
    column_schema(Rows, Prefix, ElementTypeId, ItemSchema),
    Schema = _{ type: array, items: ItemSchema }.
kind_schema(Rows, _Prefix, _Target, _Name, relation_id_list, ElementTypeId,
            Schema) :-
    relation_id_schema(Rows, ElementTypeId, ItemSchema),
    Schema = _{ type: array, items: ItemSchema }.
kind_schema(Rows, Prefix, _Target, _Name, json_list, ElementTypeId, Schema) :-
    column_schema(Rows, Prefix, ElementTypeId, ItemSchema),
    Schema = _{ type: array, items: ItemSchema }.
kind_schema(Rows, Prefix, _Target, _Name, option, ElementTypeId, Schema) :-
    column_schema(Rows, Prefix, ElementTypeId, Inner),
    % The tagged wire form is recursive: none, some(none), and some(value)
    % remain different JSON documents for option(option(T)).
    Schema = _{ anyOf: [
        _{ type: object,
           properties: _{ tag: _{ const: none } },
           required: [tag], additionalProperties: false },
        _{ type: object,
           properties: _{ tag: _{ const: some }, value: Inner },
           required: [tag, value], additionalProperties: false }
    ] }.
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
