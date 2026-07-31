% BREAK M1-1. A `scan` head whose other columns are NOT the declared key.
% Here the head is a `log` rel, so `not(head)` is a whole-relation emptiness
% test and the fold reads whichever row `pre` happens to give it.
% Expected refusal: scan_head_not_keyed_on_group(trail/2, [1]).
% break/scan_on_log_head_unsafe.dl6 is what the expansion produces when the
% refusal is skipped, and its log is the receipt.
sugar(prog(
  [ col_type(hit/2, page, text),
    col_type(hit/2, weight, int),
    col_type(trail/2, page, text),
    col_type(trail/2, score, int),
    kind(trail/2, log),
    keep(trail/2, all)
  ],
  [ (trail(Page, scan(Carried, 0, Carried + Weight)) <+ hit(Page, Weight))
  ])).
