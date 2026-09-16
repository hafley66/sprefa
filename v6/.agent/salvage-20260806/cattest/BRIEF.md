# LANE cattest: the catalog receipt test

## First action, non-negotiable

```bash
cd /Users/chrishafley/projects/sprefa-lanes/cattest
git rev-parse HEAD    # MUST print e3997cecd88322ae029255c5e3cc8402e433d122
```

If it prints anything else, STOP and write REPORT.md saying so.

## Files you own. Touch nothing else.

| file | what you do there |
|---|---|
| `v6/tsv2/tests/catalogRows.test.ts` | create it, this is your whole deliverable |

Two sibling lanes are editing `v6/prolog/{analyze.pl,lower.pl}` and `v6/prolog/ARCH.pl` right now. Editing either is a defect.

## Do NOT run these

- `npm install`, `pnpm install`, `npm ci`. `node_modules` is already present. Installing rewrites the lockfile and breaks the tree.
- `git commit`, `git push`, `git merge`, `git rebase`. Leave your work uncommitted.

## Context you need in one paragraph

A compiled dl6 program is a TypeScript module whose `ddl` field is a `readonly string[]` of SQL statements, run whole into a fresh SQLite database by `ScratchStore.boot`. A step-g1 feature adds two DDL statements plus one seed INSERT describing the program's own rel declarations. Your test proves that seed behaves under the one condition the fixture corpus can never catch: `serve/3_engine.ts:225` REPLAYS a program's entire DDL array on every program swap while swallowing "already exists", so a seed that is not idempotent silently doubles its rows under a running server.

## The exact SQL your test asserts against

The producer lane is emitting exactly this. Hard-code it in your test as a `const`; do not read it from a compiled module, because no fixture names a catalog rel yet and the producer may not have landed when your test runs.

```sql
CREATE TABLE "__catalog_rel" ("rel_id" INTEGER NOT NULL, "parent_id" INTEGER NOT NULL, "ordinal" INTEGER NOT NULL, "local_name" TEXT NOT NULL, "kind" TEXT NOT NULL, "type_id" INTEGER NOT NULL, PRIMARY KEY ("rel_id")) WITHOUT ROWID
CREATE INDEX IF NOT EXISTS "__catalog_rel_parent" ON "__catalog_rel" ("parent_id", "local_name")
INSERT OR IGNORE INTO "__catalog_rel" ("rel_id", "parent_id", "ordinal", "local_name", "kind", "type_id") VALUES (1,0,0,'text','primitive',0),(2,0,0,'int','primitive',0),(3,0,0,'float','primitive',0),(4,0,0,'bool','primitive',0),(5,0,0,'json','primitive',0),(6,0,0,'flow_edge','rel',0),(7,6,1,'from_path','column',1),(8,6,2,'to_path','column',1),(9,0,0,'flow_reach','rel',0),(10,9,1,'from_path','column',1),(11,9,2,'to_path','column',1)
```

Row semantics: `kind` is one of `primitive`, `rel`, `column`. A column is a CHILD ROW of its rel, so `parent_id` is the rel's `rel_id` and `ordinal` is the 1-based argument position. On a rel row `parent_id` and `ordinal` are both 0. `type_id` on a column points at the primitive's `rel_id`, and is 0 when the type is not one of the five primitives.

## The file to copy the shape from

Read `v6/tsv2/tests/tickCounter.test.ts` first, all of it. It is the same class of test against the same failure mode for `__tick`. Copy its structure: the header comment block ending in a `SABOTAGE RECEIPTS` section, `ScratchStore.open(":memory:")`, `firstValueFrom(ScratchStore.boot(seam, ddl))`, a small `run(seam, sql)` helper over `seam.runner.execute`, and the DDL-replay loop that tolerates `already exists`.

## The four tests to write

| name | what it proves |
|---|---|
| catalog rows land in the program database | boot the three statements, then `SELECT count(*)` is 11, and `SELECT` of the five primitive rows returns them with ids 1..5 in that order |
| a column is a child row of its rel | `SELECT local_name, ordinal FROM "__catalog_rel" WHERE parent_id = 6 ORDER BY ordinal` returns `from_path` at 1 and `to_path` at 2 |
| replaying the DDL mints no duplicate rows | run the replay loop the way `serve/3_engine.ts` does, tolerating `already exists`, then assert `count(*)` is still 11 and no `rel_id` appears twice |
| the parent index is used, never a scan | `EXPLAIN QUERY PLAN SELECT local_name FROM "__catalog_rel" WHERE parent_id = 6` returns a row whose `detail` contains `SEARCH` and names `__catalog_rel_parent`, and does not contain `SCAN` |

That last one is the repo's law for a formerly-quadratic path: assert the plan, never end-state equality alone.

## Sabotage receipts, required

Before you finish, actually run these two mutations, watch them go RED, revert them, and quote the real assertion messages in the test file's header comment:

1. Change the seed's `INSERT OR IGNORE` to a plain `INSERT` and confirm the replay test fails.
2. Delete the `CREATE INDEX` statement and confirm the plan test fails.

A header claiming a sabotage you did not run is a defect.

## Validation

```bash
cd /Users/chrishafley/projects/sprefa-lanes/cattest/v6/tsv2
pnpm test 2>&1 | tail -20     # expect: pass 149 / fail 0 / skip 1, four more than the 145 on this base
```

The package manager here is **pnpm**, declared as `pnpm@11.10.0` in `v6/tsv2/package.json`. Run `pnpm test`, never `npm test`.

## Style laws. Violations are defects.

- Comments: **at most 2 consecutive comment lines** inside function bodies. A commit hook enforces this. The file's top-of-file header block is exempt, and `tickCounter.test.ts` shows exactly how long a header may be.
- No em dashes anywhere.
- Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, support. Say source, base, critical, mode, refCount.
- Variable names are descriptive. Never single letters.
- No negative parallelism: never "not X, Y" or "X. Not Y."
- `async`/`await` is allowed in a test file only where `tickCounter.test.ts` already uses it, and nowhere else. Above the SqlRunner seam this repo bans Promises; tests are the named exception because `node:test` drives them.
- Exactly one manual `.subscribe()` per app is a repo ratchet. Your test uses `firstValueFrom`, never `.subscribe()`.

## If reality deviates from this brief

STOP. Write `REPORT.md` naming the exact contradiction. Do not improvise a different schema; the SQL above is fixed by a decision record and a sibling lane is emitting it.

## Deliverable

`REPORT.md` at the worktree root: the `pnpm test` tail verbatim, both sabotage runs with their real RED messages, and any deviation. Leave the work uncommitted.
