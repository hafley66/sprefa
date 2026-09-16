# scip-binding-text-slice

## Goal
Implement fork B of the scip_binding decision (issues/extract-scip-vocab-occurrence-binding,
Decisions section): the `record=scip_occurrence` passthrough row under
`--scip-facts` grows an OPTIONAL `text` field carrying the source slice at the
occurrence's byte span, populated only when a new CLI flag asks for it.
scip_occurrence itself is doc-close: no new record tags, no ninth family
member, no parallel vocabulary.

## First action
`git merge --ff-only <base sha printed by lane create>`. Failure or missing
tree = STOP AND REPORT.

## Read first
- `v6/sprefa-extract/src/schema.rs:44` — the passthrough row today
- `v6/sprefa-extract/src/schema.rs:160-173` — the written exclusion this
  decision amends; update its text: `scip_occurrence` stays excluded as a
  family rel (doc close), `scip_binding`'s source-slice need is now answered by
  the optional `text` field, cite the issue slug
- `v6/sprefa-extract/src/scip.rs` — where occurrence rows are emitted
- v5 comparison decl `src/rels/scip.rs:83-88` (repo root) — what scip_binding
  carried; the `text` field is its `local_name` equivalent, the source slice at
  the occurrence range

## Design pins (already decided, do not re-litigate)
- Field name `text`, JSON-absent (not null, not empty-string) when the flag is
  off or the span cannot be sliced.
- New CLI flag on the scip-facts door; spell it `--occurrence-text`. Wire it
  wherever `--scip-facts` flags already parse; follow that file's existing
  style.
- Flag OFF output is byte-identical to today. That is a graded property, not a
  hope: run the scip goldens before and after and diff.
- The slice comes from the same corpus bytes extract already holds for the
  file (the ContentId-keyed read path); no fresh disk reads, no per-row I/O.
- Spans are byte offsets; the slice is `&bytes[start..end]`, lossy-utf8 to
  string. A span past EOF drops the field, never panics.

## Tests
- One new test: run scip facts with `--occurrence-text` on an existing fixture,
  assert at least one row's `text` equals the corpus bytes at that row's span,
  and assert a flag-off run of the same fixture emits NO `text` key anywhere.
- Full crate `cargo test` in v6/sprefa-extract green, run twice.

## File ownership
`v6/sprefa-extract/src/schema.rs`, `v6/sprefa-extract/src/scip.rs`,
`v6/sprefa-extract/src/wire.rs` if the row struct lives there, the CLI arg
file, one new test file under `v6/sprefa-extract/tests/`. NOTHING else. Never
run bare `cargo fmt`; format only files you edited, by name.

## Style laws
Comment budget: constraints only, no change-log narrative, no em dashes.
Descriptive names. tracing only, no eprintln.

## Landing
Commit, push, open a GitHub PR titled
`extract: optional occurrence text slice on the scip passthrough row` with the
golden byte-identical diff receipt and both test-run summaries in the body.
