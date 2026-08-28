:- module(main, [run/2]).

:- use_module(util).

run(Input, Output) :-
    Goal =.. [helper, Input, Output],
    call(Goal).
