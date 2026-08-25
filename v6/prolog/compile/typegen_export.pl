% @comment-ok: module contract for the dl6 door, the single doc site for the row mapping.
% typegen_export.pl: the dl6 door beside 7_emit_ts_types.pl. Dumps the same
% semantic type rows the emitter consumes (lower:catalog_type_rows/6 then
% emit_jsonschema:option_rows/3, module-qualified) as one JSONL arrival per row
% for render_ts.dl6's EDB rel type_row/7, type_storage/5, type_pattern/3,
% schema_member/6, schema_member_role/4, schema_member_column/2,
% type_relation/3, type_relation_input/3, and type_relation_key/3, so the two doors
% cannot drift.
% row/11 -> type_row/7: id, parent, ordinal, name, kind, type_id, module_id
% (Arity and Hash are dropped). Constraint trailing patterns are emitted as
% ordered type_pattern rows, preserving the application pattern without
% changing the established type_row/7 wire shape. JSONL line shape:
%   {"rel":"type_row","sign":"add","row":[<id>,<parent>,<ordinal>,"<name>","<kind>",<type_id>,<module_id>]}
%   {"rel":"type_storage","sign":"add","row":[<id>,<column_id>,<ordinal>,"<local_name>",<module_id>]}
%   {"rel":"type_pattern","sign":"add","row":[<constraint_id>,<ordinal>,"<term>"]}
%   {"rel":"schema_member","sign":"add","row":[<member_id>,<owner_id>,<position>,"<name>","<authored_type>",<value_type_id>]}
%   {"rel":"schema_member_role","sign":"add","row":[<member_id>,<ordinal>,"<role>","<argument>"]}
%   {"rel":"schema_member_column","sign":"add","row":[<column_id>,<member_id>]}
%   {"rel":"type_relation","sign":"add","row":[<owner_id>,<self_member_id_or_empty>,<return_member_id_or_empty>]}
%   {"rel":"type_relation_input","sign":"add","row":[<owner_id>,<ordinal>,<member_id>]}
%   {"rel":"type_relation_key","sign":"add","row":[<owner_id>,<ordinal>,<member_id>]}
%   {"rel":"type_relation_owner","sign":"add","row":[<owner_id>,<module_id>,"<name>"]}

:- module(typegen_export, [ dump_type_rows/2, dump_fixture_rows/3,
                            dump_dl6_rows/3,
                            write_prolog_types/2 ]).

:- use_module(library(http/json)).
:- use_module(library(lists)).
:- use_module(library(pairs)).

:- use_module('../compile', [ program_plan/3 ]).
:- use_module('../compile', [ dl6_seeded_form/3 ]).
:- use_module('../lower', [ catalog_type_rows/6,
                            catalog_type_transport_rows/4,
                            catalog_type_relation_rows/3 ]).
:- use_module('../1_expansion/0_type_ids', [ semantic_type_id_text/2 ]).
:- use_module('4_emit_jsonschema', [ option_rows/3 ]).
:- use_module('7_emit_ts_types', [ ts_types_text/3 ]).
:- use_module('8_emit_rust_types', [ rust_type_relation_impl_texts/2,
                                      rust_type_relation_owner_name/3 ]).
:- use_module('../use_resolve', [ expand_uses/8 ]).

%! dump_type_rows(+CompiledProgram, +JsonlPath) is det.
%   CompiledProgram = plan(Name, prog(Decls, Rules), _, RelPlans, _, _, _, _, Mode).
dump_type_rows(plan(Name, prog(Decls, Rules), _Types, RelPlans, _, _, _, _, Mode),
               JsonlPath) :-
    catalog_type_rows(Mode, Name, Rules, RelPlans, Decls, Rows),
    option_rows(Decls, Rows, RowsOpt),
    catalog_type_relation_rows(Name, Decls, RelationRows),
    catalog_type_transport_rows(Name, RowsOpt, Decls, ChildRows),
    append([RowsOpt, RelationRows, ChildRows], MetadataRows),
    findall(type_relation_rust_impl(OwnerId, Text),
            rust_type_relation_impl_texts(MetadataRows, OwnerId-Text),
            RustImplRows),
    findall(type_relation_rust_name(OwnerId, TypeName),
            ( member(type_relation(OwnerId, _, _, _, _), RelationRows),
              rust_type_relation_owner_name(MetadataRows, OwnerId, TypeName) ),
            RustNameRows),
    append([MetadataRows, RustNameRows, RustImplRows], TransportRows),
    setup_call_cleanup(open(JsonlPath, write, Stream),
                       forall(member(Row, TransportRows),
                              write_row_line(Stream, Row)),
                       close(Stream)).

%! dump_fixture_rows(+FixtureFile, +FixtureName, +JsonlPath) is det.
%   Golden-driver door: plans FixtureName out of FixtureFile, then dumps.
dump_fixture_rows(FixtureFile, FixtureName, JsonlPath) :-
    open(FixtureFile, read, Stream),
    call_cleanup(read_fixture_term(Stream, FixtureName, Term-Bindings), close(Stream)),
    program_plan(Term-Bindings, [intern(dict)], Plan),
    dump_type_rows(Plan, JsonlPath).

%! dump_dl6_rows(+Dl6File, +FixtureName, +JsonlPath) is det.
%   The real-source door: parse and expand the authored .dl6 file, retain its
%   seeded rows, and send the resulting plan through the same catalog/typegen
%   path as dump_fixture_rows/3.  Keeping this here makes the CI receipt prove
%   parser -> use expansion -> catalog -> typegen rather than only exercising
%   hand-built fixture terms.
dump_dl6_rows(Dl6File, FixtureName, JsonlPath) :-
    expand_uses(Dl6File, [], [], _, Program, _, Bindings, Findings),
    ( Findings == [] -> true
    ; throw(unsupported_construct(surface_findings(Findings)))
    ),
    dl6_seeded_form(Program, Initial, SeededProgram),
    program_plan(fixture(FixtureName, SeededProgram, Initial, [], [])-Bindings,
                 [intern(dict)], Plan),
    dump_type_rows(Plan, JsonlPath).

read_fixture_term(Stream, Name, Out) :-
    fixture_ops,
    read_fixture_term_loop(Stream, Name, Out).

read_fixture_term_loop(Stream, Name, Out) :-
    read_term(Stream, Candidate, [variable_names(Bindings)]),
    (   Candidate == end_of_file
    ->  Out = fail, fail
    ;   Candidate = (:- Directive)
    ->  call(Directive), read_fixture_term_loop(Stream, Name, Out)
    ;   Candidate = fixture(Name0, _, _, _, _), Name0 == Name
    ->  Out = Candidate-Bindings
    ;   read_fixture_term_loop(Stream, Name, Out)
    ).

% @comment-ok: op/3-as-goal is the only form visible to read_term; module :- op is not.
fixture_ops :-
    op(1150, xfx, '<-'),
    op(1150, xfx, '<+'),
    op(700,  xfx, ':=').

%! write_prolog_types(+JsonlPath, +TextPath) is det.
%   Renders the JSONL back through the prolog emitter: one golden, both doors.
write_prolog_types(JsonlPath, TextPath) :-
    read_row_lines(JsonlPath, Rows),
    ts_types_text(rows, Rows, Text),
    setup_call_cleanup(open(TextPath, write, Stream),
                       format(Stream, '~s', [Text]),
                       close(Stream)).

read_row_lines(JsonlPath, Rows) :-
    setup_call_cleanup(open(JsonlPath, read, Stream),
                       read_row_lines_loop(Stream, RawRows),
                       close(Stream)),
    restore_pattern_rows(RawRows, Rows).

read_row_lines_loop(Stream, Rows) :-
    json_read_dict(Stream, Dict, [value_string_as(atom), end_of_file(@(end))]),
    (   Dict == @(end)
    ->  Rows = []
    ;   row_of_dict(Dict, Row),
        Rows = [Row | Rest],
        read_row_lines_loop(Stream, Rest)
    ).

% Arity, Hash and the trailing pair are not read by either type renderer.
row_of_dict(Dict, row(Id, Parent, Ordinal, Name, Kind, TypeId, 0, ModuleId,
                      '', '', '')) :-
    Dict.rel == type_row,
    _{ row: [Id, Parent, Ordinal, Name, Kind, TypeId, ModuleId] } :< Dict.
row_of_dict(Dict, row(Id, ColumnId, Ordinal, LocalName, storage, 0, 0,
                      ModuleId, '', '', '')) :-
    Dict.rel == type_storage,
    _{ row: [Id, ColumnId, Ordinal, LocalName, ModuleId] } :< Dict.
row_of_dict(Dict, type_pattern(Id, Ordinal, PatternText)) :-
    Dict.rel == type_pattern,
    _{ row: [Id, Ordinal, PatternText] } :< Dict.
row_of_dict(Dict, schema_member(MemberId, OwnerId, Position, Name,
                                AuthoredTypeText, ValueTypeId)) :-
    Dict.rel == schema_member,
    _{ row: [MemberIdText, OwnerIdText, Position, Name,
             AuthoredTypeText, ValueTypeIdText] } :< Dict,
    MemberId = MemberIdText,
    OwnerId = OwnerIdText,
    ValueTypeId = ValueTypeIdText.
row_of_dict(Dict, schema_member_role(MemberId, Ordinal, Role, Argument)) :-
    Dict.rel == schema_member_role,
    _{ row: [MemberId, Ordinal, Role, Argument] } :< Dict.
row_of_dict(Dict, schema_member_column(ColumnId, MemberId)) :-
    Dict.rel == schema_member_column,
    _{ row: [ColumnId, MemberId] } :< Dict.
row_of_dict(Dict, type_relation(OwnerId, SelfMemberId, ReturnMemberId)) :-
    Dict.rel == type_relation,
    _{ row: [OwnerId, SelfMemberId, ReturnMemberId] } :< Dict.
row_of_dict(Dict, type_relation_input(OwnerId, Ordinal, MemberId)) :-
    Dict.rel == type_relation_input,
    _{ row: [OwnerId, Ordinal, MemberId] } :< Dict.
row_of_dict(Dict, type_relation_key(OwnerId, Ordinal, MemberId)) :-
    Dict.rel == type_relation_key,
    _{ row: [OwnerId, Ordinal, MemberId] } :< Dict.
row_of_dict(Dict, type_relation_evidence(OwnerId, Evidence)) :-
    Dict.rel == type_relation_evidence,
    _{ row: [OwnerIdText, EvidenceText] } :< Dict,
    OwnerId = OwnerIdText,
    atom_to_term(EvidenceText, Evidence, []).
row_of_dict(Dict, type_relation_owner(OwnerId, ModuleId, Name)) :-
    Dict.rel == type_relation_owner,
    _{ row: [OwnerId, ModuleId, Name] } :< Dict.
row_of_dict(Dict, type_relation_rust_impl(OwnerId, Text)) :-
    Dict.rel == type_relation_rust_impl,
    _{ row: [OwnerId, Text] } :< Dict.
row_of_dict(Dict, type_relation_rust_name(OwnerId, Name)) :-
    Dict.rel == type_relation_rust_name,
    _{ row: [OwnerId, Name] } :< Dict.

restore_pattern_rows(RawRows, Rows) :-
    findall(Row,
            ( member(Row0, RawRows),
              restore_pattern_row(RawRows, Row0, Row) ),
            Rows).

restore_pattern_row(_RawRows, type_pattern(_, _, _), skip) :- !, fail.
restore_pattern_row(RawRows,
                    row(Id, Parent, Ordinal, Name, constraint, InterfaceId,
                        Arity, ModuleId, Hash, RuleHash, _),
                    row(Id, Parent, Ordinal, Name, constraint, InterfaceId,
                        Arity, ModuleId, Hash, RuleHash, Patterns)) :-
    !,
    findall(PatternOrdinal-Pattern,
            ( member(type_pattern(Id, PatternOrdinal, PatternText), RawRows),
              atom_to_term(PatternText, Pattern, []) ),
            PatternPairs),
    keysort(PatternPairs, Ordered),
    pairs_values(Ordered, Patterns).
restore_pattern_row(_RawRows, Row, Row).

write_row_line(Stream, row(Id, ColumnId, Ordinal, LocalName, storage, _TypeId,
                           _Arity, ModuleId, _Hash, _, _)) :-
    !,
    json_write_dict(Stream,
                    _{ rel: type_storage, sign: add,
                       row: [Id, ColumnId, Ordinal, LocalName, ModuleId] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream,
               schema_member(MemberId, OwnerId, Position, Name, AuthoredType,
                             ValueTypeId, _Roles)) :-
    boundary_id_text(MemberId, MemberIdText),
    boundary_id_text(OwnerId, OwnerIdText),
    term_string(AuthoredType, AuthoredTypeText,
                [quoted(false), portray(false)]),
    boundary_id_text(ValueTypeId, ValueTypeIdText),
    json_write_dict(Stream,
                    _{ rel: schema_member, sign: add,
                       row: [MemberIdText, OwnerIdText, Position, Name,
                             AuthoredTypeText, ValueTypeIdText] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream,
               schema_member(MemberId, OwnerId, Position, Name,
                             AuthoredTypeText, ValueTypeId)) :-
    boundary_id_text(MemberId, MemberIdText),
    boundary_id_text(OwnerId, OwnerIdText),
    boundary_id_text(ValueTypeId, ValueTypeIdText),
    json_write_dict(Stream,
                    _{ rel: schema_member, sign: add,
                       row: [MemberIdText, OwnerIdText, Position, Name,
                             AuthoredTypeText, ValueTypeIdText] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream,
               type_relation(OwnerId, SelfMemberId, _InputMemberIds,
                             ReturnMemberId, _KeyMemberIds)) :-
    boundary_id_text(OwnerId, OwnerIdText),
    optional_boundary_id_text(SelfMemberId, SelfMemberText),
    optional_boundary_id_text(ReturnMemberId, ReturnMemberText),
    json_write_dict(Stream,
                    _{ rel: type_relation, sign: add,
                       row: [OwnerIdText, SelfMemberText, ReturnMemberText] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream,
               schema_member_column(ColumnId, MemberId)) :-
    boundary_id_text(MemberId, MemberIdText),
    json_write_dict(Stream,
                    _{ rel: schema_member_column, sign: add,
                       row: [ColumnId, MemberIdText] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream,
               schema_member_role(MemberId, Ordinal, Role, Argument)) :-
    boundary_id_text(MemberId, MemberIdText),
    transport_argument_text(Argument, ArgumentText),
    json_write_dict(Stream,
                    _{ rel: schema_member_role, sign: add,
                       row: [MemberIdText, Ordinal, Role, ArgumentText] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream,
               type_relation(OwnerId, SelfMemberId, ReturnMemberId)) :-
    boundary_id_text(OwnerId, OwnerIdText),
    optional_boundary_id_text(SelfMemberId, SelfMemberText),
    optional_boundary_id_text(ReturnMemberId, ReturnMemberText),
    json_write_dict(Stream,
                    _{ rel: type_relation, sign: add,
                       row: [OwnerIdText, SelfMemberText, ReturnMemberText] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream, type_relation_evidence(OwnerId, Evidence)) :-
    boundary_id_text(OwnerId, OwnerIdText),
    term_string(Evidence, EvidenceText,
                [quoted(true), portray(false)]),
    json_write_dict(Stream,
                    _{ rel: type_relation_evidence, sign: add,
                       row: [OwnerIdText, EvidenceText] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream, type_relation_owner(OwnerId, ModuleId, Name)) :-
    boundary_id_text(OwnerId, OwnerIdText),
    json_write_dict(Stream,
                    _{ rel: type_relation_owner, sign: add,
                       row: [OwnerIdText, ModuleId, Name] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream, type_relation_rust_impl(OwnerId, Text)) :-
    boundary_id_text(OwnerId, OwnerIdText),
    json_write_dict(Stream,
                    _{ rel: type_relation_rust_impl, sign: add,
                       row: [OwnerIdText, Text] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream, type_relation_rust_name(OwnerId, Name)) :-
    boundary_id_text(OwnerId, OwnerIdText),
    json_write_dict(Stream,
                    _{ rel: type_relation_rust_name, sign: add,
                       row: [OwnerIdText, Name] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream, type_relation_input(OwnerId, Ordinal, MemberId)) :-
    boundary_id_text(OwnerId, OwnerIdText),
    boundary_id_text(MemberId, MemberIdText),
    json_write_dict(Stream,
                    _{ rel: type_relation_input, sign: add,
                       row: [OwnerIdText, Ordinal, MemberIdText] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream, type_relation_key(OwnerId, Ordinal, MemberId)) :-
    boundary_id_text(OwnerId, OwnerIdText),
    boundary_id_text(MemberId, MemberIdText),
    json_write_dict(Stream,
                    _{ rel: type_relation_key, sign: add,
                       row: [OwnerIdText, Ordinal, MemberIdText] },
                    [width(0)]),
    format(Stream, '\n', []).
write_row_line(Stream, row(Id, Parent, Ordinal, Name, constraint,
                           InterfaceId, Arity, ModuleId, Hash, RuleHash, Patterns)) :-
    !,
    write_constraint_type_row(Stream,
                              row(Id, Parent, Ordinal, Name, constraint,
                                  InterfaceId, Arity, ModuleId, Hash,
                                  RuleHash, Patterns)),
    write_pattern_lines(Stream, Id, Patterns, 1).
write_row_line(Stream, row(Id, Parent, Ordinal, Name, Kind, TypeId, _Arity,
                           ModuleId, _Hash, _, _)) :-
    json_write_dict(Stream,
                    _{ rel: type_row, sign: add,
                       row: [Id, Parent, Ordinal, Name, Kind, TypeId, ModuleId] },
                    [width(0)]),
    format(Stream, '\n', []).

boundary_id_text(Text, Text) :- string(Text), !.
boundary_id_text(Text, Text) :-
    atom(Text),
    atom_length(Text, 64),
    atom_chars(Text, Chars),
    maplist(hex_char, Chars),
    !.
boundary_id_text(Id, Text) :- semantic_type_id_text(Id, Text).

hex_char(Char) :-
    char_code(Char, Code),
    ( between(0'0, 0'9, Code)
    ; between(0'a, 0'f, Code)
    ; between(0'A, 0'F, Code)
    ).

optional_boundary_id_text(none, '') :- !.
optional_boundary_id_text('', '') :- !.
optional_boundary_id_text(Id, Text) :- boundary_id_text(Id, Text).

transport_argument_text('', '') :- !.
transport_argument_text(Argument, Text) :-
    term_string(Argument, Text, [quoted(false), portray(false)]).

write_pattern_lines(_Stream, _Id, [], _Ordinal).
write_pattern_lines(_Stream, _Id, '', _Ordinal).
write_pattern_lines(Stream, Id, [Pattern | Rest], Ordinal) :-
    term_to_atom(Pattern, PatternText),
    json_write_dict(Stream,
                    _{ rel: type_pattern, sign: add,
                       row: [Id, Ordinal, PatternText] },
                    [width(0)]),
    format(Stream, '\n', []),
    NextOrdinal is Ordinal + 1,
    write_pattern_lines(Stream, Id, Rest, NextOrdinal).

write_constraint_type_row(Stream,
                          row(Id, Parent, Ordinal, Name, Kind, TypeId,
                              _Arity, ModuleId, _Hash, _, _Patterns)) :-
    json_write_dict(Stream,
                    _{ rel: type_row, sign: add,
                       row: [Id, Parent, Ordinal, Name, Kind, TypeId, ModuleId] },
                    [width(0)]),
    format(Stream, '\n', []).
