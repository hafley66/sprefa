:- module(dl7_file_loader,
          [ load_dl7/2,
            load_dl7/3
          ]).

:- use_module(library(error), [must_be/2]).
:- use_module(library(readutil), [read_file_to_string/3]).
:- use_module('2_embedder', [dl7_text_unit/5]).

load_dl7(Path, Unit) :-
    load_dl7(Path, Unit, Diagnostics),
    require_clean_unit(Path, Diagnostics).

load_dl7(Path, Unit, Diagnostics) :-
    must_be(text, Path),
    once(absolute_file_name(Path, CanonicalPath,
                            [ access(read),
                              file_errors(error)
                            ])),
    once(read_file_to_string(CanonicalPath, Text, [encoding(utf8)])),
    dl7_text_unit(file(CanonicalPath), CanonicalPath, Text,
                  Unit, Diagnostics).

require_clean_unit(_, []) :-
    !.
require_clean_unit(Path, Diagnostics) :-
    throw(error(dl7_source_diagnostics(Diagnostics),
                context(load_dl7/2, Path))).
