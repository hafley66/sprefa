:- module(json_arrival,
          [ arrival_column_types/4,
            schedule_value/5
          ]).

:- use_module('../analyze', [rel_columns/5]).
:- use_module(library(http/json)).

arrival_column_types(Prog, Bindings, Ref, ColumnTypes) :-
    Ref = _Name/Arity,
    (   program_decls_rules(Prog, Decls, Rules),
        catch(rel_columns(Decls, Rules, Bindings, Ref, Columns), _, fail),
        length(Columns, Arity)
    ->  maplist(declared_column_type(Decls, Ref), Columns, ColumnTypes)
    ;   length(ColumnTypes, Arity),
        maplist(=(none), ColumnTypes)
    ).

program_decls_rules(prog(Decls, Rules), Decls, Rules).
program_decls_rules(program(Decls, Rules, _Queries), Decls, Rules).

declared_column_type(Decls, Ref, Column, Type) :-
    ( memberchk(col_type(Ref, Column, Declared), Decls) -> Type = Declared
    ; Type = none
    ).

schedule_value(Context, Rel, Type, Value, Term) :-
    ( json_arrival_type(Type)
    -> json_column_term(Context, Rel, Value, Term)
    ; structured_arrival_value(Type, Value)
    -> json_parsed_term(Value, Term)
    ; scalar_schedule_value(Value, Term)
    ).

json_arrival_type(json).
json_arrival_type(list(_)).

structured_arrival_value(Type, Value) :-
    Type \== none,
    Type \== int,
    Type \== text,
    Type \== bool,
    Type \== float,
    ( is_dict(Value) ; is_list(Value) ).

scalar_schedule_value(Value, Value) :- integer(Value), !.
scalar_schedule_value(Value, Value) :- float(Value), !.
scalar_schedule_value(Value, bool_lit(true)) :- Value == true, !.
scalar_schedule_value(Value, bool_lit(false)) :- Value == false, !.
scalar_schedule_value(Value, Atom) :- string(Value), !, atom_string(Atom, Value).
scalar_schedule_value(Value, Atom) :- term_to_atom(Value, Atom).

json_column_term(_Context, _Rel, Value, Term) :- string(Value), !,
    json_text_term(Value, Term).
json_column_term(_Context, _Rel, Value, Value) :- number(Value), !.
json_column_term(Context, Rel, Value, _) :-
    format(user_error,
           "~w: '~w' json column takes a json document as text, got ~q~n",
           [Context, Rel, Value]),
    halt(1).

json_text_term(Text, Term) :-
    (   catch(atom_json_dict(Text, Parsed, [value_string_as(string)]), Error, true)
    ->  ( var(Error) -> json_parsed_term(Parsed, Term)
        ; json_text_error(Text) )
    ; json_text_error(Text)
    ).

json_text_error(Text) :-
    format(user_error, "json_arrival: json column value is not valid json: ~q~n", [Text]),
    halt(1).

json_parsed_term(Parsed, obj(Sorted)) :- is_dict(Parsed), !,
    dict_pairs(Parsed, _, Pairs),
    findall(Key-Value,
            ( member(Key-Raw, Pairs), json_parsed_term(Raw, Value) ),
            Converted),
    keysort(Converted, Sorted).
json_parsed_term(Parsed, Terms) :- is_list(Parsed), !,
    maplist(json_parsed_term, Parsed, Terms).
json_parsed_term(Parsed, Atom) :- string(Parsed), !, atom_string(Atom, Parsed).
json_parsed_term(true, bool_lit(true)) :- !.
json_parsed_term(false, bool_lit(false)) :- !.
json_parsed_term(null, none) :- !.
json_parsed_term(Parsed, Parsed).
