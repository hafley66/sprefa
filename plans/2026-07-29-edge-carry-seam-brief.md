# CODEX BRIEF: edge-off-derived carry seam (sol-class)

User-agreed order 2026-07-29: this seam lands AHEAD of match block
sugar. It removes the `edge_trigger_is_derived` refusal so edge rules
(`<+`) can fire off derived (level-rule / enum-tag-view) heads. The
refusal currently blocks the flagship enum state machine
(`current(Id, Tag) <+ door_tag(Id, Tag)` where door_tag is the derived
tag view) and got recorded as central in the 2026-07-29 hands-on
findings (CLAUDE.md).

## The semantics (already ruled, do not re-litigate)

- Unmarked edge triggers = any-body-atom occurrence model (ruling
  2026-07-28). The oracle engine.pl already RUNS these programs; only
  tsv2 refuses. The oracle's tick log is the spec, byte-for-byte.
- The C2 agent named the missing piece: a tickLoop carry seam --
  derived-rel deltas computed inside tick N must be able to feed edge
  rules that fire in the SAME drain cascade, exactly as the oracle
  stacks them. Read the oracle's drain loop FIRST
  (v6/prolog/conformance/engine.pl) and state in your summary which
  tick the oracle fires a derived-fed edge on; your emitted code must
  match that, never a "close enough" tick.

## Scope

1. tsv2 runtime tick loop (v6/tsv2/runtime/): carry derived deltas to
   edge-rule evaluation within the drain, mirroring the oracle's
   ordering. Statement counts stay flat per tick (COUNT-test law: any
   path you touch that could regress to per-row work gets a statement
   count or EXPLAIN assertion).
2. emit_ts.pl / lower.pl: emit edge statements reading the derived
   rel's delta stream (the P1 delta tables) instead of refusing.
   Remove `edge_trigger_is_derived` refusal; keep
   edge_head_column_type_mismatch and edge_head_conflict_risk refusals
   intact.
3. Acceptance test: the door program. v6/dl/fixtures/door-handwritten.dl6
   extended (or a sibling fixture) with
   `current(Id, Tag) <+ body_tag-style derived trigger`; oracle runs it,
   tsv2 must match byte-for-byte. Also promote at least 2 of the
   sweep's edge_trigger_is_derived-refused fixtures to compiled.
4. Do NOT touch: parse_dl.pl / print_dl.pl / text_door_receipt.pl /
   dl_view/ (a concurrent lane owns the print side), v6/dl (except
   reading fixtures), labs/.

## Grades (all re-run by you, coordinator re-runs after)

conformance (max 3 full runs); sweep BOTH modes (movement =
edge_trigger_is_derived fixtures leaving unsupported, zero movement
elsewhere, wrong stays 0); final-state leg no new final_wrong;
roundtrip; plunit; tsv2 tests + import gate; tsgo clean; the door
acceptance receipt; statement-count assertion on the carry path.

## Laws

Worktree agent, no-commit flow if git metadata writes fail: STOP AND
REPORT per dispatch law, leave the tree dirty. FIRST ACTION
`git merge --ff-only <base sha stated at dispatch>`; on failure STOP.
Descriptive identifiers; no em dashes; banned words provenance,
substrate, load-bearing, regime; refCount vocabulary (never
"support" in NEW identifiers/prose; existing names stay until the
rename sweep). Final summary: the oracle-ordering statement, per-file
change list, promoted fixture list, all grades, cracks named.
