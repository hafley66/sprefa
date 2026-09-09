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

## Reconciliation details

- Extract merged without conflicts. DL7 had ten conflicted paths: eight code
  or test files and two Markdown files. Preserve main's checker/indexer flags,
  Rust checker call signature, SCIP snapshot staging, benchmark projections,
  and PostgreSQL arms. Preserve incoming region dispatch and IVM cache 128.
- Keep distinct lockfile, deleted-source and removed-workspace-member tests.
- Restore the pinned gamma.rs wire-corpus input. The re-export regression
  already constructs its own scratch gamma.rs; the wire golden stays unchanged.
- Supply go_checker: None in the engine request, matching its existing
  disabled optional Rust and TypeScript checker fields.
- Reconcile the DL7 entrypoint receipt's exact compiler-row count to 15542
  and runtime counts to (282,538,135,432,129,224,135). All other assertions,
  diagnostics, operator results, keys, history behavior and cleanup remain.
  Compiler and prelude sources are unchanged from the source branch.
- Give watch-e2e and sqlite-plan-e2e explicit test goals, exit status and
  repository-root execution. Both fixtures use root-relative source paths.
- Preserve recovery notes and the unfinished Prolog wrapper in Git. No new
  wrapper wiring or completed binding implementation is claimed.

The integration gate uses existing test assertions. It reconciles exact
receipt counts and activates the two existing E2E recipe test bodies. No test
coverage is removed and no golden-output bytes are regenerated.

## Current gate results

- Full extract CLI suite: 920 passed, 0 failed, 16 ignored across 174 result
  blocks; exit 0. `/private/tmp/sprefa-main-reconcile-extract-final.log`.
- Full DL7 entrypoint suite: 40/40, exit 0.
  `/private/tmp/sprefa-main-reconcile-entrypoints.log`.
- Reader, syntax expansion, module, trace, loader, Rust emitter, layout,
  source-query and DBSP-plan suites passed. Source-query uses an explicit
  halt to avoid executing the imported CLI main after its tests.
- SQLite query: 5/5. Native IVM integration: 42 passed, 0 failed. Native
  extension release build passed. Generation: 5/5 including compiled Rust.
- DD runner: 6 passed. Rust engine source-bind: 6 passed. Grammar: 1/1.
- Rust type-region E2E passed. Watch RAM/SQLite: 2/2. SQLite/RAM plan: 1/1.
  Actual Just recipe validation uses the root-qualified test invocations.
- Logs use `/private/tmp/sprefa-main-reconcile-` with suffixes `query.log`,
  `ivm-tests.log`, `ivm-build.log`, `generation.log`, `dd-runner.log`,
  `engine-source-bind.log`, `grammar.log`, `region-e2e.log`,
  `just-watch.log`, and `just-plan.log`.

## Staffing

Parent: integration, generated-artifact review, final tests and Git commits.
Sol (gpt-5.6-sol): bounded conflict fixes only after exact paths are assigned,
in this isolated worktree. Base is this committed brief on main 0ac1172d8.
One focused run per affected target and one correction rerun, then checkpoint.
No full-suite loops, source refactors, formatting or commits by Sol. Parent
runs cargo fmt once immediately before the relevant implementation commit.
An existing file over 500 lines may receive a localized conflict fix; no new
file over 500 lines and no unrelated file splitting.
