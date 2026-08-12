% harness.pl -- measure parse rate and round-trip rate over the corpus.
% Run: swipl -g run -t halt harness.pl > results.tsv

:- [dcg, print].

run :-
    corpus_dir(CorpusDir),
    directory_files(CorpusDir, Files),
    exclude(no_dl6, Files, Dls),
    msort(Dls, Sorted),
    format('file\tparse_ok\troundtrip_ok~n'),
    measure_all(Sorted),
    halt.

corpus_dir('../../../prolog/compile/dl_view').

no_dl6(F) :- \+ sub_atom(F, _, _, _, '.dl6').

measure_all([]).
measure_all([F|Fs]) :-
    corpus_dir(CorpusDir),
    atomic_list_concat([CorpusDir, '/', F], Path),
    read_file_to_string(Path, Text, []),
    ( parse_program(Text, T1) ->
        ( print_program(T1, S),
          parse_program(S, T2),
          ( T1 == T2 -> RT = 1 ; RT = 0 )
        -> ( PO = 1, RTO = RT )
        ; PO = 0, RTO = 0 )
    ; PO = 0, RTO = 0 ),
    format('~w\t~w\t~w~n', [F, PO, RTO]),
    measure_all(Fs).
