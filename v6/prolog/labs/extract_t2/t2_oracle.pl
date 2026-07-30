% t2_oracle.pl : compile/scripts/dl6_oracle.pl, made TYPE-DIRECTED, so that a
% real schema DOCUMENT can be fed to the reference engine through the same
% schedule file the served engine reads.
%
%   swipl -q -l t2_oracle.pl -g "oracle('p.dl6','s.schedule.json')" -g halt
%
% ── why this file exists (lab finding D1) ────────────────────────────────────
%
% There are two schedule writers in the repo and they disagree about how a
% `json` column's value is spelled.
%
%   compile/sweep.pl:arrival_value_json/4   json column -> a JSON STRING whose
%                                           text is the canonical document
%   compile/scripts/dl6_oracle.pl:schedule_value/2
%                                           every JSON string -> an ATOM
%
% sweep.pl is type-directed and right; dl6_oracle.pl is not type-directed at
% all. So a document that reaches the served engine correctly reaches the
% reference engine as an OPAQUE ATOM, and since that engine has no JSON text
% parser (json-flex verdict section 5: "the oracle has NO json text parser"),
% every `decode/2` against it derives nothing -- silently, with a clean empty
% tick log and exit 0. A door that answers "no rows" for a well-formed program
% over a well-formed document is worse than one that refuses.
%
% This file consults the parsed program's own `col_type/3` declarations, in
% declared order, and parses the schedule string for a `json` column into the
% braces term the engine destructures -- the same term shape a conformance
% fixture writes by hand. Every other column keeps dl6_oracle's mapping
% unchanged, including the reason its header gives for atoms over strings.
%
% NOT COVERED, named rather than papered: `ref(Type)` columns, whose schedule
% entry sweep.pl writes as a nested JSON object (struct-as-rows). No program in
% this lab declares one. A schedule that carried one would fall through to the
% scalar clauses and bind an atom.
%
% The proposed fix to dl6_oracle.pl is this file's schedule mapping, verbatim.

:- ensure_loaded('../../conformance/ticklog').
:- use_module('../../compile/parse_dl', [parse_dl_file/4]).
:- use_module(library(http/json)).

oracle(Dl6File, ScheduleFile) :-
    parse_dl_file(Dl6File, Prog, _Bindings, Findings),
    ( Findings == []
    -> true
    ;  format(user_error, "t2_oracle: parse findings ~q~n", [Findings]), halt(1)
    ),
    Prog = prog(Decls, _Rules),
    read_schedule(Decls, ScheduleFile, Schedule),
    print_ticklog(Prog, [], Schedule).

read_schedule(Decls, ScheduleFile, Schedule) :-
    setup_call_cleanup(
        open(ScheduleFile, read, Stream),
        json_read_dict(Stream, Batches, [value_string_as(string)]),
        close(Stream)),
    maplist(batch_terms(Decls), Batches, Schedule).

batch_terms(Decls, Batch, Terms) :- maplist(arrival_term(Decls), Batch, Terms).

arrival_term(Decls, Arrival, Term) :-
    atom_string(Rel, Arrival.rel),
    length(Arrival.row, Arity),
    column_types(Decls, Rel/Arity, ColumnTypes),
    maplist(schedule_value, ColumnTypes, Arrival.row, Args),
    Atom =.. [Rel | Args],
    ( Arrival.sign == "add" -> Term = +Atom
    ; Arrival.sign == "del" -> Term = -Atom
    ; format(user_error, "t2_oracle: unknown sign ~q~n", [Arrival.sign]), halt(1)
    ).

% col_type/3 declarations arrive in DECLARED COLUMN ORDER (parse_dl emits one
% per column, left to right), so collecting them is the ordered type list.
% A rel with no declaration at all is a legal EDB by the edb_definition ruling;
% its columns get `unknown`, which takes the scalar path.
column_types(Decls, Ref, Types) :-
    findall(Type, member(col_type(Ref, _Column, Type), Decls), Declared),
    ( Declared == []
    -> Ref = _/Arity, length(Types, Arity), maplist(=(unknown), Types)
    ;  Types = Declared
    ).

% ── the type-directed clause: a json column's string IS a document ───────────
schedule_value(json, Value, Term) :- !,
    ( string(Value) -> Text = Value
    ; atom(Value)   -> Text = Value
    ; format(user_error, "t2_oracle: json column value is not text: ~q~n", [Value]), halt(1)
    ),
    atom_string(TextAtom, Text),
    atom_json_term(TextAtom, Parsed, [value_string_as(string)]),
    json_term(Parsed, Term).
% ── every other column: dl6_oracle's own mapping, unchanged ──────────────────
schedule_value(_, Value, Value) :- integer(Value), !.
schedule_value(_, Value, Value) :- float(Value), !.
schedule_value(_, Value, Atom)  :- string(Value), !, atom_string(Atom, Value).
schedule_value(_, Value, Atom)  :- term_to_atom(Value, Atom).

% json_term/2 : library(http/json) classic term -> the term form the engine
% destructures. json_read_dict is deliberately NOT used here: atom_json_term's
% classic representation (json([Key=Value,...])) keeps SOURCE KEY ORDER, and
% the oracle's json_canon/2 sorts on the way in, so the two doors sort once
% each rather than disagreeing about which sort ran.
json_term(@(null),  none) :- !.
json_term(@(true),  bool_lit(true)) :- !.
json_term(@(false), bool_lit(false)) :- !.
json_term(json([]), '{}') :- !.
json_term(json(Pairs), '{}'(Chain)) :- !,
    maplist(json_field, Pairs, Fields),
    comma_chain(Fields, Chain).
json_term(List, Terms) :- is_list(List), !, maplist(json_term, List, Terms).
json_term(Value, Value) :- integer(Value), !.
json_term(Value, Value) :- float(Value), !.
json_term(Value, Atom)  :- string(Value), !, atom_string(Atom, Value).
json_term(Value, Value).

json_field(Key = Value, Key: Term) :- json_term(Value, Term).

comma_chain([One], One) :- !.
comma_chain([Head | Tail], (Head, Chain)) :- comma_chain(Tail, Chain).
