:- ensure_loaded(fixture).
case :-
    argument_shapes_runtime(Runtime),
    logical_program_graph_calls(Runtime, all, _),
    logical_program_graph_calls(Runtime, [kernel(':')], _),
    prelude_bindings(Bindings),
    logical_program_calls(Bindings, Runtime, _, _).
