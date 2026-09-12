:- ensure_loaded(fixture).
case :-
    emits_binding(Emits),
    emits_row("rows", output_rel, First),
    emits_row("rows", source_rel, Second),
    tiny_unit([Emits, First, Second], Unit),
    emit_compiled(dl7(the_emitter), Unit, _, _).
