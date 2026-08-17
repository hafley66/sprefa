# bindargs-ts-throw-twin (issue: bindargs-ts-throw-twin, size:med)

FIRST ACTION: `git merge --ff-only d0e8340dff067453e08eedbefaacbd6625777b8c`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root. Async law: sync stays sync above the SqlRunner seam; exactly one manual .subscribe() per app.

GOAL: user decision "no panics for lists" applied to the TS door. PR #260 fixed Rust: ScalarValue at the four scalar-only seams + BoundaryError with Display strings preserving the old text. The TS door still throws: v6/tsv2/runtime/1_incremental.ts:73 bind_args `throw new Error("a list value reached a SQL parameter")`, and the emitted-module twin template at v6/prolog/emit_ts.pl:592 bind_args_helper_lines. Mirror the Rust design: the type-side split already exists (IRowScalar names the SQL-parameter seam, PR #256); make the runtime path a typed error that aborts the tick identically to Rust's BoundaryError path — the two doors' operator-visible bytes must match (Rust Display reproduces the old panic strings verbatim; keep the same text).

READ FIRST: engine-rs types.rs BoundaryError/ScalarSeam + where run_tick propagates it (PR #260 shape); how the TS door currently propagates a tick abort (grep the tsv2 runtime for the diverging_measure_recursion throw added in PR #263 — that is the established TS tick-abort idiom; follow it, do not invent a second one).

FILES YOU OWN: v6/tsv2/runtime/1_incremental.ts (bind_args + its error path), v6/prolog/emit_ts.pl (ONLY bind_args_helper_lines and whatever the emitted twin needs to stay byte-aligned), regenerated emitted modules via sweep, a test in v6/tsv2/tests/ (new file, fail-first: assert the typed abort, not an uncaught Error).
FORBIDDEN: v6/sprefa-engine-rs/**, rows.ts (another issue owns the snapshot reader), emit_rust.pl, all other .pl.

VALIDATION (run yourself, paste): pnpm install --frozen-lockfile in v6/tsv2 AND v6/sprefa-store/js; `bash scripts/sweep.sh` from v6/tsv2 — baseline FINAL total=341 identical=335 wrong=0 no_oracle=6, yours must not move identical DOWN; tsv2 test suite — baseline 218 pass / 3 fail (the 3 = CI-KNOWN-RED trio, name them); `cd v6 && just typecheck` 0.

COMMIT in slices. Close: `issuectl --json close bindargs-ts-throw-twin --commit <sha>:<summary>`. Report: the error type/path chosen with the Rust twin cited, fail-first receipt, gate numbers.
