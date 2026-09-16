# Brief: delete the cut in normalized_member_row/2

## Base
`origin/main` = f6430510e. Worktree at `/Users/chrishafley/projects/sprefa/.boop-worktrees/fix/generic-member-row-cut`, branch `fix/generic-member-row-cut`. FIRST action: `git status` (clean) and `git log -1` (must be f6430510e). Otherwise STOP AND REPORT. NEVER `git stash` (stash refs are shared across worktrees).

## Defect
`v6/prolog/0_generic_expand.pl:186` (first clause of `normalized_member_row/2`) ends in `!`. `normalized_type_rows/2` calls it under `findall`, so the cut discards the `nth1` choicepoint and the `member(rel_template...)` choicepoint: one member row is minted for the whole program (first template, first column), and clause 2 (`type_decl` members) never runs when any template exists.

Visible output: `v6/prolog/compile/out/mixed_bounded_and_free_parameters.types.rs` emits `pub struct Entry<Key: JsonEncodable, Value> { pub key: Key }`, `value` missing. Commit ea2e5265d added `rust_phantom_property_text/4` in `v6/prolog/compile/8_emit_rust_types.pl` (comment cites `0_generic_expand.pl:186`) to paper over rustc E0392 on the missing column.

## Fix
1. Delete the `!` at `0_generic_expand.pl:186`.
2. If a `rel_template` and a `type_decl` share an owner name, both clauses mint the same `member_id`; keep the template row (dedupe on member id, template first). Check whether that case can occur (`grep -n type_decl 0_generic_expand.pl`, parse output for a template) before writing dedupe code; if it cannot occur, say so in the PR and skip the dedupe.
3. Keep `rust_phantom_property_text/4` as a rail for a genuinely unused parameter (a template that declares `T` and never uses it in a column is still legal); update its comment to drop the `:186` citation. If after the fix no fixture in `compile/out` still emits `phantom`, say so in the PR.

## Tests, fail-first, additive
plunit in `v6/prolog/compile/test/plunit_tests.pl` next to `generic_type_ir_*` tests (~line 5013). Each test names the pre-fix output in a `% FAIL-PRE-FIX:` header line. Run each new test alone while iterating: `cd v6/prolog && swipl -g "run_tests(<name>)" -t halt compile/test/plunit_tests.pl` (check the file head for the exact load recipe; `cd v6 && just plunit` runs the whole battery, ~11s, run it ONCE at the end).
1. two-column template `rel pair(T)(left: T, right: T)`: two member rows for owner pair.
2. two templates in one program: member rows for both owners.
3. template plus plain `rel cell(id: int)`: cell's `id` member row present.
4. rust emit test (next to the ea2e5265d tests, grep `E0392` in plunit_tests.pl): `mixed_bounded_and_free_parameters` shape emits `pub value: Value,` inside `Entry`.

## Regenerate
`cd v6/tsv2 && bash scripts/sweep.sh` once, then commit `v6/prolog/compile/out/**` changes in the SAME PR (Chris: outputs ride along). Also `bash v6/sprefa-engine-rs/grade.sh` once; report the line it prints. rustc check on the changed `.types.rs` files as ea2e5265d did (see that commit message for the exact command).

## Files owned
`v6/prolog/0_generic_expand.pl`, `v6/prolog/compile/8_emit_rust_types.pl`, `v6/prolog/compile/test/plunit_tests.pl`, `v6/prolog/compile/out/**`, `v6/tsv2/gen_emitted/**` if the sweep rewrites them. Nothing else. Do not touch `v6/prolog/compile/dl_view/**`.

## Pre-commit
Hook needs `DL_EXTRACT_BIN=/Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract` (exists, built Aug 17) and pnpm installs in `v6/tsv2` and `v6/sprefa-store/js` (`pnpm install --frozen-lockfile` in each, in the worktree).

## PR
`gh pr create --base main`. Body shape, exactly:
- 1-3 plain sentences on what a user gets, with a dl6 snippet and the emitted Rust before/after.
- `## Reading order`: numbered files, why each changed.
- `## Tests`: per test: name, input, expectation, what it printed before. One line "full suite unchanged otherwise".
No suite counts, no allowlist references, no words gate/leg/receipt/door/probe/refusal. Then merge it yourself: `gh pr merge --squash --delete-branch` after CI is green (`gh pr checks --watch`).

## Style
No em dashes. Comments state only constraints the code cannot show. Descriptive prolog variable names. Banned words: provenance, substrate, load-bearing, regime, refusal, ground (verb).

## Report back
Three lines: PR number + merge sha, the four test names with pass state, the grade.sh line. Plus the exact error text if anything failed.
