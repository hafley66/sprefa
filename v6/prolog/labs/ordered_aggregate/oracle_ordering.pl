json_text(Values, Text) :-
    maplist(json_string, Values, JsonValues),
    atomic_list_concat(JsonValues, ',', Inner),
    format(atom(Text), '[~w]', [Inner]).

json_string(Value, Text) :-
    format(atom(Text), '"~w"', [Value]).

value_axis(Text) :-
    msort(["pear", "apple", "orange"], SortedValues),
    json_text(SortedValues, Text).

ordinal_axis(Text) :-
    keysort([1-"pear", 3-"apple", 2-"orange"], SortedPairs),
    pairs_values(SortedPairs, SortedValues),
    json_text(SortedValues, Text).

run :-
    value_axis(ValueText),
    ordinal_axis(OrdinalText),
    format('{"value_sort":~w,"ordinal":~w}~n', [ValueText, OrdinalText]).
