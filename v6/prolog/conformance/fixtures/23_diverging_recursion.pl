% FAIL-FIRST RECEIPT (measured before the cap landed, all three doors, same
% two rules): the conformance oracle ran 30s and was killed (rc=124); the
% emitted ts module ran 45s and was killed; the emitted rust module ran 45s
% and was killed. The compiler accepted the program in every one of those
% runs: manifest.json bucket `compiled`, reason empty.
%
% What makes it diverge: `Next := Value + 1` is a measure with no upper bound,
% so every hop of the wavefront derives a row nothing has derived before and
% the fixpoint is never reached. The cap is
% v6/prolog/lower.pl:fixpoint_round_cap/1 for the two emitted doors and
% v6/prolog/conformance/level_eval.pl:level_round_cap/1 for this oracle; the
% oracle counts a stratum-group pass and the doors count a wavefront hop, so
% the term names the group's heads here and the single head at the doors.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

fixture(diverging_measure_recursion_is_bounded_and_loud,
  prog([ col_type(seed_number/1, value, int),
         col_type(counter/1, value, int) ],
       [ (counter(Value) <- seed_number(Value)),
         (counter(Next) <- (counter(Value), Next := Value + 1)) ]),
  [],
  [ [ +seed_number(0) ] ],
  [ throws(diverging_measure_recursion([counter/1], 1000)) ]).

% SABOTAGE-ADJACENT CONTROL: the SAME two-rule shape with a measure that
% cannot grow past the base rows. Removing the cap leaves this one green, so
% a cap that fired on ordinary recursion would show up here and nowhere else.
fixture(bounded_measure_recursion_still_closes,
  prog([ col_type(link/2, from_node, int),
         col_type(link/2, to_node, int),
         col_type(reachable/2, from_node, int),
         col_type(reachable/2, to_node, int) ],
       [ (reachable(FromNode, ToNode) <- link(FromNode, ToNode)),
         (reachable(FromNode, ToNode) <-
            (reachable(FromNode, MiddleNode), link(MiddleNode, ToNode))) ]),
  [],
  [ [ +link(1, 2) ],
    [ +link(2, 3) ] ],
  [ final(reachable/2, [ reachable(1, 2), reachable(1, 3), reachable(2, 3) ]) ]).
