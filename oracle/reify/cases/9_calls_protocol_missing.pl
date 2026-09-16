:- ensure_loaded(fixture).
case :-
    tiny_runtime(Runtime),
    logical_program_calls([], Runtime, _, _).
