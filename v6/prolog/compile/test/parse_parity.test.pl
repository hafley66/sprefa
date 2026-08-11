:- use_module(library(plunit)).
:- use_module('../scripts/parse_parity', [compare_files/3]).

:- dynamic(parse_parity_test_dir/1).
:- prolog_load_context(directory, Here), assertz(parse_parity_test_dir(Here)).

:- begin_tests(parse_parity).

test(pinned_same_input_splice) :-
    pinned_files(Files),
    compare_files(Files, _Mode, summary(_Parity, _Skips, Diffs)),
    Diffs =:= 0.

:- end_tests(parse_parity).

pinned_files(Files) :-
    parse_parity_test_dir(TestDir),
    maplist(relative_file(TestDir),
            [ '../dl_view/key_last_write_wins.dl6',
              '../../../dl/fixtures/conformance.dl6',
              '../../../tsv2/gen/pokeapi_gen.dl6'
            ],
            Files).

relative_file(Base, Relative, File) :-
    directory_file_path(Base, Relative, Joined),
    absolute_file_name(Joined, File, [access(read)]).
