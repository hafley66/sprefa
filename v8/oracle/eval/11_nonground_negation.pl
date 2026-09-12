program(
    [ rule(call(ref(lonely), [var(value)]),
           [ checked_goal(negative, call(ref(taken), [var(value)])),
             checked_goal(positive, call(ref(source), [var(value)])) ]),
      rule(call(ref(free), [var(value)]),
           [ checked_goal(positive, call(ref(source), [var(value)])),
             checked_goal(negative, call(ref(taken), [var(value)])) ])
    ],
    [ call(ref(source), [const(one)]),
      call(ref(source), [const(two)]),
      call(ref(taken), [const(two)]) ]).
