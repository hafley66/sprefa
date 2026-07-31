% BREAK M1-3, found in the shipped corpus rather than invented:
% conformance/fixtures/merge_family.pl:91-92 folds ONE accumulator from TWO
% different triggers,
%
%   counter(Name, Next) <+ increment(Name, _), pre(counter(Name, Total)), Next := Total + 1.
%   counter(Name, Next) <+ decrement(Name, _), pre(counter(Name, Total)), Next := Total - 1.
%
% which is two rules with no base arm at all (the fixture seeds counter(clicks, 0)
% from the world). Written with `scan` it is two scan rules on one head, and
% each of them expands to a base arm PLUS a step arm -- four rules where today
% has two. The sugar makes this program LONGER.
%
% There is no refusal for it: the expansion is correct, it just costs more. The
% receipt is the rule count, and it is the reason M1's census row is the weakest
% of the three.
sugar(prog(
  [ col_type(increment/1, name, text),
    col_type(decrement/1, name, text),
    col_type(counter/2, name, text),
    col_type(counter/2, total, int),
    keyed(counter/2, [1])
  ],
  [ (counter(Name, scan(Carried, 0, Carried + 1)) <+ increment(Name)),
    (counter(Name, scan(Carried, 0, Carried - 1)) <+ decrement(Name))
  ])).
