:- ensure_loaded(fixture).
row_count(compiler_view(_, Facts, Rows, _), counts(FactCount, RowCount)) :-
    length(Facts, FactCount),
    length(Rows, RowCount).
case :-
    tiny_unit([], Unit),
    emit_compiled(prolog(row_count), Unit, _, _).
