# CODEX BRIEF: endurance as gate + node server no-leak soak (luna-class)

User constraint 2026-07-29, verbatim intent: "running a node server
must not leak is all". The v6/dl server is the long-running node
process; this arc makes not-leaking a GATE, not a hope.

## Piece 1: endurance-as-gate

- v6/dl/scripts/goal-endurance.sh IS the end-goal definition (kill -9
  mid-delay, reboot, value lands exactly once). It runs today but is
  not wired into any battery.
- Add `endurance` to the v6/justfile green-all chain (it is already a
  recipe; make green-all include it) and make its exit code honest:
  any phase failure = nonzero, no PASS-looking partial output.

## Piece 2: leak soak, receipts not vibes

New script v6/dl/scripts/leak-soak.sh + one supporting test file.
Budget ~60-90s runtime so it can gate. It boots the server once
(single subscribe, PORT from env with a default in the 173xx range),
then in a loop (>= 20 iterations) does: program swap (load a valid
program via the existing HTTP surface), a few commits, an SSE client
connect + disconnect. After the loop, assert and PRINT:

1. Active timer/handle count flat: process._getActiveHandles() and
   _getActiveResources() via a --expose-internals-free approach --
   use process.getActiveResourcesInfo() (stable API). Count per
   resource type before loop vs after; growth = FAIL with the type
   named.
2. RSS bounded: RSS after loop <= RSS at iteration 3 + 25% slack
   (early iterations warm caches; growth after warmup = leak).
   Print both numbers.
3. Statement counts flat: DL_PERF_LOG one JSONL line per tick already
   exists; assert stmts-per-tick at iteration 20 == iteration 5 for
   an identical commit (the COUNT-test law).
4. SSE teardown: after all clients disconnect, the server holds zero
   SSE inner subscriptions (observable via the existing tracing
   channel events or handle counts; state which signal you used).
5. Program-swap teardown: bind timers from the OLD program are dead
   after swap (getActiveResourcesInfo timer count returns to the
   post-boot baseline; the F3 BindConfig.scheduler receipts are the
   precedent, read 1_binds.ts header first).

Known leak-shaped soft spots to check FIRST (read, then let the soak
prove or clear them): the commits$/reportsSubject Subject pair in
3_runtime.ts (law-debt note in CLAUDE.md), server.close/readBody
Promise wrappers in 6_http.ts, HostRunner effects$ boot replay under
defer.

If the soak FINDS a leak: fix it only if the fix is local and
mechanical (an unsubscribed inner, a missing takeUntil); anything
structural = file it in the summary as a named defect with the
receipt, do not redesign. Every fix gets a fail-pre-fix receipt in
the test header (red output pasted).

## Scope fence

Own: v6/dl/scripts/, v6/justfile, new test file(s) under v6/dl, and
minimal local fixes inside v6/dl/src with receipts. Do NOT touch
v6/prolog/, v6/tsv2/, v6/sprefa-store/, labs/, plans/ (except nothing
-- the coordinator writes the ledger). No new dependencies without a
build-vs-buy note in the summary (standing law).

## Grades

leak-soak.sh exit 0 twice in a row; a sabotage receipt proving it can
fail (comment out one teardown, soak goes red, revert -- paste the
red line in the script header); dl test suite (96+) green; ratchet
still 1; goal-endurance 3/3; green-all passes end to end including
the new gates.

## Laws

Worktree agent. FIRST ACTION `git merge --ff-only <base sha stated at
dispatch>`; on failure or missing v6/, STOP AND REPORT. If git
metadata writes fail in the codex sandbox: no-commit flow, leave tree
dirty, coordinator commits. eprintln/never-console law: diagnostics
through the tracing spine; the soak script's own receipt printing is
script-side, fine. Descriptive identifiers; no em dashes; banned
words provenance, substrate, load-bearing, regime. Final summary:
receipts for all 5 assertions with numbers, soft-spot verdicts
(leak/clear per item), fix list with red receipts, all grades,
cracks named.
