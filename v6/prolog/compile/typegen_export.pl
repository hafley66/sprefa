% @comment-ok: module contract for the dl6 door, the single doc site for the row mapping.
% typegen_export.pl: the dl6 door beside 7_emit_ts_types.pl. Dumps the same
% semantic type rows the emitter consumes (lower:catalog_type_rows/6 then
% emit_jsonschema:option_rows/3, module-qualified) as one JSONL arrival per row
% for render_ts.dl6's EDB rel type_row/7, type_storage/5, and type_pattern/3, so the two doors
% cannot drift.
% row/11 -> type_row/7: id, parent, ordinal, name, kind, type_id, module_id
% (Arity and Hash are dropped). Constraint trailing patterns are emitted as
% ordered type_pattern rows, preserving the application pattern without
% changing the established type_row/7 wire shape. JSONL line shape:
%   {"rel":"type_row","sign":"add","row":[<id>,<parent>,<ordinal>,"<name>","<kind>",<type_id>,<module_id>]}
%   {"rel":"type_storage","sign":"add","row":[<id>,<column_id>,<ordinal>,"<local_name>",<module_id>]}
%   {"rel":"type_pattern","sign":"add","row":[<constraint_id>,<ordinal>,"<term>"]}

:- module(typegen_export, [ dump_type_rows/2, dump_fixture_rows/3,
                            write_prolog_types/2 ]).

:- use_module(library(http/json)).
:- use_module(library(lists)).
:- use_module(library(pairs)).

:- use_module('../compile', [ program_plan/3 ]).
:- use_module('../lower', [ catalog_type_rows/6 ]).
:- use_module('4_emit_jsonschema', [ option_rows/3 ]).
:- use_module('7_emit_ts_types', [ ts_types_text/3 ]).

%! dump_type_rows(+CompiledProgram, +JsonlPath) is det.
%   CompiledProgram = plan(Name, prog(Decls, Rules), _, RelPlans, _, _, _, _, Mode).
dump_type_rows(plan(Name, prog(Decls, Rules), _Types, RelPlans, _, _, _, _, Mode),
               JsonlPath) :-
    catalog_type_rows(Mode, Name, Rules, RelPlans, Decls, Rows),
    option_rows(Decls, Rows, RowsOpt),
    setup_call_cleanup(open(JsonlPath, write, Stream),
                       forall(member(Row, RowsOpt), write_row_line(Stream, Row)),
                       close(Stream)).

%! dump_fixture_rows(+FixtureFile, +FixtureName, +JsonlPath) is det.
%   Golden-driver door: plans FixtureName out of FixtureFile, then dumps.
dump_fixture_rows(FixtureFile, FixtureName, JsonlPath) :-
    open(FixtureFile, read, Stream),
    call_cleanup(read_fixture_term(Stream, FixtureName, Term-Bindings), close(Stream)),
    program_plan(Term-Bindings, [intern(dict)], Plan),
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
