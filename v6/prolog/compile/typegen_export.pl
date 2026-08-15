% @comment-ok: module contract for the dl6 door, the single doc site for the row mapping.
% typegen_export.pl: the dl6 door beside 7_emit_ts_types.pl. Dumps the same
% semantic type rows the emitter consumes (lower:catalog_decl_rows/6 then
% emit_jsonschema:option_rows/3, module-qualified) as one JSONL arrival per row
% for render_ts.dl6's EDB rel type_row/7, so the two doors cannot drift.
% row/11 -> type_row/7: id, parent, ordinal, name, kind, type_id, module_id
% (Arity, Hash and the trailing pair are dropped). JSONL line shape:
%   {"rel":"type_row","sign":"add","row":[<id>,<parent>,<ordinal>,"<name>","<kind>",<type_id>,<module_id>]}

:- module(typegen_export, [ dump_type_rows/2, dump_fixture_rows/3,
                            write_prolog_types/2 ]).

:- use_module(library(http/json)).
:- use_module(library(lists)).

:- use_module('../compile', [ program_plan/3 ]).
:- use_module('../lower', [ catalog_decl_rows/6 ]).
:- use_module('4_emit_jsonschema', [ option_rows/3 ]).
:- use_module('7_emit_ts_types', [ ts_types_text/3 ]).

%! dump_type_rows(+CompiledProgram, +JsonlPath) is det.
%   CompiledProgram = plan(Name, prog(Decls, Rules), _, RelPlans, _, _, _, _, _).
dump_type_rows(plan(Name, prog(Decls, Rules), _Types, RelPlans, _, _, _, _, _),
               JsonlPath) :-
    catalog_decl_rows(Name, Rules, RelPlans, Decls, Rows, _),
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
                       read_row_lines_loop(Stream, Rows),
                       close(Stream)).

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
    _{ row: [Id, Parent, Ordinal, Name, Kind, TypeId, ModuleId] } :< Dict.

write_row_line(Stream, row(Id, Parent, Ordinal, Name, Kind, TypeId, _Arity,
                           ModuleId, _Hash, _, _)) :-
    json_write_dict(Stream,
                    _{ rel: type_row, sign: add,
                       row: [Id, Parent, Ordinal, Name, Kind, TypeId, ModuleId] },
                    [width(0)]),
    format(Stream, '\n', []).
