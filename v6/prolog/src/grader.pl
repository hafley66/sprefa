% grader.pl : the PASS/fail loop every lab was copy-pasting, written once.
% A lab defines check(Name, Goal) facts and one line: `go :- run(check).`

:- module(grader, [run/1]).

% `:` not `2`: the check FACTS carry goals that must run back in the lab's own
% module, not in grader's. Non-module labs (user) behaved either way; a module
% lab's private helpers need the qualification.
:- meta_predicate run(:).

run(Module:Check) :-
    forall(call(Module:Check, Name, Goal),
           (   catch(Module:Goal, Error, (print_message(error, Error), fail))
           ->  format("PASS  ~w~n", [Name])
           ;   format("fail  ~w~n", [Name])
           )).
