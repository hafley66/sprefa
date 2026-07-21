:- module(types, [canonical_type/2, accepts/2, type_path/2, type_name/2]).

:- use_module('0_schema').

canonical_type(Name, Type) :-
    atom(Name),
    schema:type_decl(Name, alias(Target)),
    !,
    canonical_type(Target, Type).
canonical_type(Name, Type) :-
    atom(Name),
    schema:type_decl(Name, Type),
    !.
canonical_type(Type, Type).

accepts(Type0, Value) :-
    canonical_type(Type0, Type),
    accepts_canonical(Type, Value).

accepts_canonical(string, Value) :- string(Value).
accepts_canonical(int, Value) :- integer(Value).
accepts_canonical(bool, Value) :- memberchk(Value, [true, false]).
accepts_canonical(literal(Expected), Value) :- Value == Expected.
accepts_canonical(union(Variants), Value) :-
    member(Variant, Variants),
    accepts(Variant, Value),
    !.
accepts_canonical(optional(_), null).
accepts_canonical(optional(Type), Value) :- accepts(Type, Value).
accepts_canonical(array(Type), Values) :-
    is_list(Values),
    maplist(accepts(Type), Values).
accepts_canonical(map(KeyType, ValueType), Dict) :-
    is_dict(Dict),
    dict_pairs(Dict, _, Pairs),
    maplist(accepts_pair(KeyType, ValueType), Pairs).
accepts_canonical(model(Fields), Dict) :-
    is_dict(Dict),
    maplist(accepts_field(Dict), Fields).

accepts_pair(KeyType, ValueType, Key-Value) :-
    atom_string(Key, KeyString),
    accepts(KeyType, KeyString),
    accepts(ValueType, Value).

accepts_field(Dict, field(Name, optional(Type))) :-
    (!, get_dict(Name, Dict, Value) -> accepts(Type, Value) ; true).
accepts_field(Dict, field(Name, Type)) :-
    get_dict(Name, Dict, Value),
    accepts(Type, Value).

type_path(Type, Path) :-
    type_path_segments(Type, [], Segments),
    atomics_to_string(Segments, "", Path).

type_path_segments(Type0, Seen, Segments) :-
    canonical_type(Type0, Type),
    type_path_canonical(Type0, Type, Seen, Segments).

type_path_canonical(Name, model(Fields), Seen, [FieldText|Tail]) :-
    atom(Name),
    \+ memberchk(Name, Seen),
    member(field(Field, Type), Fields),
    atom_string(Field, FieldText),
    type_path_segments(Type, [Name|Seen], Child),
    path_tail(Child, Tail).
type_path_canonical(_, optional(Type), Seen, Segments) :-
    type_path_segments(Type, Seen, Segments).
type_path_canonical(_, array(Type), Seen, ['[*]'|Tail]) :-
    type_path_segments(Type, Seen, Tail).
type_path_canonical(_, map(_, Type), Seen, ['{key}'|Tail]) :-
    type_path_segments(Type, Seen, Tail).
type_path_canonical(_, Type, _, []) :-
    memberchk(Type, [string, int, bool, literal(_), union(_)]).

path_tail([], []).
path_tail(Child, ['.'|Child]) :- Child \= [], Child \= ['[*]'|_], Child \= ['{key}'|_].
path_tail(Child, Child) :- Child = ['[*]'|_].
path_tail(Child, Child) :- Child = ['{key}'|_].

type_name(Source, Type) :-
    string_chars(Source, Chars),
    snake_chars(Chars, RawChars),
    (RawChars = ['_'|SnakeChars] -> true ; SnakeChars = RawChars),
    string_chars(Snake, SnakeChars),
    atom_string(Type, Snake).

snake_chars([], []).
snake_chars([Upper|Chars], ['_', Lower|Rest]) :-
    char_type(Upper, upper),
    !,
    downcase_atom(Upper, Lower),
    snake_chars(Chars, Rest).
snake_chars([Char|Chars], [Lower|Rest]) :-
    downcase_atom(Char, Lower),
    snake_chars(Chars, Rest).
