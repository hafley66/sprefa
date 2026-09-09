:- module(dl7_parser, [read_dl7/5]).

:- use_module(library(error), [must_be/2]).

read_dl7(Path, Text, Forms, SourceRows, Diagnostics) :-
    must_be(ground, Path),
    must_be(text, Text),
    text_to_string(Text, String),
    string_codes(String, Codes),
    once(read_top_forms(Path, Codes, position(0, 1, 1), 0, Result)),
    reader_result(Result, Forms, SourceRows, Diagnostics).

reader_result(ok(Forms, SourceRows), Forms, SourceRows, []).
reader_result(error(Diagnostic), [], [], [Diagnostic]).

read_top_forms(Path, Codes0, Position0, Index0, Result) :-
    skip_layout(Codes0, Position0, Codes, Position),
    (   Codes == []
    ->  Result = ok([], [])
    ;   TopNodeId = reader_node(Path, Index0),
        read_term(Path, TopNodeId, Codes, Position, Index0, [], TermResult),
        continue_top_forms(Path, TermResult, Result)
    ).

continue_top_forms(_, error(Diagnostic), error(Diagnostic)).
continue_top_forms(Path,
                   ok(Form, FormRows, Codes, Position, Index, _Variables),
                   Result) :-
    read_top_forms(Path, Codes, Position, Index, RestResult),
    (   RestResult = ok(RestForms, RestRows)
    ->  append(FormRows, RestRows, SourceRows),
        Result = ok([Form | RestForms], SourceRows)
    ;   Result = RestResult
    ).

read_term(Path, TopNodeId, Codes, Start, Index0, Variables0, Result) :-
    NodeId = reader_node(Path, Index0),
    Index is Index0 + 1,
    read_term_kind(Codes, Path, TopNodeId, NodeId, Start, Index,
                   Variables0, Result).

read_term_kind([0'( | Codes0], Path, TopNodeId, NodeId, Start, Index,
               Variables0, Result) :-
    !,
    advance(0'(, Start, Position),
    read_form_items(Path, TopNodeId, NodeId, Codes0, Position, Index,
                    Variables0, ItemResult),
    finish_form(Path, NodeId, Start, ItemResult, Result).
read_term_kind([0'{ | Codes0], Path, _, NodeId, Start, Index,
               Variables, Result) :-
    !,
    advance(0'{, Start, Position),
    read_query_codes(Codes0, Position, outside_string, QueryResult),
    finish_query(Path, NodeId, Start, Index, Variables,
                 QueryResult, Result).
read_term_kind([0') | _], Path, _, NodeId, Position, _, _,
               error(Diagnostic)) :-
    !,
    reader_diagnostic(Path, NodeId, unexpected_closing_parenthesis,
                      Position, Diagnostic).
read_term_kind([0'} | _], Path, _, NodeId, Position, _, _,
               error(Diagnostic)) :-
    !,
    reader_diagnostic(Path, NodeId, unexpected_closing_query_brace,
                      Position, Diagnostic).
read_term_kind([0'" | Codes0], Path, _, NodeId, Start, Index,
               Variables, Result) :-
    !,
    advance(0'", Start, Position),
    read_string_codes(Codes0, Position, StringResult),
    finish_string(Path, NodeId, Start, Index, Variables,
                  StringResult, Result).
read_term_kind([0'' | Codes0], Path, _, NodeId, Start, Index,
               Variables, Result) :-
    !,
    advance(0'', Start, Position),
    take_token(Codes0, Position, NameCodes, Codes, End),
    finish_symbol(Path, NodeId, Start, End, Index, NameCodes, Codes,
                  Variables, Result).
read_term_kind([0'? | Codes0], Path, TopNodeId, NodeId, Start, Index,
               Variables0, Result) :-
    !,
    advance(0'?, Start, Position),
    take_token(Codes0, Position, NameCodes, Codes, End),
    finish_variable(Path, TopNodeId, NodeId, Start, End, Index,
                    NameCodes, Codes, Variables0, Result).
read_term_kind([], Path, _, NodeId, Position, _, _, error(Diagnostic)) :-
    reader_diagnostic(Path, NodeId, expected_term, Position, Diagnostic).
read_term_kind(Codes0, Path, _, NodeId, Start, Index, Variables, Result) :-
    take_token(Codes0, Start, TokenCodes, Codes, End),
    finish_token(Path, NodeId, Start, End, Index, TokenCodes, Codes,
                 Variables, Result).

read_form_items(Path, TopNodeId, FormNodeId, Codes0, Position0, Index0,
                Variables0, Result) :-
    skip_layout(Codes0, Position0, Codes, Position),
    (   Codes == []
    ->  reader_diagnostic(Path, FormNodeId, unterminated_form,
                          Position, Diagnostic),
        Result = error(Diagnostic)
    ;   Codes = [0') | Rest]
    ->  advance(0'), Position, End),
        Result = ok([], [], Rest, End, Index0, Variables0)
    ;   read_term(Path, TopNodeId, Codes, Position, Index0,
                  Variables0, ItemResult),
        continue_form_items(Path, TopNodeId, FormNodeId, ItemResult, Result)
    ).

continue_form_items(_, _, _, error(Diagnostic), error(Diagnostic)).
continue_form_items(Path, TopNodeId, FormNodeId,
                    ok(Item, ItemRows, Codes, Position, Index, Variables0),
                    Result) :-
    read_form_items(Path, TopNodeId, FormNodeId, Codes, Position, Index,
                    Variables0, RestResult),
    (   RestResult = ok(RestItems, RestRows, RestCodes, End, RestIndex,
                        Variables)
    ->  append(ItemRows, RestRows, SourceRows),
        Result = ok([Item | RestItems], SourceRows, RestCodes, End,
                    RestIndex, Variables)
    ;   Result = RestResult
    ).

finish_form(_, _, _, error(Diagnostic), error(Diagnostic)).
finish_form(Path, NodeId, Start,
            ok(Items, ItemRows, Codes, End, Index, Variables),
            ok(node(NodeId, form(Items)), [Source | ItemRows], Codes, End,
               Index, Variables)) :-
    source_row(NodeId, Path, Start, End, Source).

read_query_codes([], Position, _, error(unterminated_query, Position)).
read_query_codes([0'} | Codes], Position0, outside_string,
                 ok([], Codes, Position)) :-
    !,
    advance(0'}, Position0, Position).
read_query_codes([0'" | Codes0], Position0, outside_string, Result) :-
    !,
    advance(0'", Position0, Position),
    read_query_codes(Codes0, Position, inside_string, RestResult),
    prepend_query_codes([0'"], RestResult, Result).
read_query_codes([0'" | Codes0], Position0, inside_string, Result) :-
    !,
    advance(0'", Position0, Position),
    read_query_codes(Codes0, Position, outside_string, RestResult),
    prepend_query_codes([0'"], RestResult, Result).
read_query_codes([0'\\, Escape | Codes0], Position0, inside_string, Result) :-
    !,
    advance(0'\\, Position0, Position1),
    advance(Escape, Position1, Position2),
    read_query_codes(Codes0, Position2, inside_string, RestResult),
    prepend_query_codes([0'\\, Escape], RestResult, Result).
read_query_codes([Code | Codes0], Position0, State, Result) :-
    advance(Code, Position0, Position),
    read_query_codes(Codes0, Position, State, RestResult),
    prepend_query_codes([Code], RestResult, Result).

prepend_query_codes(_, error(Code, Position), error(Code, Position)).
prepend_query_codes(Prefix, ok(Rest, Codes, Position),
                    ok(All, Codes, Position)) :-
    append(Prefix, Rest, All).

finish_query(Path, NodeId, _Start, _Index, _Variables,
             error(Code, Position), error(Diagnostic)) :-
    reader_diagnostic(Path, NodeId, Code, Position, Diagnostic).
finish_query(Path, NodeId, Start, Index, Variables,
             ok(QueryCodes, Codes, End),
             ok(node(NodeId, literal(tree_sitter_query(Text))),
                [Source], Codes, End, Index, Variables)) :-
    string_codes(Text, QueryCodes),
    source_row(NodeId, Path, Start, End, Source).

read_string_codes([], Position, error(unterminated_string, Position)).
read_string_codes([0'" | Codes], Position0,
                  ok([], Codes, Position)) :-
    !,
    advance(0'", Position0, Position).
read_string_codes([0'\\ | []], Position0,
                  error(unterminated_string, Position)) :-
    !,
    advance(0'\\, Position0, Position).
read_string_codes([0'\\, Escape | Codes0], Position0, Result) :-
    !,
    advance(0'\\, Position0, Position1),
    advance(Escape, Position1, Position2),
    decoded_escape(Escape, Prefix),
    read_string_codes(Codes0, Position2, RestResult),
    prepend_string_codes(Prefix, RestResult, Result).
read_string_codes([Code | Codes0], Position0, Result) :-
    advance(Code, Position0, Position),
    read_string_codes(Codes0, Position, RestResult),
    prepend_string_codes([Code], RestResult, Result).

prepend_string_codes(_, error(Code, Position), error(Code, Position)).
prepend_string_codes(Prefix, ok(Rest, Codes, Position),
                     ok(All, Codes, Position)) :-
    append(Prefix, Rest, All).

decoded_escape(0'n, [0'\n]) :- !.
decoded_escape(0't, [0'\t]) :- !.
decoded_escape(0'r, [0'\r]) :- !.
decoded_escape(0'\\, [0'\\]) :- !.
decoded_escape(0'", [0'"]) :- !.
decoded_escape(Other, [0'\\, Other]).

finish_string(Path, NodeId, _Start, _Index, _Variables,
              error(Code, Position), error(Diagnostic)) :-
    reader_diagnostic(Path, NodeId, Code, Position, Diagnostic).
finish_string(Path, NodeId, Start, Index, Variables,
              ok(StringCodes, Codes, End),
              ok(node(NodeId, literal(String)), [Source], Codes, End,
                 Index, Variables)) :-
    string_codes(String, StringCodes),
    source_row(NodeId, Path, Start, End, Source).

finish_symbol(Path, NodeId, Start, _, _, NameCodes, _, _,
              error(Diagnostic)) :-
    \+ valid_identifier_codes(NameCodes),
    !,
    (   NameCodes == []
    ->  Code = expected_symbol_name
    ;   atom_codes(Name, NameCodes),
        Code = invalid_symbol_name(Name)
    ),
    reader_diagnostic(Path, NodeId, Code, Start, Diagnostic).
finish_symbol(Path, NodeId, Start, End, Index, NameCodes, Codes, Variables,
              ok(node(NodeId, literal(symbol(Name))), [Source], Codes, End,
                 Index, Variables)) :-
    atom_codes(Name, NameCodes),
    source_row(NodeId, Path, Start, End, Source).

finish_variable(Path, _, NodeId, Start, _, _, NameCodes, _, _,
                error(Diagnostic)) :-
    \+ valid_identifier_codes(NameCodes),
    !,
    (   NameCodes == []
    ->  Code = expected_variable_name
    ;   atom_codes(Name, NameCodes),
        Code = invalid_variable_name(Name)
    ),
    reader_diagnostic(Path, NodeId, Code, Start, Diagnostic).
finish_variable(Path, TopNodeId, NodeId, Start, End, Index, NameCodes, Codes,
                Variables0,
                ok(node(NodeId, variable(VariableId, Name)), [Source], Codes,
                   End, Index, Variables)) :-
    atom_codes(Name, NameCodes),
    variable_identity(Name, TopNodeId, NodeId, Variables0, Variables,
                      VariableId),
    source_row(NodeId, Path, Start, End, Source).

variable_identity('_', _, NodeId, Variables, Variables,
                  variable(NodeId, '_')) :-
    !.
variable_identity(Name, TopNodeId, _, Variables0, Variables, VariableId) :-
    (   memberchk(Name-Existing, Variables0)
    ->  VariableId = Existing,
        Variables = Variables0
    ;   VariableId = variable(TopNodeId, Name),
        Variables = [Name-VariableId | Variables0]
    ).

finish_token(Path, NodeId, Start, _, _, [], _, _, error(Diagnostic)) :-
    !,
    reader_diagnostic(Path, NodeId, expected_term, Start, Diagnostic).
finish_token(Path, NodeId, Start, End, Index, TokenCodes, Codes, Variables,
             Result) :-
    (   integer_codes(TokenCodes)
    ->  number_codes(Integer, TokenCodes),
        Payload = literal(Integer),
        source_row(NodeId, Path, Start, End, Source),
        Result = ok(node(NodeId, Payload), [Source], Codes, End,
                    Index, Variables)
    ;   valid_atom_codes(TokenCodes)
    ->  atom_codes(Name, TokenCodes),
        source_row(NodeId, Path, Start, End, Source),
        Result = ok(node(NodeId, atom(Name)), [Source], Codes, End,
                    Index, Variables)
    ;   atom_codes(Token, TokenCodes),
        reader_diagnostic(Path, NodeId, invalid_atom(Token), Start,
                          Diagnostic),
        Result = error(Diagnostic)
    ).

integer_codes([0'- | Digits]) :-
    Digits = [_ | _],
    maplist(decimal_digit, Digits),
    !.
integer_codes(Digits) :-
    Digits = [_ | _],
    maplist(decimal_digit, Digits).

decimal_digit(Code) :-
    Code >= 0'0,
    Code =< 0'9.

valid_atom_codes(Codes) :-
    memberchk(Codes,
              [[0':], [0'*], [0'+], [0'-, 0'>],
               [0'<, 0'-], [0'<, 0'+]]),
    !.
valid_atom_codes(Codes) :-
    append(NameCodes, [0':], Codes),
    valid_identifier_codes(NameCodes),
    !.
valid_atom_codes(Codes) :- valid_identifier_codes(Codes).

valid_identifier_codes([First | Rest]) :-
    ( ascii_alpha(First) ; First =:= 0'_ ),
    maplist(identifier_rest_code, Rest).

identifier_rest_code(Code) :-
    ( ascii_alpha(Code) ; decimal_digit(Code) ),
    !.
identifier_rest_code(0'_).
identifier_rest_code(0'-).
identifier_rest_code(0'.).

ascii_alpha(Code) :-
    Code >= 0'a,
    Code =< 0'z,
    !.
ascii_alpha(Code) :-
    Code >= 0'A,
    Code =< 0'Z.

take_token([], Position, [], [], Position).
take_token([Code | Codes], Position, [], [Code | Codes], Position) :-
    term_delimiter(Code),
    !.
take_token([Code | Codes0], Position0, [Code | Token], Codes, Position) :-
    advance(Code, Position0, Position1),
    take_token(Codes0, Position1, Token, Codes, Position).

term_delimiter(Code) :- code_type(Code, space), !.
term_delimiter(0'().
term_delimiter(0')).
term_delimiter(0'{).
term_delimiter(0'}).
term_delimiter(0';).
term_delimiter(0'").

skip_layout([Code | Codes0], Position0, Codes, Position) :-
    code_type(Code, space),
    !,
    advance(Code, Position0, Position1),
    skip_layout(Codes0, Position1, Codes, Position).
skip_layout([0'; | Codes0], Position0, Codes, Position) :-
    !,
    advance(0';, Position0, Position1),
    skip_comment(Codes0, Position1, Rest, Position2),
    skip_layout(Rest, Position2, Codes, Position).
skip_layout(Codes, Position, Codes, Position).

skip_comment([], Position, [], Position).
skip_comment([0'\n | Codes], Position0, Codes, Position) :-
    !,
    advance(0'\n, Position0, Position).
skip_comment([Code | Codes0], Position0, Codes, Position) :-
    advance(Code, Position0, Position1),
    skip_comment(Codes0, Position1, Codes, Position).

advance(0'\n, position(Offset0, Line0, _), position(Offset, Line, 1)) :-
    !,
    Offset is Offset0 + 1,
    Line is Line0 + 1.
advance(_, position(Offset0, Line, Column0),
        position(Offset, Line, Column)) :-
    Offset is Offset0 + 1,
    Column is Column0 + 1.

source_row(NodeId, Path,
           position(StartOffset, StartLine, StartColumn),
           position(EndOffset, EndLine, EndColumn),
           source(NodeId, Path, StartOffset, EndOffset,
                  StartLine, StartColumn, EndLine, EndColumn)).

reader_diagnostic(Path, NodeId, Code,
                  position(Offset, Line, Column),
                  diagnostic(reader, Path, NodeId, Code,
                             position(Offset, Line, Column))).
