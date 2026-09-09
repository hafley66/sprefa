:- module(dl7_embedder,
          [ dl7/4,
            dl7_text_unit/5,
            dl7_text_unit/6
          ]).

:- use_module(library(crypto), [crypto_data_hash/3]).
:- use_module(library(error), [must_be/2]).
:- use_module(library(quasi_quotations),
              [ quasi_quotation_syntax/1,
                quasi_quotation_syntax_error/1,
                with_quasi_quotation_input/3
              ]).
:- use_module('0_parser', [read_dl7/5]).
:- use_module('1_expander', [expand_dl7/6]).
:- use_module('1a_syntax_grapher', [reify_syntax/4]).

:- quasi_quotation_syntax(dl7).

dl7_text_unit(Origin, ReaderPath, Text, Unit, Diagnostics) :-
    dl7_text_unit(Origin, ReaderPath, Text, Unit, _, Diagnostics).

dl7_text_unit(Origin, ReaderPath, Text, Unit, SyntaxGraphRows, Diagnostics) :-
    must_be(ground, Origin),
    must_be(ground, ReaderPath),
    must_be(text, Text),
    text_to_string(Text, String),
    crypto_data_hash(String, Digest,
                     [ algorithm(sha256),
                       encoding(utf8)
                     ]),
    read_dl7(ReaderPath, String, Forms, SourceRows, ReaderDiagnostics),
    reify_after_read(ReaderDiagnostics, Forms, SourceRows,
                     SyntaxGraphRows, SyntaxDiagnostics),
    expand_after_read(SyntaxDiagnostics, Forms, SourceRows,
                      ExpandedForms, ExpandedSourceRows,
                      ExpansionRows, Diagnostics),
    Unit = dl7_unit(Origin, content_sha256(Digest),
                    ExpandedForms, ExpandedSourceRows, ExpansionRows).

reify_after_read([], Forms, SourceRows, SyntaxGraphRows, Diagnostics) :-
    !,
    reify_syntax(Forms, SourceRows, SyntaxGraphRows, Diagnostics).
reify_after_read(Diagnostics, _, _, [], Diagnostics).

expand_after_read([], Forms, SourceRows,
                  ExpandedForms, ExpandedSourceRows,
                  ExpansionRows, Diagnostics) :-
    !,
    expand_dl7(Forms, SourceRows,
               ExpandedForms, ExpandedSourceRows,
               ExpansionRows, Diagnostics).
expand_after_read(Diagnostics, _, _, [], [], [], Diagnostics).

dl7(Content, SyntaxArguments, _VariableNames, Unit) :-
    (   SyntaxArguments == []
    ->  true
    ;   throw(error(domain_error(dl7_quasi_syntax, SyntaxArguments), _))
    ),
    with_quasi_quotation_input(
        Content, Stream,
        read_quotation(Stream, Origin, ReaderPath, Text)),
    dl7_text_unit(Origin, ReaderPath, Text, Unit, Diagnostics),
    finish_quotation(Diagnostics).

read_quotation(Stream, Origin, ReaderPath, Text) :-
    stream_property(Stream, position(Position)),
    stream_position_data(char_count, Position, StartOffset),
    stream_position_data(line_count, Position, StartLine),
    stream_position_data(line_position, Position, ZeroColumn),
    StartColumn is ZeroColumn + 1,
    quotation_source_file(Stream, SourceFile),
    read_string(Stream, _, Text),
    Origin = embedded(SourceFile,
                      position(StartOffset, StartLine, StartColumn)),
    ReaderPath = embedded(SourceFile, StartOffset).

quotation_source_file(Stream, SourceFile) :-
    (   stream_property(Stream, file_name(File)),
        catch(absolute_file_name(File, Canonical,
                                 [ access(read),
                                   file_errors(fail)
                                 ]),
              _, fail)
    ->  SourceFile = Canonical
    ;   stream_property(Stream, file_name(File))
    ->  SourceFile = File
    ;   SourceFile = '<quasi-quotation>'
    ).

finish_quotation([]) :-
    !.
finish_quotation(Diagnostics) :-
    quasi_quotation_syntax_error(dl7_diagnostics(Diagnostics)).
