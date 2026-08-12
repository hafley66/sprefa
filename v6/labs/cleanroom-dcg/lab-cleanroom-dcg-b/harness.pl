% harness.pl -- measure .dl6 parse rate and round-trip rate.
% run: swipl -g run -t halt harness.pl > results.tsv

:- use_module('dcg.pl').
:- use_module('print.pl').

run :-
    expand_file_name('/Users/chrishafley/projects/sprefa/.boop-worktrees/lab/cleanroom-dcg-b/v6/prolog/compile/dl_view/*.dl6', Files),
    msort(Files, Sorted),
    format('file\tparse_ok\troundtrip_ok~n'),
    forall(member(F, Sorted),
        ( file_base_name(F, Base),
          read_file_to_string(F, S, []),
          once(parse_dcg(S, T1)),
          ( round_trip(T1) -> R = 1 ; R = 0 ),
          ( ground(T1) -> P = 1 ; P = 0 ),
          format('~w\t~w\t~w~n', [Base, P, R]) )).

round_trip(T1) :-
    print_program(T1, Text2),
    parse_dcg(Text2, T2),
    T1 == T2.
