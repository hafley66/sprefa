walk([], Total, Total).
walk([Item | Rest], Acc, Total) :-
    (   Item < 0
    ->  Next = Acc
    ;   Item > 100
    ->  !,
        Next = 100
    ;   Next is Acc + Item
    ),
    \+ skip(Item),
    walk(Rest, Next, Total).

pick(Item) :- small(Item) ; big(Item).
