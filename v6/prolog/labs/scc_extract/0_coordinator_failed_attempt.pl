:- use_module(library(clpfd)).
:- use_module(library(pairs)).
edges([a-b, b-a, b-c, c-d, d-c]).
succ_of(Map, V, Tos) :-
    ( member(Name-Var, Map), Var == V -> true ; Name = none ),
    edges(Es),
    findall(To, member(Name-To, Es), Names),
    findall(TVar, (member(N, Names), member(N-TVar, Map)), Tos).
go :-
    Map = [a-_A, b-_B, c-_C, d-_D],
    pairs_values(Map, Vs),
    ( catch(clpfd:scc(Vs, user:succ_of(Map)), E, (print_message(error,E),fail))
      -> writeln(scc_ok) ; writeln(scc_failed) ),
    forall(member(Name-V, Map),
      ( get_attr(V, lowlink, L) -> format("~w lowlink=~w~n",[Name,L])
      ; format("~w NO-LOWLINK~n",[Name]) )).
