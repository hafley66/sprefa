:- ensure_loaded(fixture).
case :-
    tiny_runtime([ call(ref(source_rel), [const(1), const(alpha)]),
                   call(ref(source_rel), [const(1), const(beta)])
                 ],
                 [[0]], Runtime),
    prelude_bindings(Bindings),
    emits_binding(Emits),
    emits_row("rows", output_rel, Row),
    append(Bindings, [Emits, Row], Facts),
    emit_compiled(dl7(the_emitter),
                  compiled_unit([], Runtime, Facts), _, _).
