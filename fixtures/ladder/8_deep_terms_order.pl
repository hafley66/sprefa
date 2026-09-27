program(
    [ rule(call(ref(seen), [var(v)]),
           [checked_goal(positive, call(ref(bag), [var(v)]))])
    ],
    [ call(ref(bag), [const(3)]),
      call(ref(bag), [const(-1)]),
      call(ref(bag), [const(zebra)]),
      call(ref(bag), [const(apple)]),
      call(ref(bag), [const("zebra")]),
      call(ref(bag), [const("apple")]),
      call(ref(bag), [const([])]),
      call(ref(bag), [const([b])]),
      call(ref(bag), [const([a, b])]),
      call(ref(bag), [const(f(x))]),
      call(ref(bag), [const(f(x, y))]),
      call(ref(bag), [const(g(a))]),
      call(ref(bag), [const('Quoted Atom')]),
      call(ref(bag), [ref(application(option, [primitive(text)]))]),
      call(ref(bag), [ref(edge(node(1), "name"))]) ]).
