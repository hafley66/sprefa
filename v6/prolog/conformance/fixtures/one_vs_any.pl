% fixtures/one_vs_any.pl : the any/one divergence ledger.
%
% `any` IS expressible: two edge arms on one unbounded log head, both firing on
% the same tick, both rows landing, nothing choosing between them. That is the
% first fixture and it is the only one here that says what a caller wants.
%
% `one` is NOT expressible. The three fixtures after it are the honest attempts,
% each pinning what the language ACTUALLY does with the shape a caller would
% reach for, so the day a `one { }` construct exists these expectations are the
% record of what it has to change:
%
%   keyed head        the loser is not refused and not reported. It is
%                     overwritten inside the tick and never appears in a delta,
%                     so a reader of the tick log cannot tell that two arms
%                     fired at all.
%   bounded log       refused, retention_head_conflict_risk (ruling
%                     bounded_log_arm_order, rulings.pl: "refuse it").
%   guard by negation compiles on both doors and is NOT refused by the clock
%                     checker, which names the non-refusing boundary
%                     not_provable(arm_absence_batch_invariance(...)) once per
%                     arm. Exactly one row lands and the program never said
%                     WHICH, which is the defect this file is about. Ruling
%                     one_pick_order settles the referee: the winner is the
%                     first ARRIVAL, on both doors (attempt 3 grades the case
%                     where arrival order and source arm order agree, 3b the
%                     case where they disagree). The oracle already read
%                     arrival order; lower.pl:arrival_trigger_kind/4 is what
%                     stopped the emitted module reading source arm order.
%                     The oracle-side race pin is
%                     compile/test/3_clock_check.test.pl
%                     (two_arm_negation_guard_is_a_race_not_one).

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
           [ [ +dispatch_note(1, acked), +dispatch_note(1, sealed) ], [] ]),
    final(dispatch_note/2, [ dispatch_note(1, acked),
                             dispatch_note(1, sealed) ]) ]).

% ═══ one, attempt 1: a keyed head ═══════════════════════════════════════════
% The two arms have DIFFERENT triggers, so edge_head_conflict_risk (which needs
% a shared trigger ref) does not fire and this compiles. The key then folds the
% two writes to one row. The row that loses is not in the add list, not in the
% del list, and not in any refusal: the fold is silent.
fixture(one_attempt_keyed_head_loses_the_first_arm_silently,
  prog([ keyed(dispatch_winner/2, [1]) ],
       [ (dispatch_winner(DispatchId, acked) <+ dispatch_ack(DispatchId)),
         (dispatch_winner(SealedId, sealed)  <+ dispatch_seal(SealedId)) ]),
  [],
  [ [ +dispatch_ack(1), +dispatch_seal(1) ] ],
  [ deltas(dispatch_winner/2, [ [ +dispatch_winner(1, sealed) ], [] ]),
    final(dispatch_winner/2, [ dispatch_winner(1, sealed) ]) ]).

% ═══ one, attempt 2: a bounded log ══════════════════════════════════════════
% keep(count(1)) reads like "keep one". It is refused: retention prunes at tick
% end, so the survivor would be whichever arm ran last, and arm order is source
% line order. Refused for every N, not only 1.
fixture(one_attempt_bounded_log_two_arms_refused,
  prog([ kind(dispatch_first/2, log), keep(dispatch_first/2, count(1)) ],
       [ (dispatch_first(DispatchId, acked) <+ dispatch_ack(DispatchId)),
         (dispatch_first(SealedId, sealed)  <+ dispatch_seal(SealedId)) ]),
  [],
  [ [ +dispatch_ack(1), +dispatch_seal(1) ] ],
  [ throws(retention_head_conflict_risk(dispatch_first/2, count(1))) ]).

% ═══ one, attempt 3: guard by negation ══════════════════════════════════════
% Each arm reads not(head) over its own edge-headed head, so the second arm to
% run finds the row the first one wrote and drops out. Here the arrival order
% (ack first) and the source arm order (ack first) agree, so both doors answer
% `acked` whichever axis they read; 3b below reverses the arrivals so only the
% arrival axis can produce the graded answer.
fixture(one_attempt_guard_by_negation_lands_one_unnamed_winner,
  prog([ kind(dispatch_first/2, log), keep(dispatch_first/2, all) ],
       [ (dispatch_first(DispatchId, acked) <+
            (dispatch_ack(DispatchId), not(dispatch_first(DispatchId, _AckTag)))),
         (dispatch_first(SealedId, sealed) <+
            (dispatch_seal(SealedId), not(dispatch_first(SealedId, _SealTag)))) ]),
  [],
  [ [ +dispatch_ack(1), +dispatch_seal(1) ] ],
  [ deltas(dispatch_first/2, [ [ +dispatch_first(1, acked) ], [] ]),
    final(dispatch_first/2, [ dispatch_first(1, acked) ]) ]).

% ═══ one, attempt 3b: the two orders DISAGREE ═══════════════════════════════
% Same two arms, same source order (ack arm written first), one arrival list
% reversed: seal arrives first. Ruling one_pick_order makes the arrival index
% the only axis, so the winner is `sealed` on BOTH doors.
%
% FAIL-FIRST RECEIPT. Written before lower.pl:arrival_trigger_kind/4 existed,
% run through `just sweep` (the both-door leg), verbatim:
%   RUN total=203 identical=201 wrong=1 emitted_crash=0 rejection=1 no_oracle_log=0
%     WRONG one_attempt_guard_by_negation_arrival_order_beats_arm_order first
%     diff at line 1:
%       actual={"tick":1,"deltas":{"dispatch_ack":{"add":[[1]],"del":[]},
%         "dispatch_first":{"add":[[1,"acked"]],"del":[]},
%         "dispatch_seal":{"add":[[1]],"del":[]}}}
%       oracle={"tick":1,"deltas":{"dispatch_ack":{"add":[[1]],"del":[]},
%         "dispatch_first":{"add":[[1,"sealed"]],"del":[]},
%         "dispatch_seal":{"add":[[1]],"del":[]}}}
% The oracle needed no change; the emitted module was reading source arm order.
%
% dl surface and its pure-rxjs lowering (the shape the ruling names):
%     dispatch_first(DispatchId, acked)  <+ dispatch_ack(DispatchId),
%                                           not(dispatch_first(DispatchId, _)).
%     dispatch_first(SealedId, sealed)   <+ dispatch_seal(SealedId),
%                                           not(dispatch_first(SealedId, _)).
%   merge(ackArm$, sealArm$).pipe(
%     groupBy((occurrence) => occurrence.dispatchId),
%     mergeMap((perDispatch$) => perDispatch$.pipe(take(1))),
%   )
% merge, never concat: concat would drain the ack arm's whole batch before the
% seal arm's first occurrence, which is the arm-major order this fixture bans.
fixture(one_attempt_guard_by_negation_arrival_order_beats_arm_order,
  prog([ kind(dispatch_first/2, log), keep(dispatch_first/2, all) ],
       [ (dispatch_first(DispatchId, acked) <+
            (dispatch_ack(DispatchId), not(dispatch_first(DispatchId, _AckTag)))),
         (dispatch_first(SealedId, sealed) <+
            (dispatch_seal(SealedId), not(dispatch_first(SealedId, _SealTag)))) ]),
  [],
  [ [ +dispatch_seal(1), +dispatch_ack(1) ] ],
  [ deltas(dispatch_first/2, [ [ +dispatch_first(1, sealed) ], [] ]),
    final(dispatch_first/2, [ dispatch_first(1, sealed) ]) ]).
