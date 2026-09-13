:- ensure_loaded(fixture).
case :-
    emits_binding(Emits),
    tiny_unit([Emits], Unit),
    emit_compiled(dl7(the_emitter), Unit, _, _).
