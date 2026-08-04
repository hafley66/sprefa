% Prints the reference engine's exact tick log followed by its exact final
% relation envelope for one .dl6 program and JSON schedule.

:- ensure_loaded('../../../prolog/compile/scripts/dl6_oracle').
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
    % read_schedule/4, not /2: a json column's schedule value is JSON TEXT that
    % the door parses into oracle json terms, which needs the parsed program
    % and its bindings to look the column types up.
    read_schedule(Prog, Bindings, ScheduleFile, Schedule),
    print_ticklog(Prog, [], Schedule),
    run_program(Prog, [], Schedule, FinalAll, _DeltaTicks),
    final_state_line(FinalAll, FinalLine),
    format('~w~n', [FinalLine]).
