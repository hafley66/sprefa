:- ensure_loaded(fixture).
case :-
    emits_binding(Emits),
    Other = call(ref(kernel(':')),
                 [ref(module(other)), const(emits),
                  ref(other_emits_rel), const(0)]),
    tiny_unit([Emits, Other], Unit),
    emit_compiled(dl7(the_emitter), Unit, _, _).
