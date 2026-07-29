# CODEX BRIEF: keyed-arrival divergence + silent-inert refusals (sol-class)

Source: the language design review (2026-07-29) findings A4 (live bug)
and A1/A3 (silences), its B3/B2 recommendations. Coordinator
re-verified A4 at HEAD: oracle keyed-replaces a world-fed key(1) rel
(delta [-("mode","a"), +("mode","b")]) while the emitted module builds
PRIMARY KEY over ALL columns + INSERT OR IGNORE (both rows survive).
Zero fixture coverage let 131/131 stay green through the divergence.

## Piece 1 (B3): world-fed keyed rels, emitter matches oracle

- lower.pl rel_ddl: a rel with decl_key that is an ARRIVAL TARGET
  emits PRIMARY KEY over the KEY columns and the arrival add becomes
  the keyed-replace shape (INSERT OR REPLACE, or delete-then-insert if
  the delta stream needs the explicit minus row -- READ engine.pl's
  absorb_set_arrival/5 first and match its delta semantics EXACTLY;
  the tick log is byte-graded, the minus row must appear).
- REWRITE the two lower.pl header paragraphs (near :110-118 and
  :520-524) whose premise ("absorb_arrivals never consults decl_key")
  the hosts-wiring commit made false.
- FAIL-FIRST fixture: keyed world-fed rel, two same-key arrivals, no
  rule heading it; expectations = the oracle's replace deltas + final
  single row. Show it red against current emit, green after.

## Piece 2 (B2): three silences become load-time refusals

In engine.pl check_program/1, beside the four existing forall checks
(the finalize_in_level_rule precedent), add named refusals:
1. kind(Ref, log) where Ref is level-headed
   (log_on_level_headed_rel) -- review finding A1: such a rel has no
   delta channel, downstream edge rules die silently.
2. latest/1 inside a level-rule body (latest_in_level_rule) -- A3:
   provably identical to a bare read, byte-for-byte (body.pl:99 vs
   :110).
3. pre/1 inside a level-rule body (pre_in_level_rule) -- A3: level
   bodies get ctx(..., [], ...), pre always fails silently.
Mirror the same three refusals in the COMPILER front (analyze.pl)
with the same names. One fail-first fixture per refusal (program that
compiled-and-silently-misbehaved before, named refusal after). If any
existing fixture trips a new refusal, STOP AND REPORT that fixture
rather than weakening the check.

## Piece 3 (small rider): sweep gen_emitted footgun

scripts/sweep.ts (or sweep.sh) deletes non-fixture modules from
gen_emitted/ (door-handwritten.ts dropped 4 times, ledger-recorded).
Make the regen remove ONLY files whose basename matches a fixture
name it is about to rewrite. Receipt: run sweep, git status shows
door-handwritten.ts untouched.

## Grades (all re-run by you; coordinator re-runs after)

conformance grows by the new fixtures, 0 fail; sweep BOTH modes:
existing buckets zero movement except any fixture the new refusals
legitimately move (report exact movement); TEXT_DOOR all-compiled
pass; roundtrip ALL GRADES PASS; plunit grows (fail-first receipts
red->green pasted in test headers); tsv2 tests + import gate; tsgo
clean; the footgun receipt.

## Laws

Codex no-commit flow (git READ-ONLY; tree stays dirty, coordinator
commits). FIRST ACTION verify HEAD equals the dispatch-stated sha;
STOP AND REPORT on mismatch. Descriptive identifiers; no em dashes;
banned words provenance, substrate, load-bearing, regime. Final
summary: per-piece receipts, exact sweep movement, cracks.
