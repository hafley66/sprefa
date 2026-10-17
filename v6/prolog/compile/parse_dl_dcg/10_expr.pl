expr(E) --> { arithmetic_tiers(Tiers) }, tier_expr(Tiers, E).

arithmetic_tiers(Tiers) :-
    findall(P, expression(_/2, arithmetic, P, _, _), Ps),
    sort(Ps, Tiers).

tier_operators(Prec, Ops) :-
    findall(Op, expression(Op/2, arithmetic, Prec, _, _), Ops0),
    longest_first(Ops0, Ops).

tier_expr([], E) --> factor(E).
tier_expr([P | Tighter], E) -->
    tier_expr(Tighter, Acc),
    { tier_operators(P, Ops) },
    tier_rest(Ops, Tighter, Acc, E).

tier_rest(Ops, Tighter, Acc, E) -->
    here(Start), ws,
    ( tier_op(Ops, Op)
    -> ws, tier_expr(Tighter, Rhs),
       { Next =.. [Op, Acc, Rhs] },
       tier_rest(Ops, Tighter, Next, E)
    ; { E = Acc }, back(Start)
    ).

tier_op([Op | Rest], Matched) -->
    { atom_codes(Op, Cs) },
    ( op_codes(Cs) -> { Matched = Op } ; tier_op(Rest, Matched) ).


factor(E) -->
    ws, here(S), { no_tagged_brace(S) },
    ( @`(` -> ws, expr(E), #`)`
    ; bool_lit(E) -> []
    ; float_lit(E) -> []
    ; int_lit(E) -> []
    ; atom_lit(E) -> []
    ; string_lit(E) -> []
    ; dollar_var(E)
    ; json_literal(E) -> []
    ; braces_term(E)
    ; list_term(E)
    ; wildcard_var(E) -> []
    ; compound_or_var(E)
    ).

% Native JSON keeps a separate AST from object patterns and relation-value
% braces. A JSON object is selected by its required double-quoted key; bare,
% capture, descent, and typed brace pairs continue through braces_term//1.
json_literal(json_object(Pairs)) -->
    @`{`, ws, string_lit(KeyText), #`:`, ws, json_value(Value),
    { atom_string(Key, KeyText) },
    json_object_pairs(Key-Value, Pairs), #`}`.
json_literal(json_array(Values)) -->
    @`[`, ws, json_value(First), json_array_rest(First, Values), #`]`.

json_object_pairs(First, Pairs) -->
    ws,
    ( @`,` -> ws, string_lit(KeyText), #`:`, ws, json_value(Value),
       { atom_string(Key, KeyText) }, json_object_pairs(Key-Value, Rest),
       { Pairs = [First | Rest] }
    ; { Pairs = [First] }
    ).

json_array_rest(First, Values) -->
    ws,
    ( @`,` -> ws, json_value(Value), json_array_rest(Value, Rest),
       { Values = [First | Rest] }
    ; { Values = [First] }
    ).

json_value(json_object(Pairs)) -->
    @`{`, ws,
    ( peek(0'}) -> { Pairs = [] }
    ; string_lit(KeyText), #`:`, ws, json_value(Value),
      { atom_string(Key, KeyText) }, json_object_pairs(Key-Value, Pairs)
    ), #`}`.
json_value(json_array(Values)) -->
    @`[`, ws,
    ( peek(0']) -> { Values = [] }
    ; json_value(First), json_array_rest(First, Values)
    ), #`]`.
json_value(json_null) --> ~`null`.
json_value(Value) --> bool_lit(Value).
json_value(Value) --> float_lit(Value).
json_value(Value) --> int_lit(Value).
json_value(Value) --> string_lit(Value).

no_tagged_brace(S) :-
    ( ident(Name, S, [0'{ | _])
    -> throw(unsupported_construct(tagged_brace_reserved(Name)))
    ; true
    ).

dollar_var(Var) -->
    [0'$], ident(Name),
    { hole_var(Name, Var) }.

bool_lit(bool_lit(B)) -->
    { member(B, [true, false]) }, kw(B), !.

wildcard_var(_) --> ~`_`.

compound_or_var(E) -->
    here(Start),
    ( dotted_path(Segs), ws, peek(0'()
    -> @`(`, args(expr, Args), #`)`,
       { expression_path_application(Segs, Args, E) }
    ; back(Start), ident(Name), here(AfterName), ws,
      { get_or_make_var(Name, Rec) },
      back(AfterName), dot_chain(Rec, E)
    ).

expression_path_application([Name], Args, E) :-
    !,
    E =.. [Name | Args].
expression_path_application(Segs, Args, rel_path(Segs, Args)).

% Slash and dot are one path spelling (ruling executor_modules_use_import) and
% both reach module_path_name/2's `__` join, as `use soopy.` plus a leaf does.
dotted_path(Segments) -->
    ( peek(0'/) -> slash_path(Segments) ; dotted_path_segments(Segments) ).

dotted_path_segments([Segment | Rest]) -->
    ident(Segment),
    ( dot_then_ident -> dotted_path_segments(Rest) ; { Rest = [] } ).

slash_path([Segment | Rest]) -->
    slash_then_ident, ident(Segment),
    ( peek(0'/) -> slash_path(Rest) ; { Rest = [] } ).

slash_then_ident([0'/ | S], S) :-
    S = [C | _],
    ( code_type(C, alpha) ; C == 0'_ ),
    !.

dot_chain(Rec, Final) -->
    ( dot_then_ident
    -> ident(Field), dot_chain(dot_get(Rec, Field), Final)
    ; { Final = Rec }
    ).

dot_then_ident([0'. | S], S) :-
    S = [C | _],
    ( code_type(C, alpha) ; C == 0'_ ),
    !.


braces_term(Term) -->
    @`{`, !, ws,
    ( peek(0'})
    -> { Term = '{}' }
    ; { Term = '{}'(Pairs) },
      brace_pairs(Pairs)
    ),
    #`}`.

brace_pairs((Pair, Rest)) -->
    brace_pair(Pair), #`,`, !, ws,
    brace_pairs(Rest).
brace_pairs(Pair) --> brace_pair(Pair).

brace_pair(Key:Typed) -->
    brace_key(Key), #`:`, ws,
    expr(Value),
    ( #`:` -> ws, ident(Type), { Typed = Value:Type } ; { Typed = Value } ).

brace_key('**') --> @`**`, !.
brace_key($(Var)) -->
    [0'$], !, ident(Name),
    { hole_var(Name, Var) }.
brace_key(Key) --> atom_lit(Key), !.
brace_key(Key) --> string_lit(Text), !, { atom_string(Key, Text) }.
brace_key(Key) --> ident(Key).


list_term(Term) -->
    @`[`, !, ws,
    ( @`...` -> ws, expr(Element), { Term = spread(Element) }
    ; peek(0']) -> { Term = [] }
    ; sep(expr, Term)
    ),
    #`]`.
