rule_stmt(Rule) -->
    head_atom(Head), ws,
    ( @`<-` -> { Arrow = (<-) }, ws, body(Body)
    ; @`<+` -> { Arrow = (<+) }, ws, body(Body)
    ; { Arrow = (<-), Body = true }
    ),
    #`.`,
    { Rule =.. [Arrow, Head, Body] }.


head_atom(Term) -->
    dotted_path(Segs), #`(`,
    head_args(Args), #`)`,
    { path_atom(head, Segs, Args, Term) }.

% the one spelling of "dotted path plus args becomes a term", shared by head
% atoms and body atoms; a single segment stays plain, longer ones go rel_path.
path_atom(Mode, Segs, Args, Term) :-
    module_path_name(Segs, Resolved),
    resolve_named_args(Mode, Resolved, Args, Pos),
    ( Segs = [Name] -> Term =.. [Name | Pos] ; Term = rel_path(Segs, Pos) ).

head_args(Args) --> args(atom_arg, Args).

atom_arg(named(Name, Value)) -->
    ident(Name), ws,
    here([0':, Next | _]), { Next \== 0'=, Next \== 0': }, !,
    @`:`, ws, expr(Value).
atom_arg(pos(Value)) --> expr(Value).


% A SHORT all-positional body call puns by name when EVERY argument is a
% capitalized variable naming a declared column (user 2026-08-22: "only when
% all puns are matching cap first, otherwise its ambiguous"). A full-arity
% call stays positional; a short call with one non-punning argument stays
% positional and lands on the arity check.
resolve_named_args(body, Rel, Args, Pos) :-
    \+ member(named(_, _), Args),
    lookup_column_order(Rel, Cols),
    length(Args, ArgCount), length(Cols, ColCount), ArgCount < ColCount,
    activate_keyword_puns(Args, Cols, Resolved),
    forall(member(Arg, Resolved), Arg = named(_, _)),
    !,
    resolve_mixed_args(body, Rel, Resolved, Cols, Pos).
resolve_named_args(_, _, Args, Pos) :-
    \+ member(named(_, _), Args), !,
    maplist(arg_value, Args, Pos).
resolve_named_args(Mode, Rel, Args, Pos) :-
    ( lookup_column_order(Rel, Cols)
    -> activate_keyword_puns(Args, Cols, ResolvedArgs),
       resolve_mixed_args(Mode, Rel, ResolvedArgs, Cols, Pos)
    ; unsupported(named_args_unresolved(Rel)),
      maplist(arg_value, Args, Pos)
    ).

arg_value(pos(V), V) :- !.
arg_value(named(_, V), V).

% In mixed calls, `Name` puns `name: Name` when lowercasing its first letter
% names a column. Fully positional and unmatched arguments retain their order.
capitalized_keyword_pun(Name, Column) :-
    atom_chars(Name, [First | Rest]),
    char_type(First, upper),
    downcase_atom(First, Lower),
    atom_chars(Column, [Lower | Rest]).
% `PollPeriod` also puns `poll_period`: the camel form is how every rule in
% the corpus spells a multi-word column variable.
capitalized_keyword_pun(Name, Column) :-
    atom_chars(Name, [First | Rest]),
    char_type(First, upper),
    once(( member(Upper, Rest), char_type(Upper, upper) )),
    snake_chars([First | Rest], SnakeChars),
    atom_chars(Column, SnakeChars).

snake_chars([First | Rest], [Lower | More]) :-
    downcase_atom(First, LowerAtom), atom_chars(LowerAtom, [Lower]),
    snake_tail(Rest, More).

snake_tail([], []).
snake_tail([C | Rest], Out) :-
    (   char_type(C, upper)
    ->  downcase_atom(C, LowerAtom), atom_chars(LowerAtom, [Lower]),
        snake_tail(Rest, More), Out = ['_', Lower | More]
    ;   snake_tail(Rest, More), Out = [C | More]
    ).

activate_keyword_puns([], _, []).
activate_keyword_puns([pos(Value) | Rest], Cols, [Arg | More]) :-
    var(Value),
    variable_source_name(Value, Name),
    capitalized_keyword_pun(Name, Column),
    memberchk(Column, Cols),
    !,
    Arg = named(Column, Value),
    activate_keyword_puns(Rest, Cols, More).
activate_keyword_puns([Arg | Rest], Cols, [Arg | More]) :-
    activate_keyword_puns(Rest, Cols, More).

variable_source_name(Value, Name) :-
    b_getval(dl_vars, Vars),
    member(Name-Existing, Vars),
    Existing == Value,
    !.

resolve_mixed_args(Mode, Rel, Args, Cols, Pos) :-
    length(Cols, N),
    length(Pos, N),
    validate_named_columns(Rel, Args, Cols),
    maplist(place_named(Args), Cols, Pos),
    findall(Col, member(named(Col, _), Args), NamedCols),
    findall(I, ( nth1(I, Cols, Col), \+ memberchk(Col, NamedCols) ), FreeIdxs),
    positional_values(Args, PosValues),
    fill_partial_slots(Mode, Rel, N, FreeIdxs, PosValues, Pos).

% Recursive collection preserves variable identity; findall/3 would copy the
% positional variables away from matching head variables.
positional_values([], []).
positional_values([pos(Value) | Rest], [Value | Values]) :-
    !,
    positional_values(Rest, Values).
positional_values([_ | Rest], Values) :-
    positional_values(Rest, Values).

validate_named_columns(Rel, Args, Cols) :-
    findall(Name, member(named(Name, _), Args), Names),
    ( member(Name, Names), \+ memberchk(Name, Cols)
    -> unsupported(unknown_named_arg(Rel, Name))
    ; select(Dup, Names, Rest), memberchk(Dup, Rest)
    -> unsupported(duplicate_named_arg(Rel, Dup))
    ; true
    ).

% Pos is already a fresh list as long as Cols, so maplist/3 pairs column to
% slot and the hand-rolled index walk disappears.
place_named(Args, Col, Slot) :-
    ( member(named(Col, V), Args) -> Slot = V ; true ).

fill_free_slots(Is, Vs, Pos) :-
    maplist({Pos}/[I, V]>>nth1(I, Pos, V), Is, Vs).

fill_partial_slots(Mode, Rel, Arity, FreeIdxs, PosValues, Pos) :-
    same_length(PosValues, FilledIdxs),
    append(FilledIdxs, OmittedIdxs, FreeIdxs),
    fill_free_slots(FilledIdxs, PosValues, Pos),
    finish_omitted_slots(Mode, Rel/Arity, OmittedIdxs, Pos).

% anonymous slots are free slots whose value list maplist/3 invents, so the
% second argument stays unbound and fill_anonymous_slots/2 is not needed.
finish_omitted_slots(Mode, Ref, Idxs, Pos) :-
    ( Mode == head, Idxs \== [] -> unsupported(partial_head(Ref)) ; true ),
    fill_free_slots(Idxs, _, Pos).


