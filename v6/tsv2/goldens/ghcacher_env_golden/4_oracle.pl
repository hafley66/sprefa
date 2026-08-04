% Prints the reference engine's exact tick log followed by its exact final
% relation envelope for one .dl6 program and JSON schedule.
%
% This oracle prepares (host-expands) the program BEFORE reading the schedule:
% the generated `__host_response_toml_json` relation carries a json `doc`
% column, and read_schedule resolves a column's type out of the program's
% decls. On the raw parsed program the generated response rel is absent, so
% the schedule reader falls back to type `none` and stores a json doc as an
% opaque term no decode pattern ever matches. Preparing first lets the seam
% inject the canonical json text and gives the decode plane a real obj term.

:- ensure_loaded('../../../prolog/compile/scripts/dl6_oracle').
:- use_module('../../../prolog/1_host_expand', [prepare_program/5]).
:- ensure_loaded('../../../prolog/compile/oracle_dump').

:- initialization(main, main).

main(Argv) :-
    ( Argv = [ProgramFile, ScheduleFile]
    -> golden_oracle(ProgramFile, ScheduleFile)
    ; format(user_error,
             'usage: swipl -q -l 4_oracle.pl -- <program.dl6> <schedule.json>~n',
             []),
      halt(2)
    ).

golden_oracle(ProgramFile, ScheduleFile) :-
    parse_dl_file(ProgramFile, Prog, Bindings, Findings),
    ( Findings == []
    -> true
    ; format(user_error, 'ghcacher oracle: parse findings ~q~n', [Findings]),
      halt(1)
    ),
    host_expand:prepare_program(Prog, Prepared, _HostPlans, _BindPlans,
                                _QueryPlans),
    read_schedule(Prepared, Bindings, ScheduleFile, Schedule),
    print_ticklog(Prepared, [], Schedule),
    run_program(Prog, [], Schedule, FinalAll, _DeltaTicks),
    final_state_line(FinalAll, FinalLine),
    format('~w~n', [FinalLine]).
