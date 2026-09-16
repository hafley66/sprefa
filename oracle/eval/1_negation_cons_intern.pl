program(
    [ rule(call(ref(lower), [var(source), var(result)]),
           [ checked_goal(positive, call(ref(kernel(nil)), [var(empty)])),
             checked_goal(positive, call(ref(kernel(cons)), [var(source), var(empty), var(arguments)])),
             checked_goal(positive, call(ref(kernel(intern)), [ref(option), var(arguments), var(result)])) ]),
      rule(call(ref(upper), [var(result)]),
           [ checked_goal(positive, call(ref(source), [var(source)])),
             checked_goal(negative, call(ref(blocked), [var(source)])),
             checked_goal(positive, call(ref(lower), [var(source), var(result)])) ])
    ],
    [ call(ref(source), [ref(primitive(text))]),
      call(ref(source), [ref(primitive(int))]),
      call(ref(blocked), [ref(primitive(int))]) ]).
