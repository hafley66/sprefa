# Golden json seam + json-axis flex — brief (codex luna)

Closes GAP 1 from the 2026-07-30 golden coverage review: the entire json
value/pattern axis (spread `[...]`, `$key` capture, `**` descent, `{}`,
typed captures, `list(T)`) is live and sweep-graded but ABSENT from
golden-flex.dl6 because the golden path's json arrival seam is unwired.
The golden's own header (v6/dl/fixtures/golden-flex.dl6, the
`spread/1, $/1, **/0, {}/0` absence entry) states the measured reason and
the exact two-door fix. This arc executes it, then flexes the axis.

## The two seam fixes (both documented already — read before writing)

1. **Oracle door**: `golden_oracle.pl` `schedule_value/2` maps a JSON string
   to an ATOM, so a canonical-text payload at a json column derives nothing.
   The SAME defect was fixed on `dl6_oracle.pl` by the json-potholes lane
   (merge ddffb8b6): type-directed arrival mapping via `analyze:rel_columns`
   — json-declared columns parse the arriving canonical text into the
   oracle's own json terms (obj/list/number; `null` per card C3; bool per
   bool_lit). That landed code is the TEMPLATE; golden_oracle.pl gets the
   same treatment, sharing the implementation if extractable (one
   implementation, never a third copy).
2. **TS door**: `golden-schedules.ts` would bind a JS object into a TEXT
   column. A json-declared column's arrival must be canonicalized to
   canonical JSON text at the seam (the emitter contract: "a json document
   arrives as its text", serve/4_http.ts columnProblem; the canonicalizer
   already exists in the tsv2 runtime — reuse it, never a second encoder).

## Then flex the axis in golden-flex.dl6

Add rules exercising, at minimum: object literal `{k: v}`, open object
pattern with typed capture `{stars: Stars: int}`, `$key` capture, `**`
descent, array spread `[... {…}]` fan-out, `list(T)` typed column round-trip
(rows -> `json_group_array` -> `decode(spread)` -> rows), `{}` empty-object
match. Stay in the orchard domain (the existing style: every construct
carries a comment naming it). Move each construct from the golden's NAMED
ABSENCES header list to exercised, and update `golden_coverage.pl`'s
`expected_absent/2` bookkeeping to match — the coverage gate must PASS with
the json rows exercised, not excused.

## Receipts required

- `bash v6/tsv2/scripts/golden-flex.sh` HOLDS: coverage (state the new
  exercised/absent split), text door, all cardinalities + perturbed, mode
  parity, served e2e.
- Full battery: conformance, sweep BOTH modes, TEXT_DOOR, roundtrip, plunit,
  `just green` exit 0 (golden-flex is inside green), compile-speed (a grown
  golden may trip the gate — REPORT the numbers, never bypass; the
  coordinator accepts baselines), staleness gate.
- The header absence entry rewritten to record the seam is closed.

## Fences

- Files you may touch: `v6/tsv2/scripts/golden-schedules.ts`,
  `v6/prolog/compile/scripts/golden_oracle.pl` (and a shared arrival-mapping
  helper if you extract one from dl6_oracle.pl), `v6/dl/fixtures/
  golden-flex.dl6`, `v6/prolog/compile/scripts/golden_coverage.pl`,
  `v6/tsv2/tests/goldenFlexServed.test.ts` if the served leg needs the seam.
- Do NOT touch: registry.pl, lower.pl, parse_dl.pl, print_dl.pl, engine.pl,
  conformance fixtures, `v6/prolog/labs/**` (a concurrent lane owns labs/).
- No-commit flow; coordinator reviews and commits.
