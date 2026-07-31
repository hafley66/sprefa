% Shared PASS/fail loop for lab check(Name, Goal) facts.

:- module(grader, [run/1]).

% `:` not `2`: the check FACTS carry goals that must run back in the lab's own
% module, not in grader's. Non-module labs (user) behaved either way; a module
% lab's private helpers need the qualification.
:- meta_predicate run(:).

% run/1 fails when any check fails, so the command exits nonzero.
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
