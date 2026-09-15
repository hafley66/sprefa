:- ensure_loaded(fixture).
case :-
    tiny_unit([], Unit),
    emit_compiled(dl7(the_emitter), Unit, _, _).
