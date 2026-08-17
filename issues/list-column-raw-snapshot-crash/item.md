---
created: 2026-08-14
updated: 2026-08-15
type: bug
status: fixed
priority: normal
epic: list-ergonomics-closeout
labels:
- size:med
- area:engine
- pkg:tsv2
closed: 2026-08-15
commits:
- hash: d4f6abca
  summary: 'emit_ts: read_stored_snapshot reads a list column as its surrogate id'
---

# Emitted TS runtime crashes reading a live list(T) column back through read_stored_snapshot

_Source: v6/tsv2/runtime/rows.ts:31_

## Description

row_value_from_sql (v6/tsv2/runtime/rows.ts:31) treats every column typed `list` as the JSON-array TEXT a `__list_...` view renders (json_group_array of the interned members), and throws `list column crossed SQLite with <value>` otherwise. `read_stored_snapshot` (gen_emitted/<module>.ts, the raw before/after snapshot builder `build_deltas` diffs) selects list-typed columns DIRECTLY off the base table (`SELECT "tree_id","sites" FROM "tree_bundle"`, no `__list_...` view join), so it hands back the raw interned surrogate INTEGER id, not the array text. `read_snapshot` (the sibling boundary/final-read function) correctly LEFT JOINs the matching `__list_...` view and passes value_text. Both paths share one `rel_column_types` map keyed only by declared type, so the raw path inherits "list" and crashes on the first non-empty row.

Reproduced: v6/dl/fixtures/golden-flex.dl6 (golden-flex-coverage issue) tried exercising `split/2` via `rel display_words(tree_id: int, words: list(text)). display_words(TreeId, Words) <- display(TreeId, Note), Words := split(Note, ' ').` -- `golden-run.ts --final` on the "one"-cardinality schedule throws `Error: list column crossed SQLite with 1` inside `row_value_from_sql`, stack rooted in `read_stored_snapshot`.

This is not new to split/2: the pre-existing `rel tree_bundle(tree_id: int, sites: list(patch))` in the same golden has the identical column shape and would crash the same way -- it never has, only because `v6/tsv2/scripts/golden-schedules.ts` never seeds a `tree_bundle` arrival row, so the column is always empty (confirmed: probed a golden-flex.dl6 copy with tree_bundle/tree_bundle_read intact and display_words/display_word removed -- "one" cardinality ran clean end to end, zero tree_bundle rows in every tick). The defect is therefore latent on ANY `list(T)`-typed rule that ever produces a row, not specific to one construct.

Fix candidate (not applied -- runtime/emitter files are out of scope for the issue that found this): `read_stored_snapshot` needs its own column-type view for the raw-storage shape (surrogate int, not list) separate from `rel_column_types`, OR its generated SELECT needs the same `__list_...` LEFT JOIN `read_snapshot` already uses.
