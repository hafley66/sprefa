# LANE catseed: implement the g1 catalog producer

## First action, non-negotiable

```bash
cd /Users/chrishafley/projects/sprefa-lanes/catseed
git rev-parse HEAD    # MUST print e3997cecd88322ae029255c5e3cc8402e433d122
```

If it prints anything else, STOP and write REPORT.md saying so. Do not work around it.

## Files you own. Touch nothing else.

| file | what you do there |
|---|---|
| `v6/prolog/analyze.pl` | add `program_uses_catalog/2`, export it |
| `v6/prolog/lower.pl` | fill in `catalog_table_ddl/1` and `catalog_row_ddl/3`, gate the call site |
| `v6/prolog/compile/test/plunit_tests.pl` | add the new unit tests |

Two sibling lanes are editing `v6/tsv2/tests/catalogRows.test.ts` and `v6/prolog/ARCH.pl` right now. Editing either is a defect.

## Do NOT run these

- `npm install`, `pnpm install`, `npm ci`. `node_modules` is already present in every package. Installing rewrites the lockfile and breaks the tree.
- `git commit`, `git push`, `git merge`, `git rebase`. Leave your work uncommitted in the worktree.

## The scaffold that is already there

`v6/prolog/lower.pl` around line 630 already carries the contract fact, two stubs returning `[]`, and the wired call site inside `lower_program/2`. Read it before writing anything. Your job is to fill the two stub bodies and add the gate.

## What to build, exactly

### 1. `program_uses_catalog/2` in `v6/prolog/analyze.pl`

Mirror `program_uses_tick/2`, which is at `analyze.pl:180`. Read it first; copy its shape.

```prolog
% True when any rule in this program names the catalog rel. Every other program keeps a byte-identical emitted module, exactly the way program_uses_tick/2 keeps now/1 free.
program_uses_catalog(prog(_Decls, Rules), UsesCatalog).
```

`UsesCatalog = true` when some rule (head or body) mentions the functor `'__catalog_rel'`; `false` otherwise. Export it by adding `program_uses_catalog/2,` to the `:- module(...)` export list at `analyze.pl:19`, next to `program_uses_tick/2`.

### 2. `catalog_table_ddl/1` in `v6/prolog/lower.pl`

Replace `catalog_table_ddl([]).` with a clause returning exactly these two statement atoms, in this order, byte for byte:

```
CREATE TABLE "__catalog_rel" ("rel_id" INTEGER NOT NULL, "parent_id" INTEGER NOT NULL, "ordinal" INTEGER NOT NULL, "local_name" TEXT NOT NULL, "kind" TEXT NOT NULL, "type_id" INTEGER NOT NULL, PRIMARY KEY ("rel_id")) WITHOUT ROWID
CREATE INDEX IF NOT EXISTS "__catalog_rel_parent" ON "__catalog_rel" ("parent_id", "local_name")
```

`WITHOUT ROWID` matches how `rel_ddl/6` builds keyed tables at `lower.pl:778-780`. `IF NOT EXISTS` on the index is required because `serve/3_engine.ts:225` replays the whole DDL array on every program swap.

### 3. `catalog_row_ddl/3` in `v6/prolog/lower.pl`

Replace `catalog_row_ddl(_Decls, _RelPlans, []).` with a clause producing ONE statement: a single `INSERT OR IGNORE` whose VALUES list carries every row. One statement, never one per row. That is the repo's N+1 law.

Ids are assigned by POSITION in a single pass, so a recompile of the same program is byte-stable:

| pass | rows | rel_id | parent_id | ordinal | local_name | kind | type_id |
|---|---|---|---|---|---|---|---|
| 1 | the five primitives, in this exact order: `text`, `int`, `float`, `bool`, `json` | 1, 2, 3, 4, 5 | 0 | 0 | the primitive's name | `primitive` | 0 |
| 2 | then walk `RelPlans` IN ORDER. For each `relplan(Name/Arity, _Kind, Columns, _Key, ColumnTypes)`: emit the rel row FIRST, then that rel's column rows, then move to the next rel | next free integer | 0 | 0 | `Name` | `rel` | 0 |
| 3 | each column of that rel, in `Columns` order | next free integer | the rel row's `rel_id` | 1-based index into `Columns` | the column name | `column` | see below |

`type_id` on a column row is the primitive id of that column's boundary type: `text`->1, `int`->2, `float`->3, `bool`->4, `json`->5, and **0 for anything else including `ref(_)`**. Use `boundary_column_type/2` from `emit_ts.pl:718` to get the boundary type from the `ColumnTypes` entry at the same position; if `lower.pl` cannot call it, write a private four-clause mapping in `lower.pl` rather than adding an import.

Text values go in single quotes. A single quote inside a name doubles, per SQL. Rel and column names in this corpus are identifiers, so a doubling helper is belt only.

Statement shape:

```
INSERT OR IGNORE INTO "__catalog_rel" ("rel_id", "parent_id", "ordinal", "local_name", "kind", "type_id") VALUES (1,0,0,'text','primitive',0),(2,0,0,'int','primitive',0),...
```

### 4. The gate at the call site

`lower_program/2` currently reads:

```prolog
    catalog_table_ddl(CatalogTableDdl),
    catalog_row_ddl(Decls, RelPlans, CatalogRowDdl),
```

Change it to gate on use, mirroring the two lines above it that gate `TickDdl`:

```prolog
    program_uses_catalog(prog(Decls, Rules), UsesCatalog),
    ( UsesCatalog == true
    -> catalog_table_ddl(CatalogTableDdl), catalog_row_ddl(Decls, RelPlans, CatalogRowDdl)
    ;  CatalogTableDdl = [], CatalogRowDdl = [] ),
```

Delete the `TODO(g1)` comment line that sits directly above those two calls. Leave the `TODO(g2)` and `TODO(g3)` lines near the stubs alone; they describe work outside this lane.

### 5. Tests in `v6/prolog/compile/test/plunit_tests.pl`

Add a new test block with at least these four tests. Match the file's existing `:- begin_tests(...)` style; read a neighbouring block first.

| test | assertion |
|---|---|
| `catalog_absent_by_default` | a program naming no catalog rel produces a `Ddl` list containing no `__catalog_rel` text |
| `catalog_table_shape` | when a rule names `'__catalog_rel'`, the `Ddl` list contains the two statements from section 2, byte for byte |
| `catalog_rows_are_one_statement` | the seed is exactly ONE atom, and it starts with `INSERT OR IGNORE INTO "__catalog_rel"` |
| `catalog_ids_are_positional` | for a program with two rels of two columns each, the emitted VALUES text contains the primitives at ids 1..5, the first rel at 6 with its columns at 7 and 8 with ordinals 1 and 2, the second rel at 9 with columns 10 and 11 |

## Validation. Run all five. Paste real output into REPORT.md.

```bash
cd /Users/chrishafley/projects/sprefa-lanes/catseed/v6
just conformance          # expect: 302 lines starting PASS, zero starting FAIL
just prolog-lint          # expect: PROLOG_LINT findings=1 baseline=1 OK
just plunit               # expect: all tests pass, count >= the count before your change

cd /Users/chrishafley/projects/sprefa-lanes/catseed/v6/tsv2
bash scripts/sweep.sh     # expect: RUN total=420 identical=418 wrong=0 emitted_crash=0

cd /Users/chrishafley/projects/sprefa-lanes/catseed
git status --short v6/tsv2/gen_emitted/ | grep -c '^ M'   # expect: 0
```

That last one is the gate that matters most. **Zero modified emitted modules.** 212 modules are tracked and none of them names a catalog rel, so the gate must keep every one of them byte-identical. A non-zero count means your gate is wrong; fix the gate, never the modules.

## Style laws. Violations are defects.

- Comments: **at most 2 consecutive comment lines**. A commit hook enforces this and will block you. Use one wide line rather than three narrow ones. Comments state only constraints the code cannot show; no change-log narrative, no dates.
- No em dashes anywhere.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, support. Say source, base, critical, mode, refCount.
- Variable names are descriptive. Never single letters. `RelPlan`, not `R`.
- No negative parallelism: never "not X, Y" or "X. Not Y." State the positive claim.
- Follow the surrounding style of each file you edit even where it differs from the above.

## If reality deviates from this brief

STOP. Write `REPORT.md` naming the exact line that contradicts the brief and what you found instead. Do not improvise a different design. A wrong brief is the coordinator's defect to fix, and a lane that guesses costs more than a lane that stops. This has happened before and stopping was the right call both times.

## Deliverable

`REPORT.md` at the worktree root containing: the five validation outputs verbatim, a list of every file you changed with a one-line reason each, and any deviation you had to make. Leave all work uncommitted.
