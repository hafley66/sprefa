% BREAK M1-2. `scan` in a LEVEL head. A level rule is recomputed from the
% current world each tick and has no occurrence to advance a fold on, so there
% is nothing for the accumulator to accumulate over.
% Expected refusal: scan_in_level_rule.
sugar(prog(
  [ col_type(hit/2, page, text),
    col_type(hit/2, weight, int),
    col_type(score/2, page, text),
    col_type(score/2, total, int),
    keyed(score/2, [1])
  ],
  [ (score(Page, scan(Carried, 0, Carried + Weight)) <- hit(Page, Weight))
  ])).
