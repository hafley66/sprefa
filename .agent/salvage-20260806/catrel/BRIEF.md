# LANE catrel — two commits, both mechanical

You are working in `/Users/chrishafley/projects/sprefa-lanes/catrel`, a git
worktree on branch `lane/catrel`, based on `94524991`.

**If reality deviates from this brief at any point, STOP and write what you
found into REPORT.md. Do not improvise, do not widen the scope, do not "fix"
anything this brief does not name.**

## 0. First actions, in order

1. `git rev-parse HEAD` must print `94524991c1b1c21f56c80f59fd304c5db6dbe680`.
   Anything else: STOP and report.
2. `git rev-parse --abbrev-ref HEAD` must print `lane/catrel`. Else STOP.
3. `grep -rc "__catalog_rel" v6/prolog/lower.pl` must print `3`. Else STOP:
   the file has moved under you and the line numbers below are stale.

Package manager for `v6/tsv2` is **pnpm** (`v6/tsv2/pnpm-lock.yaml` exists).
Never run `npm install`, `npm test`, or `yarn`. It rewrites the lockfile.

## 1. Files you own

| file | occurrences of the old name | you may edit |
|---|---|---|
| `v6/prolog/lower.pl` | 3 | YES |
| `v6/prolog/analyze.pl` | 1 | YES |
| `v6/prolog/compile.pl` | 0 (commit 2 only) | YES |
| `v6/prolog/compile/test/plunit_tests.pl` | 12 | YES |
| `v6/tsv2/tests/catalogRows.test.ts` | 13 | YES |
| `v6/prolog/ARCH.pl` | 1 | **NO. LEAVE IT ALONE.** It records what shipped under the old name and is a historical record, not live code. |

Nothing else. If a validation run points at a file outside that list, STOP and
report rather than editing it.

## 2. COMMIT 1 (h1) — rename the SQL name `__catalog_rel` to `__rel`

Only the SQL identifier and the prolog ATOM change. **Do not rename any
predicate**: `catalog_ddl_contract/2`, `catalog_table_ddl/1`,
`catalog_row_ddl/3`, `catalog_primitive_rows/*`, `catalog_rel_rows/*`,
`catalog_column_rows/*`, `catalog_type_id/2`, `materialize_catalog_rel/2`,
`program_uses_catalog/2`, `catalog_mentions_atom/1` all keep their names.

Edits:

1. `v6/prolog/lower.pl:635` — `catalog_ddl_contract('__catalog_rel',` becomes
   `catalog_ddl_contract('__rel',`.
2. `v6/prolog/lower.pl:642` — the index DDL string. Both identifiers change:
   `"__catalog_rel_parent"` becomes `"__rel_parent"`, and
   `ON "__catalog_rel"` becomes `ON "__rel"`.
3. `v6/prolog/lower.pl` — the `format/3` template inside `catalog_row_ddl/3`:
   `INSERT OR IGNORE INTO "__catalog_rel"` becomes
   `INSERT OR IGNORE INTO "__rel"`.
4. `v6/prolog/analyze.pl:209` — `functor(Atom, '__catalog_rel', 6)` becomes
   `functor(Atom, '__rel', 6)`. The comment two lines above mentions the atom;
   update the atom inside it and change nothing else about that comment.
5. `v6/prolog/compile/test/plunit_tests.pl` — all 12 occurrences, including the
   exact-string assertion around `:583` which contains
   `CREATE TABLE "__catalog_rel" (...)`.
6. `v6/tsv2/tests/catalogRows.test.ts` — all 13 occurrences, including the
   scratch DDL string around `:57` and the two comment mentions of
   `__catalog_rel_parent`.

Verify before committing:

```
grep -rn "__catalog_rel" v6/prolog v6/tsv2 | grep -v ARCH.pl
```

must print NOTHING. Exactly one occurrence remains repo-wide, in `ARCH.pl`.

Then run the commit-1 gates in section 4 and commit with subject:

```
catalog: rename __catalog_rel to __rel
```

## 3. COMMIT 2 (h2) — the `arity` column, and a refusal for two arities

### Part A — add `arity` as the SEVENTH catalog column

Column order is `rel_id, parent_id, ordinal, local_name, kind, type_id, arity`.
`arity` goes LAST. Everything downstream derives the count from the contract, so
`materialize_catalog_rel/2` and the `ArrivalTargets` subtraction in `compile.pl`
need no edit: both already call `length(ColumnSpecs, Arity)`.

1. `v6/prolog/lower.pl` `catalog_ddl_contract/2` — append `arity-int` to the
   column-spec list, making it 7 entries.
2. `v6/prolog/lower.pl` `catalog_row_ddl/3` — the INSERT column list in the
   `format/3` template gains `, "arity"` at the end.
3. `v6/prolog/lower.pl` `catalog_row_part/*` — the folder that turns one
   `row(...)` into VALUES text. It matches `row/6` today; it becomes `row/7`.
4. `v6/prolog/lower.pl` `catalog_primitive_rows/4` —
   `row(Id, 0, 0, Name, primitive, 0)` becomes
   `row(Id, 0, 0, Name, primitive, 0, 0)`. A primitive has no arity, so 0.
5. `v6/prolog/lower.pl` `catalog_rel_rows/4` — two changes in one clause:
   `RelPlan = relplan(Name/_, _Kind, Columns, _Key, ColumnTypes)` becomes
   `RelPlan = relplan(Name/RelArity, _Kind, Columns, _Key, ColumnTypes)`, and
   `RelRow = row(Id0, 0, 0, Name, rel, 0)` becomes
   `RelRow = row(Id0, 0, 0, Name, rel, 0, RelArity)`.
6. `v6/prolog/lower.pl` `catalog_column_rows/7` —
   `ColumnRow = row(Id0, RelId, Ordinal, ColumnName, column, TypeId)` becomes
   `row(Id0, RelId, Ordinal, ColumnName, column, TypeId, 0)`. A column has no
   arity, so 0.
7. `v6/prolog/analyze.pl:208-209` — `functor(Atom, '__rel', 6)` becomes
   `functor(Atom, '__rel', 7)`, and the comment above it says "Arity 6"; it
   becomes "Arity 7".
8. `v6/prolog/compile/test/plunit_tests.pl` — the exact `CREATE TABLE` string
   gains `"arity" INTEGER NOT NULL` as the last column AND `"arity"` as the last
   entry of the `PRIMARY KEY (...)` list. Any test rule that writes
   `__rel(A, B, C, D, E, F)` gains a seventh argument. The test whose name
   contains `arity_exact` pins the gate arity; update it to 7.
9. `v6/tsv2/tests/catalogRows.test.ts` — same two changes: the DDL string and
   any 6-argument row literal or 6-column INSERT.

### Part B — refuse two arities of one rel name

`v6/prolog/lower.pl:162` is `table_name(Name/_Arity, Name).` It DROPS the arity,
so a program declaring both `edge/2` and `edge/3` emits
`CREATE TABLE "edge"` twice and boot dies on the second. Nothing refuses this
today. You are adding the refusal, NOT changing `table_name/2` (58 call sites;
out of scope, and a different design question).

In `v6/prolog/compile.pl`, find this existing line inside `program_plan/2`
(around `:171`):

```prolog
    append([RuleRefs, DeclaredRefs, SeededRefs], AllRefs0), sort(AllRefs0, AllRefs),
```

Insert a call on the NEXT line:

```prolog
    check_single_arity_per_name(AllRefs),
```

and define the predicate near the other check predicates in that file:

```prolog
% table_name/2 drops the arity, so two arities of one name would emit
% CREATE TABLE twice for one table.
check_single_arity_per_name(Refs) :-
    (   member(Name/LowArity, Refs),
        member(Name/HighArity, Refs),
        LowArity < HighArity
    ->  throw_as_compiler_refusal(rel_arity_collision(Name, LowArity, HighArity))
    ;   true
    ).
```

Use `throw_as_compiler_refusal/1` (already defined at `compile.pl:118`), never a
bare `throw/1`.

### Part C — one new test for the refusal

Add ONE plunit test to `v6/prolog/compile/test/plunit_tests.pl`. Find an
existing test in that file that asserts a refusal (search for
`unsupported_construct`) and copy its exact shape, including how it builds a
fixture and how it catches. The test: a program declaring `edge/2` and `edge/3`
must raise `unsupported_construct(rel_arity_collision(edge, 2, 3))`.

Put it in the `catalog_g1` group if the file's groups are thematic, or in a new
group named `arity_collision` if that matches the file's existing convention.
Match whatever the file already does.

Commit subject:

```
catalog: arity column, and refuse two arities of one rel name
```

## 4. Validation — run every command, paste every output into REPORT.md

Run from the worktree root `/Users/chrishafley/projects/sprefa-lanes/catrel`.

```bash
cd v6/prolog/compile   && swipl -q -l test/plunit_tests.pl -g run_tests -g halt
cd ../conformance      && swipl -q -l go.pl -g go -g halt
cd ..                  && bash compile/scripts/text_door_receipt.sh
cd .                   && bash tools/prolog-lint.sh
cd ../tsv2             && pnpm test
cd .                   && bash scripts/sweep.sh
```

Expected verdicts:

| rail | before your change | after |
|---|---|---|
| plunit | 351 / 351 | 352 / 352 (your one new test) |
| conformance | 302 PASS / 0 FAIL | **302 PASS / 0 FAIL, unchanged** |
| TEXT_DOOR | compiled=420 byte_identical=420 | **420 / 420, unchanged** |
| prolog-lint | findings=1 baseline=1 | **findings=1 baseline=1, unchanged** |
| tsv2 | 149 pass / 1 skip / 0 fail | 149 / 1 / 0, unchanged |
| sweep | wrong=0 final_wrong=0 over 420 | **wrong=0 final_wrong=0, unchanged** |

**THE CRITICAL RECEIPT.** `TEXT_DOOR byte_identical` must stay `420`. Any
program that never names the catalog rel must emit a byte-identical module. If
that number drops, the rename or the new column leaked into programs that do not
use the catalog. STOP, do not commit, and report the count plus the first
differing module name.

If plunit reports 352/352 but conformance drops, STOP: green units with a red
corpus means the contract moved in a way the units do not see.

## 5. Style laws, non-negotiable, applied to every line you write

- **Comment budget**: a comment states only a constraint the code cannot show.
  No change-log narrative, no dates, no references to this brief, no restating
  the next line. **Maximum 2 consecutive comment lines in new code** — a hook
  rejects a third.
- Banned words in prose AND in identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source, base layer, critical, mode.
- No em dashes anywhere, in code or in REPORT.md.
- Variable names are descriptive, never single-letter. `LowArity`, not `A`.
- **Colocated consistency**: inside a file, follow that file's existing style
  for naming, quoting, and layout, even if it differs from another file.
- Never `npm`. pnpm only.

## 6. Deliverable

1. Exactly two commits on `lane/catrel`, in the order given.
2. `REPORT.md` at the worktree root containing:
   - the two commit shas
   - the full stdout of all six validation commands, per commit if they differ
   - the `grep -rn "__catalog_rel" v6/prolog v6/tsv2 | grep -v ARCH.pl` output
     (must be empty)
   - a section titled `DEVIATIONS` listing anything that did not match this
     brief, or the single word `none`
3. **Final action, do not skip it**: this harness emits no completion event, so
   the coordinator learns nothing unless you send it:

```bash
bus hail --to fable-main --from catrel \
  --body "catrel done: <sha1> <sha2>; plunit <n>, conformance <n>/<n>, text_door <n>/<n>, sweep wrong=<n>; deviations: <none|summary>"
```
