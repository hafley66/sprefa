removed_world_decl_stmt([]) -->
    ( ~`sh` -> { Word = sh } ; ~`bind` -> { Word = bind } ),
    ws, consume_removed_statement,
    { unsupported(removed_word(Word)) }.

consume_removed_statement -->
    [C],
    ( { C == 0'. } -> []
    ; { memberchk(C, [0'`, 0'\', 0'"]) } -> skip_quoted_span(C), consume_removed_statement
    ; consume_removed_statement
    ).

skip_quoted_span(Quote) -->
    [C],
    ( { C == Quote } -> []
    ; { C == 0'\\ } -> ( [_] -> [] ; [] ), skip_quoted_span(Quote)
    ; skip_quoted_span(Quote)
    ).

host_output_columns(Rel, Specs) --> args(typed_col(host_col_type(Rel)), Specs).

host_col_type(Rel, Col, none) -->
    coltype(W), { W \== none },
    { unsupported(column_type_wrapper(Rel, Col, W)) }.
host_col_type(_, _, Type) --> type_expr(Type).

specs_to_columns(Specs, Cols) :- maplist([column(N, T), col(N, T)]>>true, Specs, Cols).


template_lit(Template) -->
    [0'`], !,
    template_codes(Cs),
    { string_codes(Template, Cs) }.

template_codes([]) --> [0'`], !.
template_codes([0'` | Cs]) --> [0'\\, 0'`], !, template_codes(Cs).
template_codes([0'\\ | Cs]) --> [0'\\, 0'\\], !, template_codes(Cs).
template_codes([C | Cs]) --> [C], template_codes(Cs).


% A tail-free `?` keeps the query/1 term, so its emitted bytes cannot move.
