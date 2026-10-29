query_stmt(Query) -->
    @`?`, ws, dotted_path(Segs), #`(`,
    head_args(Args), #`)`,
    { module_path_name(Segs, Name) },
    order_tail(Name, Args, OrderCols), #`.`,
    { path_atom(head, Segs, Args, Atom),
      ( OrderCols == [] -> Query = query(Atom)
      ; Query = query(Atom, order(OrderCols)) ) }.

% `order by defs desc, path` -- SQL words, `asc` when a direction is unwritten.
order_tail(Name, Args, OrderCols) -->
    ws, ~`order`, ws, ~`by`, !, ws,
    sep(order_col(Name, Args), OrderCols).
order_tail(_, _, []) --> [].

% The position resolves here against the QUERY's argument names, so it indexes
% the rel's own column list and no emitter repeats the lookup.
order_col(Name, Args, order_col(Position, Direction)) -->
    ident(Column), ws,
    ( ~`desc` -> { Direction = desc }
    ; ~`asc` -> { Direction = asc }
    ; { Direction = asc }
    ),
    { ( query_arg_position(Args, Column, Position)
      -> true
      ;  parse_failure(order_column_unknown(Name, Column))
      ) }.

query_arg_position(Args, Column, Position) :-
    nth1(Position, Args, Arg),
    query_arg_name(Arg, Column),
    !.

query_arg_name(named(Column, _), Column).
query_arg_name(pos(Value), Column) :-
    var(Value),
    variable_source_name(Value, Column).


match_stmt(match(Source, Arms)) -->
    ~`match`, ws, head_atom(Source), ws,
    @`(`, match_arms(Arms), #`)`, #`.`.

match_arms(Arms) -->
    ws, ( @`;` -> ws ; [] ),
    match_arm(First),
    match_arm_tail(First, Arms).

match_arm_tail(First, Arms) -->
    ws,
    ( @`;`
    -> ws, match_arm(Next),
       match_arm_tail(Next, Rest),
       { Arms = (First ; Rest) }
    ; { Arms = First }
    ).

match_arm(Arm) -->
    body(Guards), ws,
    ( @`|->` -> { Arrow = (<-) } ; @`|+>` -> { Arrow = (<+) } ),
    ws, head_atom(Head),
    { Arm =.. [Arrow, Head, Guards] }.

