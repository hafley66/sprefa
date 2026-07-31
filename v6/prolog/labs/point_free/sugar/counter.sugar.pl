% M1 sugar: the running counter. One rule.
%
% The `.dl6` spelling this term stands for:
%
%   total(Counter, scan(Prev, 0, Prev + Delta)) <+ tick_event(Counter, Delta).
%
% rx lowering:
%   tickEvent$.pipe(
%     groupBy(event => event.counter),
%     mergeMap(group => group.pipe(scan((prev, event) => prev + event.delta, 0),
%                                  map(amount => ({ counter: group.key, amount })))))
sugar(prog(
  [ col_type(tick_event/2, counter, text),
    col_type(tick_event/2, delta, int),
    col_type(total/2, counter, text),
    col_type(total/2, amount, int),
    keyed(total/2, [1])
  ],
  [ (total(Counter, scan(Prev, 0, Prev + Delta)) <+ tick_event(Counter, Delta))
  ])).
