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
 * points at a primitive's rel_id, and is 0 when the type is not one of the
 * five primitives.
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
  `CREATE TABLE "__rel" ("rel_id" INTEGER NOT NULL, "parent_id" INTEGER NOT NULL, "ordinal" INTEGER NOT NULL, "local_name" TEXT NOT NULL, "kind" TEXT NOT NULL, "type_id" INTEGER NOT NULL, PRIMARY KEY ("rel_id")) WITHOUT ROWID`,
  `CREATE INDEX IF NOT EXISTS "__rel_parent" ON "__rel" ("parent_id", "local_name")`,
  `INSERT OR IGNORE INTO "__rel" ("rel_id", "parent_id", "ordinal", "local_name", "kind", "type_id") VALUES (1,0,0,'text','primitive',0),(2,0,0,'int','primitive',0),(3,0,0,'float','primitive',0),(4,0,0,'bool','primitive',0),(5,0,0,'json','primitive',0),(6,0,0,'flow_edge','rel',0),(7,6,1,'from_path','column',1),(8,6,2,'to_path','column',1),(9,0,0,'flow_reach','rel',0),(10,9,1,'from_path','column',1),(11,9,2,'to_path','column',1)`,
];

function run(seam: ISqlSeam, sql: string) {
  return firstValueFrom(seam.runner.execute(seam.db, sql));
}

/** What serve/3_engine.ts:228 does on a program swap: replay the DDL, swallow
 *  "already exists" per statement. */
async function replayDdl(seam: ISqlSeam) {
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
  assert.equal(Number(count.rows[0]!.c), 11, "the seed must insert every declared row");

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
    `SELECT local_name AS name, ordinal AS ordinal FROM "__rel" WHERE parent_id = 6 ORDER BY ordinal`,
  );
  assert.deepEqual(columns.rows.map((row) => row.name), ["from_path", "to_path"], "the rel's columns must be its child rows");
  assert.deepEqual(columns.rows.map((row) => Number(row.ordinal)), [1, 2], "each column must carry its 1-based argument position");
});

test("replaying the DDL mints no duplicate rows", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, CATALOG_DDL));
  await replayDdl(seam);

  const count = await run(seam, `SELECT count(*) AS c FROM "__rel"`);
  assert.equal(Number(count.rows[0]!.c), 11, "re-running the DDL must not double the catalog rows");

  const duplicates = await run(
    seam,
    `SELECT rel_id AS id FROM "__rel" GROUP BY rel_id HAVING count(*) > 1`,
  );
  assert.equal(duplicates.rows.length, 0, "no catalog rel_id may appear twice after replay");
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
