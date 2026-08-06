# LANE cattest - REPORT.md

## HEAD check

`git rev-parse HEAD` printed `e3997cecd88322ae029255c5e3cc8402e433d122`, the
pinned receipt. Proceeded.

## Deliverable

`v6/tsv2/tests/catalogRows.test.ts` created: four tests (rows land, column-as-
child-row, replay mints no duplicates, index used not scanned). Uncommitted.

## DEVIATION: `node_modules` is absent

The brief asserts node_modules is already present and forbids `npm install` /
`pnpm install` / `npm ci`. It is not present anywhere in the tree:

- `v6/tsv2/node_modules`: does not exist
- no `node_modules` directory exists under `cattest/` at all

`pnpm test` therefore cannot load the suite: every module import fails with
`ERR_MODULE_NOT_FOUND`. The confirmation run against the new test file alone:

```
✖ tests/catalogRows.test.ts (58.274041ms)
ℹ tests 1
ℹ suites 0
ℹ pass 0
ℹ fail 1
ℹ cancelled 0
ℹ skipped 0
ℹ todo 0
ℹ duration_ms 62.162542
✖ failing tests:
test at tests/catalogRows.test.ts:1:1
✖ tests/catalogRows.test.ts (58.274041ms)
  'test failed'
```
Root cause (node module resolution):
```
code: 'ERR_MODULE_NOT_FOUND'
Node.js v24.15.0
```

Per the brief's deviation clause ("If reality deviates from this brief: STOP...
Report.md naming the exact contradiction"), I did NOT run any package-manager
install. That means two required items cannot be produced:

1. the `pnpm test` tail (expect `pass 149 / fail 0 / skip 1`)
2. the two sabotage receipts run inside the node test runner

## What was validated instead

The seed SQL is fixed by the brief; I validated it and both sabotages against a
standalone scrap sqlite db (`sqlite3` CLI at /usr/bin/sqlite3), which touches
nothing in the project.

Normal seed (all green):

```
count|11
prim|text,int,float,bool,json      (ids 1..5)
cols6|from_path:1,to_path:2
replay_count|11|distinct|11        (OR IGNORE replay: no duplicates)
EXPLAIN: SEARCH __catalog_rel USING COVERING INDEX __catalog_rel_parent (parent_id=?)
```

Sabotage 1 (seed `INSERT OR IGNORE` -> plain `INSERT`, slug the replay leg):

```
Error: stepping, UNIQUE constraint failed: __catalog_rel.rel_id (19)
```

In the in-framework replay loop the tolerated-error assertion
`assert.ok(/already exists/i.test(...))` would reject this (the text "UNIQUE
constraint failed" does not match "already exists"), failing with the message
"unexpected DDL failure: <...>", exactly as designed. The in-framework wording
is not captured because the runner cannot load.

Sabotage 2 (delete the `CREATE INDEX` statement):

```
QUERY PLAN
`--SCAN __catalog_rel
```

This fails the fourth test's `assert.ok(!details.includes("SCAN"))`, message
"the parent lookup must not scan, got: ...".

## Next action

Run `pnpm install --frozen-lockfile` in `v6/tsv2` (restores node_modules
exactly per the committed `pnpm-lock.yaml`, no lockfile rewrite), then
`pnpm test 2>&1 | tail -20` to confirm `pass 149 / fail 0 / skip 1`, and run
the two sabotages for the in-framework assertion messages to quote in the test
file's header (currently marked "not captured").

## Files touched

- `v6/tsv2/tests/catalogRows.test.ts` (created)
- `REPORT.md` (created)

No commit made.
