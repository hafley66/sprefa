% ticklog.pl : the oracle-side tick-log printer for the tsv2 compile-target
% arc (plans/2026-07-27-tsv2-compile-target-header.md, phase A). Loads the
% existing reference engine UNCHANGED (via go.pl, which loads engine.pl and
% every fixtures/*.pl) and prints the same JSONL envelope the tsv2 runtime
% prints, so the two logs can be diffed byte-for-byte. NEVER edits engine.pl,
% go.pl, body.pl, level_eval.pl, or any fixtures/*.pl.
%
% Envelope (both sides agree on this exact text):
%   {"tick":N,"deltas":{"relName":{"add":[[..],...],"del":[[..],...]}}}
% Rel names ascending; only rels with a nonempty add or del; a tick with no
% deltas still emits its line with "deltas":{}; rows are JSON arrays of
% column values in the row's own argument order (an atom's arity IS its
% declared column order — Prolog terms carry no separate column-name
% metadata). A json value is a value produced by body.pl's JSON semantics:
% decode/2 and json_each/2 yield canonical JSON scalars, lists, or obj(Pairs),
% braces literals yield the same canonical forms, json_array/1 yields a list,
% and json_object/2 yields obj(Pairs). Lists, raw braces literals, and
% obj(Pairs) are encoded as JSON recursively.
% Their canonical text has no whitespace, object keys in sorted order, and
% array elements in their semantic order. Integers remain JSON numbers. Plain
% compound terms keep canonical term text as JSON strings.
% add/del each sorted lexicographically by their own JSON text; no spaces, LF
% line endings (format's ~n), no trailing whitespace.
%
% Usage:
%   swipl -q -l v6/prolog/conformance/ticklog.pl \
%         -g "emit(demand_laziness_effect_rows)" -g halt
%   swipl -q -l v6/prolog/conformance/ticklog.pl \
%         -g "emit_perturbed(demand_laziness_effect_rows)" -g halt

:- ensure_loaded(go).   % pulls in engine.pl + every fixtures/*.pl, unedited
:- use_module(body, [json_canon/2, rel_ref/2]).   % read-only reuse; body.pl is untouched

% ═══ perturbed schedules (HARD RULE receipt: proves the tsv2 side computes
% deltas from the rules rather than replaying the fixture's own expected
% answers — same program/Decls/Rules/Initial as the fixture, one extra tick
% appended with a brand new session/target the fixture's Expectations never
% mention). Mirrors tests/schedules.ts's DEMAND_LAZINESS_SCHEDULE_PERTURBED
% on the tsv2 side. ═══════════════════════════════════════════════════════

perturbed_schedule(demand_laziness_effect_rows, Schedule) :-
    fixture(demand_laziness_effect_rows, _, _, BaseSchedule, _),
    append(BaseSchedule, [ [ +open_feed(session_four, gamma) ] ], Schedule).

% ═══ entry points ═══════════════════════════════════════════════════════════

%% emit(+Name) is det.
%  Run fixture Name's own canned Schedule and print its tick log.
emit(Name) :-
    ( fixture(Name, Prog, Initial, Schedule, _)
    -> print_ticklog(Prog, Initial, Schedule)
    ;  format(user_error, "ticklog: unknown fixture ~w~n", [Name]), halt(1)
    ).

%% emit(+Name, +Schedule) is det.
%  Run fixture Name's own Prog/Initial against a CALLER-SUPPLIED Schedule
%  (the perturbed-run hook), and print its tick log.
emit(Name, Schedule) :-
    ( fixture(Name, Prog, Initial, _, _)
    -> print_ticklog(Prog, Initial, Schedule)
    ;  format(user_error, "ticklog: unknown fixture ~w~n", [Name]), halt(1)
    ).

%% emit_perturbed(+Name) is det.
%  emit/2 against this file's perturbed_schedule/2 table.
emit_perturbed(Name) :-
    ( perturbed_schedule(Name, Schedule)
    -> emit(Name, Schedule)
    ;  format(user_error, "ticklog: no perturbed_schedule/2 for ~w~n", [Name]), halt(1)
    ).

print_ticklog(Prog, Initial, Schedule) :-
    run_program(Prog, Initial, Schedule, _FinalAll, DeltaTicks),
    print_tick_lines(1, DeltaTicks).

print_tick_lines(_, []).
print_tick_lines(Tick, [Deltas | Rest]) :-
    tick_line(Tick, Deltas, Line),
    format('~w~n', [Line]),
    NextTick is Tick + 1,
    print_tick_lines(NextTick, Rest).

% ═══ envelope formatting ════════════════════════════════════════════════════

tick_line(Tick, Deltas, Line) :-
    findall(Ref, ( member(Delta, Deltas), delta_row_ref(Delta, Ref) ), Refs0),
    sort(Refs0, Refs),   % Name/Arity standard order sorts by Name first: "rel names ascending"
    findall(RelJson, ( member(Ref, Refs), rel_delta_json(Ref, Deltas, RelJson) ), RelJsons),
    atomic_list_concat(RelJsons, ',', DeltasInner),
    format(atom(Line), '{"tick":~w,"deltas":{~w}}', [Tick, DeltasInner]).

delta_row_ref(+Row, Ref) :- rel_ref(Row, Ref).
delta_row_ref(-Row, Ref) :- rel_ref(Row, Ref).

rel_delta_json(Name/Arity, Deltas, Json) :-
    findall(Row, ( member(+Row, Deltas), rel_ref(Row, Name/Arity) ), AddRows),
    findall(Row, ( member(-Row, Deltas), rel_ref(Row, Name/Arity) ), DelRows),
    maplist(row_json, AddRows, AddJsonsRaw), msort(AddJsonsRaw, AddJsons),
    maplist(row_json, DelRows, DelJsonsRaw), msort(DelJsonsRaw, DelJsons),
    atomic_list_concat(AddJsons, ',', AddInner),
    atomic_list_concat(DelJsons, ',', DelInner),
    format(atom(Json), '"~w":{"add":[~w],"del":[~w]}', [Name, AddInner, DelInner]).

row_json(Row, Json) :-
    Row =.. [_ | Args],
    maplist(value_json, Args, ArgJsons),
    atomic_list_concat(ArgJsons, ',', Inner),
    format(atom(Json), '[~w]', [Inner]).

value_json(Value, Json) :- integer(Value), !, format(atom(Json), '~w', [Value]).
value_json(bool_lit(Boolean), Json) :- !, format(atom(Json), '~w', [Boolean]).
value_json(Value, Json) :- float(Value), !, finite_float_json(Value, Json).
value_json(Value, Json) :- json_value_term(Value), !, json_value_json(Value, Json).
value_json(Value, Json) :- compound(Value), !, term_text(Value, Text), string_json(Text, Json).
value_json(Value, Json) :- string_json(Value, Json).

finite_float_json(Value, Json) :-
    float_class(Value, Class),
    memberchk(Class, [normal, subnormal, zero]),
    ( Value =:= 0.0
    -> Json = '0'
    ; format(atom(Raw), '~h', [Value]),
      normalize_float_json_atom(Raw, Json)
    ).

normalize_float_json_atom(Raw, Text) :-
    ( sub_atom(Raw, Before, 2, After, '.0'),
      ( After =:= 0
      ; Start is Before + 2,
        sub_atom(Raw, Start, _, 0, Exponent),
        ( sub_atom(Exponent, 0, 1, _, 'e')
        ; sub_atom(Exponent, 0, 1, _, 'E') ) )
    -> sub_atom(Raw, 0, Before, _, Prefix),
       Start2 is Before + 2,
       sub_atom(Raw, Start2, After, 0, Suffix),
       atom_concat(Prefix, Suffix, Text)
    ; Text = Raw
    ).

% The forms below are the landed JSON representation from body.pl: lists for
% arrays and obj(SortedPairs) for objects. A braces literal is still written
% as {}(Fields) in raw fixture/host rows and is canonicalized here before
% rendering. A plain compound term reaches the following term_text/2 clause.
% The empty object is the ATOM `{}` on both doors (parse_dl.pl braces_term/5;
% term_to_atom reads `{}` at arity 0). Without this pair it would fall through
% to string_json/2 and render as the JSON STRING "{}" rather than an object.
json_value_term('{}') :- !.
json_value_term(Value) :- Value = {}(_), !.
json_value_term(Value) :- is_list(Value), !.
json_value_term(obj(Pairs)) :- is_list(Pairs).

json_value_json('{}', '{}') :- !.
json_value_json({}(Fields), Json) :- !,
    json_canon({}(Fields), Canon),
    json_value_json(Canon, Json).
json_value_json(List, Json) :- is_list(List), !,
    maplist(value_json, List, Values),
    atomic_list_concat(Values, ',', Inner),
    format(atom(Json), '[~w]', [Inner]).
json_value_json(obj(Pairs), Json) :-
    keysort(Pairs, SortedPairs),
    maplist(json_object_entry, SortedPairs, Entries),
    atomic_list_concat(Entries, ',', Inner),
    format(atom(Json), '{~w}', [Inner]).

json_object_entry(Key-Value, Json) :-
    string_json(Key, KeyJson),
    value_json(Value, ValueJson),
    format(atom(Json), '~w:~w', [KeyJson, ValueJson]).

% A compound term's canonical prolog text form, e.g. route_data(settings) ->
% 'route_data(settings)'. Recurses so a deeper compound argument (not
% exercised by the two phase-A fixtures) still renders correctly.
term_text(Value, Text) :- atomic(Value), !, format(atom(Text), '~w', [Value]).
term_text(Value, Text) :- compound(Value), !,
    Value =.. [Name | Args],
    maplist(term_text, Args, ArgTexts),
    atomic_list_concat(ArgTexts, ',', Inner),
    format(atom(Text), '~w(~w)', [Name, Inner]).

string_json(Value, Json) :-
    format(atom(Raw), '~w', [Value]),
    atom_codes(Raw, Codes),
    escape_json_codes(Codes, EscapedCodes),
    atom_codes(Escaped, EscapedCodes),
    format(atom(Json), '"~w"', [Escaped]).

escape_json_codes([], []).
escape_json_codes([Code | Rest], Out) :-
    ( Code =:= 0'"  -> Escaped = [0'\\, 0'"]
    ; Code =:= 0'\\ -> Escaped = [0'\\, 0'\\]
    ; Code =:= 10   -> Escaped = [0'\\, 0'n]
    ; Code =:= 9    -> Escaped = [0'\\, 0't]
    ; Code < 32     -> format(atom(HexAtom), '\\u~`0t~16r~4|', [Code]), atom_codes(HexAtom, Escaped)
    ; Escaped = [Code]
    ),
    escape_json_codes(Rest, RestOut),
    append(Escaped, RestOut, Out).
