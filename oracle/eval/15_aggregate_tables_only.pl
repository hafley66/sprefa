% Aggregate bodies match completed rows only: a kernel goal inside an
% aggregate body finds the nil row (a real row) but never computes cons.
program(
    [ rule(call(ref(nil_count), [aggregate(count, var(empty))]),
           [checked_goal(positive, call(ref(kernel(nil)), [var(empty)]))]),
      rule(call(ref(cons_count), [aggregate(count, var(list))]),
           [ checked_goal(positive, call(ref(item), [var(head)])),
             checked_goal(positive, call(ref(kernel(nil)), [var(empty)])),
             checked_goal(positive, call(ref(kernel(cons)), [var(head), var(empty), var(list)])) ]),
      rule(call(ref(item_count), [aggregate(count, var(head))]),
           [checked_goal(positive, call(ref(item), [var(head)]))]),
      rule(call(ref(pair_count), [var(head), aggregate(count, var(other))]),
           [ checked_goal(positive, call(ref(item), [var(head)])),
             checked_goal(positive, call(ref(item), [var(other)])),
             checked_goal(negative, call(ref(kernel(int_eq)), [var(head), var(other)])) ])
    ],
    [ call(ref(item), [const(1)]),
      call(ref(item), [const(2)]),
      call(ref(item), [const(3)]) ]).
