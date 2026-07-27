% grader.pl : the PASS/fail loop every lab was copy-pasting, written once.
% A lab defines check(Name, Goal) facts and one line: `go :- run(check).`

:- module(grader, [run/1]).

:- meta_predicate run(2).

run(Check) :-
    forall(call(Check, Name, Goal),
           (   catch(Goal, Error, (print_message(error, Error), fail))
           ->  format("PASS  ~w~n", [Name])
           ;   format("fail  ~w~n", [Name])
           )).
