:- ensure_loaded(fixture).
case :-
    emits_binding(Emits),
    emits_row("rows", output_rel, Row),
    tiny_unit([Emits, Row], Unit),
    emit_compiled(dl7(the_emitter), Unit, _, _).
