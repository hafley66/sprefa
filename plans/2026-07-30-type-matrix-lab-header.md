# Type-matrix lab — header (planner-seeded contract)

Closes GAP 2 from the 2026-07-30 golden coverage review: type corners are
not systematically enumerated. Proof incident, same day: `lane: text`
declared over an int source produced NO refusal, both doors green, and only
the golden's byte diff caught the divergence (oracle logs `4`, emitter logs
`"4"`). The decl-type-vs-source-type cross product has never been walked.

## The matrix

Axes (enumerate the FULL cross product; cells that cannot be constructed
get a named n/a with the reason):

- **Declared column type**: `int`, `float`, `text`, `json`, `list(text)`,
  undeclared (witness-inferred).
- **Actual value type feeding it**: int, float, numeric text (`"4"`), plain
  text, json object, json array, bool (`true`), wide int (> 2^53), negative
  zero / `1.0`-vs-`1` (canonicalization corners).
- **Position**: world-fed EDB arrival, level-rule head from a typed body
  var, edge-rule head, json typed capture (`{k: V: type}`), aggregate head
  (`sum`/`json_group_array` operand), join column (both sides).

## Grading per cell — one of exactly four verdicts, receipts for each

1. **NAMED REFUSAL** (load or compile time, both doors agree) — cite the
   refusal name.
2. **IDENTICAL** — both doors byte-identical AND the value is lossless.
3. **DIVERGENT** — the doors disagree (the `4` vs `"4"` class). These are
   the trophies: each becomes a fail-first fixture candidate.
4. **SILENT COERCION** — doors agree but the value is lossy or reinterpreted
   (bool -> 1, wide-int RangeError cliff, float text collapse). Named, with
   the loss stated.

Known seeds to re-confirm (do not re-derive from scratch, verify at HEAD):
`join_column_type_mismatch` refusal (cross-type join), `edge_head_column_
type_mismatch`, `decl_type_conflicts_witness`, the @libsql int->bigint REAL
corruption class (expression-lift landing), wide-int cliff (json-flex C2),
bool degradation (C4), today's text-decl-over-int-source divergence.

## Method

- The matrix RUNS: a generator (prolog or a small script in the lab dir)
  emits one tiny program + schedule per constructible cell, runs BOTH doors,
  classifies the outcome mechanically. No hand-graded prose cells — the
  table must be regenerable with one command.
- Output: the matrix table (markdown, one row per cell, verdict + receipt
  pointer), the DIVERGENT and SILENT-COERCION lists ranked by blast radius,
  fail-first fixture candidates for every DIVERGENT cell, and a recommended
  refusal set (which divergences should become load-time refusals vs which
  are defined coercions that need a documented contract).

## Named slots

- slot_decl_source_conflict_fate: refuse at load (the
  `decl_type_conflicts_witness` precedent) vs defined coercion for
  decl-vs-source mismatches that today pass silently.
- slot_float_int_boundary: `1` vs `1.0` in json cells and tick logs.
- slot_undeclared_column_default: what the witness-inference default does in
  each cell (C2a zero-witness default = text — is that ever divergent?).

## Receipts required to land

- The regenerable matrix (state cell counts: constructible / n-a / per
  verdict). Fail-first fixture candidates for every DIVERGENT cell.
- Verdict `plans/2026-07-30-type-matrix-verdict.md`; lab files under
  `v6/prolog/labs/type_matrix/**`, die on landing (last-copy hash recorded).

## Fences

- Writes ONLY under `v6/prolog/labs/type_matrix/**` and the verdict doc.
- NO edits to the compiler, oracle, registry, golden files, or conformance
  fixtures — a concurrent lane owns the golden path
  (golden-schedules.ts / golden_oracle.pl / golden-flex.dl6 /
  golden_coverage.pl); fixture PROMOTION is the follow-up arc's job, this
  lab only names the candidates.
