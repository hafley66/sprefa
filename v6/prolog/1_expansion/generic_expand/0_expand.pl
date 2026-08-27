expand_generic_in_context(expansion_context(_, Bindings), Program, Expanded) :-
    !,
    expand_generic_program_with_bindings(Program, Bindings, Expanded).
expand_generic_in_context(_, Program, Expanded) :-
    expand_generic_program(Program, Expanded).

expand_generic_program(Program, Expanded) :-
    expand_generic_program_with_bindings(Program, [], Expanded).

expand_generic_program_with_bindings(prog(Decls0, Rules0), Bindings,
                                     Expanded) :-
    type_apply_refreeze(Decls0, Rules0, Bindings, [], none, 0, Expanded).

type_apply_refreeze(Decls0, Rules0, Bindings, Seen0, PreviousRows, Round,
                    prog(Decls, Rules)) :-
    ( Round >= 16
    -> throw(unsupported_construct(type_apply_round_limit_exhausted(16)))
    ; true
    ),
    expand_generic_program_round(prog(Decls0, Rules0), Bindings,
                                 prog(RoundDecls, RoundRules)),
    canonical_semantic_type_rows(RoundDecls, CurrentRows),
    type_apply_requests(Decls0, RoundDecls, Requests),
    subtract(Requests, Seen0, NewRequests),
    ( NewRequests == [],
      ( PreviousRows == none ; PreviousRows == CurrentRows )
    -> erase_type_apply_transport(RoundDecls, Decls),
       Rules = RoundRules
    ; append(Decls0, NewRequests, NextDecls),
      append(Seen0, NewRequests, Seen1),
      NextRound is Round + 1,
      type_apply_refreeze(NextDecls, Rules0, Bindings, Seen1, CurrentRows,
                          NextRound, prog(Decls, Rules))
    ).

canonical_semantic_type_rows(Decls, Rows) :-
    findall(Row,
            ( member(semantic_type_rows(SourceRows), Decls),
              member(Row, SourceRows) ),
            Rows0),
    sort(Rows0, Rows).
