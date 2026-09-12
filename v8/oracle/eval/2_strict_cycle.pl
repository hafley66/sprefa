program(
    [ rule(call(ref(left), [var(value)]),
           [checked_goal(negative, call(ref(right), [var(value)]))]),
      rule(call(ref(right), [var(value)]),
           [checked_goal(positive, call(ref(left), [var(value)]))])
    ],
    [ call(ref(left), [const(one)]) ]).
