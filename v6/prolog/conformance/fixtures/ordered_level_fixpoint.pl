% ordered_level_fixpoint.pl : a self-referential level head under an ORDERED
% program (one edge rule carrying seq/1 or pre/1) must still reach fixpoint.
%
% The pair below is one program with and without the ordered edge rule. The
% level plane is identical in both: two clauses on leg_total/3, a base clause
% and a self-referential step clause, fed one leg per tick so the chain has to
% grow to three links.

:- op(1150, xfx, <-).
:- op(1150, xfx, <+).
:- op(700,  xfx, :=).

% ═══ the ordered twin ═══════════════════════════════════════════════════════
% FAIL-FIRST RECEIPT. Written before emit_ts.pl's recomputeLevels looped, run
% through `just sweep` (the both-door leg), verbatim:
%   RUN total=205 identical=203 wrong=1 emitted_crash=0 rejection=1 no_oracle_log=0
%     WRONG ordered_program_level_fold_reaches_three_links first diff at line 3:
%       actual={"tick":3,"deltas":{"dispatch_leg":{"add":[[13,1,12,4]],"del":[]}}}
%       oracle={"tick":3,"deltas":{"dispatch_leg":{"add":[[13,1,12,4]],"del":[]},
%         "leg_total":{"add":[[13,1,9]],"del":[]}}}
%   FINAL total=205 final_identical=203 final_wrong=1 no_oracle_final=1
% The third link never landed: recomputeLevels ran DELETE plus one INSERT per
% head clause with no loop, so a self-referential head got exactly as many
% derivation rounds per tick as it had clauses, from an empty table, forever.
% Root cause ledger: sprefa-lab-foldwall/FOLDWALL.md (chain length tracked
% clause count exactly, measured 2 clauses -> 2 links, 3 clauses -> 3 links).
% ping_ordinal/2 is the whole reason this twin walled and the one below did
% not: seq/1 makes the program ordered, which routes every tick through
% runOrderedTick. It receives no arrivals in this schedule; its PRESENCE in
% the program is the trigger.
%
% dl surface and its pure-rxjs lowering:
%     ping_ordinal(NodeId, Ordinal) <+ ping(NodeId), Ordinal := seq("ping").
%     leg_total(LegId, DispatchId, Kilos) <-
%       dispatch_leg(LegId, DispatchId, 0, Kilos).
%     leg_total(LegId, DispatchId, KilosSoFar) <-
%       dispatch_leg(LegId, DispatchId, PreviousLeg, Kilos),
%       leg_total(PreviousLeg, DispatchId, KilosBefore),
%       KilosSoFar := KilosBefore + Kilos.
%   const legTotal$ = wipeHead$.pipe(
%     map(() => -1),
%     expand((priorRows) => runEveryClause$.pipe(
%       concatMap(() => countHeadRows$),
%       concatMap((rows) => (rows === priorRows ? EMPTY : of(rows))),
%     )),
%     last(),
%   )
% expand, never a fixed number of clause runs: the step clause has to re-read
% its own head until a round adds nothing.
fixture(ordered_program_level_fold_reaches_three_links,
  prog([ col_type(ping_ordinal/2, partition, text),
         kind(ping_ordinal/2, log),
         keep(ping_ordinal/2, all) ],
       [ (ping_ordinal(Partition, Ordinal) <+
            (ping(Partition), Ordinal := seq('ping'))),
         (leg_total(LegId, DispatchId, Kilos) <-
            dispatch_leg(LegId, DispatchId, 0, Kilos)),
         (leg_total(LegId, DispatchId, KilosSoFar) <-
            (dispatch_leg(LegId, DispatchId, PreviousLeg, Kilos),
             leg_total(PreviousLeg, DispatchId, KilosBefore),
             KilosSoFar := KilosBefore + Kilos)) ]),
  [],
  [ [ +dispatch_leg(11, 1, 0, 2) ],
    [ +dispatch_leg(12, 1, 11, 3) ],
    [ +dispatch_leg(13, 1, 12, 4) ] ],
  [ final(leg_total/3, [ leg_total(11, 1, 2),
                         leg_total(12, 1, 5),
                         leg_total(13, 1, 9) ]) ]).

% ═══ the unordered twin ═════════════════════════════════════════════════════
% Same level plane, no ordered edge rule, so the module dispatches
% runIncrementalTick, whose frontier-driven level plane already reached three
% links before the fix. It is here so a future regression names WHICH plane
% broke.
%
% The sweep runs this module in incremental mode only. Its NAIVE mode shared
% the ordered door's ceiling, because runNaiveTick calls the same
% recomputeLevels. Measured by hand on this module with recomputeLevels put
% back to its single-pass form (scripts/golden-run.ts, both env values):
%   SPREFA_TSV2_EMITTER_MODE=incremental  leg_total [[11,1,2],[12,1,5],[13,1,9]]
%   SPREFA_TSV2_EMITTER_MODE=naive        leg_total [[11,1,2],[12,1,5]]
% Both read three links once recomputeLevels loops.
fixture(unordered_program_level_fold_reaches_three_links,
  prog([],
       [ (leg_total(LegId, DispatchId, Kilos) <-
            dispatch_leg(LegId, DispatchId, 0, Kilos)),
         (leg_total(LegId, DispatchId, KilosSoFar) <-
            (dispatch_leg(LegId, DispatchId, PreviousLeg, Kilos),
             leg_total(PreviousLeg, DispatchId, KilosBefore),
             KilosSoFar := KilosBefore + Kilos)) ]),
  [],
  [ [ +dispatch_leg(11, 1, 0, 2) ],
    [ +dispatch_leg(12, 1, 11, 3) ],
    [ +dispatch_leg(13, 1, 12, 4) ] ],
  [ final(leg_total/3, [ leg_total(11, 1, 2),
                         leg_total(12, 1, 5),
                         leg_total(13, 1, 9) ]) ]).

fixture(recount_retraction_reaches_two_heads_same_tick,
  prog([],
       [ (b(Value) <- a(Value)),
         (c(Value) <- b(Value)) ]),
  [],
  [ [ +a(1) ],
    [ -a(1) ] ],
  [ deltas(b/1, [ [ +b(1) ], [ -b(1) ] ]),
    deltas(c/1, [ [ +c(1) ], [ -c(1) ] ]),
    final(b/1, []),
    final(c/1, []) ]).
