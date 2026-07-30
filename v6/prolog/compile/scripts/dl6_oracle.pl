% dl6_oracle.pl : run the REFERENCE ENGINE over a `.dl6` text program and a
% JSON arrival schedule, printing the shared tick-log envelope.
%
% Why this exists (runtime-bridge arc, plans/2026-07-29-runtime-bridge-header.md
% scope 4): the schedule-fed byte grading stays the referee for the SERVED
% engine too. oracle_dump.pl covers every conformance FIXTURE, whose program is
% a prolog term and whose schedule is a prolog list. A served program is `.dl6`
% TEXT and its schedule is the very JSON the http client posts. This file closes
% that gap by reading both in their served form and handing them to
% conformance/ticklog.pl's own `print_ticklog/3` -- the same predicate
% oracle_dump.pl calls, unedited, so the two logs are produced by one printer.
%
% The schedule file is the SAME file the http client posts from (the
% out/<name>.schedule.json shape sweep.pl already writes):
%
%   [ [ {"rel":"event","sign":"add","row":[1,"boot"]} ], ... ]
%
% VALUE MAPPING, stated because prolog makes it a real choice: a JSON number
% becomes an integer and a JSON string becomes an ATOM, not a prolog string.
% Atoms and strings both serialize as JSON strings on the way out (ticklog.pl
% `value_json/2`), so the printed log is identical either way -- but joins are
% not. The compiler's own generated text (a probe's witness digest, built by
% concatenation in 1_host_expand.pl) is an ATOM, so a schedule that fed a host
% response's witness column as a string would silently fail to join and the
% derived rows would vanish. Measured, not assumed: with strings, the served
% receipt (b) log carried `answered` at its third tick and the oracle's did not.
% The remaining edge, named rather than fixed: a program whose RULES carry
% double-quoted string literals compared against a world-fed column needs its
% schedule expressed as a fixture term instead.
%
% ── the mapping is TYPE-DIRECTED at a `json` column ──────────────────────────
%
% The rule above is right for every SCALAR column and wrong for a json one,
% and it was wrong SILENTLY: a program declaring `rel event(payload: json)`
% and destructuring it with a brace pattern derived NOTHING through this door
% while its term-door twin derived every row. Both schedule spellings failed,
% for two different reasons:
%
%   "row": [ {"repo":"cli","stars":4} ]      the object fell to term_to_atom/2
%                                            and became the ATOM `#{repo:...}`,
%                                            a SWI dict's text, which no
%                                            pattern can ever match
%   "row": [ "{\"repo\":\"cli\"}" ]          the string became an atom and
%                                            STAYED an atom: json_decode/2
%                                            wants obj(Pairs), never text
%
% The shape a json column takes is not this file's choice to make -- the
% EMITTER door already fixed it, deliberately and in writing:
%
%   serve/4_http.ts:columnProblem   "`json` takes exactly what `text` takes,
%                                    and deliberately so ... A json document
%                                    arrives as its text."
%   compile/sweep.pl:arrival_value_json/4  writes a json column's schedule
%                                    entry as a JSON STRING carrying
%                                    canonical_json_text/2 of the value
%
% So a json column's schedule value is JSON TEXT, and this door PARSES it into
% the oracle's own json terms (obj(SortedPairs) / list / number / atom), which
% is exactly what the term-door fixtures feed. Non-json columns keep the
% mapping above, byte for byte.
%
% A non-scalar value at a json column is a NAMED REFUSAL rather than a
% structural map, and the refusal is the point: accepting `{"repo":"cli"}` as
% a raw object here would make this door accept a schedule the served door
% answers 400 to, which is the divergence-by-unilateral-fix the json_flex
% verdict named (its Q2). Widening the arrival grammar to take a raw object is
% a real option, but it is ONE decision taken on BOTH doors at once, not a
% convenience added to the referee.
%
% Run: swipl -q -l dl6_oracle.pl -g "oracle('p.dl6','s.json')" -g halt

:- ensure_loaded('../../conformance/ticklog').
:- use_module('../parse_dl', [parse_dl_file/4]).
:- use_module('../analyze', [rel_columns/5]).
:- use_module(library(http/json)).

oracle(Dl6File, ScheduleFile) :-
    parse_dl_file(Dl6File, Prog, Bindings, Findings),
    ( Findings == []
    -> true
    ;  format(user_error, "dl6_oracle: parse findings ~q~n", [Findings]), halt(1)
    ),
    read_schedule(Prog, Bindings, ScheduleFile, Schedule),
    print_ticklog(Prog, [], Schedule).

read_schedule(Prog, Bindings, ScheduleFile, Schedule) :-
    setup_call_cleanup(
        open(ScheduleFile, read, Stream),
        json_read_dict(Stream, Batches, [value_string_as(string)]),
        close(Stream)),
    maplist(batch_terms(Prog, Bindings), Batches, Schedule).

batch_terms(Prog, Bindings, Batch, Terms) :-
    maplist(arrival_term(Prog, Bindings), Batch, Terms).

arrival_term(Prog, Bindings, Arrival, Term) :-
    atom_string(Rel, Arrival.rel),
    length(Arrival.row, Arity),
    arrival_column_types(Prog, Bindings, Rel/Arity, ColumnTypes),
    maplist(schedule_value(Rel), ColumnTypes, Arrival.row, Args),
    Atom =.. [Rel | Args],
    ( Arrival.sign == "add" -> Term = +Atom
    ; Arrival.sign == "del" -> Term = -Atom
    ; format(user_error, "dl6_oracle: unknown sign ~q~n", [Arrival.sign]), halt(1)
    ).

% The DECLARED type per column position, through analyze.pl's own
% rel_columns/5 rather than a private read of the decl list: column ORDER is
% significant (ruling decl_column_spelling = colon_typed_ordered_columns) and
% a second implementation of that order is a second place for it to drift.
% A rel with no decl at all, or one whose columns this resolver cannot name,
% types every position `none` and therefore keeps the pre-existing mapping.
arrival_column_types(Prog, Bindings, Ref, ColumnTypes) :-
    Ref = _Name/Arity,
    (   program_decls_rules(Prog, Decls, Rules),
        catch(rel_columns(Decls, Rules, Bindings, Ref, Columns), _, fail),
        length(Columns, Arity)
    ->  maplist(declared_column_type(Decls, Ref), Columns, ColumnTypes)
    ;   length(ColumnTypes, Arity),
        maplist(=(none), ColumnTypes)
    ).

% BOTH program shapes parse_dl_file/4 can answer with. A file carrying hosts
% or queries comes back as program/3, and reading only prog/2 here made every
% hosts program's schedule silently fall through -- measured as three served
% tests failing with a bare `dl6_oracle failed (1)` and no message, because
% the whole predicate just FAILED.
program_decls_rules(prog(Decls, Rules), Decls, Rules).
program_decls_rules(program(Decls, Rules, _Queries), Decls, Rules).

declared_column_type(Decls, Ref, Column, Type) :-
    ( memberchk(col_type(Ref, Column, Declared), Decls) -> Type = Declared
    ; Type = none
    ).

schedule_value(Rel, json, Value, Term) :- !, json_column_term(Rel, Value, Term).
schedule_value(_, _, Value, Value) :- integer(Value), !.
schedule_value(_, _, Value, Atom) :- string(Value), !, atom_string(Atom, Value).
schedule_value(_, _, Value, Atom) :- term_to_atom(Value, Atom).

% A json column's arrival is its document TEXT (the served door's own words).
% A bare JSON number is text too as far as SQLite is concerned -- it binds
% into the TEXT column as its digits and `json_valid` accepts it -- so the two
% scalar shapes the http door lets through are the two shapes accepted here.
json_column_term(_, Value, Term) :- string(Value), !,
    json_text_term(Value, Term).
json_column_term(_, Value, Value) :- number(Value), !.
json_column_term(Rel, Value, _) :-
    format(user_error,
           "dl6_oracle: '~w' json column takes a json document as text, got ~q~n",
           [Rel, Value]),
    halt(1).

json_text_term(Text, Term) :-
    (   catch(atom_json_dict(Text, Parsed, [value_string_as(string)]), Error, true)
    ->  ( var(Error) -> json_parsed_term(Parsed, Term)
        ; format(user_error, "dl6_oracle: json column value is not valid json: ~q~n",
                 [Text]), halt(1) )
    ;   format(user_error, "dl6_oracle: json column value is not valid json: ~q~n",
               [Text]), halt(1)
    ).

% json_read_dict's own value kinds -> the oracle's json terms. `value_string_
% as(string)` is what keeps this total: with atoms, a document's STRING "true"
% and its literal `true` would arrive as the same term and the boolean arm
% would silently swallow one of them.
json_parsed_term(Parsed, obj(Sorted)) :- is_dict(Parsed), !,
    dict_pairs(Parsed, _, Pairs),
    findall(Key-Value,
            ( member(Key-Raw, Pairs), json_parsed_term(Raw, Value) ),
            Converted),
    keysort(Converted, Sorted).
json_parsed_term(Parsed, Terms) :- is_list(Parsed), !,
    maplist(json_parsed_term, Parsed, Terms).
json_parsed_term(Parsed, Atom) :- string(Parsed), !, atom_string(Atom, Parsed).
json_parsed_term(true, bool_lit(true)) :- !.
json_parsed_term(false, bool_lit(false)) :- !.
% json null becomes the atom `none`, which is what the oracle's json plane
% already uses for it (body.pl's `Value \== none` guard). It is an AMBIGUOUS
% stand-in -- the ordinary string "none" is the same term -- and that
% ambiguity is json_flex card C3, open and untouched here.
json_parsed_term(null, none) :- !.
json_parsed_term(Parsed, Parsed).
