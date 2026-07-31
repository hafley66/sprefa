% grader.pl : the PASS/fail loop every lab was copy-pasting, written once.
% A lab defines check(Name, Goal) facts and one line: `go :- run(check).`

:- module(grader, [run/1]).

% `:` not `2`: the check FACTS carry goals that must run back in the lab's own
% module, not in grader's. Non-module labs (user) behaved either way; a module
% lab's private helpers need the qualification.
:- meta_predicate run(:).

% run/1 FAILS when any check failed, so `swipl -g go -g halt` exits nonzero.
% It printed `fail` lines and succeeded until 2026-07-31: every runner riding
% it (conformance included) was exit-0 advisory, and a red fixture shipped
% invisible under battery tails that trusted the exit code.
run(Module:Check) :-
    Failures = failures(0),
    forall(call(Module:Check, Name, Goal),
           (   catch(Module:Goal, Error, (print_message(error, Error), fail))
           ->  format("PASS  ~w~n", [Name])
           ;   format("fail  ~w~n", [Name]),
               arg(1, Failures, SoFar),
               Bumped is SoFar + 1,
               nb_setarg(1, Failures, Bumped)
           )),
    arg(1, Failures, FailureCount),
    (   FailureCount =:= 0
    ->  true
    ;   format("FAILURES  ~w~n", [FailureCount]),
        fail
    ).
