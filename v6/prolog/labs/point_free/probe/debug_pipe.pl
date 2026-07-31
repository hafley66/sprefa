% Scratch driver: read the pipe sugar and print each expansion stage, so a
% refusal names which pass threw it.
:- use_module('../expand').
:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(1100, xfy, ~>).

go :-
    open('sugar/sensor_pipeline.sugar.pl', read, Stream),
    read_term(Stream, sugar(Program), [variable_names(_)]),
    close(Stream),
    format("read: ~q~n~n", [Program]),
    ( catch(expand_pipe(Program, Piped), E1, ( format("pipe threw ~q~n", [E1]), fail ))
    -> format("piped: ~q~n~n", [Piped])
    ;  format("pipe failed~n") ),
    ( catch(expand_seq(Piped, Seqd), E2, ( format("seq threw ~q~n", [E2]), fail ))
    -> format("seqd: ~q~n", [Seqd])
    ;  format("seq failed~n") ).
