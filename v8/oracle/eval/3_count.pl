program(
    [ rule(call(ref(region_count), [var(region), aggregate(count, var(item))]),
           [checked_goal(positive, call(ref(sale), [var(region), var(item)]))]),
      rule(call(ref(big_region), [var(region)]),
           [ checked_goal(positive, call(ref(region_count), [var(region), var(n)])),
             checked_goal(positive, call(ref(kernel(int_gt)), [var(n), const(1)])) ])
    ],
    [ call(ref(sale), [const(east), const(one)]),
      call(ref(sale), [const(east), const(two)]),
      call(ref(sale), [const(west), const(three)]) ]).
