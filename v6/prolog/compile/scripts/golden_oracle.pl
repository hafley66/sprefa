% golden_oracle.pl : the golden-flex grading door -- run the REFERENCE ENGINE
% over a `.dl6` TEXT program and a JSON arrival schedule, printing BOTH legs
% the sweep grades on:
%
%   oracle_ticklog(File, ScheduleFile)  one {"tick":N,"deltas":{...}} line per tick
%   oracle_final(File, ScheduleFile)    one {"final":{...}} line
%
% The final-state leg reuses oracle_dump.pl's encoder. Arrival values use
% 0_json_arrival.pl for the same declared-type mapping as the text-door oracle.
%
% Run:
%   swipl -q -l golden_oracle.pl -g "oracle_ticklog('p.dl6','s.json')" -g halt
%   swipl -q -l golden_oracle.pl -g "oracle_final('p.dl6','s.json')"   -g halt

:- ensure_loaded('../oracle_dump').        % pulls ticklog.pl -> go.pl -> engine.pl
:- use_module('../parse_dl', [parse_dl_file/4]).
:- use_module('0_json_arrival', [arrival_column_types/4, schedule_value/5]).

oracle_ticklog(Dl6File, ScheduleFile) :-
    golden_program(Dl6File, ScheduleFile, Prog, Schedule),
    print_ticklog(Prog, [], Schedule).

oracle_final(Dl6File, ScheduleFile) :-
    golden_program(Dl6File, ScheduleFile, Prog, Schedule),
    run_program(Prog, [], Schedule, FinalAll, _DeltaTicks),
    final_state_line(FinalAll, Line),
    format('~w~n', [Line]).

% Both legs off ONE evaluation. run_program/5 already returns the tick deltas
% and the final state together, and the two entry points above each discard the
% half they did not ask for, so grading a schedule through them replays the
% whole program twice. Output is the concatenation the two-call form produced,
% in the same order, byte for byte.
oracle_both(Dl6File, ScheduleFile) :-
    golden_program(Dl6File, ScheduleFile, Prog, Schedule),
    run_program(Prog, [], Schedule, FinalAll, DeltaTicks),
    print_tick_lines(1, DeltaTicks),
    final_state_line(FinalAll, Line),
    format('~w~n', [Line]).

golden_program(Dl6File, ScheduleFile, Prog, Schedule) :-
    parse_dl_file(Dl6File, Prog, Bindings, Findings),
    ( Findings == []
    -> true
    ;  format(user_error, "golden_oracle: parse findings ~q~n", [Findings]), halt(1)
    ),
    read_schedule(Prog, Bindings, ScheduleFile, Schedule).

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
    maplist(schedule_value(golden_oracle, Rel), ColumnTypes, Arrival.row, Args),
    Atom =.. [Rel | Args],
    ( Arrival.sign == "add" -> Term = +Atom
    ; Arrival.sign == "del" -> Term = -Atom
    ; format(user_error, "golden_oracle: unknown sign ~q~n", [Arrival.sign]), halt(1)
    ).
