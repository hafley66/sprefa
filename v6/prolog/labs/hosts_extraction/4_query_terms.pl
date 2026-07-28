:- module(hosts_extraction_query_terms,
          [ compile_ts_query/2,
            compile_sg_pattern/2,
            feature_slot/2,
            query_has_feature/2,
            query_rx_lowering/1,
            sg_rx_lowering/1
          ]).

:- use_module(library(lists)).

feature_slot(node_types, mapped(node)).
feature_slot(field_names, mapped(field)).
feature_slot(captures, mapped(capture)).
feature_slot(anonymous_nodes, mapped(anonymous)).
feature_slot(predicate_eq, mapped(predicate_eq)).
feature_slot(predicate_match, mapped(predicate_match)).
feature_slot(quantifier_optional, mapped(quant_optional)).
feature_slot(quantifier_zero_or_more, mapped(quant_zero_or_more)).
feature_slot(quantifier_one_or_more, mapped(quant_one_or_more)).
feature_slot(alternations, mapped(alternative)).
feature_slot(wildcard_any, mapped(wildcard)).
feature_slot(wildcard_named, mapped(named_wildcard)).

compile_ts_query(ts_query(Patterns), Text) :-
    maplist(pattern_text, Patterns, Parts),
    atomics_to_string(Parts, "\n", Text),
    !.
compile_ts_query(Term, _) :-
    Term = sg_pattern(_, _, _),
    throw(unmapped_feature(slot_sg_metavariable_semantics, Term)).
compile_ts_query(Term, _) :-
    throw(unmapped_feature(slot_ts_query_term, Term)).

pattern_text(group(Root, Predicates), Text) :-
    pattern_text(Root, RootText),
    maplist(pattern_text, Predicates, PredicateTexts),
    append([RootText], PredicateTexts, Parts),
    atomics_to_string(Parts, " ", Inner),
    string_concat("(", Inner, Open),
    string_concat(Open, ")", Text).
pattern_text(node(Type, Children), Text) :-
    atom(Type),
    maplist(pattern_text, Children, ChildTexts),
    ( ChildTexts == []
    -> format(string(Text), "(~w)", [Type])
    ; atomics_to_string(ChildTexts, " ", ChildrenText),
      format(string(Text), "(~w ~s)", [Type, ChildrenText])
    ).
pattern_text(field(Name, Pattern), Text) :-
    atom(Name),
    pattern_text(Pattern, PatternText),
    format(string(Text), "~w: ~s", [Name, PatternText]).
pattern_text(capture(Name, Pattern), Text) :-
    atom(Name),
    pattern_text(Pattern, PatternText),
    format(string(Text), "~s @~w", [PatternText, Name]).
pattern_text(capture_ref(Name), Text) :-
    atom(Name),
    format(string(Text), "@~w", [Name]).
pattern_text(anonymous(Value), Text) :-
    quoted(Value, Quoted),
    Text = Quoted.
pattern_text(string(Value), Text) :-
    quoted(Value, Quoted),
    Text = Quoted.
pattern_text(predicate(eq, Left, Right), Text) :-
    pattern_text(Left, LeftText),
    pattern_text(Right, RightText),
    format(string(Text), "(#eq? ~s ~s)", [LeftText, RightText]).
pattern_text(predicate(match, Left, Right), Text) :-
    pattern_text(Left, LeftText),
    pattern_text(Right, RightText),
    format(string(Text), "(#match? ~s ~s)", [LeftText, RightText]).
pattern_text(quant(optional, Pattern), Text) :-
    quantified(Pattern, "?", Text).
pattern_text(quant(zero_or_more, Pattern), Text) :-
    quantified(Pattern, "*", Text).
pattern_text(quant(one_or_more, Pattern), Text) :-
    quantified(Pattern, "+", Text).
pattern_text(alternative(Patterns), Text) :-
    maplist(pattern_text, Patterns, Parts),
    atomics_to_string(Parts, " ", Inner),
    format(string(Text), "[~s]", [Inner]).
pattern_text(wildcard, "_").
pattern_text(named_wildcard, "(_)").
pattern_text(Term, _) :-
    throw(unmapped_feature(slot_ts_pattern_form, Term)).

quantified(Pattern, Glyph, Text) :-
    pattern_text(Pattern, PatternText),
    string_concat(PatternText, Glyph, Text).

quoted(Value, Quoted) :-
    string_codes(Value, Codes),
    phrase(escaped_codes(Codes), Escaped),
    string_codes(EscapedString, Escaped),
    string_concat("\"", EscapedString, Open),
    string_concat(Open, "\"", Quoted).

escaped_codes([]) --> [].
escaped_codes([0'\\ | Rest]) --> "\\\\", escaped_codes(Rest).
escaped_codes([0'" | Rest]) --> "\\\"", escaped_codes(Rest).
escaped_codes([Code | Rest]) --> [Code], escaped_codes(Rest).

query_has_feature(Query, node_types) :-
    sub_term(node(_, _), Query).
query_has_feature(Query, field_names) :-
    sub_term(field(_, _), Query).
query_has_feature(Query, captures) :-
    sub_term(capture(_, _), Query).
query_has_feature(Query, anonymous_nodes) :-
    sub_term(anonymous(_), Query).
query_has_feature(Query, predicate_eq) :-
    sub_term(predicate(eq, _, _), Query).
query_has_feature(Query, predicate_match) :-
    sub_term(predicate(match, _, _), Query).
query_has_feature(Query, quantifier_optional) :-
    sub_term(quant(optional, _), Query).
query_has_feature(Query, quantifier_zero_or_more) :-
    sub_term(quant(zero_or_more, _), Query).
query_has_feature(Query, quantifier_one_or_more) :-
    sub_term(quant(one_or_more, _), Query).
query_has_feature(Query, alternations) :-
    sub_term(alternative(_), Query).
query_has_feature(Query, wildcard_any) :-
    sub_term(wildcard, Query).
query_has_feature(Query, wildcard_named) :-
    sub_term(named_wildcard, Query).

compile_sg_pattern(
    sg_pattern(language(Language), source(Source), captures(Captures)),
    sg_plan(Language, Source, Captures)) :-
    atom(Language),
    string(Source),
    is_list(Captures),
    !.
compile_sg_pattern(Term, _) :-
    throw(unmapped_feature(slot_sg_pattern_form, Term)).

query_rx_lowering(
    "fileDemand$.pipe(groupBy(({fileDigest, queryDigest}) => fileDigest + ':' + queryDigest), mergeMap(runTreeSitterQuery), mergeMap(commitEdbArrival))").
sg_rx_lowering(
    "fileDemand$.pipe(groupBy(({fileDigest, patternDigest}) => fileDigest + ':' + patternDigest), mergeMap(runAstGrepPattern), mergeMap(commitEdbArrival))").
