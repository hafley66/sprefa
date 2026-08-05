:- module(cst_query,
          [ parse_cst_query/2,
            serialize_ts_query/2,
            ts_query_capture_names/2
          ]).

parse_cst_query(Codes, ts_query(Patterns)) :-
    query_terms(Codes, Terms),
    normalize_query_terms(Terms, Patterns).

query_terms(Codes, Terms) :-
    query_terms(Codes, [], Reversed),
    reverse(Reversed, Terms).

query_terms(S0, Acc, Terms) :-
    query_ws(S0, S1),
    (   S1 == []
    ->  Terms = Acc
    ;   query_pattern(Pattern, S1, S2),
        query_terms(S2, [Pattern | Acc], Terms)
    ).

normalize_query_terms([], []).
normalize_query_terms([predicate(_, _, _) | _], _) :-
    throw(unmapped_feature(slot_ts_query_term, predicate)).
normalize_query_terms([Root | Predicates], [group(Root, Predicates)]) :-
    Predicates \== [],
    query_predicate_list(Predicates),
    !.
normalize_query_terms(Patterns, Patterns).

query_predicate_list([]).
query_predicate_list([predicate(_, _, _) | Rest]) :-
    query_predicate_list(Rest).

query_pattern(Pattern, S0, S) :-
    query_ws(S0, S1),
    query_atom(Pattern0, S1, S2),
    query_suffix(Pattern0, Pattern, S2, S).

query_atom(Pattern, [0'[ | Rest], S) :-
    !,
    query_pattern_list(Patterns, Rest, S1),
    Pattern = alternative(Patterns),
    S = S1.
query_atom(Pattern, [0'( | Rest], S) :-
    !,
    query_parenthesized(Pattern, Rest, S).
query_atom(wildcard, S0, S) :-
    query_ident('_', S0, S),
    !.
query_atom(anonymous(Value), S0, S) :-
    query_string(Value, S0, S),
    !.
query_atom(_, _, _) :-
    throw(unmapped_feature(slot_ts_pattern_form, query_atom)).

query_suffix(Pattern0, Pattern, S0, S) :-
    query_capture_suffix(Pattern0, Pattern1, S0, S1),
    query_quantifier(Pattern1, Pattern, S1, S),
    !.
query_suffix(Pattern, Pattern, S, S).

query_capture_suffix(Pattern0, capture(Name, Pattern0), S0, S) :-
    query_ws(S0, S1),
    query_char(0'@, S1, S2),
    query_ident(Name, S2, S),
    !.
query_capture_suffix(Pattern, Pattern, S, S).

query_quantifier(Pattern, quant(optional, Pattern), S0, S) :-
    query_ws(S0, S1), query_char(0'?, S1, S), !.
query_quantifier(Pattern, quant(zero_or_more, Pattern), S0, S) :-
    query_ws(S0, S1), query_char(0'*, S1, S), !.
query_quantifier(Pattern, quant(one_or_more, Pattern), S0, S) :-
    query_ws(S0, S1), query_char(0'+, S1, S), !.
query_quantifier(Pattern, Pattern, S, S).

query_parenthesized(Pattern, S0, S) :-
    query_ws(S0, S1),
    (   query_predicate(Pattern, S1, S2)
    ->  query_ws(S2, S3), query_char(0'), S3, S)
    ;   query_ident(Type, S1, S2)
    ->  query_node_children(Children, S2, S3),
        ( Type == '_', Children == []
        -> Pattern = named_wildcard
        ;  Pattern = node(Type, Children)
        ),
        S = S3
    ;   query_pattern(First, S1, S2),
        query_patterns_until_close(Rest, S2, S3),
        (   query_predicate_list(Rest)
        ->  Pattern = group(First, Rest)
        ;   throw(unmapped_feature(slot_ts_pattern_form, [First | Rest]))
        ),
        S = S3
    ).

query_node_children([], S0, S) :-
    query_ws(S0, S1), query_char(0'), S1, S),
    !.
query_node_children([Child | Rest], S0, S) :-
    query_ws(S0, S1),
    query_node_child(Child, S1, S2),
    query_node_children(Rest, S2, S).

query_node_child(field(Name, Pattern), S0, S) :-
    query_ident(Name, S0, S1),
    query_ws(S1, S2),
    query_char(0':, S2, S3),
    query_pattern(Pattern, S3, S),
    !.
query_node_child(Pattern, S0, S) :-
    query_pattern(Pattern, S0, S).

query_patterns_until_close([], S0, S) :-
    query_ws(S0, S1), query_char(0'), S1, S),
    !.
query_patterns_until_close([Pattern | Rest], S0, S) :-
    query_pattern(Pattern, S0, S1),
    query_patterns_until_close(Rest, S1, S).

query_pattern_list([], S0, S) :-
    query_ws(S0, S1), query_char(0'], S1, S),
    !.
query_pattern_list([Pattern | Rest], S0, S) :-
    query_pattern(Pattern, S0, S1),
    query_pattern_list(Rest, S1, S).

query_predicate(predicate(eq, Left, Right), S0, S) :-
    query_word("#eq?", S0, S1),
    query_predicate_args(Left, Right, S1, S),
    !.
query_predicate(predicate(match, Left, Right), S0, S) :-
    query_word("#match?", S0, S1),
    query_predicate_args(Left, Right, S1, S),
    !.
query_predicate(predicate(not_match, Left, Right), S0, S) :-
    query_word("#not-match?", S0, S1),
    query_predicate_args(Left, Right, S1, S),
    !.

query_predicate_args(capture_ref(Name), string(Value), S0, S) :-
    query_ws(S0, S1),
    query_char(0'@, S1, S2),
    query_ident(Name, S2, S3),
    query_ws(S3, S4),
    query_string(Value, S4, S5),
    query_ws(S5, S).

query_ident(Name, S0, S) :-
    S0 = [Code | Rest],
    query_ident_start(Code),
    query_ident_tail(Rest, Tail, S),
    atom_codes(Name, [Code | Tail]).

query_ident_start(Code) :- code_type(Code, alpha), !.
query_ident_start(0'_).

query_ident_tail([Code | Rest], [Code | More], S) :-
    query_ident_code(Code),
    !,
    query_ident_tail(Rest, More, S).
query_ident_tail(S, [], S).

query_ident_code(Code) :- code_type(Code, alnum), !.
query_ident_code(0'_).

query_string(Value, [0'" | Rest], S) :-
    query_string_codes(Rest, Codes, S),
    string_codes(Value, Codes).

query_string_codes([0'" | Rest], [], Rest) :- !.
query_string_codes([0'\\, Code | Rest], Codes, S) :-
    !,
    query_escape(Code, Escaped),
    query_string_codes(Rest, More, S),
    append(Escaped, More, Codes).
query_string_codes([Code | Rest], [Code | More], S) :-
    query_string_codes(Rest, More, S).

query_escape(0'n, [0'\n]).
query_escape(0't, [0'\t]).
query_escape(0'r, [0'\r]).
query_escape(0'\\, [0'\\]).
query_escape(0'", [0'"]).
query_escape(Code, [0'\\, Code]).

query_ws([Code | Rest], S) :-
    code_type(Code, space),
    !,
    query_ws(Rest, S).
query_ws(S, S).

query_char(Code, [Code | Rest], Rest).

query_word(Text, S0, S) :-
    string_codes(Text, Codes),
    query_word_codes(Codes, S0, S).

query_word_codes([], S, S).
query_word_codes([Code | Codes], [Code | Rest], S) :-
    query_word_codes(Codes, Rest, S).

serialize_ts_query(ts_query(Patterns), Text) :-
    maplist(ts_pattern_text, Patterns, Parts),
    atomics_to_string(Parts, "\n", Text),
    !.
serialize_ts_query(Term, _) :-
    Term = sg_pattern(_, _, _),
    throw(unmapped_feature(slot_sg_metavariable_semantics, Term)).
serialize_ts_query(Term, _) :-
    throw(unmapped_feature(slot_ts_query_term, Term)).

ts_pattern_text(group(Root, Predicates), Text) :-
    ts_pattern_text(Root, RootText),
    maplist(ts_pattern_text, Predicates, PredicateTexts),
    append([RootText], PredicateTexts, Parts),
    atomics_to_string(Parts, " ", Inner),
    format(string(Text), "(~s)", [Inner]).
ts_pattern_text(node(Type, Children), Text) :-
    maplist(ts_pattern_text, Children, ChildTexts),
    ( ChildTexts == []
    -> format(string(Text), "(~w)", [Type])
    ;  atomics_to_string(ChildTexts, " ", ChildrenText),
       format(string(Text), "(~w ~s)", [Type, ChildrenText])
    ).
ts_pattern_text(field(Name, Pattern), Text) :-
    ts_pattern_text(Pattern, PatternText),
    format(string(Text), "~w: ~s", [Name, PatternText]).
ts_pattern_text(capture(Name, Pattern), Text) :-
    ts_pattern_text(Pattern, PatternText),
    format(string(Text), "~s @~w", [PatternText, Name]).
ts_pattern_text(capture_ref(Name), Text) :-
    format(string(Text), "@~w", [Name]).
ts_pattern_text(anonymous(Value), Text) :-
    ts_quoted(Value, Text).
ts_pattern_text(string(Value), Text) :-
    ts_quoted(Value, Text).
ts_pattern_text(predicate(eq, Left, Right), Text) :-
    ts_predicate_text("#eq?", Left, Right, Text).
ts_pattern_text(predicate(match, Left, Right), Text) :-
    ts_predicate_text("#match?", Left, Right, Text).
ts_pattern_text(predicate(not_match, Left, Right), Text) :-
    ts_predicate_text("#not-match?", Left, Right, Text).
ts_pattern_text(quant(optional, Pattern), Text) :-
    ts_quantified(Pattern, "?", Text).
ts_pattern_text(quant(zero_or_more, Pattern), Text) :-
    ts_quantified(Pattern, "*", Text).
ts_pattern_text(quant(one_or_more, Pattern), Text) :-
    ts_quantified(Pattern, "+", Text).
ts_pattern_text(alternative(Patterns), Text) :-
    maplist(ts_pattern_text, Patterns, Parts),
    atomics_to_string(Parts, " ", Inner),
    format(string(Text), "[~s]", [Inner]).
ts_pattern_text(wildcard, "_").
ts_pattern_text(named_wildcard, "(_)").
ts_pattern_text(Term, _) :-
    throw(unmapped_feature(slot_ts_pattern_form, Term)).

ts_predicate_text(Name, Left, Right, Text) :-
    ts_pattern_text(Left, LeftText),
    ts_pattern_text(Right, RightText),
    format(string(Text), "(~s ~s ~s)", [Name, LeftText, RightText]).

ts_quantified(Pattern, Glyph, Text) :-
    ts_pattern_text(Pattern, PatternText),
    string_concat(PatternText, Glyph, Text).

ts_quoted(Value, Quoted) :-
    string_codes(Value, Codes),
    phrase(ts_escaped_codes(Codes), Escaped),
    string_codes(EscapedString, Escaped),
    format(string(Quoted), "\"~s\"", [EscapedString]).

ts_escaped_codes([]) --> [].
ts_escaped_codes([0'\\ | Rest]) --> "\\\\", ts_escaped_codes(Rest).
ts_escaped_codes([0'" | Rest]) --> "\\\"", ts_escaped_codes(Rest).
ts_escaped_codes([Code | Rest]) --> [Code], ts_escaped_codes(Rest).

ts_query_capture_names(ts_query(Patterns), Names) :-
    query_capture_names(Patterns, [], Reversed),
    reverse(Reversed, Names).

query_capture_names([], Names, Names).
query_capture_names([Term | Rest], Seen, Names) :-
    query_capture_names_term(Term, Seen, More),
    query_capture_names(Rest, More, Names).

query_capture_names_term(capture(Name, Pattern), Seen, Names) :-
    ( memberchk(Name, Seen) -> Next = Seen ; Next = [Name | Seen] ),
    query_capture_names_term(Pattern, Next, Names).
query_capture_names_term(Term, Seen, Names) :-
    compound(Term),
    Term \= capture(_, _),
    Term \= capture_ref(_),
    Term =.. [_ | Arguments],
    query_capture_names(Arguments, Seen, Names).
query_capture_names_term(_, Names, Names).
