:- ensure_loaded(fixture).
case :-
    tiny_runtime(Runtime),
    prelude_bindings(Bindings),
    Extra = call(ref(kernel(':')),
                 [ref(module(prelude)), const(program_relation),
                  ref(second_program_relation), const(0)]),
    logical_program_calls([Extra | Bindings], Runtime, _, _).
