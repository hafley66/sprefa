# grade.sh concurrency lock

## Problem
`docs/failure-modes.md` entry 50: two concurrent `bash v6/sprefa-engine-rs/grade.sh`
runs race each other (shared build dir + graded.tsv/ratchet writes) and corrupt each
other's results. The ledger entry has an RCA but no rail. Build the rail.

## Task
1. Read `docs/failure-modes.md` entry 50 first; the incident text defines the race.
2. Add a lock to `v6/sprefa-engine-rs/grade.sh` so a second concurrent run FAILS FAST
   with a one-line message naming the holder pid, rather than racing. macOS has no
   `flock(1)` binary; use an atomic `mkdir` lockdir (with trap cleanup on EXIT/INT/TERM,
   stale-lock detection by dead pid) or an equivalent portable mechanism. Justify the
   mechanism choice in the commit message, one sentence.
3. Fail-first receipt: before the fix, demonstrate the second run proceeding (two
   backgrounded runs, show both writing); after, show run 2 exiting nonzero immediately
   with the message. Paste both receipts into the ledger entry 50 `rail` field in
   `docs/failure-modes.md`.
4. `bash v6/sprefa-engine-rs/grade.sh` single-run must keep its current rc and output
   shape. Run it once before your change and once after; report both.

## Ownership
You own ONLY: `v6/sprefa-engine-rs/grade.sh`, `docs/failure-modes.md` (entry 50 only).
FORBIDDEN: everything else, especially `v6/prolog/**`, `graded.tsv`, any `*.rs`.

## Style laws
- Comments state only constraints the code cannot show. No change-log narrative.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime.
- Descriptive variable names, never single-letter.

## Deliverable
Commits on this branch. Final message: lock mechanism chosen + why in one sentence,
fail-first receipts, single-run rc before/after.
