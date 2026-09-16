% seminaive.pl : the semi-naive fixpoint. Known set + frontier; each round
% expands ONLY the frontier (never re-derives from the whole set), subtracts
% what is already known, unions in the rest. Terminates on cycles because a
% known fact can never re-enter the frontier.
% Run: swipl -q -l seminaive.pl -g go -g halt

:- use_module(library(lists)).
:- use_module(library(ordsets)).

kids(1, [2, 3]).
kids(2, [4]).
kids(3, [4]).
kids(4, [1]).                       % cycle back to the root

reach(Roots, Set) :-
    sort(Roots, Known),
    loop(Known, Known, Set).

loop(Known, [], Known) :- !.
loop(Known0, Frontier, Known) :-
    findall(Child,
            ( member(Parent, Frontier), kids(Parent, Cs), member(Child, Cs) ),
            Children0),
    sort(Children0, Children),
    ord_subtract(Children, Known0, New),
    ord_union(Known0, New, Known1),
    loop(Known1, New, Known).

check(reaches_all, ( reach([1], [1, 2, 3, 4]) )).      % cycle did not loop
check(frontier_only, ( reach([2], [1, 2, 3, 4]) )).    % 4 -> 1 -> 3 wraps round

go :- forall(check(N, G),
             ( catch(G, E, (print_message(error, E), fail))
             -> format("PASS  ~w~n", [N]) ; format("fail  ~w~n", [N]) )).
