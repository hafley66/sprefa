# Lane: the 9 unlisted plunit failures

## Base
`git merge --ff-only e70417d9` is your FIRST action. Failure = STOP AND REPORT.
Worktree: `.boop-worktrees/fix/plunit-nine-unlisted`.

## The job

`cd v6 && just plunit` reports **15 tests failed** of 624, exit 1.

`.github/CI-KNOWN-RED.md` allowlists 4 of the named ones. The other 9 are NOT
allowlisted. By the repo's own law, an unlisted failing leg is the real signal.

Fix all 9. Do not widen the allowlist to make them go away. If one of them
turns out to be a test asserting something the user has since decided against,
say so with the decision quoted and the file:line, and STOP on that one.

## The 9, grouped by suspected cause

### Group 1: DDL and catalog shape, 6 tests

```
catalog_g1:catalog_table_shape
catalog_g1:catalog_table_shape_at_dict
catalog_g1:catalog_rel_id_is_the_key_in_both_modes
sql_text_snapshots:switch_as_keyed_replace_ddl_pk_shape
sql_text_snapshots:world_fed_keyed_arrival_uses_key_constraint_and_replace
phase5_value_plane:bool_and_float_storage_constraints_are_exact
```

Suspected one cause. The user decided 2026-08-12, verbatim: "TAKE THE CORRECT
AND MOST CONSISTENT ONE EVEN IF IT MEANS MORE WORK". Applied to the two set-rel
DDL shapes, the answer is ONE shape:

```sql
("__id" INTEGER PRIMARY KEY, <cols>, UNIQUE (<cols>))
```

with zero columns collapsing to `("__id" INTEGER PRIMARY KEY)`. The
`PRIMARY KEY (<cols>) WITHOUT ROWID` branch was the outlier and contradicted
the surrogate-keys law.

These snapshot tests very likely still assert the OLD shape. Verify that before
you assume it. If they assert the old shape, the tests are stale and get updated
to the decided shape. If they assert something else, that is a real defect and
you fix the code.

Read `.claude/skills/sql-relational-design` and `.claude/skills/sqlite-costs`
BEFORE touching any DDL. Both are mandatory reads for schema work.

### Group 2: subset gate, 2 tests

```
supported_subset_gate:accepts_edge_head_column_typed_from_its_body
supported_subset_gate:initial_only_ref_still_gets_a_table
```

### Group 3: dd determinism, 1 test

```
isolated_compiler_dd:json_twins_are_deterministic
```

"Deterministic" failing usually means an unordered map or set is being
serialized. Find the iteration order, do not paper over it with a retry.

## The 4 that ARE allowlisted, do not touch

```
catalog_plane_rail:level_plane_family_corpus_counts
expression_inventory:inventory_is_exactly_the_expected_rows
json_merge_patch:json_patch_lowers_with_the_null_stand_in_guard
rel_zero_arity:a_root_rel_zero_still_has_no_storage
```

Leave them. A separate decision covers them.

## Method

Run each failing test ALONE first and read its actual expected-versus-got. Do
not start from the whole battery. Measure a leg three times, never once: two
back-to-back whole-gate runs on one tree gave DIFFERENT failing sets under lane
load, measured 2026-08-12.

Report, per test, a one-line cause before you fix anything. Six of them may
share one cause and that changes the shape of the fix.

## Gates, three runs each
```
cd v6 && just plunit                    # 624 tests, target 4 failed (the allowlisted only)
cd v6/prolog/conformance && swipl -g go -t halt go.pl   # 392 PASS / 0 FAIL
swipl -g go -t halt v6/prolog/ARCH.pl
cd v6/tsv2 && bash scripts/sweep.sh     # RUN identical must not drop
bash v6/sprefa-engine-rs/grade.sh       # byte-clean must not fall below 280
```

`just green-all` is RED by design; `.github/CI-KNOWN-RED.md` is the allowlist.
Update that file ONLY to remove rows you fixed, never to add rows.

## Files you own
`v6/prolog/**` except the four listed below, `v6/prolog/compile/test/plunit_tests.pl`,
`.github/CI-KNOWN-RED.md` (removals only), plan doc
`plans/2026-08-12-plunit-nine-unlisted.md`.

## Files you must NOT touch, live lanes own them
- `v6/prolog/emit_rust.pl`, `v6/sprefa-engine-rs/**`
- `v6/prolog/compile/7_emit_ts_types.pl`, `8_emit_rust_types.pl`
- `v6/labs/exec_shootout/**`
- `v6/boop/**`, any `Cargo.toml`, `v6/justfile`
- `CLAUDE.md`

## COMMIT YOUR WORK
Seven lanes today wrote a full deliverable and exited rc=0 with a dirty tree.
Run `git add -A && git commit` before you exit and confirm with
`git log --oneline -1`.

## Laws
- Doubt yourself before asserting. Cite the assertion site for every failure.
- A test asserting a superseded decision is STALE, and saying so needs the
  user's decision quoted.
- Never delete or skip a test to make a battery green. That is the defect the
  `test-false-green` rail exists to catch.
- Comments state only constraints the code cannot show. No dates, no narrative.
- Surrogate keys: INTEGER ids, natural TEXT keys once in a dictionary with
  UNIQUE. A composite TEXT PRIMARY KEY in emitted or hand DDL is a DEFECT.
- No em dashes. No negative parallelism. No sycophancy.
- Banned in prose AND identifiers: provenance, substrate, load-bearing, regime.

## Report
One line per test: name, cause, fix. Then the plunit count, and the four other
gate outputs.
