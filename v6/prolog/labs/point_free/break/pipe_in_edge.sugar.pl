% BREAK M3-1. `|>` in an EDGE rule. Each cut makes the next stage's source a
% DERIVED rel, and an edge rule triggered by a derived rel fires one tick after
% the rel changes. A two-cut chain therefore delivers the head TWO ticks late.
% Expected refusal: pipe_in_edge_rule.
% break/pipe_in_edge_unsafe.dl6 is the skipped-refusal expansion; its tick
% numbers against break/pipe_in_edge_today.dl6 are the receipt.
sugar(prog(
  [ col_type(ping/2, client, text),
    col_type(ping/2, size, int),
    col_type(logged/2, client, text),
    col_type(logged/2, size, int),
    kind(logged/2, log),
    keep(logged/2, all)
  ],
  [ (logged(Client, Scaled) <+
       ( ping(Client, Size) )
     ~> ( Doubled := Size * 2 )
     ~> ( Scaled := Doubled + 1 ))
  ])).
