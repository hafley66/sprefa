:- dynamic edge/2.
:- table path/2.

path(X, Y) :- edge(X, Y).
path(X, Y) :- edge(X, Z), path(Z, Y).

elapsed(Goal, Milliseconds) :-
    get_time(Start),
    call(Goal),
    get_time(End),
    Milliseconds is (End - Start) * 1000.0.

assert_chain(Count, Cycle) :-
    retractall(edge(_, _)),
    Last is Count - 1,
    forall(
        between(0, Last, Source),
        (
            ( Cycle == true, Source =:= Last
            -> Target = 0
            ;  Target is Source + 1
            ),
            assertz(edge(Source, Target))
        )
    ).

run_assert(Count) :-
    elapsed(assert_chain(Count, false), AssertionMs),
    format(
        'engine=swi mode=assert n=~d assert_ms=~6f query_ms=0 answers=~d~n',
        [Count, AssertionMs, Count]
    ).

run_cycle(Count) :-
    abolish_all_tables,
    elapsed(assert_chain(Count, true), AssertionMs),
    elapsed(setof(Y, path(0, Y), Answers), QueryMs),
    length(Answers, AnswerCount),
    format(
        'engine=swi mode=cycle n=~d assert_ms=~6f query_ms=~6f answers=~d~n',
        [Count, AssertionMs, QueryMs, AnswerCount]
    ).

run_all_pairs(Count) :-
    abolish_all_tables,
    retractall(edge(_, _)),
    LastSource is Count - 2,
    elapsed(
        forall(
            between(0, LastSource, Source),
            (Target is Source + 1, assertz(edge(Source, Target)))
        ),
        AssertionMs
    ),
    elapsed(setof(X-Y, path(X, Y), Answers), QueryMs),
    length(Answers, AnswerCount),
    format(
        'engine=swi mode=all-pairs n=~d assert_ms=~6f query_ms=~6f answers=~d~n',
        [Count, AssertionMs, QueryMs, AnswerCount]
    ).

main([ModeAtom, CountAtom]) :-
    atom_number(CountAtom, Count),
    ( ModeAtom == assert -> run_assert(Count)
    ; ModeAtom == cycle -> run_cycle(Count)
    ; ModeAtom == 'all-pairs' -> run_all_pairs(Count)
    ; throw(error(domain_error(mode, ModeAtom), _))
    ).

:- initialization(main, main).

