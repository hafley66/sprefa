% fixtures/one_vs_any.pl : the any/one divergence ledger. FAIL-FIRST COMMIT.
%
% Every expectation below is written as what a caller ASKING FOR `one` would
% want, before any of it was measured. The three `one` attempts are red on this
% commit and the next commit writes what the language actually does beside each
% one. This file is the record the future `one { }` construct has to turn green.
%
% The wish, stated once: two arms fire on one tick, exactly one row lands, and
% the PROGRAM says which one -- not the arm order, not the arrival order, not
% the retention window.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).

% ═══ any ════════════════════════════════════════════════════════════════════
% Two arms, two triggers, one batch, one unbounded log head. Both rows land in
% the same tick and both survive. No arm order, no key, no retention: nothing in
% this shape can drop a row, which is what makes it `any`.
fixture(any_two_tagged_arms_land_on_one_tick,
  prog([ kind(dispatch_note/2, log), keep(dispatch_note/2, all) ],
       [ (dispatch_note(DispatchId, acked)  <+ dispatch_ack(DispatchId)),
         (dispatch_note(SealedId, sealed)   <+ dispatch_seal(SealedId)) ]),
  [],
  [ [ +dispatch_ack(1), +dispatch_seal(1) ] ],
  [ deltas(dispatch_note/2,
           [ [ +dispatch_note(1, acked), +dispatch_note(1, sealed) ] ]),
    final(dispatch_note/2, [ dispatch_note(1, acked),
                             dispatch_note(1, sealed) ]) ]).

% ═══ one, attempt 1: a keyed head ═══════════════════════════════════════════
% WISH: the arm the program wrote FIRST is the winner, so the survivor is a
% property of the source and not of what ran last.
fixture(one_attempt_keyed_head_loses_the_first_arm_silently,
  prog([ keyed(dispatch_winner/2, [1]) ],
       [ (dispatch_winner(DispatchId, acked) <+ dispatch_ack(DispatchId)),
         (dispatch_winner(SealedId, sealed)  <+ dispatch_seal(SealedId)) ]),
  [],
  [ [ +dispatch_ack(1), +dispatch_seal(1) ] ],
  [ deltas(dispatch_winner/2, [ [ +dispatch_winner(1, acked) ] ]),
    final(dispatch_winner/2, [ dispatch_winner(1, acked) ]) ]).

% ═══ one, attempt 2: a bounded log ══════════════════════════════════════════
% WISH: keep(count(1)) reads like "keep one", so two arms and a window of one
% leave one row standing.
fixture(one_attempt_bounded_log_two_arms_refused,
  prog([ kind(dispatch_first/2, log), keep(dispatch_first/2, count(1)) ],
       [ (dispatch_first(DispatchId, acked) <+ dispatch_ack(DispatchId)),
         (dispatch_first(SealedId, sealed)  <+ dispatch_seal(SealedId)) ]),
  [],
  [ [ +dispatch_ack(1), +dispatch_seal(1) ] ],
  [ deltas(dispatch_first/2, [ [ +dispatch_first(1, acked) ] ]),
    final(dispatch_first/2, [ dispatch_first(1, acked) ]) ]).

% ═══ one, attempt 3: guard by negation ══════════════════════════════════════
% WISH: whatever the two attempts above settle on, they settle on the SAME
% thing, so one language does not carry two answers to one question. Attempt 1
% is written expecting the first arm; this one is written expecting the last,
% and exactly one of the two can survive contact with the engine.
fixture(one_attempt_guard_by_negation_lands_one_unnamed_winner,
  prog([ kind(dispatch_first/2, log), keep(dispatch_first/2, all) ],
       [ (dispatch_first(DispatchId, acked) <+
            (dispatch_ack(DispatchId), not(dispatch_first(DispatchId, _AckTag)))),
         (dispatch_first(SealedId, sealed) <+
            (dispatch_seal(SealedId), not(dispatch_first(SealedId, _SealTag)))) ]),
  [],
  [ [ +dispatch_ack(1), +dispatch_seal(1) ] ],
  [ deltas(dispatch_first/2, [ [ +dispatch_first(1, sealed) ] ]),
    final(dispatch_first/2, [ dispatch_first(1, sealed) ]) ]).
