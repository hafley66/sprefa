% golden_oracle.pl : the golden-flex grading door -- run the REFERENCE ENGINE
% over a `.dl6` TEXT program and a JSON arrival schedule, printing BOTH legs
% the sweep grades on:
%
%   oracle_ticklog(File, ScheduleFile)  one {"tick":N,"deltas":{...}} line per tick
%   oracle_final(File, ScheduleFile)    one {"final":{...}} line
%
% WHY THIS EXISTS BESIDE dl6_oracle.pl, rather than as an edit to it. Two gaps,
% both measured while writing v6/dl/fixtures/golden-flex.dl6:
%
%  (1) NO FINAL-STATE LEG. `dl6_oracle.pl:oracle/2` calls `print_ticklog/3`,
%      which throws FinalAll away. `oracle_dump.pl` has the final-state encoder
%      but only over `fixture/5` terms, so a `.dl6` TEXT program has no way to
%      earn the grade an empty-or-short schedule needs. `final_state_line/2`
%      below is oracle_dump.pl's own predicate, reused by loading that file, not
%      reimplemented.
%
%  (2) OBJECT VALUES CANNOT CROSS THE JSON SCHEDULE DOOR. `dl6_oracle.pl`'s
%      `schedule_value/2` ends in `term_to_atom(Value, Atom)`, so a JSON object
%      in an arrival row becomes the ATOM of a SWI dict's printed text. Measured
%      against a two-deep struct program:
%
%        type_arrival_shape_mismatch(tree/2, site, patch,
%          not_an_object(patch, '#{at: #{col:3,row:2},label:"north"}'))
%
%      Consequence, stated because it is the reason the golden needs this file:
%      ANY program with a struct-typed arrival column is ungradeable against the
%      oracle through the text door today. `schedule_value/2` here maps a JSON
%      object to `obj(Pairs)` (key order sorted, matching what the conformance
%      fixtures write by hand: `obj([end-9, start-3])`) and a JSON array to a
%      prolog list, which is the shape body.pl's own JSON semantics expect.
%      dl6_oracle.pl is deliberately LEFT ALONE -- this is a golden-lane script,
%      and the one-clause fix it implies is a finding for whoever owns that file.
%
% Run:
%   swipl -q -l golden_oracle.pl -g "oracle_ticklog('p.dl6','s.json')" -g halt
%   swipl -q -l golden_oracle.pl -g "oracle_final('p.dl6','s.json')"   -g halt

:- ensure_loaded('../oracle_dump').        % pulls ticklog.pl -> go.pl -> engine.pl
:- use_module('../parse_dl', [parse_dl_file/4]).
:- use_module(library(http/json)).

oracle_ticklog(Dl6File, ScheduleFile) :-
    golden_program(Dl6File, ScheduleFile, Prog, Schedule),
    print_ticklog(Prog, [], Schedule).

oracle_final(Dl6File, ScheduleFile) :-
    golden_program(Dl6File, ScheduleFile, Prog, Schedule),
    run_program(Prog, [], Schedule, FinalAll, _DeltaTicks),
    final_state_line(FinalAll, Line),
    format('~w~n', [Line]).

golden_program(Dl6File, ScheduleFile, Prog, Schedule) :-
    parse_dl_file(Dl6File, Prog, _Bindings, Findings),
    ( Findings == []
    -> true
    ;  format(user_error, "golden_oracle: parse findings ~q~n", [Findings]), halt(1)
    ),
    read_schedule(ScheduleFile, Schedule).

read_schedule(ScheduleFile, Schedule) :-
    setup_call_cleanup(
        open(ScheduleFile, read, Stream),
        json_read_dict(Stream, Batches, [value_string_as(string)]),
        close(Stream)),
    maplist(batch_terms, Batches, Schedule).

batch_terms(Batch, Terms) :-
    maplist(arrival_term, Batch, Terms).

arrival_term(Arrival, Term) :-
    atom_string(Rel, Arrival.rel),
    maplist(schedule_value, Arrival.row, Args),
    Atom =.. [Rel | Args],
    ( Arrival.sign == "add" -> Term = +Atom
    ; Arrival.sign == "del" -> Term = -Atom
    ; format(user_error, "golden_oracle: unknown sign ~q~n", [Arrival.sign]), halt(1)
    ).

% A JSON string becomes an ATOM, never a prolog string: the compiler's own
% generated text (a probe's witness digest) is an atom, and a schedule feeding
% it as a string would silently fail to join. Same rule dl6_oracle.pl states.
schedule_value(Value, Value) :- integer(Value), !.
schedule_value(Value, Value) :- float(Value), !.
schedule_value(Value, Atom)  :- string(Value), !, atom_string(Atom, Value).
% A JSON boolean is the engine's `bool_lit(B)` wrapper, the shape
% conformance/fixtures/5_value_plane.pl writes by hand; a bare `true` atom is
% rejected field_not_bool(true), which is the whole point of the wrapper.
schedule_value(Value, bool_lit(true))  :- Value == true, !.
schedule_value(Value, bool_lit(false)) :- Value == false, !.
schedule_value(Value, List)  :- is_list(Value), !, maplist(schedule_value, Value, List).
schedule_value(Value, obj(Pairs)) :-
    is_dict(Value), !,
    dict_pairs(Value, _Tag, Raw),
    maplist(schedule_pair, Raw, Pairs0),
    msort(Pairs0, Pairs).
schedule_value(Value, Atom) :- term_to_atom(Value, Atom).

schedule_pair(Key-Value, Key-Encoded) :- schedule_value(Value, Encoded).
