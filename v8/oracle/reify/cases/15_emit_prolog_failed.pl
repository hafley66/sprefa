:- ensure_loaded(fixture).
never_emits(_, _) :- fail.
case :-
    tiny_unit([], Unit),
    emit_compiled(prolog(never_emits), Unit, _, _).
