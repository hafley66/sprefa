:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

:- use_module('/Users/chrishafley/projects/sprefa/.claude/worktrees/agent-aa38cec430379621e/v6/prolog/conformance/engine.pl').

fixture(probe_diverging_counter,
  prog([ col_type(seed_number/1, value, int),
         col_type(counter/1, value, int) ],
       [ (counter(Value) <- seed_number(Value)),
         (counter(Next) <- (counter(Value), Next := Value + 1)) ]),
  [],
  [ [ +seed_number(0) ] ],
  [ final(counter/1, [ counter(0) ]) ]).

go :-
    (   catch(engine:fixture_expectations_hold(probe_diverging_counter, _), Error,
              ( print_message(error, Error), format("THREW~n", []) ))
    ->  format("HELD~n", [])
    ;   format("FAILED-EXPECTATION~n", [])
    ).
