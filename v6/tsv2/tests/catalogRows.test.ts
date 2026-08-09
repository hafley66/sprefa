/**
 * catalogRows.test.ts: receipts for the step-g1 catalog seed over
 * __rel, the program's own rel-declaration table.
 *
 * A compiled dl6 program is a module whose `ddl` field is a readonly
 * string[] of SQL statements, run whole into a fresh SQLite db by
 * ScratchStore.boot. serve/3_engine.ts:228 RE-RUNS a program's entire DDL
 * on every program swap while swallowing "already exists" (isAlreadyExists,
 * serve/3_engine.ts:224). Three things can go wrong that a fixture replay
 * would still call IDENTICAL, because no fixture names a catalog rel yet:
 *
 *   1. The seed row set multiplying. A plain INSERT (no OR IGNORE) crashes
 *      the replay on its UNIQUE parent key, or, if the key were relaxed,
 *      doubles the rows. Either way the catalog silently diverges under a
 *      running server, and the corpus never restarts a program mid-run to
 *      expose it.
 *   2. The column-to-rel parent link being misplaced, so a column is not
 *      addressable as a child row of its rel at its 1-based ordinal.
 *   3. The parent lookup costing a scan. parent_id = X is the formerly
 *      quadratic path; the plan, not the rows, is the law for it.
 *
 * The seed SQL is hard-coded here, not read from a compiled module, because
 * the producer lane may not have landed when this runs and no emitted
 * fixture names a catalog rel yet. Row semantics: kind is one of
 * `primitive`, `rel`, `column`; a column is a CHILD row of its rel, so
 * parent_id is the rel's rel_id and ordinal is the 1-based argument
 * position; a rel row has parent_id and ordinal both 0; a column's type_id
 * points at its type's row -- a primitive row, a list row, or a ref's rel
 * row (lower.pl catalog_column_type_id/4).
 *
 * SABOTAGE RECEIPTS:
 *   1. Change the seed's `INSERT OR IGNORE` to a plain `INSERT` and the
 *      replay test fails: the replay loop's tolerated-error assertion
 *      rejects the UNIQUE constraint on the duplicate rel_id. Verified RED
 *      against the seed SQL in a scratch SQLite db (see REPORT.md).
 *   2. Delete the `CREATE INDEX` statement and the plan test fails: the
 *      EXPLAIN detail reports a SCAN of __rel, never a SEARCH of
 *      __rel_parent. Verified RED against the seed SQL in a scratch
 *      SQLite db (see REPORT.md).
 *
 * NOTE ON DEVIATION: these two sabotages could not be run inside the node
 * test runner -- node_modules is absent from v6/tsv2 (and the whole tree),
 * so `pnpm test` cannot load this module at all. They were executed against
 * the identical seed SQL in a standalone SQLite db; the in-framework
 * assertion messages are therefore not captured. Full detail in REPORT.md.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import type { ISqlSeam } from "../runtime/types.ts";

const CATALOG_DDL: readonly string[] = [
  `CREATE TABLE "__rel" ("rel_id" INTEGER NOT NULL, "parent_id" INTEGER NOT NULL, "ordinal" INTEGER NOT NULL, "local_name" TEXT NOT NULL, "kind" TEXT NOT NULL, "type_id" INTEGER NOT NULL, "arity" INTEGER NOT NULL, "module_id" INTEGER NOT NULL, "h_id" TEXT NOT NULL, "h_schema" TEXT NOT NULL, "h_rule" TEXT NOT NULL, PRIMARY KEY ("rel_id")) WITHOUT ROWID`,
  `CREATE INDEX IF NOT EXISTS "__rel_parent" ON "__rel" ("parent_id", "local_name")`,
  `INSERT OR IGNORE INTO "__rel" ("rel_id", "parent_id", "ordinal", "local_name", "kind", "type_id", "arity", "module_id", "h_id", "h_schema", "h_rule") VALUES (1,0,0,'text','primitive',0,0,0,'','',''),(2,0,0,'int','primitive',0,0,0,'','',''),(3,0,0,'float','primitive',0,0,0,'','',''),(4,0,0,'bool','primitive',0,0,0,'','',''),(5,0,0,'json','primitive',0,0,0,'','',''),(6,0,0,'catalog','module',0,0,6,'652f55016243bf1b','',''),(7,6,0,'flow_edge','rel',0,2,6,'e088c4e83f6bd590','76bdfb91a44e84a1',''),(8,7,1,'from_path','column',1,0,6,'8e573bdafd8b831d','',''),(9,7,2,'to_path','column',1,0,6,'6ab063888c4aeed5','',''),(10,6,0,'flow_reach','rel',0,2,6,'fbdcdb48481fdfb8','76bdfb91a44e84a1',''),(11,10,1,'from_path','column',1,0,6,'8e573bdafd8b831d','',''),(12,10,2,'to_path','column',1,0,6,'6ab063888c4aeed5','','')`,
];

function run(seam: ISqlSeam, sql: string) {
  return firstValueFrom(seam.runner.execute(seam.db, sql));
}

/** What serve/3_engine.ts:228 does on a program swap: replay the DDL, swallow
 *  "already exists" per statement. */
async function replay_ddl(seam: ISqlSeam) {
  for (const sql of CATALOG_DDL) {
    try {
      await run(seam, sql);
    } catch (failure) {
      assert.ok(/already exists/i.test(String(failure)), `unexpected DDL failure: ${String(failure)}`);
    }
  }
}

test("catalog rows land in the program database", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, CATALOG_DDL));

  const count = await run(seam, `SELECT count(*) AS c FROM "__rel"`);
  assert.equal(Number(count.rows[0]!.c), 12, "the seed must insert every declared row");

  const primitives = await run(
    seam,
    `SELECT rel_id AS id, local_name AS name FROM "__rel" WHERE kind = 'primitive' ORDER BY rel_id`,
  );
  assert.deepEqual(
    primitives.rows.map((row) => row.name),
    ["text", "int", "float", "bool", "json"],
    "the five primitives must land in declaration order",
  );
  assert.deepEqual(
    primitives.rows.map((row) => Number(row.id)),
    [1, 2, 3, 4, 5],
    "the primitives must carry rel ids 1..5 in that order",
  );
});

test("a column is a child row of its rel", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, CATALOG_DDL));

  const columns = await run(
    seam,
    `SELECT local_name AS name, ordinal AS ordinal FROM "__rel" WHERE parent_id = 7 ORDER BY ordinal`,
  );
  assert.deepEqual(columns.rows.map((row) => row.name), ["from_path", "to_path"], "the rel's columns must be its child rows");
  assert.deepEqual(columns.rows.map((row) => Number(row.ordinal)), [1, 2], "each column must carry its 1-based argument position");
});

test("replaying the DDL mints no duplicate rows", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, CATALOG_DDL));
  await replay_ddl(seam);

  const count = await run(seam, `SELECT count(*) AS c FROM "__rel"`);
  assert.equal(Number(count.rows[0]!.c), 12, "re-running the DDL must not double the catalog rows");

  const duplicates = await run(
    seam,
    `SELECT rel_id AS id FROM "__rel" GROUP BY rel_id HAVING count(*) > 1`,
  );
  assert.equal(duplicates.rows.length, 0, "no catalog rel_id may appear twice after replay");
});

test("exactly one module row with a non-empty h_id", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, CATALOG_DDL));

  const modules = await run(
    seam,
    `SELECT rel_id AS id, local_name AS name, h_id AS h FROM "__rel" WHERE kind = 'module'`,
  );
  assert.equal(modules.rows.length, 1, "there must be exactly one module row");
  const the_module = modules.rows[0]!;
  assert.equal(Number(the_module.id), 6, "the module row owns rel_id 6");
  assert.equal(the_module.name, "catalog", "the module row carries the program name");
  assert.equal(typeof the_module.h, "string", "the module h_id must be text");
  assert.equal(String(the_module.h).length, 16, "the module h_id must be 16 hex characters");
});

test("the parent index is used, never a scan", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, CATALOG_DDL));

  const plan = await run(
    seam,
    `EXPLAIN QUERY PLAN SELECT local_name FROM "__rel" WHERE parent_id = 6`,
  );
  const details = plan.rows.map((row) => String(row.detail)).join("\n");
  assert.ok(details.includes("SEARCH"), `the parent lookup must SEARCH, got: ${details}`);
  assert.ok(details.includes("__rel_parent"), `the parent lookup must use __rel_parent, got: ${details}`);
  assert.ok(!details.includes("SCAN"), `the parent lookup must not scan, got: ${details}`);
});

test("rel_id is the primary key: an id equality lookup SEARCHes, never scans", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, CATALOG_DDL));

  const plan = await run(
    seam,
    `EXPLAIN QUERY PLAN SELECT local_name FROM "__rel" WHERE rel_id = 7`,
  );
  const details = plan.rows.map((row) => String(row.detail)).join("\n");
  assert.ok(details.includes("SEARCH"), `the rel_id lookup must SEARCH, got: ${details}`);
  assert.ok(!details.includes("SCAN"), `the rel_id lookup must not scan, got: ${details}`);
});

test("every rel row carries a non-empty h_schema", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, CATALOG_DDL));

  const rels = await run(
    seam,
    `SELECT local_name AS name, h_schema AS schema FROM "__rel" WHERE kind = 'rel'`,
  );
  assert.ok(rels.rows.length >= 1, "there must be at least one rel row");
  for (const row of rels.rows) {
    assert.equal(typeof row.schema, "string", `rel ${row.name} h_schema must be text`);
    assert.equal(String(row.schema).length, 16, `rel ${row.name} must carry a 16-hex h_schema`);
  }
});
