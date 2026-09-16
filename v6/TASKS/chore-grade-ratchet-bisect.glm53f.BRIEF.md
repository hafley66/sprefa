# Brief: find the commit that dropped 23 fixtures out of the Rust grade ratchet

Read `CLAUDE.md` in full first. Measurement only; you fix nothing and edit no source file.

## First action
```bash
git merge --ff-only 7be76330e60a3281001153474e58edf9472d7ee3   # STOP AND REPORT on failure
```

## The fact
`v6/sprefa-engine-rs/graded.tsv` (last written at `021ecad22`, 2026-08-23) lists 345 fixtures `clean`. `bash v6/sprefa-engine-rs/grade.sh` on `7be76330e60a3281001153474e58edf9472d7ee3` prints `RUST-GRADE graded=449 byte-clean=322` and a `RUST-GRADE REGRESSION` list of 23 fixtures (`relation_depth2_*`, `relation_depth3_*`, `struct_*`, `json_patch_fold_rfc7396_clauses`, `enum_variant_field_typed_as_rel_is_a_ref`, `one_colliding_ref_column_beside_a_disjoint_sibling`, `recursive_list_arg_parent_holds_child_node_values`, `relation_reference_target_and_parent_share_tick`, `two_bounded_parameters_mint_one_instance`, `variant_field_typed_as_struct_is_a_ref`, ...). Nobody knows which commit between `021ecad22` and `7be76330e60a3281001153474e58edf9472d7ee3` (177 commits) did it.

## Method
`git bisect start 7be76330e60a3281001153474e58edf9472d7ee3 021ecad22`, test script = `bash v6/sprefa-engine-rs/grade.sh > /tmp/grade.$(git rev-parse --short HEAD).txt 2>&1; grep -q 'RUST-GRADE REGRESSION' /tmp/grade.*.txt && exit 1 || exit 0` (write it to a file in your worktree's `.git`-ignored scratch, never commit it). One grade.sh run is a multi-fixture battery (5-8 min): run each in the background with `timeout 900`, never foreground-wait. First confirm `021ecad22` itself is green (`byte-clean=345`, no REGRESSION line); if it is not, STOP and hail: the ratchet was never true.
Also record, per bisect step, the `byte-clean=N` number and the first 5 lost names, so a two-step drop (e.g. 345->330->322) shows.

## Deliverable
No code. One markdown file `plans/2026-08-28-grade-ratchet-bisect.md`, committed on your branch and posted as a PR: a table (commit, date, subject, byte-clean, lost count), the culprit commit(s) with `git show --stat`, the exact reason text from `grade.sh`'s verdicts for 3 of the lost fixtures (the `compiled`/`diff`/`unsupported` reason column), and the one-line fix hypothesis with the throw site cited. Do not implement the fix.

## Receipts (PR body)
- The bisect log (`git bisect log`) verbatim.
- For the culprit: `byte-clean` on it and on its parent.
- `git diff 7be76330e60a3281001153474e58edf9472d7ee3 --stat`: only the one plan file.

## Style
Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth. Tables over prose. No narrative of what you tried.

## Delivery
PR title `plan: grade ratchet bisect, 345 -> 322`. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, culprit sha, byte-clean before/after>"`.
