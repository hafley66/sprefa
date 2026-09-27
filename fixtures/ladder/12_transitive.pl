program(
    [ rule(call(ref(path), [var(from), var(to)]),
           [checked_goal(positive, call(ref(edge), [var(from), var(to)]))]),
      rule(call(ref(path), [var(from), var(to)]),
           [ checked_goal(positive, call(ref(path), [var(from), var(via)])),
             checked_goal(positive, call(ref(edge), [var(via), var(to)])) ])
    ],
    [ call(ref(edge), [const(a), const(b)]),
      call(ref(edge), [const(b), const(c)]),
      call(ref(edge), [const(c), const(d)]),
      call(ref(edge), [const(d), const(a)]) ]).
