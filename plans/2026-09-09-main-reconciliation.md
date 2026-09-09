# Main reconciliation checkpoint

## Context

Chris requested the completed extract, TypeSpec storage and DL7/IVM work be
reconciled onto main. Integration starts at local main 0ac1172d8. Extract is
feature/extract-tsp-rusqlite-20260909 at e8dc3b2c7. DL7 is
feature/dl7-source-intelligence at e7a0d839f. TypeSpec's generator is already
on hafley-tsp main at 1af0853. Primary and sibling dirty files must survive.

## Decisions

- Merge both source branches into this isolated integration branch, preserving
  history and independently landed fixes. No whole-tree replacement or reset.
- First merge extract, then DL7. Parent performs Git operations and reviews.
- Sol gets exact conflict paths and resolves only mechanical reconciliation.
  Stop and report any conflict that requires new compiler semantics.
- Preserve the unfinished Prolog subprocess wrapper as inactive work. Do not
  implement bindings, interning, watchers, effects or new transport here.
- Do not edit hafley-tsp's unrelated Rust emitter work or hafley-rs Boop repair.
- No push, worktree removal, stash apply/drop, snapshot update or global state
  cleanup. Fast-forward local main only after the parent reads current gates.

## Sequence and exit condition

The parent creates and merges in .recovery/main-reconcile-20260909. Sol works
only there if conflicts are assigned. Report status before edits and every
60 seconds or milestone. No subdelegation. No invented APIs or schema changes.

Existing callable boundaries remain Source::extract / dispatch and generated
insert_all(&Connection, &Source, &[Fact]). Caller transaction ownership and
source/ordinal contracts remain unchanged. Keep generated SQL and Rust outputs
consistent with TypeSpec; regenerate with the existing toolchain if needed.

Exit: both feature tips are ancestors of local main, no unmerged paths remain,
combined-checkout gates pass, and dirty work in original checkouts is preserved.
Binding design remains a later checkpoint.

## Verification

- Existing extract complete CLI suite, generation parity, SQLite tests.
- DL7 compiler/source-loader/query/layout suites and SQLite IVM integration.
- Relevant engine build/test coverage for any reconciled host seam.
- Existing tests should retain their assertions; report added/changed coverage.
- Parent runs full gates and verifies history before main fast-forward.

## Staffing

Parent: integration, generated-artifact review, final tests and Git commits.
Sol (gpt-5.6-sol): bounded conflict fixes only after exact paths are assigned,
in this isolated worktree. Base is this committed brief on main 0ac1172d8.
One focused run per affected target and one correction rerun, then checkpoint.
No full-suite loops, source refactors, formatting or commits by Sol. Parent
runs cargo fmt once immediately before the relevant implementation commit.
An existing file over 500 lines may receive a localized conflict fix; no new
file over 500 lines and no unrelated file splitting.
