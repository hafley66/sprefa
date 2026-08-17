# soopy-blobsource-revive

Issue: `issues/soopy-blobsource-revive/item.md` (epic soopy-full-wiring).
Measured base: `plans/2026-08-16-soopy-extract-entanglement.md` sections 3.2,
6.1, collapse candidates 2+3.

## First action

```bash
git merge --ff-only f7ed05fa434aab8808c5a833a2bd94cb8448aead
```

Failure = STOP AND REPORT.

## Ownership

You own ONLY `v6/sprefa-extract/src/project.rs` and
`v6/sprefa-extract/tests/**`. Forbidden: `v6/sprefa-engine-rs/**` (a sibling
lane owns hosts.rs), `v6/sprefa-extract/src/types.rs`, everything else.

## The defect

`BlobSource` has two impls (`types.rs:839` trait): `FsBlobSource`
(`project.rs:689-692`, raw `std::fs::read`, no revision) serves BOTH
production readers (`project.rs:147` resolve_project, `project.rs:195`
scip_facts). The rev-correct `SourceTreeBlobSource` (`project.rs:663-674`,
`read_many` with `expected: Some(entry.content)`) is tested
(`tests/10_source_tree.rs:42-57`) and has ZERO production callers. Separately,
`read_inputs` (`project.rs:376-390`) bypasses BlobSource with a per-path
`std::fs::read` loop at `:379`.

## The fix, pinned decisions (do not re-decide)

1. **Untracked files must stay visible.** `FsBlobSource` reads any path;
   soopy's git-files worktree enumeration does NOT see untracked files
   (`_9_git_files.rs:79-82`). The default worktree mode of the revived source
   must use soopy's fs-glob snapshot enumeration
   (`SourceQuery` + `Pattern`, `_7_source_tree.rs:44-49` -> `_4_worktree.rs`),
   which sees untracked + dirty. Behavior of resolve_project/scip_facts on a
   dirty tree is UNCHANGED; add a test proving an untracked file is still
   extracted.
2. **Rev-pinning is a mode, not the default.** Callers that pass a revision
   get `SourceTreeBlobSource` semantics (content-verified reads at that rev);
   default stays worktree. No caller signature breaks outside project.rs.
3. **`read_inputs` goes through BlobSource** with one batched read per corpus,
   never a per-path loop (N+1 law).
4. `FsBlobSource` may remain as a test utility only; zero production call
   sites at the end. If deleting it breaks the public re-export
   (`lib.rs:52`), keep the export and mark the doc comment test-only.

## Receipts

- `cargo test` in v6/sprefa-extract rc=0, run twice, counts in the PR body.
- New test: dirty + untracked worktree, extraction sees both (extend the
  `tests/10_source_tree.rs` harness shape).
- FAIL-PRE-FIX receipt for the untracked-file test against a naive git-files
  enumeration.
- `grep -n 'FsBlobSource' v6/sprefa-extract/src -r` output in the PR body:
  no production call sites.

## Style laws

Banned words: provenance, substrate, load-bearing, regime; "refusal" banned in
prose. Comment budget: constraints only. Descriptive names. Commit trailer:
`Refs-Issue: @soopy-blobsource-revive`.

## Landing

Branch, commit, push, `gh pr create` with receipts. Do not merge. Lanes never
spawn subagents.
