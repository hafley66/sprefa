# orphan-executors: delete five unreachable host executors + their pinning tests

## TOC
1. Orphan proof, per executor
2. Extra finding: three golden gates already broken by the same PR #370 cut (out of scope, not touched)
3. Deleted/retargeted tests, per file, with the one-line note
4. Deleted dead helpers/consts/imports
5. TypeScript twin: no change needed
6. `.github/CI-KNOWN-RED.md`
7. Gate table, before/after, verbatim
8. Branch state

## 1. Orphan proof, per executor

Root cause, confirmed at the code site: `81bb20ce5 tsv2: process adapters from sidecar rows (#370)` deleted the
name-based routing branches. `v6/sprefa-engine-rs/src/hosts.rs:44-52` `executor_for` matches exactly three
strings: `"shell"`, `"soopy"`, `"sprefa_extract"`. Every other execution string falls to `None`.

| executor | type-name grep (whole repo, code) | execution-string grep (whole repo, code) | routing reachability | verdict |
|---|---|---|---|---|
| `DepCrawlExecutor` | only `src/hosts.rs` (def) and `tests/dep_resolve.rs` (direct construction) | `dep_crawl_repo/_visited/_edge/_unresolved` not matched anywhere in `executor_for` | `HostLiveRunner::new` fails at construction with `unknown process adapter 'dep_crawl_repo'` before ever reaching the type | orphaned |
| `GitRefExecutor` | only `src/hosts.rs` (def) and `tests/git_refs.rs` (direct construction) | `git_ref`/`git_tag` not matched | same as above | orphaned |
| `GitRevisionExecutor` | only `src/hosts.rs` (def) and `tests/git_refs.rs` (direct construction) | `git_merge_base`/`git_ahead_behind`/`git_ancestor` not matched | same as above | orphaned |
| `ChangeFactExecutor` | only `src/hosts.rs` (def) and `tests/change_facts.rs` (direct construction) | `git_change`/`git_rename`/`git_changed_line` not matched | same as above | orphaned |
| `SoopyFilesExecutor` | only `src/hosts.rs` (def) and `tests/live_hosts.rs` (direct construction) | `files`/`files_at`/`repo_files`/`repo_files_at` route through the generic `"shell"` arm now, no name check left | `live_runner_selects_soopy_for_the_unchanged_shell_host_plan` (pre-change) failed with `sh: this: command not found` — the shell template actually ran | orphaned |

All five: **zero live callers** in `src/**`. Every non-test reference outside `src/hosts.rs` is a comment/doc
(`plans/2026-08-16-soopy-extract-entanglement.md`, `TASKS/*.BRIEF.md`, `issues/*/item.md`,
`.github/CI-KNOWN-RED.md`, golden `.dl6`/`.sh` comments). Every test reference that isn't one of the deleted
tests is a comment (`change_facts.rs:12`, my own added note).

The TypeScript twin (`v6/tsv2/serve/1_hosts.ts:501-506`) never had these five. `ProcessAdapters` there is exactly
`ShellAdapter`, `SprefaExtractAdapter`, `SoopyAdapter` (refuses: `"soopy requires the Rust runtime target"`),
`BoopAdapter`. No TS-side change was needed or made.

## 2. Extra finding, not mine to fix

Independently verified (not part of the mission's evidence, found while proving orphan status): three `just`
recipes still exercise these exact execution strings and are **already red on `origin/main`**, independent of
this deletion:

- `just dep-crawl-golden` (`v6/tsv2/goldens/multirepo_crawl/5_dep_gate.sh`) — `emit_rust_harness` panics:
  `sh host 'dep_crawl_edge': exited exit status: 3: ... is linked in-process`
- `just git-refs-golden` (`v6/tsv2/goldens/multirepo_crawl/8_git_gate.sh`) — same shape, `git_ahead_behind`
- `just scip-combo` (`v6/tsv2/goldens/scip_combo/8_gate.sh`) — same shape, `dep_crawl_repo`

Ran all three verbatim on a clean `bf2eb4bc0` worktree before touching anything: all three FAIL identically.
Deleting the Rust structs changes nothing about their outcome — the routing that would have reached them was
already gone. None of the three is in `just green-all` (`v6/tools/green-parallel.sh` runs only
`multirepo-golden`, which does not touch these hosts); they live only in the deprecated `green-all-serial`
bisect chain. This is why nothing reported it. Left untouched: fixing the routing is a language/host-routing
design decision, not this lane's mandate, and the gates list in the brief didn't ask for these three. Flagging
for the user to decide whether it's worth an issue or a `CI-KNOWN-RED.md` row.

## 3. Deleted/retargeted tests, per file

Total removed: **31 tests** (not 5 — the 5 named in the brief were only the ones that fail today because they
route through `HostLiveRunner`; most of the pinning surface constructs the executor directly and was still
green, per the brief's own rule that direct construction is "the pinning test, not a live caller").

### `v6/sprefa-engine-rs/tests/git_refs.rs` — deleted whole file (17 tests)

Every one of the file's 17 tests constructs `GitRefExecutor` or `GitRevisionExecutor` directly, or drives them
through `HostLiveRunner`. Nothing else lives in the file (confirmed: no test uses `soopy::Refs`/
`soopy::RevisionGraph` without going through one of the two executors). One-line note: this file was 100% executor
coverage — ref/tag naming, annotated-tag peeling, ref-store witness memoization (3 tests), merge-base/
ahead-behind/ancestor answers, revision-memo triple-keying, the two named-stop tests, and the two routing tests
(`the_five_ruled_names...`, `the_response_rows_carry_the_demanded_inputs`). Nothing here tested `soopy` itself
(that crate has its own test suite); it all tested the now-deleted memoizing wrapper. Nothing is lost that isn't
already covered by `soopy`'s own tests plus the underlying git-plumbing behavior neither executor added logic
over besides memoization and shape translation.

### `v6/sprefa-engine-rs/tests/dep_resolve.rs` — 4 of 13 tests removed

| removed test | one-line note |
|---|---|
| `the_host_arm_projects_one_crawl_into_four_relations` | Covered `DepCrawlExecutor.run()` decoding four `dep_crawl_*` host names into rows. The underlying projection (`DepResolveOutcome::arrivals()`) is still covered by `the_outcome_projects_to_signed_arrivals`, which is retained. |
| `the_four_ruled_names_reach_the_linked_arm_through_the_host_plan` | The routing test — this is the one in `.github/CI-KNOWN-RED.md`'s retired row. Proved `HostLiveRunner` reaches `DepCrawlExecutor` by name; that routing no longer exists. |
| `an_unlinked_frontier_kind_is_a_named_stop` | Covered the executor's error message when `frontier` isn't `go_mod`/`specifier`. `DepResolver`'s own frontier dispatch is untested at this level now, but the executor was a thin `match` with no logic beyond string comparison — nothing computational is lost. |
| `a_missing_checkout_root_is_a_named_stop` | Covered the executor's required-input check (`checkout_root`), a property of `HostDemand` decoding generic to every executor, not specific to dep-crawl logic. |

The other 9 tests (`a_dependency_cycle_closes_...`, `a_drifting_revision_answer_...`, `a_two_hop_chain_...`,
`an_absent_checkout_emits_...`, `an_absent_seed_is_...`, `a_package_path_resolves_...`,
`the_outcome_projects_to_signed_arrivals`, `a_local_corpus_frontier_closes` (ignored), and
`the_roster_reads_manifests_at_head_not_the_worktree`) exercise `DepResolver`/`LocalRepoRoster`/`GoModFrontier`
directly and are untouched — the module's real logic (cycle closure, revision caching, path resolution,
manifest-at-HEAD reads) keeps full coverage.

### `v6/sprefa-engine-rs/tests/change_facts.rs` — 7 of 17 tests removed

| removed test | one-line note |
|---|---|
| `the_change_host_names_its_kind_in_the_row` | Covered `ChangeFactExecutor` decoding `git_change` rows. The underlying kind-naming is covered by the retained `a_new_path_is_created`/`a_removed_path_is_deleted`/`a_changed_blob_is_modified_...` (they call `SoopyRevisionDiffer` directly). |
| `the_changed_line_host_answers_integers` | Covered `git_changed_line` row decoding. Retained `changed_line_names_the_head_side_lines` covers the same line-number logic through the differ directly. |
| `the_diff_memo_keys_on_the_whole_triple` | Covered the executor's `(repo, rev_base, rev_head)` memo key. No replacement — this was memoization-only behavior with no analogue in the retained differ-level tests, since `SoopyRevisionDiffer` itself has no memo. Nothing computational is lost, only a caching property of the now-deleted wrapper. |
| `an_unresolvable_revision_is_a_named_stop` | Covered the executor's error surface for an absent revision spelling. |
| `a_missing_host_input_is_a_named_stop` | Covered the executor's required-input check (`rev_base`), generic `HostDemand` behavior, not diff-specific. |
| `work_revision_is_not_memoised` | Covered the memo's `WORK`-pair exclusion. Retained `work_revision_diffs_the_dirty_worktree` still proves `WORK` reads the live worktree correctly through the differ directly; only the "and it's never memoised" property (irrelevant with no memo) is gone. |

`the_three_ruled_names_reach_the_linked_arm_through_the_host_plan` (the routing test, retired
`CI-KNOWN-RED.md` row) was also removed as part of this group.

Retained 10 tests still exercise `SoopyRevisionDiffer`/`parse_revision`/`ChangeKind` directly: creation, deletion,
modification, rename detection, changed-line numbers, binary handling, pair ordering, self-diff, dirty-worktree
diffing, and revision-string parsing. `const KEEP_DIRTY_AGAIN` became dead (used only by the deleted
`work_revision_is_not_memoised`) and was removed with it.

### `v6/sprefa-engine-rs/tests/live_hosts.rs` — 3 of 15 tests removed

| removed test | one-line note |
|---|---|
| `linked_soopy_file_hosts_match_the_existing_shell_contracts` | Compared `SoopyFilesExecutor`'s in-process answer for `repo_files`/`repo_files_at` against the equivalent hand-written shell pipeline, byte for byte. No replacement: the shell pipeline itself is still emitted in `.dl6` templates (it's the documented fallback contract), just never diffed against a linked arm anymore since there is none. |
| `linked_soopy_unscoped_hosts_match_the_existing_shell_contracts` | Same, for `files`/`files_at`. |
| `live_runner_selects_soopy_for_the_unchanged_shell_host_plan` | The routing test (retired `CI-KNOWN-RED.md` row) — proved a `"shell"`-tagged `files` plan got redirected to the native executor rather than the shell template. That redirect no longer exists; the shell template is now genuinely what runs. |

Helpers `git_fixture_repo()` and `output_lines()` were used only by the two removed comparison tests and were
deleted with them. `git_run()` stays — used by the retained `digest_carrying_demand_reads_the_blob_not_the_worktree`.
The other 12 tests in the file (Shell/SprefaExtract executor tests, `run_schedule_live` receipts,
`structured_shell_input_refuses_...`, five `#[tokio::test]` async tests, etc.) are untouched.

## 4. Deleted dead helpers/consts/imports (`src/hosts.rs`)

Deleted with the executors (all exclusively used by them, verified by grep before removal):

- `path_from_cwd` (was used only by `SoopyFilesExecutor`)
- `host_row`, `dep_crawl_row`, `ndjson` (used only across the five)
- `DEP_CRAWL_HOSTS`, `DEP_CRAWL`, `GIT_REF_HOSTS`, `GIT_REVISION_HOSTS`, `GIT_REFS`, `GIT_REVISIONS`,
  `GIT_CHANGE_HOSTS`, `GIT_CHANGES` (the 8 dead-code-warned items — see gate table, not 5)
- `GIT_REF_COLUMNS`, `GIT_TAG_COLUMNS`, `GIT_MERGE_BASE_COLUMNS`, `GIT_AHEAD_BEHIND_COLUMNS`,
  `GIT_ANCESTOR_COLUMNS`, `TAG_PREFIX`, `ref_kind`, `ref_target`, `RefStoreWitness`, `ref_store_witness`
- the whole `use crate::change_facts::{...}` and `use crate::dep_resolve::{...}` import blocks (lines 9-17):
  `IRevisionDiffer`, `RevisionDiff`, `SoopyRevisionDiffer`, `GIT_CHANGED_LINE_COLUMNS`, `GIT_CHANGE_COLUMNS`,
  `GIT_RENAME_COLUMNS`, `DepResolveOutcome`, `DepResolver`, `GoModFrontier`, `IDepFrontierSource`,
  `LocalRepoRoster`, `SpecifierFrontier`, `DEP_EDGE_COLUMNS`, `DEP_REPO_COLUMNS`, `DEP_UNRESOLVED_COLUMNS`,
  `DEP_VISITED_COLUMNS`
- `Component` dropped from `use std::path::{Component, Path, PathBuf}` (only `path_from_cwd` used it)

`v6/sprefa-engine-rs/src/dep_resolve.rs` and `src/change_facts.rs` (the underlying modules `DepResolver`,
`SoopyRevisionDiffer`, etc.) are **untouched** — they have their own full test coverage via `tests/dep_resolve.rs`
and `tests/change_facts.rs`'s retained tests, and stay reachable/tested independent of the deleted host-executor
wrappers.

## 5. TypeScript twin

No change. `v6/tsv2/serve/1_hosts.ts:501-506` never carried these five adapters (verified in section 1).

## 6. `.github/CI-KNOWN-RED.md`

Retired the `cargo test (no CI leg)` row (was line 102) naming the 5 now-deleted failing tests. All five are gone
from the tree; `cargo test` is clean.

## 7. Gates, before/after (measured on `bf2eb4bc0`, this repo, this session — not assumed)

| gate | command | before | after |
|---|---|---|---|
| build warnings | `cd v6/sprefa-engine-rs && cargo check --all-targets` | **8** dead-code warnings (not 5 — `DEP_CRAWL_HOSTS`, `DEP_CRAWL`, `GIT_REF_HOSTS`, `GIT_REVISION_HOSTS`, `GIT_REFS`, `GIT_REVISIONS`, `GIT_CHANGE_HOSTS`, `GIT_CHANGES`; the executor structs themselves are `pub` so never warned) | **0** warnings, 0 errors |
| cargo test | `cargo test --no-fail-fast` | **131 passed, 5 failed**, 1 ignored (137 total). Failures: `the_three_ruled_names_reach_the_linked_arm_through_the_host_plan` (change_facts.rs), `the_four_ruled_names_reach_the_linked_arm_through_the_host_plan` (dep_resolve.rs), `the_five_ruled_names_reach_the_linked_arm_through_the_host_plan` (git_refs.rs), `the_response_rows_carry_the_demanded_inputs` (git_refs.rs), `live_runner_selects_soopy_for_the_unchanged_shell_host_plan` (live_hosts.rs) | **105 passed, 0 failed**, 1 ignored (106 total). 31 tests removed matches the 31 deleted above exactly (137-106=31). Zero remaining failures. |
| conformance | `cd v6/prolog/conformance && swipl -g go -t halt go.pl` | 433 PASS, 1 known red (`nested_zero_column_child_is_one_row_per_parent`) | unchanged: 433 PASS, same 1 known red |
| sweep | `cd v6/tsv2 && bash scripts/sweep.sh` | `RUN total=335 identical=322 wrong=0 emitted_crash=7 rejection=6 no_oracle_log=0` (same 7 enum-identity crashers) | unchanged, byte-for-byte same line |
| npm test | `cd v6/tsv2 && npm test` | 245 tests, 240 pass, 4 fail, 1 skip (measured after regenerating `gen_emitted` via sweep first — the raw first-run number without that step is noise, see below), same 4 failing names (`golden-flex served: ...`, `tests/listStoredSnapshot.test.ts`, `flag off/on + zero-query: ...` x2) | unchanged: 245 tests, 240 pass, 4 fail, 1 skip, same 4 names |

Note on npm test: a `npm test` run against a worktree whose `gen_emitted/`/`gen_served/` caches haven't been
populated by a prior `sweep.sh` run reports a different, much lower total (195 tests, ~166-165 pass) because
several test files import from generated fixtures that don't exist yet. That is pre-existing environment
behavior in both before and after trees, not a regression — confirmed by running `sweep.sh` then `npm test`
identically on both the `bf2eb4bc0` baseline and this branch and getting matching 245/240/4/1 both times. One
of the four failures (`sabotage: editing fixture in temp dir modifies only the changed row`) showed up once as a
5th failure on an early baseline run and was gone on a re-run — flaky/order-dependent, unrelated to this change,
confirmed not reproducible on a clean re-run of the identical baseline tree.

Rust-grade (`bash v6/sprefa-engine-rs/grade.sh`) and the three golden `just` recipes in section 2 were not part
of the brief's gate list and were only used for the orphan proof / the extra finding; not re-verified as a gate.

## 8. Branch state

Committed and pushed `chore/orphan-executors` to `origin`. No PR opened, per instructions.

Files touched:
- `v6/sprefa-engine-rs/src/hosts.rs`
- `v6/sprefa-engine-rs/tests/dep_resolve.rs`
- `v6/sprefa-engine-rs/tests/change_facts.rs`
- `v6/sprefa-engine-rs/tests/live_hosts.rs`
- `v6/sprefa-engine-rs/tests/git_refs.rs` (deleted)
- `.github/CI-KNOWN-RED.md`
