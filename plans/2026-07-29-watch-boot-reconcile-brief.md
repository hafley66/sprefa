# watch boot-reconcile brief (codex sol): the restart+delete retraction gap

ARCH row: extraction_live_p2 named gap; morning-list #9. The watch bind's own
header states it (v6/tsv2/serve/2_binds.ts:56-61): the runner can only retract
paths IT has emitted, because a `-` row needs the digest that was there. A file
deleted while the server is DOWN leaves its watch row (and everything derived
from it: extraction findings, diags) behind forever.

## Contract (behavior, graded by receipts — the seam mechanics are yours)
At watch-bind subscribe, before live events flow, run ONE boot reconcile per
watched glob:
1. Learn the engine's CURRENT watch rows for that glob (path -> digest as
   stored). The serve stack already reads engine state elsewhere (1_hosts.ts
   reads host response/cache rows; 4_http.ts serves /idb/<rel>); use an
   existing read seam, do not invent a parallel one.
2. Enumerate the glob on disk the same way the enumerate host does
   (git ls-files pathspec — tracked-only is the standing decision; read how
   enumerate is spelled before writing anything) and digest the files.
3. Emit exactly the difference as one boot batch:
   - stored path absent on disk -> `-` row with the STORED digest
   - disk path absent or digest-changed -> the `+` (and `-old` when replacing)
     rows
   - identical path+digest -> NOTHING (must not regress the content-addressed
     zero-tick restart receipt in extraction-live.sh)
4. Seed `lastDigest` from the reconciled state so a later delete of a file
   this process never touched retracts correctly (the original gap's second
   half).

## Design note you must honor, not relitigate
The bind header says this fix "crosses the push/demand line the A12 finding
drew, so it is named here rather than smuggled in". The user has now
sanctioned it as a BOOT RECONCILE (a one-shot read at subscribe, not an
ongoing demand loop). Update that header paragraph to record the resolution
and cite this brief. Live behavior after boot is UNCHANGED: bare paths from
the watch source, sign from digest comparison, bufferTime coalescing.

## Receipts (fail-first where marked; extend v6/tsv2/scripts/extraction-live.sh
or add a test beside the existing serve tests — read serveWatch.test.ts and
watchCounts.test.ts first and follow their injected-seam style)
1. FAIL-FIRST: delete a watched file while the server is down; boot; the watch
   row AND its downstream extraction finding retract. Record the red (row
   survives today) before your change in the test header or script comment.
2. Restart with zero changes -> zero ticks (the existing receipt must stay
   green; run extraction-live.sh before and after).
3. File CONTENT changed while down -> one boot batch carrying -old +new.
4. Post-boot delete of a file never touched by this process -> retracts.

## Laws
- Files you may touch: v6/tsv2/serve/2_binds.ts, NEW test file(s) under
  v6/tsv2/tests/, v6/tsv2/scripts/extraction-live.sh. NOTHING in
  v6/tsv2/runtime/ or v6/prolog/ — other lanes own them right now. If the fix
  genuinely needs a runtime/ or engine change, STOP and name it.
- No new dependencies. Injected schedulers/seams for tests, no sleeps.
- One-subscribe law: no new manual .subscribe() outside existing entry
  points; the ratchet (v6/tools/one-subscribe.sh) must stay at its baseline.
- Descriptive variable names; vocabulary law (rxjs/prolog/SQL words).

## Validation (report exact counts)
- v6/tsv2 suite: `npm test` green (currently 58 pass / 1 skip; yours add to
  it).
- `bash v6/tsv2/scripts/extraction-live.sh` HOLDS exit 0.
- `bash v6/tsv2/scripts/leak-soak.sh` still green (boot reconcile must not
  leak handles across swap cycles).

## Final summary shape
Base sha; the read seam you chose and why (one paragraph, cite the precedent
file); each receipt red/green with the key output line; exact suite counts;
any named stop.
