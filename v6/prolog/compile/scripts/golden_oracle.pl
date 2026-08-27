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
:- use_module('../../7_lower/use_resolve', [expand_uses/8]).
:- use_module('../../9_json_arrival/0_json_arrival', [arrival_column_types/4, schedule_value/5]).

oracle_ticklog(Dl6File, ScheduleFile) :-
    golden_program(Dl6File, ScheduleFile, Prog, Schedule),
    print_ticklog(Prog, [], Schedule).

oracle_final(Dl6File, ScheduleFile) :-
    golden_program(Dl6File, ScheduleFile, Prog, Schedule),
    run_program(Prog, [], Schedule, FinalAll, _DeltaTicks),
    final_state_line(FinalAll, Line),
    format('~w~n', [Line]).

% Both legs off ONE run_program/5, which returns the deltas and the final state
% together. Prints them in the order the two separate entry points did.
oracle_both(Dl6File, ScheduleFile) :-
    golden_program(Dl6File, ScheduleFile, Prog, Schedule),
    run_program(Prog, [], Schedule, FinalAll, DeltaTicks),
    print_tick_lines(1, DeltaTicks),
    final_state_line(FinalAll, Line),
    format('~w~n', [Line]).

golden_program(Dl6File, ScheduleFile, Prog, Schedule) :-
    expand_uses(Dl6File, [], [], _Loaded, Prog, _ModuleTable, Bindings, Findings),
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
