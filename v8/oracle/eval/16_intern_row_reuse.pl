% An intern request row from a lower stratum is a real row: a later stratum
% can match it with the constructor unbound, which the kernel function cannot.
program(
    [ rule(call(ref(made), [var(result)]),
           [ checked_goal(positive, call(ref(seed), [var(value)])),
             checked_goal(positive, call(ref(kernel(nil)), [var(empty)])),
             checked_goal(positive, call(ref(kernel(cons)), [var(value), var(empty), var(args)])),
             checked_goal(positive, call(ref(kernel(intern)), [ref(box), var(args), var(result)])) ]),
      rule(call(ref(who_made), [var(ctor), var(result)]),
           [ checked_goal(positive, call(ref(made), [var(result)])),
             checked_goal(negative, call(ref(hidden), [var(result)])),
             checked_goal(positive, call(ref(kernel(intern)), [var(ctor), var(args), var(result)])) ])
    ],
    [ call(ref(seed), [const(1)]),
      call(ref(seed), [ref(thing)]) ]).
