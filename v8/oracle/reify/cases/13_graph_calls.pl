:- ensure_loaded(fixture).
case :-
    tiny_runtime(Runtime),
    logical_program_graph_calls(Runtime, all, _),
    logical_program_graph_calls(Runtime, [kernel(':')], _),
    logical_program_graph_calls(Runtime, [kernel(node)], _).
