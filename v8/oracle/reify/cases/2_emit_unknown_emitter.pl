:- ensure_loaded(fixture).
case :-
    tiny_unit([], Unit),
    emit_compiled(no_such_emitter_kind, Unit, _, _).
