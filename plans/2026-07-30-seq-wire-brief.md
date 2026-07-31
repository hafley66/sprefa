# seq() wiring + pre doc truth — brief (codex luna)

User approved M2 seq (ruling seq_sugar = approved_wire_m2). M1/M3 stay
unwired. Same lane carries the pre doc-truth fix (scouted: pre is LIVE via
pre_occurrence_loop 7f086fd3; only docs/registry say refused).

## Part 1: seq

Design record: plans/2026-07-30-point-free-verdict.md (M2 section + slots)
and the stream-card 1b ruling. Lab receipts recoverable at 89ccaccf.

Surface: `Ordinal := seq(name)` in an edge-rule body. Atom argument = one
global order per name. Variable argument = one order per value
(slot_seq_scope: the argument answers it, no switch).

Expansion (ONE shared module, the 0_match_expand precedent — oracle and
compiler both consult it): a rule using seq expands to the 4-rule cursor
block (base cursor + base head + pre step cursor + step head) over a minted
cursor rel. Minted names must be deterministic and collision-refused; note
`__`-prefixed rel names do not parse (point-free slot_stage_naming), so
pick a writable prefix and refuse collisions by name. The minted cursor rel
appears in the tick log (receipt 2c in the lab: its name decides byte
identity) — name it stably.

Grading law: a seq program's tick log is BYTE-IDENTICAL to its hand-written
4-rule desugar, both doors, or the wiring is wrong. Promote the lab's
grading pair as fixtures.

Wire: registry row (surface/5), parse_dl/print_dl round-trip, SYNTAX
generated table, grammar regen, golden-flex.dl6 exercises seq (coverage
gate will demand it once the registry row lands — replace the hand cursor
block if one exists, or add a seq rule; state the golden log movement).

## Part 2: pre doc truth

- registry.pl pre/1 row: refused -> live, LowerRole to the real shape
  (edge_sampled_goals already buckets pre; mirror the latest/1 row style).
- golden-flex.dl6: the named-absence entry for pre states a now-false
  reason. Move pre to exercised (an ordered fold rule; the orchard has
  pick_event folds) or rewrite the absence honestly — exercised is right
  since it compiles and runs.
- SYNTAX.md hand half + SCOREBOARD.md stale pre references: refresh.
- golden_coverage expected_absent bookkeeping follows the registry flip.

## Receipts required

Full battery: conformance (state new count), sweep BOTH modes, TEXT_DOOR,
roundtrip, plunit, golden-flex.sh all legs (served may EPERM in sandbox —
report, coordinator runs it), compile-speed (report regressions, never
bypass), staleness gate. The seq byte-identity receipt vs hand desugar.

## Fences

- Do NOT touch: v6/dl/fixtures/self-map.dl6, any devlog files, labs/**.
  Two concurrent lanes own those.
- No-commit flow. STOP AND REPORT on any blocked command.
