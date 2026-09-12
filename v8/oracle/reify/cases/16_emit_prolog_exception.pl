:- ensure_loaded(fixture).
throwing_emitter(_, _) :- throw(emitter_blew_up).
case :-
    tiny_unit([], Unit),
    emit_compiled(prolog(throwing_emitter), Unit, _, _).
