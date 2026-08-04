# Morning report 2026-08-04 (drawn)

## 0. The whole night, one board

```
                      OVERNIGHT (5 opus agents)
   ┌────────────────────┬──────────────────────┬─────────────────────┐
   │ MERGED to main     │ DECISION DOCS        │ HANDED to instant-  │
   │                    │ (waiting on you)     │ fable (stalled!)    │
   ├────────────────────┼──────────────────────┼─────────────────────┤
   │ 1 ticks$ decouple  │ 4 fold-wall cause    │ 6 reactive review   │
   │ 2 prune flag (L2)  │ 5 duel verdict       │   12 findings       │
   │   +typecheck fixed │ 3 host-edge hole     │   ONE KEYPRESS      │
   └────────────────────┴──────────────────────┴─────────────────────┘
   main: plunit 324 · conf 289/0 · text-door 202/202 · golden-flex ✔ · tsc ✔
   UNPUSHED. push+tag = yours.
```

## 1. MERGED: engine no longer dies with its last viewer

```
BEFORE                                   AFTER (share resetOnRefCountZero:false)
arrivals ─▶ concatMap ─▶ share() ─▶ ticks$      arrivals ─▶ concatMap ─▶ share(keep) ─▶ ticks$
                │                                        │
   reader count 1→0 = LANE TORN DOWN             reader count 1→0 = lane stays up
   running=false                                 running stays true
   next submit ✗ "engine is not running"         next submit ✓ ticks, late reader
                                                 sees everything, numbering 1..n
   │t1 sub│t2 unsub│t3 submit ✗│                 │t1 sub│t2 unsub│t3 submit ✓ tick│t4 sub sees it│
```
Kept on purpose: submit before the FIRST-ever subscribe still errors (lane has
never connected; batch would vanish). Eager-connect = separate card.

## 2. MERGED: laziness ladder step 2 (prune behind flag, default OFF)

```
                     ladder
  ✔ 0 qcols          (query carries columns)
  ✔ 1 cone+wire      (subscribedRels computed, consumed by nothing)   ← was here
  ✔ 2 PRUNE FLAG     (tick path filters to the cone)                  ← now here
  □ 3 harness roots + oracle parity

  SPREFA_TSV2_SUBSCRIBE_PRUNE
        =off (default)                    =on
  ┌──────────────────────────┐   ┌───────────────────────────────────┐
  │ SAME ARRAYS BY REFERENCE │   │ statements/rels/boot filtered to  │
  │ + text-door 202/202      │   │ cone ∪ arrivalTargets             │
  │ = provably identical     │   │ ingestion NEVER pruned (keep()=   │
  └──────────────────────────┘   │ replay ruling)                    │
                                 └───────────────────────────────────┘
  4-combination gate (all tested):
              │ flag off              │ flag on
  query prog  │ fixture rows verbatim │ subscribed rels byte-equal;
              │                       │ unsubscribed: [] + 0 statements
  zero-query  │ derives as today      │ derives NOTHING; ingest still lands
```
Bonus: tsc was red all night (202 errors, field declared on interface, never
emitted). Fixed in this lane. First clean typecheck since the rename.

## 3. YOUR RULING: the host edge the cone cannot see

```
        query ──▶ response_rel ──▶ (cone walks rule bodies backward)
                      ▲
                      │ fires only if…
        __host_demand_N ──▶ [ live host runs ] ──▶ __host_response_N
              ▲                                          │
              └────────────── MISSING EDGE ◀─────────────┘
                     (no rule body connects them; the pairing
                      exists only in the host wiring)

  replay fixtures: responses injected directly  → hole INVISIBLE, gates green
  live host + flag on: demand never derives → host never runs → rel = [] silently

  ∴ RULE UNTIL RULED: flag stays OFF for host-bearing programs.
  The fix widens the oracle-shared constant AND un-prunes the only pruning
  fixture, so it wants your word, twice.
```

## 4. YOUR GO: fold wall root cause (1a and 1b were ONE defect)

```
  program has ANY `seq`/`pre` rule?
        │yes                                │no
        ▼                                   ▼
  runOrderedTick            runIncrementalTick (fixpoint ✔)
  level plane =
  DELETE + 1 INSERT per clause, NO LOOP     ← emit_ts.pl:1566-1582, :1953
        │
        ▼
  self-ref rule ceiling = ITS CLAUSE COUNT
  2 clauses → chain stops at 2   (golden-flex fold = the bug you saw)
  3 clauses → stops at exactly 3 (measured; ceiling MOVES with clauses)

  proof both ways: sed the dispatch line → incremental → third link LANDS
  (minimal repro s6_seq.dl6 = 8 lines AND real golden-flex)

  side finds: MODE PARITY gate compares ordered runs to THEMSELVES
              (EMITTER_MODE emitted, never read) · naive door has its own wall
```
Fix site is the ordered path's level plane. Shares emit_ts.pl with the duel's
one() change → sequence the two lanes.

## 5. YOUR TWO WORDS: duel verdict

```
   kimi ────┐  block sugar ▷ graft        FUSE = opus spine
   flash ───┤  reconcile table,           + 5 grafts
            │  loud-loser ▷ graft         − 3 discards (both typed-merges
   opus ━━━━┷━━ BASE (only leg whose        assumed machinery that does
                claims re-execute clean)    not exist)

   opus's key find: lower.pl:1560-1566, ONE TriggerKind test
   = arm-order (concat) vs arrival-order (merge) fork.
   Widen it → one_pick_order closed for new AND legacy shapes, oracle untouched.

   WORD 1: reserved door name   throttle(1)  vs  zip(tick)
   WORD 2: block sugar          same wave    vs  later
```

## 6. INSTANT: review delivered, courier frozen

```
  fable-main ──6 envelopes──▶ instant-fable ✋ FROZEN at "approve `bus list`?"
                                   │              (verifying MY envelope is real:
                                   ▼               anti-injection doing its job)
                          UNSTICK: tmux attach -t instant-fable → press 2

  REVIEW-reactive.md, 12 findings, drawn small:
  ┌ waterfall bar click ─▶ resurrects dead lane (3rd call site, + boot replay 4th)
  ├ your brush drag ──✗ wiped every 8s by refreshRogue → store notify → nowMs churn
  ├ ticks after 1st session ──✗ never paint: bump(1) hits React Object.is bailout
  ├ one failed list_sessions ─▶ liveness graded vs boot-time list FOREVER (??-chain)
  ├ InTabStrip + MailPreview ─ NO live feed at all (button-only)
  └ useAgentTree ─ rebuilds arrays every render → every memo below it is dead
```

## 7. Loose ends board

```
  YOURS TODAY            NOBODY'S YET                  STANDING
  ├ push + tag           ├ roundtrip rewrites 126      ├ rxprim worktrees x3
  ├ duel words 1+2       │  dl_view files (stale       │  + foldwall kept for
  ├ fold-wall fix GO     │  printer corpus)            │  your read, then die
  ├ host-edge ruling     ├ flash-prolog fate           └ monitor still watching
  └ instant-fable key    └ 9 unmerged branches                instant-fable
```
