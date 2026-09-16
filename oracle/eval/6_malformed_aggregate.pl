program(
    [ rule(call(ref(twice), [var(region), aggregate(count, var(item)), aggregate(count, var(item))]),
           [checked_goal(positive, call(ref(sale), [var(region), var(item)]))])
    ],
    [ call(ref(sale), [const(east), const(one)]) ]).
