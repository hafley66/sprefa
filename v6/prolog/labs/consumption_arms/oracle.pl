% oracle.pl : read-only access to the reference interpreter.
%
% Every scenario in this lab that can be written as a kernel program runs
% HERE, on v6/prolog/conformance/engine.pl, exactly the way ticklog.pl
% consumes it. Nothing in this lab edits conformance/**.

:- module(ca_oracle, [ oracle_log/4, oracle_log_final/5, oracle_throws/4,
                       final_has/2, final_lacks/2 ]).

:- use_module(library(lists)).
:- use_module('../../conformance/engine').

oracle_log(Prog, Initial, Schedule, DeltaTicks) :-
    run_program(Prog, Initial, Schedule, _Final, DeltaTicks).

oracle_log_final(Prog, Initial, Schedule, FinalAll, DeltaTicks) :-
    run_program(Prog, Initial, Schedule, FinalAll, DeltaTicks).

oracle_throws(Prog, Initial, Schedule, Expected) :-
    catch(( run_program(Prog, Initial, Schedule, _, _), fail ), Thrown, true),
    Thrown == Expected.

final_has(Final, Row)   :- memberchk(Row, Final).
final_lacks(Final, Row) :- \+ memberchk(Row, Final).
