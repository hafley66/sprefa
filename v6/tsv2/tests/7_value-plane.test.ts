import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom, map } from "rxjs";

import { row_value_from_sql, select_rows } from "../runtime/rows.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";

const DDL = [
  `CREATE TABLE "value_plane" (
    "name" TEXT NOT NULL,
    "enabled" INTEGER NOT NULL CHECK ("enabled" IN (0,1)),
    "score" REAL NOT NULL CHECK (
      typeof("score") = 'real'
      AND "score" BETWEEN -1.7976931348623157e+308 AND 1.7976931348623157e+308
    )
  )`,
  'CREATE INDEX "value_plane_enabled_idx" ON "value_plane" ("enabled")',
];

test("bool and float storage rejects invalid values and decodes canonical values", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, DDL));
  await firstValueFrom(
    seam.runner.execute(seam.db, {
      sql: 'INSERT INTO "value_plane" ("name", "enabled", "score") VALUES (?, ?, ?), (?, ?, ?), (?, ?, ?)',
      args: ["decimal", 1n, 0.3, "integral", 1n, 1n, "sum", 0n, 0.30000000000000004],
    }),
  );

  await assert.rejects(
    firstValueFrom(
      seam.runner.execute(seam.db, {
        sql: 'INSERT INTO "value_plane" ("name", "enabled", "score") VALUES (?, ?, ?)',
        args: ["bad-bool", 2n, 1.0],
      }),
    ),
    /CHECK constraint failed/,
  );
  await assert.rejects(
    firstValueFrom(
      seam.runner.execute(
        seam.db,
        'INSERT INTO "value_plane" ("name", "enabled", "score") VALUES (\'infinite\', 1, 1e999)',
      ),
    ),
    /CHECK constraint failed/,
  );

  const rows = await firstValueFrom(
    select_rows(
      seam,
      'SELECT "name", "enabled", "score" FROM "value_plane" ORDER BY "name"',
      ["name", "enabled", "score"],
      ["text", "bool", "float"],
    ),
  );
  assert.deepEqual(rows, [
    ["decimal", true, 0.3],
    ["integral", true, 1],
    ["sum", false, 0.30000000000000004],
  ]);

  const exact = await firstValueFrom(
    seam.runner
      .execute(
        seam.db,
        'SELECT "name" FROM "value_plane" WHERE "score" = 0.30000000000000004 ORDER BY "name"',
      )
      .pipe(map((result) => result.rows.map((row) => row.name))),
  );
  assert.deepEqual(exact, ["sum"]);
});

test("bool filters use an indexed SQLite search", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, DDL));
  const details = await firstValueFrom(
    seam.runner
      .execute(
        seam.db,
        'EXPLAIN QUERY PLAN SELECT "name" FROM "value_plane" WHERE "enabled" = 1',
      )
      .pipe(map((result) => result.rows.map((row) => String(row.detail)))),
  );
  assert.match(details.join("\n"), /SEARCH value_plane USING INDEX value_plane_enabled_idx/);
});

test("tick boundary emits booleans and shortest finite float JSON", () => {
  assert.deepEqual(
    // Not point-free: `valueText` takes an optional COLUMN TYPE as its second
    // parameter and `Array.map` would hand it the index.
    [true, false, -0, 0.30000000000000004].map((value) => TickLogEmitter.value_text(value)),
    ["true", "false", "0", "0.30000000000000004"],
  );
  assert.throws(() => TickLogEmitter.value_text(Number.NaN), /non-finite float/);
  assert.throws(() => TickLogEmitter.value_text(Number.POSITIVE_INFINITY), /non-finite float/);
  assert.throws(() => row_value_from_sql("float", Number.NaN), /float column crossed SQLite/);
  assert.equal(row_value_from_sql("float", -0), 0);
  assert.equal(Object.is(row_value_from_sql("float", -0), -0), false);
});
