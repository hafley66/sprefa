:- module(dl7_driver, [main/0, main/1]).

:- use_module('4_loader', [load_dl7/3]).

:- initialization(main, main).

main :-
    current_prolog_flag(argv, Argv),
    main(Argv).

main(Argv) :-
    catch(driver_exit_code(Argv, Code),
          Error,
          implementation_failure(Error, Code)),
    halt(Code).

driver_exit_code([Path], Code) :-
    !,
    load_dl7(Path, Unit, Diagnostics),
    unit_exit(Diagnostics, Unit, Code).
driver_exit_code(_, 1) :-
    format(user_error,
           "usage: swipl -q -s v7/0_SWIPL/5_driver.pl -- path/to/program.dl7~n",
           []).

unit_exit([], Unit, 0) :-
    write_term(Unit,
               [ quoted(true),
                 ignore_ops(true),
                 numbervars(true),
                 fullstop(true),
                 nl(true)
               ]).
unit_exit(Diagnostics, _, 2) :-
    maplist(write_diagnostic, Diagnostics).

write_diagnostic(Diagnostic) :-
    write_term(user_error, Diagnostic,
               [ quoted(true),
                 ignore_ops(true),
                 numbervars(true),
                 fullstop(true),
                 nl(true)
               ]).

implementation_failure(Error, 1) :-
    write_term(user_error, implementation_failure(Error),
               [ quoted(true),
                 ignore_ops(true),
                 numbervars(true),
                 fullstop(true),
                 nl(true)
               ]).
