# list-column-raw-snapshot-crash (issue: list-column-raw-snapshot-crash, size:med)

FIRST ACTION: `git merge --ff-only 9e88f078` (the chore/recursion-throw-pinning
head; this lane STACKS on PR #279 so compile/out regeneration cannot conflict
with it). Failure = STOP AND REPORT. Read CLAUDE.md at repo root. Full issue
body: /Users/chrishafley/projects/sprefa/issues/list-column-raw-snapshot-crash/item.md
— read it, the mechanism below is the short form.

MECHANISM (already traced): row_value_from_sql (v6/tsv2/runtime/rows.ts:31)
decodes every column whose declared type is `list` as the JSON-array TEXT a
`__list_...` view renders, and throws `list column crossed SQLite with <value>`
otherwise. read_snapshot (the boundary/final-read function in emitted modules)
correctly LEFT JOINs the `__list_...` view. read_stored_snapshot (the raw
before/after builder build_deltas diffs) SELECTs list-typed columns DIRECTLY
off the base table, handing back the interned surrogate INTEGER — and both
paths share one rel_column_types map keyed only by declared type, so the raw
path crashes on the first non-empty row. Latent on EVERY list(T) column that
ever holds a row; goldens pass today only because golden-schedules.ts never
seeds a tree_bundle arrival.

FIX CANDIDATES (issue names two; decide by the referee below):
A. read_stored_snapshot gets its own column-type view for the raw-storage
   shape (surrogate int, not list), separate from rel_column_types.
B. The generated read_stored_snapshot SELECT gains the same `__list_...` LEFT
   JOIN read_snapshot uses.
REFEREE: cross-door byte-parity. Whichever candidate keeps sweep + golden-flex
+ grade.sh byte-identical on the EXISTING corpus while fixing the repro wins.
Weigh A first: the raw snapshot exists for delta diffing, and diffing surrogate
ints is cheaper than rendering arrays; but if delta/tick-log parity with the
oracle needs array text, say so with the diff pasted and take B.

FAIL-FIRST REPRO (do this before touching the fix): per the issue, seed one
tree_bundle arrival row (a golden-flex.dl6 copy or a minimal schedule) and
capture the `list column crossed SQLite with 1` throw from read_stored_snapshot.
Paste the stack. Then fix, then rerun: clean.

PINNING TEST: additive. Either a golden schedule that seeds a non-empty
list(T) row (preferred; kills the "always empty" blind spot forever) or a tsv2
unit test on the emitted module. A COUNT/EXPLAIN-style assertion if your fix
adds a join (formerly-quadratic paths law).

FILES YOU OWN: v6/prolog/compile/emit_ts.pl (the read_stored_snapshot
emission), v6/tsv2/runtime/rows.ts if candidate A needs it, the regenerated
v6/prolog/compile/out/** (sweep product), v6/tsv2/scripts/golden-schedules.ts
if you take the golden-seed pinning route, plus your one new test file.
FORBIDDEN: v6/prolog/lower.pl, conformance/** (engine.pl and fixtures belong
to PR #279 below you — do not touch), emit_rust.pl, v6/sprefa-engine-rs/**,
v6/sprefa-extract/**.

VALIDATION (paste every output, run each leg twice):
1. Fail-first repro throw, then post-fix clean run.
2. `cd v6/tsv2 && bash scripts/sweep.sh` — RUN identical count unchanged, 0
   wrong, manifest reason-diff shows zero restated/moved.
3. golden-flex: all legs HOLD.
4. tsv2 test battery — the known pre-existing failing trio is the only red.
5. `bash v6/sprefa-engine-rs/grade.sh` unchanged (your fix is TS-door only;
   any Rust-door drift = STOP AND REPORT).

COMMIT plain, COMMENT_RAIL_IDLE_MS=3000 on every commit, never pipe a commit.
Close: `issuectl --json close list-column-raw-snapshot-crash --commit <sha>:<summary>`.
Report: candidate chosen + why in one line, fail-first receipt, the five gate
numbers.
