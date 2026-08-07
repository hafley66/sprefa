/**
 * bootBind.test.ts — FAIL-FIRST CHECK (b) of the EXPRESSION + AGGREGATE LIFT
 * arc: the `@libsql` number -> REAL bind corruption, on the ONE bind path
 * that was still binding raw.
 *
 * The driver behavior itself is not a bug in any SQL this compiler emits and
 * is measured, not assumed, by `driver_binds_a_js_number_as_real` below: a
 * bound JS `1` lands in a TEXT-affinity column as the text "1.0"; a bound
 * `1n` lands as "1". Every other bind path already routes through an
 * integer -> bigint conversion (emit_ts.pl's `bindArgs` helper for arrivals
 * and edge projections, 1_incremental.ts's own `bindArgs` for the
 * incremental family). The BOOT path did not: both harnesses that seed a
 * compiled program spread `statement.params` straight into `args`, so an
 * integer Initial value destined for a TEXT column was corrupted digit-for-
 * digit before tick 1 ever ran.
 *
 * SABOTAGE RECEIPT (run before the fix, both directions):
 *   - with the boot path binding raw (the state this test was written
 *     against), `boot_runner_preserves_an_integer_into_a_text_column` FAILS
 *     with got "1.0" want "1".
 *   - reverting BootRunner.run's conversion to `[...statement.params]` after
 *     the fix reproduces the same red.
 * The first test stays as the driver-behavior receipt: if a future driver
 * version stops widening numbers to REAL, it goes red and this whole seam
 * can be simplified rather than silently carrying dead conversion code.
 *
 * Reachability, stated honestly: the expression lift is what makes this
 * corruption reachable in ordinary programs. Before it, a column holding
 * integers was almost always INTEGER-typed (PHASE C2 RULING 1 infers int
 * from literal witnesses), and an INTEGER column round-trips a plain number
 * correctly. Expression and aggregate columns get their type from a computed
 * expression rather than a literal witness, so an integer value reaching a
 * TEXT column is no longer an exotic shape.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { concatMap, firstValueFrom, of, toArray } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IBootStatement, ISqlSeam } from "../runtime/types.ts";

const TEXT_COLUMN_DDL = 'CREATE TABLE "probe" ("value" TEXT NOT NULL)';
const INT_COLUMN_DDL = 'CREATE TABLE "probe_int" ("value" INTEGER NOT NULL)';

function read_probe(seam: ISqlSeam, table: string): Promise<readonly { value: unknown; ty: unknown }[]> {
  return firstValueFrom(
    seam.runner
      .execute(seam.db, `SELECT "value", typeof("value") AS ty FROM "${table}" ORDER BY rowid`)
      .pipe(
        concatMap((result) => of(result.rows.map((row) => ({ value: row.value, ty: row.ty })))),
      ),
  );
}

test("driver_binds_a_js_number_as_real (the hazard, measured not assumed)", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, [TEXT_COLUMN_DDL]));
  await firstValueFrom(
    seam.runner.execute(seam.db, { sql: 'INSERT INTO "probe" ("value") VALUES (?)', args: [1] }),
  );
  await firstValueFrom(
    seam.runner.execute(seam.db, { sql: 'INSERT INTO "probe" ("value") VALUES (?)', args: [2n] }),
  );
  const rows = await read_probe(seam, "probe");
  assert.deepEqual(
    rows.map((row) => row.value),
    ["1.0", "2"],
    "a plain JS number bound into a TEXT column widens to REAL and stores as N.0; a bigint does not",
  );
});

test("boot_runner_preserves_an_integer_into_a_text_column", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, [TEXT_COLUMN_DDL]));
  const statements: readonly IBootStatement[] = [
    { rel: "probe", sql: 'INSERT INTO "probe" ("value") VALUES (?)', params: [1] },
    { rel: "probe", sql: 'INSERT INTO "probe" ("value") VALUES (?)', params: [40] },
    { rel: "probe", sql: 'INSERT INTO "probe" ("value") VALUES (?)', params: ["src/db.rs"] },
  ];
  await firstValueFrom(BootRunner.run(seam, statements).pipe(toArray()));
  const rows = await read_probe(seam, "probe");
  assert.deepEqual(
    rows.map((row) => row.value),
    ["1", "40", "src/db.rs"],
    "boot must not corrupt an integer Initial value on its way into a TEXT column",
  );
});

test("boot_runner_leaves_an_integer_column_an_integer", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, [INT_COLUMN_DDL]));
  await firstValueFrom(
    BootRunner.run(seam, [
      { rel: "probe_int", sql: 'INSERT INTO "probe_int" ("value") VALUES (?)', params: [7] },
    ]).pipe(toArray()),
  );
  const rows = await read_probe(seam, "probe_int");
  assert.deepEqual(rows, [{ value: 7, ty: "integer" }]);
});
