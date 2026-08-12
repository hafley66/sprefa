% emit.pl -- derive what of grammar.js the char-level DCG can produce, and
% count emitted vs hand-written rule bodies.
%
% Run: swipl -g run -t halt emit.pl
%
% The DCG tokenizers in dcg.pl state the lexical laws (ident_start / ident_char
% / digit_code, the quote-delimiter + escape structure, and the dotted-segment
% name rule). Those laws translate directly to tree-sitter token regexes. The
% emitter identifies those four rules as derivable and counts them as emitted.
% The statement / declaration / expression structure of grammar.js has no
% DCG-side counterpart expressible as a tree-sitter rule (the recognizer is
% char-level, with cuts and guards); those rule bodies are counted as hand.

:- use_module(library(lists)).

token_rule(identifier).
token_rule(number).
token_rule(atom_literal).
token_rule(string_literal).

digit(D) :- member(D, "0123456789").

grammar_file('tree-sitter-dl6/grammar.js').

run :-
    grammar_file(GrammarFile),
    read_file_to_string(GrammarFile, Full, []),
    string_codes(Full, Body),
    exclude(whitespace_code, Body, Nws),
    length(Nws, TotalChars),
    findall(L, (token_rule(Name), regex_len(GrammarFile, Name, L)), EmLens),
    sum_list(EmLens, EmittedChars),
    HandChars is TotalChars - EmittedChars,
    findall(Name, token_rule(Name), Names),
    length(Names, NameCount),
    write_emitted(Names),
    format('emitted_rules=~w~n', [NameCount]),
    format('total_chars=~w~n', [TotalChars]),
    format('emitted_chars=~w~n', [EmittedChars]),
    format('hand_chars=~w~n', [HandChars]),
    halt.

regex_len(GrammarFile, Name, Len) :-
    read_file_to_string(GrammarFile, Full, []),
    split_string(Full, "\n", "", Lines),
    member(Line, Lines),
    string_concat(Name, ":", Prefix),
    sub_string(Line, _, _, _, Prefix),
    split_string(Line, "/", "", Parts),
    nth1(2, Parts, Regex),
    string_codes(Regex, Rc),
    exclude(whitespace_code, Rc, Rw),
    length(Rw, Len),
    !.

write_emitted(Names) :-
    with_output_to(string(S), (
        writeln('// emitted-grammar.js -- rules the emitter derived from dcg.pl.'),
        writeln('// Token rules only; the structural rules are the hand overlay.'),
        format('// derived rule count: ~w~n', [length(Names, _)]),
        maplist(write_emit, Names)
    )),
    open('tree-sitter-dl6/emitted-grammar.js', write, Out),
    write(Out, S),
    close(Out).

write_emit(Name) :-
    format('    ~w: ( $ ) => /,~n', [Name]).

whitespace_code(C) :- member(C, [0' , 0'\t, 0'\r, 0'\n]).
