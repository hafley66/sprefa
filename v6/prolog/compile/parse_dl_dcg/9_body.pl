body(Body) -->
    ws,
    ( @`(` -> body(Inner), #`)`
    ; body_item(Inner)
    ),
    ws,
    ( @`,` -> ws, body(Rest), { Body = (Inner, Rest) }
    ; { Body = Inner }
    ).


body_item(Item) --> cst_item(Item), !.
body_item(Item) -->
    { Name = pre, Arity = 2, Shape = rel_atom_default
    ; surface(Name/Arity, _, _, LowerRole, _),
      wrapper_lower_role(LowerRole, Shape, _)
    },
    kw(Name), #`(`, balanced(Inner),
    { parse_surface_wrapper(Shape, Arity, Inner, Args) },
    !,
    { Item =.. [Name | Args] }.
body_item(Name) -->
    { surface(Name/0, _, _, word(_), _) }, kw(Name), !.
body_item(Item) --> infix_item(infix_op(bind), Item), !.
body_item(Item) --> infix_item(cmp_op, Item), !.
body_item(not(Atom)) -->
    @`!`, dotted_path(Segs), #`(`,
    head_args(Args), #`)`, !,
    { path_atom(body, Segs, Args, Atom) }.
body_item(Item) --> relatom_item(Item).


cst_item(cst(Path, Digest, Language, Query)) -->
    ~`cst`, #`(`,
    expr(Path), #`,`,
    expr(Digest), #`,`,
    ws, ident(Language), #`)`,
    #`{`,
    cst_block(Inner), here(S),
    { parse_cst_query_or_error(Inner, S, Query) }.

parse_cst_query_or_error(Codes, Rem, Query) :-
    catch(parse_cst_query(Codes, Query), _,
          ( mark(Rem), parse_failure(cst_query) )),
    !.

cst_block([]) --> [0'}], !.
cst_block(Codes) -->
    [0'"], cst_block_string(Str), !, cst_block(More),
    { append([0'" | Str], More, Codes) }.
cst_block([C | More]) --> [C], cst_block(More).

cst_block_string([0'"]) --> [0'"], !.
cst_block_string([0'\\, C | More]) --> [0'\\, C], !, cst_block_string(More).
cst_block_string([C | More]) --> [C], cst_block_string(More).

annotate_cst_item(Vars, Rule0, Rule) :-
    Rule0 =.. [Op, Head, Body], memberchk(Op, [<-, <+]), !,
    term_variables((Head, Body), RVars),
    map_tree(',', annotate_cst_leaf(Head, Vars, RVars), Body, Annotated),
    Rule =.. [Op, Head, Annotated].
annotate_cst_item(Vars, match(Source, Arms), match(Source, Annotated)) :- !,
    map_tree(';', annotate_cst_item(Vars), Arms, Annotated).
annotate_cst_item(_, Item, Item).

annotate_cst_leaf(Head, Vars, RVars,
                  cst(Path, Digest, Language, Query),
                  cst(Path, Digest, Language, Query,
                      cst_bindings(Caps, Cands, RNames))) :- !,
    ts_query_capture_names(Query, Caps),
    term_variables((Path, Digest), IVars),
    cst_variable_names(RVars, Vars, RNames),
    cst_variable_names(IVars, Vars, InNames),
    cst_body_variable_names(Head, Vars, InNames, Cands).
annotate_cst_leaf(_, _, _, Item, Item).

% the caller already derived InNames from (Path, Digest), and append(L, L, LL)
% then sort/2 is sort/2, so the exclusion set is just the sorted InNames.
cst_body_variable_names(Head, Vars, InNames, Names) :-
    term_variables(Head, HVars),
    cst_variable_names(HVars, Vars, HNames),
    sort(InNames, Excluded),
    subtract(HNames, Excluded, WithoutInputs),
    subtract(WithoutInputs, [line, end_line], Names).

cst_variable_names([], _, []).
cst_variable_names([Var | Rest], Vars, Names) :-
    ( member(Name-Candidate, Vars), Candidate == Var
    -> Names = [Name | More]
    ; Names = More
    ),
    cst_variable_names(Rest, Vars, More).


balanced(Inner, S0, S) :- bp(S0, 0, [], Rev, S), reverse(Rev, Inner).

bp([C | T], D, A, Out, S) :-
    ( memberchk(C, `"'`)
    -> bp_quoted(C, T, [C | A], A1, S1), bp(S1, D, A1, Out, S)
    ; C == 0'), D == 0
    -> Out = A, S = T
    ; ( C == 0'( -> D1 is D + 1 ; C == 0') -> D1 is D - 1 ; D1 = D ),
      bp(T, D1, [C | A], Out, S)
    ).

bp_quoted(Q, [C | T], A, Out, S) :-
    ( C == Q, T = [Q | T1] -> bp_quoted(Q, T1, [Q, Q | A], Out, S)
    ; C == Q -> Out = [Q | A], S = T
    ; C == 0'\\, T = [E | T1] -> bp_quoted(Q, T1, [E, 0'\\ | A], Out, S)
    ; bp_quoted(Q, T, [C | A], Out, S)
    ).

parse_full(Goal, Codes) :-
    call(Goal, Codes, Left),
    ws(Left, Left1),
    ( Left1 == [] -> true ; throw(dl_parse_error(trailing_input(Left1))) ).

% maplist/3 runs arg_value/2 backwards to lift positional exprs into the pos/1
% shape path_atom/4 reads; a wrapped rel atom resolves as a bare body one does.
rel_atom_term(Term) -->
    dotted_path(Segments), #`(`,
    args(expr, Values), #`)`,
    { maplist(arg_value, Args, Values),
      path_atom(body, Segments, Args, Term) }.

comma_pair(P1, P2, A, B) -->
    ws, call(P1, A), #`,`, ws, call(P2, B).

parse_surface_wrapper(atom_list, Arity, Codes, Atoms) :-
    parse_full(sep(rel_atom_term, Atoms), Codes),
    ( Arity == variadic -> true ; integer(Arity), length(Atoms, Arity) ).
parse_surface_wrapper(Shape, Arity, Codes, Args) :-
    wrapper_parser(Shape, Arity, Args, Goal),
    parse_full(Goal, Codes).

wrapper_parser(Shape, 1, [Arg], call(Parser, Arg)) :-
    memberchk(Shape-Parser,
              [rel_atom-rel_atom_term, body_item-body_item, expr-expr]).
wrapper_parser(expr_pair, 2, [A, B], comma_pair(expr, expr, A, B)).
wrapper_parser(rel_atom_default, 2, [Atom, D],
               comma_pair(rel_atom_term, expr, Atom, D)).


% infix_item//2: the operator nonterminal arrives partially applied via call//2
infix_item(OpParser, Term) -->
    expr(Lhs), ws, call(OpParser, Op), ws, expr(Rhs),
    { Term =.. [Op, Lhs, Rhs] }.

cmp_op(=<) --> @`<=`, !.
cmp_op(\==) --> @`!=`, !.
cmp_op(Op) --> infix_op(guard, Op), !.
cmp_op(==) --> @`=`.

infix_op(Axis, Op) -->
    { findall(C, surface(C/2, Axis, no_refs, infix(_), _), Cands),
      longest_first(Cands, Ordered),
      member(Op, Ordered),
      atom_codes(Op, Cs) },
    op_codes(Cs).

neg_length(Atom, NegLen) :- atom_length(Atom, Len), NegLen is -Len.
longest_first(Atoms, Sorted) :-
    map_list_to_pairs(neg_length, Atoms, Keyed),
    keysort(Keyed, Ordered),
    pairs_values(Ordered, Sorted).

op_codes([F | R]) -->
    ( { code_type(F, alpha) } -> ~[F | R] ; @[F | R] ).


% append/3 against an InCount-long prefix already fails when Values is short,
% so the explicit length comparison is redundant.
split_probe_values(InCount, Values, Ins, Outs) :-
    ( length(Ins, InCount), append(Ins, Outs, Values)
    -> true
    ; Ins = Values, Outs = []
    ).

partition_hiv([], [], [], [], []).
partition_hiv([col(Name, _) | Cols], [V | Vs], [Role | Roles], Ids, Salts) :-
    ( Role == identity -> Ids = [V | Ids1], Salts = Salts1
    ; Role == freshness -> Salts = [salt(Name, V) | Salts1], Ids = Ids1
    ),
    partition_hiv(Cols, Vs, Roles, Ids1, Salts1).

relatom_item(Item) -->
    dotted_path(Segs), ws,
    ( @`!` -> { Mut = true }, ws ; { Mut = false } ),
    @`(`, head_args(Args), #`)`,
    { ( Mut == true
      -> last(Segs, Name),
         length(Args, Arity),
         unsupported(mutation(Name/Arity)),
         module_path_name(Segs, Resolved),
         resolve_named_args(body, Resolved, Args, Pos),
         Item =.. [Name | Pos]
      ; path_atom(body, Segs, Args, Item)
      ) }.


