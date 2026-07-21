:- module(schema, [type_decl/2, pattern_decl/2, consumer/4, parse_schema/2, load_schema/1]).

:- use_module(library(dcg/basics)).

:- dynamic type_decl/2, pattern_decl/2, consumer/4.

parse_schema(Source, Declarations) :-
    string_codes(Source, Codes),
    phrase(schema(Declarations), Codes).

load_schema(Path) :-
    read_file_to_string(Path, Source, []),
    parse_schema(Source, Declarations),
    retractall(type_decl(_, _)),
    retractall(pattern_decl(_, _)),
    retractall(consumer(_, _, _, _)),
    maplist(assert_declaration, Declarations).

assert_declaration(type_decl(Name, Type)) :- assertz(type_decl(Name, Type)).
assert_declaration(pattern_decl(Name, Pattern)) :- assertz(pattern_decl(Name, Pattern)).
assert_declaration(consumer(Kind, Action, Pattern, Result)) :-
    assertz(consumer(Kind, Action, Pattern, Result)).

schema(Declarations) -->
    layout,
    declarations(Declarations),
    eos.

declarations([Declaration|Declarations]) -->
    declaration(Declaration),
    !,
    layout,
    declarations(Declarations).
declarations([]) --> [].

declaration(type_decl(Name, model(Fields))) -->
    keyword("type"), identifier(Name), symbol("{"), fields(Fields), symbol("}").
declaration(type_decl(Name, Type)) -->
    keyword("type"), identifier(Name), symbol("="), type_expression(Expression), optional_semicolon,
    { named_type(Expression, Type) }.
declaration(pattern_decl(Name, Pattern)) -->
    keyword("pattern"), identifier(Name), symbol("="), backtick_string(Pattern), optional_semicolon.
declaration(consumer(Kind, Action, Pattern, Result)) -->
    keyword("consumer"), identifier(Kind), symbol("{"),
    identifier(Action), identifier(Pattern), symbol("->"), identifier(Result),
    optional_semicolon, symbol("}").

fields([Field|Fields]) --> field(Field), !, fields(Fields).
fields([]) --> [].

field(field(Name, Type)) -->
    identifier(Name), symbol(":"), type_expression(Type), optional_separator.

type_expression(Type) --> union_type(First, Rest), { union_result(First, Rest, Type) }.

union_type(First, Rest) --> postfix_type(First), union_tail(Rest).
union_tail([Type|Types]) --> symbol("|"), postfix_type(Type), !, union_tail(Types).
union_tail([]) --> [].

postfix_type(optional(Type)) --> primary_type(Type), symbol("?"), !.
postfix_type(array(Type)) --> primary_type(Type), symbol("["), symbol("]"), !.
postfix_type(Type) --> primary_type(Type).

primary_type(array(Type)) --> keyword("Array"), symbol("<"), type_expression(Type), symbol(">"), !.
primary_type(optional(Type)) --> keyword("Optional"), symbol("<"), type_expression(Type), symbol(">"), !.
primary_type(map(Key, Value)) -->
    keyword("Map"), symbol("<"), type_expression(Key), symbol(","), type_expression(Value), symbol(">"), !.
primary_type(literal(Value)) --> quoted_string(Value), !.
primary_type(Type) --> identifier(Type).

union_result(First, [], First).
union_result(First, Rest, union([First|Rest])).

named_type(union(Variants), union(Variants)) :- !.
named_type(Type, alias(Type)).

keyword(Word) --> layout, text(Word), identifier_boundary, layout.
symbol(Symbol) --> token(text(Symbol)).

token(Parser) --> layout, call(Parser), layout.
text(Text) --> { string_codes(Text, Codes) }, Codes.

identifier(Name) -->
    layout,
    identifier_codes(Codes),
    layout,
    { string_codes(Source, Codes), snake_name(Source, Name) }.

identifier_codes([Code|Codes]) -->
    [Code], { code_type(Code, csymf) }, identifier_rest(Codes).
identifier_rest([Code|Codes]) --> [Code], { code_type(Code, csym) }, !, identifier_rest(Codes).
identifier_rest([]) --> [].
identifier_boundary([Code|Rest], [Code|Rest]) :- \+ code_type(Code, csym), !.
identifier_boundary([], []).

quoted_string(Value) -->
    layout, "\"", string_without("\"", Codes), "\"", layout,
    { string_codes(Value, Codes) }.
backtick_string(Value) -->
    layout, "`", string_without("`", Codes), "`", layout,
    { string_codes(Value, Codes) }.

optional_separator --> symbol(";"), !.
optional_separator --> symbol(","), !.
optional_separator --> layout.
optional_semicolon --> symbol(";"), !.
optional_semicolon --> layout.

layout --> blanks, comment, !, layout.
layout --> blanks.
comment --> "//", string_without("\n", _), ("\n" ; eos).

snake_name(Source, Name) :-
    string_chars(Source, Chars),
    snake_chars(Chars, Raw),
    (Raw = ['_'|Snake] -> true ; Snake = Raw),
    atom_chars(Name, Snake).

snake_chars([], []).
snake_chars([Upper|Chars], ['_', Lower|Rest]) :-
    char_type(Upper, upper), !,
    downcase_atom(Upper, Lower),
    snake_chars(Chars, Rest).
snake_chars([Char|Chars], [Lower|Rest]) :-
    downcase_atom(Char, Lower),
    snake_chars(Chars, Rest).

load_default_schema :-
    source_file(schema:load_default_schema, File),
    file_directory_name(File, Directory),
    directory_file_path(Directory, 'schema.soup', Path),
    load_schema(Path).

:- initialization(load_default_schema).
