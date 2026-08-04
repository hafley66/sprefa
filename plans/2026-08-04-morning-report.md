# Morning report (overnight 2026-08-04, coordinator fable-main)

Five opus agents ran overnight per your word (released/readied/labbed). All
five landed. Two merged, three await your rulings.

## Shipped into sprefa main (audited, gates re-run by coordinator before merge)

1. **ticks$ decouple** (was: queue item). `share({ resetOnRefCountZero: false })`
   in serve/3_engine.ts + 109-line fail-first test. Engine keeps ticking with
   zero readers; late reader sees post-gap state; tick numbering unbroken.
   Deliberately out of scope: submit before ANY first subscriber is still an
   error (needs eager connect, separate card).
2. **Ladder step 2: prune-behind-flag** (`SPREFA_TSV2_SUBSCRIBE_PRUNE=on`,
   default off). Runtime SubscribeCone module + emitter wiring + 4-combination
   receipt (flag on/off x query-bearing/zero-query), flag-off identity proven
   by reference AND by text-door 202/202. Bonus: fixed the pre-existing
   typecheck red (subscribedRels missing on emitted program objects; green
   leg was red at base). Boot now rel-tagged (bootstmt/3). Post-merge main:
   plunit 324/324, conformance 289/0, text-door 202/202, golden-flex HOLDS,
   typecheck exit 0.

## Labbed for your decisions

3. **rxprim duel verdict** (plans/2026-08-04-rxprim-duel-verdict.md): fuse on
   the opus spine, 5 named grafts, 3 discards. YOUR TWO CALLS: reserved-door
   spelling `throttle(1)` vs shelf's `zip(tick)`; block sugar same wave or
   later. Worktrees sprefa-plan-rxprim-{kimi,flash,opus} kept until ruled.
4. **Fold wall root-caused** (sprefa-lab-foldwall, FOLDWALL.md, commit
   6f2533ac): defects 1a and 1b are ONE defect. Any ordered edge rule
   (seq/pre) routes the program onto runOrderedTick whose level plane is
   per-clause recompute with NO fixpoint loop (emit_ts.pl:1566-1582, dispatch
   :1953). Ceiling = clause count (2 clauses = 2 links; 3 clauses = exactly
   3, measured). Swapping the dispatch line to runIncrementalTick lands the
   third leg on the minimal repro and real golden-flex. Fix belongs on the
   ordered tick's level plane. DECISION: dispatch the fix lane? Also two
   sub-findings: MODE PARITY is vacuous for ordered programs (EMITTER_MODE
   emitted, never read), and the naive door walls independently on
   non-ordered programs.
5. **Host cone edge, deliberately not closed** (prune lane): __host_response_N
   does not co-subscribe __host_demand_N (impact doc 4.3 predicted it;
   measured: native_ts_query_term cone misses the demand rel). Invisible
   under replay, breaks LIVE hosts with the flag on. STANDING RULE: flag
   stays off for host-bearing programs until the edge lands. This IS your
   owed host co-subscription ruling, now with a measured consequence.

## Instant (owned by instant-fable, out of main chat)

- REVIEW-reactive.md delivered (12 findings: 3 more resurrection paths incl.
  every waterfall bar click, brush destroyed by the 8s refreshRogue tick,
  bump(1) React bailout, 640px frozen width, transient list_sessions failure
  permanently downgrading liveness, no live legs on InTabStrip/MailPreview,
  registrySeeds never ages, tickType "dispatch" declared but never produced,
  useAgentTree identity churn at the source). Priority list at its tail.
- instant-fable is STALLED at a permission prompt: it wants `bus list` to
  verify my envelope (anti-injection challenge working as designed). One
  keypress: `tmux attach -t instant-fable`, press 2. It has 6 unacked
  envelopes queued; nothing merges in instant until it moves.

## Pre-existing rot noticed by lanes (nobody's work item yet)

- roundtrip.sh rewrites 126 dl_view/*.dl6 at base (checked-in corpus printed
  by an older printer); lanes restored what they touched.
- Duel legs + overnight lanes all regenerate the INDEX.md -20 hunk in fresh
  worktrees (gitignored out/ files); self-heals on main commits.

## Standing queue after tonight

- sprefa push + tag (main greener than it has ever been; your word).
- Fuse lane for the rxprim PLAN (after your two calls).
- Fold-wall fix lane (after your go).
- Host cone edge (after your ruling; unblocks flag-on for host programs).
- ARCH rows for the (now one) engine defect + edge-arm retraction loss.
- flash-prolog fate; 9 unmerged sprefa branches (list in 20260803.9 save).
