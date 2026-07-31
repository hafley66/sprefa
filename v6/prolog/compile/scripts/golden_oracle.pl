% golden_oracle.pl : the golden-flex grading door -- run the REFERENCE ENGINE
% over a `.dl6` TEXT program and a JSON arrival schedule, printing BOTH legs
% the sweep grades on:
%
%   oracle_ticklog(File, ScheduleFile)  one {"tick":N,"deltas":{...}} line per tick
%   oracle_final(File, ScheduleFile)    one {"final":{...}} line
%
% WHY THIS EXISTS BESIDE dl6_oracle.pl. Two gaps were measured while writing
% v6/dl/fixtures/golden-flex.dl6:
%
%  (1) NO FINAL-STATE LEG. `dl6_oracle.pl:oracle/2` calls `print_ticklog/3`,
%      which throws FinalAll away. `oracle_dump.pl` has the final-state encoder
%      but only over `fixture/5` terms, so a `.dl6` TEXT program has no way to
%      earn the grade an empty-or-short schedule needs. `final_state_line/2`
%      below is oracle_dump.pl's own predicate, reused by loading that file, not
%      reimplemented.
%
%  (2) ARRIVAL VALUES NEED DECLARED-TYPE MAPPING. The shared
%      `0_json_arrival.pl` helper resolves column order through `rel_columns/5`.
%      JSON and list carriers parse canonical text into the reference engine's
%      object/list terms, while structured relation values keep the golden's
%      existing object/list conversion:
%
%        type_arrival_shape_mismatch(tree/2, site, patch,
%          not_an_object(patch, '#{at: #{col:3,row:2},label:"north"}'))
%
%      Both text-door oracle scripts call that helper, so the same declared
%      column mapping handles served schedules and the golden schedule.
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
