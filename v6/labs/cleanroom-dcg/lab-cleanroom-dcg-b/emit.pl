% emit.pl -- audits the derived-vs-hand split of grammar.js against dcg.pl.
% Run: swipl -g emit -t halt.

:- module(emitpi, [emit/0]).

read_heads(File, Ns) :-
    open(File, read, S), hloop(S, Ns), close(S).
hloop(S, L) :- read_term(S, T, []),
    ( T = end_of_file -> L = []
    ; T = (Head --> _) -> functor(Head, Name, _), hloop(S, L0), L = [Name|L0]
    ; hloop(S, L) ).

emit :-
    derived_with(Derived),
    read_rules('grammar.js', Rules),
    classify2(Rules, Derived, EmittedRules, HandRules),
    nchars(EmittedRules, EChars), nchars(HandRules, HChars),
    length(Rules, TN), length(EmittedRules, EN), length(HandRules, HN),
    format('TOTAL_RULES ~w~n', [TN]),
    format('EMITTED_RULES ~w (~w chars)~n', [EN, EChars]),
    format('HAND_RULES ~w (~w chars)~n', [HN, HChars]).

% grammar.js rule -> the DCG nonterminal its structure was derived from
derived_with(D) :-
    D = [program, _statement, _rel_decl, _sh_decl, _bind_decl, _query_decl,
         _type, _match, _arm, _rule, _head, _body, _item, _cmpop, _bindop,
         _args, _expr, _braces, _pair, _key, _name].

classify2([], _, [], []).
classify2([Name-C|Rs], Derived, [Name-C|E], H) :- member(Name, Derived), !, classify2(Rs, Derived, E, H).
classify2([Name-C|Rs], Derived, E, [Name-C|H]) :- classify2(Rs, Derived, E, H).

nchars([], 0).
nchars([_-C|Rs], N) :- nchars(Rs, N0), N is N0 + C.

read_rules(File, Out) :-
    open(File, read, S), collect(S, outside, Out), close(S).
collect(S, _, Out) :- at_end_of_stream(S), !, Out = [].
collect(S, State, Out) :-
    read_line_to_string(S, Line),
    ( Line == end_of_file -> Out = []
    ; step(State, Line, NewState, MaybePair),
      collect(S, NewState, Out0),
      ( MaybePair = none -> Out = Out0 ; Out = [MaybePair|Out0] ) ).

step(State, Line, NewState, MaybePair) :-
    ( sub_atom(Line, _, _, _, "rules: {") -> NewState = inside, MaybePair = none
    ; State = inside, sub_atom(Line, _, _, _, "  }") -> NewState = done, MaybePair = none
    ; State = inside, extract(Line, Pair) -> NewState = inside, MaybePair = Pair
    ; State = done -> NewState = done, MaybePair = none
    ; NewState = State, MaybePair = none ).

% a rule line looks like:    _rel_decl: $ => seq(...),
extract(Line, NameOut-Chars) :-
    sub_string(Line, I, 2, _, "=>"),
    I > 0,
    sub_string(Line, 0, I, _, Lhs),
    split_string(Lhs, " \t:", " \t:", [NameStr|_]),
    atom_string(NameOut, NameStr),
    I2 is I + 2,
    string_length(Line, Len),
    SubLen is Len - I2,
    sub_string(Line, I2, SubLen, _, Body),
    nwchars(Body, Chars).

nwchars(S, N) :- string_codes(S, Cs), include(not_wsp, Cs, Keep), length(Keep, N).
not_wsp(C) :- \+ code_type(C, space).
