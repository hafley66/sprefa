% Aggregate bodies match completed rows only: a kernel goal inside an
% aggregate body never computes a tuple.
program(
    [ rule(call(ref(tuple_count), [aggregate(count, var(list))]),
           [ checked_goal(positive, call(ref(item), [var(head)])),
             checked_goal(positive, call(ref(kernel(tuple)), [var(head), var(list)])) ]),
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
