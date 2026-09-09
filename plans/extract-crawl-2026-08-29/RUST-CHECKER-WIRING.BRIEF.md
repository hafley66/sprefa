# Lane `fix/extract-rust-checker-wiring` (opus): recover the checker answers our wiring drops

First action: `git merge --ff-only <sha the coordinator states in --goal>`. Failure = STOP, beep the coordinator.

## Goal, one sentence

Close the census's biggest block — 7,145 of 13,764 missed CodeQL call rows
(51.9%) are rows the ra_ap checker CAN answer but our joining/wiring drops —
by fixing the join legs the census names, one class per commit, fail-first
per class, with the rust call floors ratcheted UP at the end.

## Why

`plans/extract-crawl-2026-08-29/rust.REPORT.md` sec 31 (PR #620): the miss
classes funded as "checker answer coverage" are A2 (3,723 rows, same dst file
different callee name), T2 (2,512 + 411 rows, impl methods where our rows are
disjoint or the caller emits nothing), T1 (466, trait default methods), plus
F1 (237, free fns unresolved). Read sec 31's per-class example rows FIRST;
re-derive each class's mechanism from 3 examples before touching code. The
census script `plans/extract-bench-2026-08-29/rust.call_census.py` re-runs on
your binary and is your per-class receipt: the class count must DROP.

## Method

- Per class, in this order: A2, T2 (both), T1, F1. For each: run the census,
  read 3 cited examples at their path:line, name the mechanism in one
  sentence, write the fail-first fixture test (tests/93_rust_checker_wiring.rs,
  one test fn per class), fix the join in the smallest owned-file diff, re-run
  the census, commit with the class's before -> after count in the message.
- A class whose mechanism turns out to need macro expansion or new checker
  queries beyond the existing seam is OUT OF SCOPE: write the finding in the
  report section, skip it, move on. No seam redesign in this lane.
- Precision guard: `rust.call_census.py` also reports the contradicted count;
  if a fix RAISES contradicted rows, the fix is a guess — revert it.

## Files you own

- `v6/sprefa-extract/src/lang/rust_checker_ra.rs`, `rust.rs` (join legs only),
  `src/types.rs` + `src/project.rs` ONLY if mechanism-forced and small
- `v6/sprefa-extract/tests/93_rust_checker_wiring.rs` (new) + fixtures
  `tests/fixtures/rust_checker_wiring/`
- `plans/extract-crawl-2026-08-29/rust.REPORT.md` — one new section, next free
  number (check the head; renumber on collision)
- `plans/extract-bench-2026-08-29/RATCHET.tsv` — ONLY via the final gate with
  `RATCHET_BUMP=1`, after the coordinator's go
- `plans/extract-bench-2026-08-29/OPEN-PROBLEMS.md` — the rust rows, same PR
- FORBIDDEN: ts/go/python lang files, tests/bench/mod.rs, corpus dirs.

## Receipts

1. Per class: census count before -> after, the fail-first test name, and the
   3 example rows that defined the mechanism.
2. Suite `cargo test --features cli,rust-checker,ts-checker` rc=0 direct.
3. BEEP THE COORDINATOR BEFORE the final gate (ONE ratchet on the machine at a
   time): `boop beep --no-wait --as fix-extract-rust-checker-wiring sprefa-coordinator "ready for ratchet go"`.
   After the go: `cd v6 && RATCHET_BUMP=1 just extract-ratchet`, paste the new
   rust rows; recall must rise, precision must not fall past 0.10 pt.
4. `git diff --stat origin/main...HEAD` = owned files only.

## Laws

Banned words prose+identifiers: provenance, substrate, load-bearing, regime,
ground truth (say oracle), honest, signal. Comment budget: constraints only.
Numbers carry units. Commit per class; push; `gh pr create`; then beep the
coordinator with the PR number and the per-class count table.
10-second law: census runs and builds go background with caps, nice -n 15.
