:- ensure_loaded(fixture).
case :-
    tiny_unit([], Unit),
    emit_compiled(relational_program, Unit, _, _).
