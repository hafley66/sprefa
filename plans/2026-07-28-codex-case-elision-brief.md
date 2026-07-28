# CODEX BRIEF: elide the json CASE wrapper for INTEGER columns (luna-class)

## Task

In v6/prolog/compile/lower.pl, `canonical_column_expr/2` (or its current
name; re-find by symbol, line numbers rot) wraps EVERY column read in the
delta/snapshot SELECTs with:
  CASE WHEN json_valid("col") AND json_type("col") = 'object' THEN
  <json_extract canonical-term rendering> ELSE "col" END
This exists to render stored compound terms (json1 objects) back to
canonical term text for the tick log. An INTEGER-typed column (the C2a
type inference in analyze.pl: every literal witness a prolog integer ->
INTEGER storage) can never hold a json compound, so for those columns the
wrapper is dead weight. Emit the plain quoted column reference instead,
for INTEGER columns ONLY. TEXT columns keep the CASE unconditionally
(runtime arrivals may carry compounds the fixture never witnessed; do not
widen the elision beyond INTEGER no matter how tempting).

## Scope

- Edit only v6/prolog/compile/lower.pl (and, if strictly needed for the
  type lookup plumbing, the narrowest touch in emit_ts.pl). Nothing else.
- Regenerate v6/prolog/compile/out/ and v6/tsv2/gen_emitted/ ONLY via the
  existing sweep script; never hand-edit generated files.

## Grades (all must hold; run them yourself)

1. `cd v6/tsv2 && bash scripts/sweep.sh` — SCOREBOARD totals UNCHANGED
   (109+1=110 swept, 31 compiled, 28 identical, same buckets per fixture).
   Byte-identical tick logs are the point: the elision must be invisible
   to semantics.
2. `swipl -q -l v6/prolog/conformance/go.pl -g go -g halt` — 110 PASS.
3. `bash v6/prolog/compile/scripts/roundtrip.sh` — ALL GRADES PASS.
4. `swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt`
   — all pass.
5. Visible receipt: at least one regenerated out/*.ts shows a plain
   `SELECT "col"` where an INTEGER column previously wore the CASE; paste
   one before/after line pair in the final summary.

## Laws

- Descriptive prolog variables (ColumnName, not C). No em dashes in any
  prose you write. Banned words: provenance, substrate, load-bearing,
  regime.
- One commit per logical step, `git commit -n` (pre-commit hooks are
  un-hermetic in worktrees), do NOT push, do NOT merge.
- If any grade fails and the fix is not obvious within the scope above,
  STOP and write the failure into the final summary instead of widening
  the scope.

## Final summary shape

Per-step commits, the before/after SQL line pair, all five grade results,
skips with reasons.
