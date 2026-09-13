program(
    [ rule(call(ref(edge_of), [var(owner), var(label), var(edge)]),
           [ checked_goal(positive, call(ref(field), [var(owner), var(label)])),
             checked_goal(positive, call(ref(kernel(edge_ref)), [var(owner), var(label), var(edge)])) ]),
      rule(call(ref(tagged), [var(result)]),
           [ checked_goal(positive, call(ref(field), [var(owner), var(label)])),
             checked_goal(positive, call(ref(kernel(edge_ref)), [var(owner), var(label), var(edge)])),
             checked_goal(positive, call(ref(kernel(nil)), [var(empty)])),
             checked_goal(positive, call(ref(kernel(cons)), [var(edge), var(empty), var(one)])),
             checked_goal(positive, call(ref(kernel(cons)), [var(owner), var(one), var(two)])),
             checked_goal(positive, call(ref(kernel(intern)), [ref(pair), var(two), var(result)])) ]),
      rule(call(ref(literal_vars), [var(result)]),
           [checked_goal(positive, call(ref(kernel(intern)), [ref(pair), const([var(owner), var(edge)]), var(result)]))])
    ],
    [ call(ref(field), [ref(node(1)), const("name")]),
      call(ref(field), [ref(node(1)), ref(type)]),
      call(ref(field), [const(not_a_ref), const(x)]) ]).
