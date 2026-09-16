program(
    [ rule(call(ref(total), [aggregate(count, var(item))]),
           [checked_goal(positive, call(ref(feed), [var(item)]))]),
      rule(call(ref(feed), [var(n)]),
           [checked_goal(positive, call(ref(total), [var(n)]))])
    ],
    [ call(ref(feed), [const(one)]) ]).
