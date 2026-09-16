program(
    [ rule(call(ref(loop), [var(x)]),
           [checked_goal(positive, call(ref(edge), [var(x), var(x)]))]),
      rule(call(ref(triple), [var(x), var(y), var(z)]),
           [ checked_goal(positive, call(ref(edge), [var(x), var(y)])),
             checked_goal(positive, call(ref(edge), [var(y), var(z)])),
             checked_goal(positive, call(ref(edge), [var(z), var(x)])) ]),
      rule(call(ref(fact), []), []),
      rule(call(ref(pinned), [const(k), var(y)]),
           [checked_goal(positive, call(ref(edge), [const(a), var(y)]))])
    ],
    [ call(ref(edge), [const(a), const(a)]),
      call(ref(edge), [const(a), const(b)]),
      call(ref(edge), [const(b), const(c)]),
      call(ref(edge), [const(c), const(a)]),
      call(ref(edge), [const(a), const(b), const(extra)]) ]).
