---
name: feedback_rx_operators_when_sensible
description: think in RxJS operators (switchMap/mergeMap) for the reactive effect runtime when it fits; map their cancel/concurrency semantics onto effect drain + caching
metadata: 
  node_type: memory
  type: feedback
  originSessionId: edd6a76b-d044-4536-b605-abe3adb0c58b
---

Chris wants RxJS operator semantics in the reactive effect runtime (the
sh/@async/@stream world) WHEN it makes sense — not a wholesale rewrite, use the Rx
mental model where it clarifies. Specifically asked for switchMap/mergeMap caching
behavior.

**Why:** the effect runtime IS a reactive graph; Rx already names the
cancel/concurrency/caching patterns the ghcacher port keeps re-deriving by hand.

**How to apply:**
- **switchMap** = cancel the in-flight effect when a new input arrives, latest
  wins. Maps onto gap B (latest-wins/upsert): a new poll for an endpoint should
  supersede/cancel the stale in-flight one. The `pending_effect` claim + a cancel
  on superseded args.
- **mergeMap** = run all concurrently, no cancel (today's rayon par_iter drain).
- **shareReplay / cache** = the content-addressed `pending_effect` digest (a
  request is memoized by args) — already Rx-shaped.
- The four generic gaps (A clock-as-fact, B latest-wins, C pagination, D header
  capture) read naturally as Rx: A=interval/timer, B=switchMap, C=expand,
  D=map over the response.

Do NOT bolt a full Rx layer on; reach for the operator name when it explains the
behavior, then implement it on the rev/tx spine. See [[project_sh_effect_runtime]],
[[feedback_rule_is_function_not_channel]] (rule=fn vocabulary still holds).
