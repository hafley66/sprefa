:- module(documents, [
    open_document/4,
    change_document/4,
    close_document/1,
    document_diagnostics/2,
    document_symbols/2,
    hover_at/4,
    definition_at/4,
    references_at/4,
    completions/2
]).

:- use_module('0_schema').

:- dynamic document/7.

open_document(Uri, Version, Text, Diagnostics) :-
    replace_document(Uri, Version, Text, Diagnostics).

change_document(Uri, Version, Text, Diagnostics) :-
    replace_document(Uri, Version, Text, Diagnostics).

close_document(Uri) :-
    retractall(document(Uri, _, _, _, _, _, _)).

replace_document(Uri, Version, Text, Diagnostics) :-
    retractall(document(Uri, _, _, _, _, _, _)),
    scan_symbols(Text, Symbols),
    (   schema:parse_schema(Text, Declarations)
    ->  declaration_index(Declarations, Symbols, Definitions),
        semantic_diagnostics(Declarations, Symbols, Diagnostics)
    ;   Declarations = [],
        Definitions = [],
        Diagnostics = [diagnostic(span(0, 1), error, "Soup parser could not recover at this position")]
    ),
    assertz(document(Uri, Version, Text, Declarations, Symbols, Definitions, Diagnostics)).

document_diagnostics(Uri, Diagnostics) :-
    document(Uri, _, Text, _, _, _, Internal),
    maplist(lsp_diagnostic(Text), Internal, Diagnostics).

document_symbols(Uri, Symbols) :-
    document(Uri, _, Text, Declarations, _, Definitions, _),
    findall(Symbol, (
        member(Definition, Definitions),
        definition_symbol(Text, Declarations, Definition, Symbol)
    ), Symbols).

hover_at(Uri, Line, Character, Hover) :-
    document(Uri, _, Text, Declarations, Symbols, _, _),
    lsp_offset(Text, Line, Character, Offset),
    symbol_at(Symbols, Offset, symbol(Name, _, _)),
    semantic_name(Name, SemanticName),
    hover_text(Declarations, SemanticName, Markdown),
    Hover = _{contents:_{kind:"markdown", value:Markdown}}.

definition_at(Uri, Line, Character, Location) :-
    document(Uri, _, Text, _, Symbols, Definitions, _),
    lsp_offset(Text, Line, Character, Offset),
    symbol_at(Symbols, Offset, symbol(Name, _, _)),
    semantic_name(Name, SemanticName),
    member(definition(SemanticName, _, Span), Definitions),
    span_range(Text, Span, Range),
    Location = _{uri:Uri, range:Range}.

references_at(Uri, Line, Character, Locations) :-
    document(Uri, _, Text, _, Symbols, _, _),
    lsp_offset(Text, Line, Character, Offset),
    symbol_at(Symbols, Offset, symbol(Name, _, _)),
    semantic_name(Name, SemanticName),
    findall(_{uri:Uri, range:Range}, (
        member(symbol(SourceName, Start, End), Symbols),
        semantic_name(SourceName, SemanticName),
        span_range(Text, span(Start, End), Range)
    ), Locations).

completions(Uri, Items) :-
    document(Uri, _, _, Declarations, _, _, _),
    findall(Item, completion_item(Declarations, Item), Items).

completion_item(Declarations, _{label:Label, kind:7, detail:"Soup type"}) :-
    member(type_decl(Name, _), Declarations),
    atom_string(Name, Label).
completion_item(Declarations, _{label:Label, kind:6, detail:"Soup pattern"}) :-
    member(pattern_decl(Name, _), Declarations),
    atom_string(Name, Label).

declaration_index(Declarations, Symbols, Definitions) :-
    declaration_names(Declarations, Names),
    findall(definition(Name, Kind, Span), (
        member(Name-Kind, Names),
        first_symbol_span(Symbols, Name, Span)
    ), Definitions).

declaration_names(Declarations, Names) :-
    findall(Name-type, member(type_decl(Name, _), Declarations), Types),
    findall(Name-pattern, member(pattern_decl(Name, _), Declarations), Patterns),
    append(Types, Patterns, Names).

first_symbol_span([symbol(SourceName, Start, End)|_], Name, span(Start, End)) :-
    semantic_name(SourceName, Name),
    !.
first_symbol_span([_|Symbols], Name, Span) :-
    first_symbol_span(Symbols, Name, Span).

semantic_diagnostics(Declarations, Symbols, Diagnostics) :-
    findall(Diagnostic, undefined_type_diagnostic(Declarations, Symbols, Diagnostic), Undefined),
    findall(Diagnostic, duplicate_diagnostic(Declarations, Symbols, Diagnostic), Duplicates),
    append(Undefined, Duplicates, Diagnostics).

undefined_type_diagnostic(Declarations, Symbols, diagnostic(Span, error, Message)) :-
    member(type_decl(_, Type), Declarations),
    referenced_type(Type, Name),
    \+ builtin_type(Name),
    \+ member(type_decl(Name, _), Declarations),
    first_symbol_span(Symbols, Name, Span),
    format(string(Message), "Undefined type ~w", [Name]).

duplicate_diagnostic(Declarations, Symbols, diagnostic(Span, error, Message)) :-
    declaration_names(Declarations, Names),
    select(Name-Kind, Names, Rest),
    memberchk(Name-Kind, Rest),
    findall(FoundSpan, symbol_span(Symbols, Name, FoundSpan), [_First, Span|_]),
    format(string(Message), "Duplicate ~w declaration ~w", [Kind, Name]).

referenced_type(alias(Type), Name) :- referenced_type(Type, Name).
referenced_type(model(Fields), Name) :- member(field(_, Type), Fields), referenced_type(Type, Name).
referenced_type(optional(Type), Name) :- referenced_type(Type, Name).
referenced_type(array(Type), Name) :- referenced_type(Type, Name).
referenced_type(map(Key, _), Name) :- referenced_type(Key, Name).
referenced_type(map(_, Value), Name) :- referenced_type(Value, Name).
referenced_type(union(Variants), Name) :- member(Type, Variants), referenced_type(Type, Name).
referenced_type(Name, Name) :- atom(Name).

builtin_type(string).
builtin_type(int).
builtin_type(bool).

symbol_span(Symbols, Name, span(Start, End)) :-
    member(symbol(SourceName, Start, End), Symbols),
    semantic_name(SourceName, Name).

hover_text(Declarations, Name, Markdown) :-
    member(type_decl(Name, Type), Declarations),
    !,
    format(string(Markdown), "```soup\ntype ~w = ~q\n```", [Name, Type]).
hover_text(Declarations, Name, Markdown) :-
    member(pattern_decl(Name, Pattern), Declarations),
    !,
    format(string(Markdown), "```soup\npattern ~w = `~s`\n```", [Name, Pattern]).
hover_text(_, Name, Markdown) :-
    format(string(Markdown), "`~w`", [Name]).

definition_symbol(Text, Declarations, definition(Name, Kind, Span), Symbol) :-
    span_range(Text, Span, Range),
    symbol_kind(Kind, SymbolKind),
    definition_detail(Declarations, Name, Kind, Detail),
    atom_string(Name, Label),
    Symbol = _{name:Label, kind:SymbolKind, detail:Detail, range:Range, selectionRange:Range}.

symbol_kind(type, 23).
symbol_kind(pattern, 13).

definition_detail(Declarations, Name, type, Detail) :-
    member(type_decl(Name, Type), Declarations),
    format(string(Detail), "~q", [Type]).
definition_detail(Declarations, Name, pattern, Detail) :-
    member(pattern_decl(Name, Pattern), Declarations),
    format(string(Detail), "`~s`", [Pattern]).

lsp_diagnostic(Text, diagnostic(Span, Severity, Message), Lsp) :-
    span_range(Text, Span, Range),
    severity_number(Severity, Number),
    Lsp = _{range:Range, severity:Number, source:"soup", message:Message}.

severity_number(error, 1).
severity_number(warning, 2).

span_range(Text, span(Start, End), _{start:StartPosition, end:EndPosition}) :-
    offset_position(Text, Start, StartPosition),
    offset_position(Text, End, EndPosition).

lsp_offset(Text, TargetLine, TargetCharacter, Offset) :-
    string_chars(Text, Chars),
    lsp_offset_chars(Chars, TargetLine, TargetCharacter, 0, 0, 0, Offset).

lsp_offset_chars(_, Line, Character, Line, Character, Offset, Offset) :- !.
lsp_offset_chars([Char|Chars], TargetLine, TargetCharacter, Line0, Character0, Offset0, Offset) :-
    next_position(Char, Line0, Character0, Line1, Character1),
    Offset1 is Offset0 + 1,
    lsp_offset_chars(Chars, TargetLine, TargetCharacter, Line1, Character1, Offset1, Offset).

offset_position(Text, TargetOffset, _{line:Line, character:Character}) :-
    string_chars(Text, Chars),
    offset_position_chars(Chars, TargetOffset, 0, 0, 0, Line, Character).

offset_position_chars(_, Target, Target, Line, Character, Line, Character) :- !.
offset_position_chars([Char|Chars], Target, Offset0, Line0, Character0, Line, Character) :-
    next_position(Char, Line0, Character0, Line1, Character1),
    Offset1 is Offset0 + 1,
    offset_position_chars(Chars, Target, Offset1, Line1, Character1, Line, Character).
offset_position_chars([], _, _, Line, Character, Line, Character).

next_position('\n', Line0, _, Line, 0) :- !, Line is Line0 + 1.
next_position(Char, Line, Character0, Line, Character) :-
    char_code(Char, Code),
    (Code > 16'FFFF -> Width = 2 ; Width = 1),
    Character is Character0 + Width.

symbol_at([symbol(Name, Start, End)|_], Offset, symbol(Name, Start, End)) :-
    Start =< Offset,
    Offset < End,
    !.
symbol_at([_|Symbols], Offset, Symbol) :- symbol_at(Symbols, Offset, Symbol).

scan_symbols(Text, Symbols) :-
    string_chars(Text, Chars),
    scan_symbols(Chars, 0, Symbols).

scan_symbols([], _, []).
scan_symbols([Char|Chars], Offset, Symbols) :-
    (   identifier_start(Char)
    ->  take_identifier(Chars, Rest, [Char], NameChars),
        length(NameChars, Length),
        End is Offset + Length,
        atom_chars(Name, NameChars),
        Symbols = [symbol(Name, Offset, End)|Tail],
        scan_symbols(Rest, End, Tail)
    ;   Next is Offset + 1,
        scan_symbols(Chars, Next, Symbols)
    ).

take_identifier([Char|Chars], Rest, Acc, Name) :-
    identifier_continue(Char),
    !,
    append(Acc, [Char], Next),
    take_identifier(Chars, Rest, Next, Name).
take_identifier(Rest, Rest, Name, Name).

identifier_start(Char) :- char_type(Char, alpha) ; Char == '_'.
identifier_continue(Char) :- char_type(Char, alnum) ; Char == '_'.

semantic_name(Source, Name) :-
    atom_chars(Source, Chars),
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
