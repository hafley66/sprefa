# Brief: regenerate the 8 stale engine-rs program.rs snapshots

## Base
`origin/main` = 0705f1f62 (PR #367 merged). Worktree `/Users/chrishafley/projects/sprefa/.boop-worktrees/chore/regen-program-rs`, branch `chore/regen-program-rs`. FIRST action: `git status` clean and `git log -1` = 0705f1f62, else STOP AND REPORT. NEVER `git stash`.

## Defect
`v6/sprefa-engine-rs/tests/fixtures/*.program.rs` (8 files) are emitter output frozen before the storage-name digest landed (PR #364, table names now `<module>_<rel>_<12hex>`). Tests still pass (the runtime creates whatever names the JSON says) but the snapshots no longer match what the emitter produces today.

## Files and sources
| snapshot | source | recipe |
|---|---|---|
| bytes_type_system.program.rs | v6/dl/fixtures/bytes-type-system.dl6 | swipl recipe below |
| source-mutations.program.rs | v6/dl/fixtures/source-mutations.dl6 | same (the exact line is in `tests/15_source_mutation_hosts.rs:14-18`) |
| source-offline-golden.program.rs | v6/dl/fixtures/source-offline-golden.dl6 | same |
| live_extract_calls.program.rs | v6/sprefa-engine-rs/tests/fixtures/live_extract_calls.dl6 | same |
| live_shell_probe.program.rs | v6/sprefa-engine-rs/tests/fixtures/live_shell_probe.dl6 | same |
| bounded_measure_recursion.program.rs (`// Program: bounded_measure_recursion_still_closes`) | program text inside `v6/prolog/conformance/fixtures/23_diverging_recursion.pl` | compile that program's source with the same emitter; commit 700b329e7 regenerated it last time, `git show 700b329e7` for how the header changed |
| diverging_measure_recursion.program.rs (`diverging_measure_recursion_is_bounded_and_loud`) | same file | same |
| list_persistence.program.rs (`split_value_is_the_interned_list_id`) | `v6/prolog/conformance/fixtures/19_list_value_position.pl` | same |

Recipe (from `tests/15_source_mutation_hosts.rs`), run from repo root:
```
swipl -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl \
  -g "compile_dl6('<src>.dl6', '<snapshot>.program.rs', [emitter(emit_rust:emit_program)])" -g halt
```
For the three conformance-embedded programs, write the program text to a temp `.dl6` under the scratch dir and use the same recipe; keep the `// Program: <name>` header the tests expect (check `tests/diverging_recursion.rs`, `tests/list_boundary.rs` for what they read).

## Verify
1. `git diff --stat` shows the 8 snapshots changed; `grep -c '_[0-9a-f]\{12\}"' <file>` shows digest names present in each.
2. `cd v6/sprefa-engine-rs && cargo test --tests` ONCE at the end; report the per-target `test result:` lines. Run single tests while iterating (`cargo test --test list_boundary` etc.).
3. `bash v6/sprefa-engine-rs/grade.sh` once; report its line.

## Owned files
`v6/sprefa-engine-rs/tests/fixtures/*.program.rs` only. If a test needs a header or name change to accept the regen, that test file too; say which and why in the PR. Nothing else.

## Pre-commit
`DL_EXTRACT_BIN=/Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract`; `pnpm install --frozen-lockfile` in `v6/tsv2` and `v6/sprefa-store/js` inside the worktree.

## PR
`gh pr create --base main`. Body: 1-2 plain sentences (snapshots regenerated to the digest table names, one before/after name), `## Reading order` (files, why), `## Tests` (targets run, result lines; "full suite unchanged otherwise"). No words gate/leg/receipt/door/probe/refusal, no em dashes. Do NOT merge; the coordinator merges. Report: PR number, sha, the cargo `test result:` lines, the grade.sh line, exact error text on any failure.
