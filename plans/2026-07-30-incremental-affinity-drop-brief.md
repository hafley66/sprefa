# Incremental arrival affinity-drop fix — brief (codex luna)

THE DEFECT (type-matrix lab headline, reproduces at merged main): the
DEFAULT incremental emitter path DROPS an arrival delta whenever SQLite
column affinity rewrites the value between the wire and the table.

Minimal repro (no types declared anywhere):

```
rel probe_in(value).
rel probe_out(value).
probe_out(ProbeValue) <- probe_in(ProbeValue).
```

Feed `probe_in(4)` (undeclared column defaults to text; INTEGER-looking
text triggers affinity conversion). Oracle logs both rels; NAIVE mode
derives probe_out; INCREMENTAL logs `{"tick":1,"deltas":{}}` and probe_out
never exists. Three-way disagreement, zero refusals, zero errors.

ROOT CAUSE (located by the lab, verify at HEAD): v6/tsv2/runtime/
1_incremental.ts (region formerly :728-734 — re-find by symbol):
`changedRows` holds the post-affinity `RETURNING` value (`"4"`), the
arrival's `entry.row` holds the wire value (`4`), the membership check is
`JSON.stringify` equality, so the arrival never matches and is filtered
out of the delta stream WHILE THE ROW SITS IN THE TABLE. The row exists;
its delta does not; every downstream rule is blind to it.

THE FIX SHAPE (smallest correct): compare in post-affinity space — the
RETURNING row IS the truth of what was stored; the delta entry must carry
the stored value, not the wire value (this also makes the delta stream
agree with what a boundary read would see). Do NOT normalize the wire value
by re-implementing affinity rules in JS; read the stored value back from
RETURNING and build the delta from it. If you find a reason this is wrong,
STOP AND REPORT with the evidence instead of choosing differently.

## Verification

1. Fail-first fixture: the exact repro above as a conformance fixture
   (`arrival_affinity_rewrite_keeps_delta` or similar), red on incremental
   before the fix, green both modes after, oracle-verified.
2. The type-matrix lab is the wide verification tool:
   `bash v6/prolog/labs/type_matrix/matrix.sh` (about 4 minutes; symlinks
   are committed). Before-fix merged-main counts: 79 IDENTICAL / 156
   DIVERGENT / 71 SILENT_COERCION / 116 NAMED_REFUSAL, with
   DIVERGENT/emitter_modes_disagree = 48. Your fix should move a large
   share of those 48 (the lab attributed ~50 cells to this one bug). Report
   the after counts per label; do NOT edit the lab's classifier or
   generator — if a cell classification looks wrong, report it.
3. Full battery: conformance, sweep BOTH modes, TEXT_DOOR, roundtrip,
   plunit, `just green` (serve legs may EPERM in your sandbox — report,
   never work around; coordinator runs them), compile-speed, staleness.

## Fences

- Touch: v6/tsv2/runtime/1_incremental.ts, new conformance fixture(s) +
  their generated artifacts, tests.
- Do NOT touch: the type-matrix lab dir except RUNNING matrix.sh; the
  oracle; lower.pl (the emitted SQL is not the bug); labs/point_free/**
  (concurrent lane).
- No-commit flow; coordinator reviews and commits.
- This fix is deliberately NARROW: the head-decl-cast divergences and the
  refusal-widening recommendation from the same lab are USER RULING
  territory and out of scope here. Fix only the delta-drop mechanism.
