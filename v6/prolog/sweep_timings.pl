% The sweep's per-fixture timing ledger, shared by the stages that write it.
%
% out/sweep.timings.tsv holds one row per unit of work a pass actually did:
% `fixture<TAB>stage<TAB>ms`. A cached fixture writes no row, so a fully cached
% pass leaves the file at its header and the slowest-ten report says the pass
% did nothing rather than restating the previous pass's numbers.
%
% scripts/sweep.sh truncates the file once at the head of a run; a stage run on
% its own recreates the header and appends to it. The file is gitignored: it is
% a measurement of this machine on this pass, not a fact about the corpus.
%
% One stage's whole block goes out in ONE write() against an O_APPEND fd: the
% compile and oracle stages now run CONCURRENTLY (scripts/sweep.sh), and
% line-at-a-time buffered writes from two processes into one file split lines
% at the buffer boundary.

:- module(sweep_timings, [ timings_path/2, append_timings/3, report_slowest/2 ]).

:- use_module(library(lists)).

timings_path(OutDir, Path) :-
    atomic_list_concat([OutDir, '/sweep.timings.tsv'], Path).

append_timings(OutDir, Stage, Entries) :-
    timings_path(OutDir, Path),
    (   exists_file(Path)
    ->  true
    ;   setup_call_cleanup(open(Path, write, Header),
                           format(Header, "fixture\tstage\tms~n", []),
                           close(Header))
    ),
    findall(Line,
            ( member(Name-Millis, Entries),
              format(atom(Line), "~w\t~w\t~w~n", [Name, Stage, Millis]) ),
            Lines),
    atomic_list_concat(Lines, Block),
    atom_length(Block, Length),
    Size is max(Length + 1, 4096),
    setup_call_cleanup(
        open(Path, append, Stream),
        ( set_stream(Stream, buffer(full)),
          set_stream(Stream, buffer_size(Size)),
          write(Stream, Block) ),
        close(Stream)).

% Descending on the millisecond count, so `sort/4` and not `msort/2`: the pair
% key has to be the number and duplicate counts have to survive.
report_slowest(Stage, Entries) :-
    findall(Millis-Name, member(Name-Millis, Entries), Pairs),
    sort(0, @>=, Pairs, Sorted),
    length(Sorted, Count),
    (   Count =:= 0
    ->  format("SWEEP_TIMINGS ~w: nothing to do this pass~n", [Stage])
    ;   Take is min(10, Count),
        length(Top, Take), append(Top, _, Sorted),
        format("SWEEP_TIMINGS ~w slowest ~w of ~w~n", [Stage, Take, Count]),
        forall(member(Millis-Name, Top), format("  ~w ~wms~n", [Name, Millis]))
    ).
