% A rule whose body needs its head bound (cons construct) is a function under
% demand. suffix_of/2 walks a list by demand from a bound list; the free-head
% closure of wrap/2 stays empty because cons cannot construct without a head.
program(
    [ rule(call(ref(wrap), [var(item), var(list)]),
           [ checked_goal(positive, call(ref(kernel(nil)), [var(empty)])),
             checked_goal(positive, call(ref(kernel(cons)), [var(item), var(empty), var(list)])) ]),
      rule(call(ref(wrapped), [var(list)]),
           [ checked_goal(positive, call(ref(thing), [var(item)])),
             checked_goal(positive, call(ref(wrap), [var(item), var(list)])) ]),
      rule(call(ref(wrap_twice), [var(list)]),
           [ checked_goal(positive, call(ref(wrapped), [var(inner)])),
             checked_goal(positive, call(ref(wrap), [var(inner), var(list)])) ]),
      rule(call(ref(chain), [var(from), var(to)]),
           [checked_goal(positive, call(ref(step), [var(from), var(to)]))]),
      rule(call(ref(chain), [var(from), var(to)]),
           [ checked_goal(positive, call(ref(step), [var(from), var(mid)])),
             checked_goal(positive, call(ref(chain), [var(mid), var(to)])) ]),
      rule(call(ref(reaches_end), [var(from)]),
           [ checked_goal(positive, call(ref(thing), [var(from)])),
             checked_goal(positive, call(ref(chain), [var(from), const(end)])) ])
    ],
    [ call(ref(thing), [const(one)]),
      call(ref(thing), [const(two)]),
      call(ref(step), [const(one), const(mid)]),
      call(ref(step), [const(mid), const(end)]),
      call(ref(step), [const(two), const(loop)]),
      call(ref(step), [const(loop), const(two)]) ]).
