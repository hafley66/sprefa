% Runtime shootout arm: SWI-Prolog native logic.
%
% Algorithm (native logic only): build deterministic directed edges from N,
% then compute transitive closure with a tabled recursive reachability
% predicate. tabling memoizes every distinct (From, To) answer, so the
% closure is fully materialized in the table before counting via
% aggregate_all over the tabled predicate. Timing boundary: setup_ms covers
% edge assertion only; closure_ms covers recursive evaluation plus
% materialization, both measured with SWI's monotonic process wall clock
% (statistics(walltime/1)).

:- table reach/2.

:- dynamic edge/2.

main(Argv) :-
    Argv = [CaseAtom, NAtom],
    (   atom_number(NAtom, N), integer(N), N > 0
    ->  true
    ;   format(user_error, 'N must be an integer > 0: ~w~n', [NAtom]),
        halt(1)
    ),
    (   member(Case, [chain, ring]), atom_string(Case, CaseAtom)
    ->  true
    ;   format(user_error, 'CASE must be chain or ring: ~w~n', [CaseAtom]),
        halt(1)
    ),
    current_prolog_flag(version_data, swi(Major, Minor, Patch, _)),
    format(atom(RuntimeVersion), '~d.~d.~d', [Major, Minor, Patch]),
    statistics(walltime, [T0, _]),
    setup_edges(Case, N),
    statistics(walltime, [T1, _]),
    evaluate_closure,
    closure_count(ClosureCount),
    statistics(walltime, [T2, _]),
    aggregate_all(count, edge(_, _), EdgeCount),
    expected_closure(Case, N, Expected),
    (   ClosureCount =:= Expected
    ->  SetupMs is T1 - T0,
        ClosureMs is T2 - T1,
        json_write(current_output,
                   json(['runtime'-"swi",
                         'version'-RuntimeVersion,
                         'case'-Case,
                         'n'-N,
                         'edge_count'-EdgeCount,
                         'closure_count'-ClosureCount,
                         'setup_ms'-SetupMs,
                         'closure_ms'-ClosureMs]),
                   [width(0)])
    ;   format(user_error, 'closure mismatch: got ~d, expected ~d~n',
               [ClosureCount, Expected]),
        halt(1)
    ).

setup_edges(chain, N) :-
    NMinus2 is N - 2,
    forall(between(0, NMinus2, I),
           ( Next is I + 1, assertz(edge(I, Next)) )).
setup_edges(ring, N) :-
    NMinus1 is N - 1,
    forall(between(0, NMinus1, I),
           ( J is (I + 1) mod N, assertz(edge(I, J)) )).

% Solve a query through the tabled predicate from every source node so the
% full transitive closure is evaluated and materialized in the table.
evaluate_closure :-
    forall(reach(_, _), true).

reach(From, To) :-
    edge(From, To).
reach(From, To) :-
    edge(From, Mid),
    reach(Mid, To).

closure_count(ClosureCount) :-
    aggregate_all(count, reach(_, _), ClosureCount).

expected_closure(chain, N, Expected) :-
    Expected is N * (N - 1) / 2.
expected_closure(ring, N, Expected) :-
    Expected is N * N.

:- current_prolog_flag(argv, Argv),
   (   Argv = [CaseAtom, NAtom]
   ->  main([CaseAtom, NAtom])
   ;   format(user_error, 'usage: swipl -q -s 2_swi.pl -- CASE N~n', []),
       halt(1) ).
