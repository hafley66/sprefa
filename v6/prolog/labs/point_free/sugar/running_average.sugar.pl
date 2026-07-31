% corpus 2 sugar. Two `scan` columns in ONE head = one fold over a pair, which
% is what rx's array accumulator says too.
%
%   rel running(sensor: text, total: int, seen: int) key(1).
%   running(Sensor, scan(CarriedTotal, 0, CarriedTotal + Value),
%                   scan(CarriedSeen,  0, CarriedSeen + 1))
%     <+ sample(Sensor, Value).
%   mean(Sensor, Average) <- running(Sensor, Total, Seen), Average := Total / Seen.
%
% rx lowering:
%   sample$.pipe(groupBy(s => s.sensor), mergeMap(group => group.pipe(
%     scan(([total, seen], s) => [total + s.value, seen + 1], [0, 0]),
%     map(([total, seen]) => ({ sensor: group.key, average: total / seen })))))
sugar(prog(
  [ col_type(sample/2, sensor, text),
    col_type(sample/2, value, int),
    col_type(running/3, sensor, text),
    col_type(running/3, total, int),
    col_type(running/3, seen, int),
    keyed(running/3, [1]),
    col_type(mean/2, sensor, text),
    col_type(mean/2, average, int)
  ],
  [ (running(Sensor,
             scan(CarriedTotal, 0, CarriedTotal + Value),
             scan(CarriedSeen, 0, CarriedSeen + 1))
       <+ sample(Sensor, Value)),
    (mean(Sensor, Average) <- running(Sensor, Total, Seen), Average := Total / Seen)
  ])).
