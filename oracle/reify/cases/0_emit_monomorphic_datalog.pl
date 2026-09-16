:- ensure_loaded(fixture).
case :-
    tiny_unit([], Unit),
    emit_compiled(monomorphic_datalog, Unit, _, _).
