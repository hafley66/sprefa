:- module(patterns, [parse_pattern/2, pattern_string/2, pattern_value/3, slots/2]).

:- use_module('1_types').

parse_pattern(Source, Parts) :-
    string_codes(Source, Codes),
    phrase(pattern_parts(Parts), Codes).

pattern_string(Parts, Source) :-
    maplist(part_source, Parts, Sources),
    atomics_to_string(Sources, "", Source).

part_source(literal(Text), Text).
part_source(slot(Name, Type), Source) :-
    format(string(Source), "{~w:~w}", [Name, Type]).

pattern_parts([]) --> [].
pattern_parts([slot(Name, Type)|Rest]) -->
    "{", identifier(NameCodes), ":", blanks, identifier(TypeCodes), "}",
    { atom_codes(Name, NameCodes), string_codes(TypeSource, TypeCodes), types:type_name(TypeSource, Type) },
    pattern_parts(Rest).
pattern_parts([slot(Name, string)|Rest]) -->
    ":", identifier(NameCodes),
    { atom_codes(Name, NameCodes) },
    pattern_parts(Rest).
pattern_parts([literal(Text)|Rest]) -->
    literal_codes(Codes),
    { Codes \= [], string_codes(Text, Codes) },
    pattern_parts(Rest).

identifier([Code|Codes]) -->
    [Code], { code_type(Code, csymf) },
    identifier_rest(Codes).
identifier_rest([Code|Codes]) -->
    [Code], { code_type(Code, csym) },
    !,
    identifier_rest(Codes).
identifier_rest([]) --> [].

blanks --> [Code], { code_type(Code, space) }, !, blanks.
blanks --> [].

literal_codes([]) --> peek_slot, !.
literal_codes([]) --> eos, !.
literal_codes([Code|Codes]) --> [Code], literal_codes(Codes).

peek_slot, [Code] --> [Code], { memberchk(Code, [0'{, 0':]) }.
eos([], []).

slots(Parts, Slots) :-
    findall(Name-Type, member(slot(Name, Type), Parts), Slots).

pattern_value(Parts, Bindings, Text) :-
    (   nonvar(Text)
    ->  string_codes(Text, Codes), phrase(value_parts(Parts, Bindings), Codes)
    ;   render_parts(Parts, Bindings, Sources), atomics_to_string(Sources, "", Text)
    ).

render_parts([], [], []).
render_parts([literal(Text)|Parts], Bindings, [Text|Sources]) :-
    render_parts(Parts, Bindings, Sources).
render_parts([slot(Name, Type)|Parts], [Name-Value|Bindings], [Text|Sources]) :-
    types:accepts(Type, Value),
    format(string(Text), "~w", [Value]),
    render_parts(Parts, Bindings, Sources).

value_parts([], []) --> [].
value_parts([literal(Text)|Parts], Bindings) -->
    { string_codes(Text, Codes) },
    Codes,
    value_parts(Parts, Bindings).
value_parts([slot(Name, Type)|Parts], [Name-Value|Bindings]) -->
    slot_codes(Parts, Codes),
    { decode(Type, Codes, Value), types:accepts(Type, Value) },
    value_parts(Parts, Bindings).

slot_codes(_, Codes) -->
    { nonvar(Codes) },
    Codes.
slot_codes([], Codes) --> remainder(Codes).
slot_codes([literal(Delimiter)|_], Codes) -->
    { string_codes(Delimiter, DelimiterCodes) },
    until(DelimiterCodes, Codes).

remainder([]) --> [].
remainder([Code|Codes]) --> [Code], remainder(Codes).

until(Delimiter, []) --> lookahead(Delimiter), !.
until(Delimiter, [Code|Codes]) --> [Code], until(Delimiter, Codes).

lookahead(Codes, Input, Input) :- append(Codes, _, Input).

decode(string, Codes, Value) :- string_codes(Value, Codes).
decode(int, Codes, Value) :- number_codes(Value, Codes), integer(Value).
decode(bool, Codes, Value) :- string_codes(Text, Codes), member(Text-Value, ["true"-true, "false"-false]).
decode(Type, Codes, Value) :-
    types:canonical_type(Type, Canonical),
    Type \== Canonical,
    decode(Canonical, Codes, Value).
decode(union(_), Codes, Value) :- string_codes(Value, Codes).
decode(literal(_), Codes, Value) :- string_codes(Value, Codes).
