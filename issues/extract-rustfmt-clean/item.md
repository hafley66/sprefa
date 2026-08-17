---
created: 2026-08-16
updated: 2026-08-17
type: chore
status: done
priority: normal
labels:
- pkg:extract
closed: 2026-08-17
closed_by: extract-driver
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

## Resolution

### 2026-08-17T04:55:46Z · @extract-driver

PR #342 merged, origin/main d1a5556b0. cargo fmt --check clean on v6/sprefa-extract, 19 files formatting only, gate 151/151 unchanged. Both pre-commit rails (comment-budget) needed the release extract binary and pnpm install in v6/tsv2 AND v6/sprefa-store/js inside a fresh worktree; boop lane create does that provisioning, a hand git worktree add does not.
