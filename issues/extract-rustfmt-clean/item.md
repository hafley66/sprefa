---
created: 2026-08-16
updated: 2026-08-16
type: chore
status: open
priority: normal
labels:
- pkg:extract
---

# sprefa-extract: origin/main not rustfmt-clean

## Description

Found 2026-08-16 by the extract driver: a lane ran bare `cargo fmt` in
v6/sprefa-extract and it reformatted 15 files outside its fence, formatting
only. Proof site: src/wire.rs:270, `to_blob: edge.dst_blob.to_string(),` at
column 0 inside an indented struct literal. Any lane running bare fmt
reproduces the same 15-file diff.

Files: examples/typegraph_d2.rs, src/cpg_decode.rs, src/cpg_types.rs,
src/lang/dl6/_0_source.rs, src/lang/prolog/_0_source.rs, src/lib.rs,
tests/0_dl6.rs, tests/13_flow_join.rs, tests/19_docs_lang_arms.rs,
tests/1_resolve_cli.rs, tests/9a_query_blob_door.rs, tests/golden_parity.rs,
tree-sitter-dl6/src/lib.rs, plus hunks in src/types.rs ~:842,
src/wire.rs ~:270, src/lang/go.rs ~:126.

## Acceptance Criteria

- [ ] One fmt-only chore PR after the current extract lanes land (to avoid
      rebase noise), `cargo fmt --check` clean on origin/main.
- [ ] Until then every extract brief carries: never bare `cargo fmt`, only
      `rustfmt <owned files>`.
