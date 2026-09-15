:- ensure_loaded(fixture).
case :-
    tiny_runtime(Runtime),
    prelude_bindings(Bindings),
    logical_program_calls(Bindings, Runtime, _, _).
