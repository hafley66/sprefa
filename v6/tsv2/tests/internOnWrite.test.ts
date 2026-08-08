/**
 * internOnWrite.test.ts — the ORDER rail for intern-on-write (interning
 * contract §5.7.1). The row statement resolves a built string to its id, so a
 * dictionary row that does not exist YET loses the row silently: the sequence
 * is the whole property, and a byte diff of the emitted SQL cannot see it.
 *
 * SABOTAGE RECEIPTS (each edit made, this file run, then reverted; the quoted
 * text is what the run printed):
 *   a. `intern_then_execute` reordered to run the row statement first. 2 of 5
 *      RED: "the intern statement must run before the row statement".
 *   b. `intern_then_execute`'s `args` reuse replaced by `[]`. 1 of 5 RED:
 *      "the intern statement must carry the row statement's bind args".
 *   c. the `intern_sql === undefined` early return deleted. 1 of 5 RED:
 *      "an absent intern list must run exactly 1 statement, got 0".
 *
 * NAMED BLIND SPOT: which statements the EMITTER puts in `intern_sql` is
 * pinned in plunit (`delta_arm_interns_before_the_row_insert` and its
 * siblings); this file drives the runtime helper directly.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";
import type { ISqlRunner } from "sprefa-store-engine/src/engine/types.ts";

import { intern_then_execute } from "../runtime/1_incremental.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { ISqlSeam, SqlStatement } from "../runtime/types.ts";

const DICTIONARY_DDL = `CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`;
const HITS_DDL = `CREATE TABLE "hits" ("path" TEXT NOT NULL, "n" INTEGER NOT NULL)`;
const DIAG_DDL = `CREATE TABLE "diag" ("path" TEXT NOT NULL, "message" INTEGER NOT NULL)`;
const SEED_HITS = `INSERT INTO "hits" ("path", "n") VALUES ('a.rs', 3), ('b.rs', 7), ('c.rs', 3)`;

const INTERN_SQL = `INSERT OR IGNORE INTO "__str" ("content") SELECT DISTINCT (b0."n" || ' hits') FROM "hits" b0 WHERE b0."n" > ?`;
const INSERT_SQL = `INSERT OR IGNORE INTO "diag" ("path", "message") SELECT b0."path", (SELECT s."__id" FROM "__str" s WHERE s."content" = (b0."n" || ' hits')) FROM "hits" b0 WHERE b0."n" > ?`;

interface IRecordedRun {
  readonly seam: ISqlSeam;
  readonly statements: string[];
  readonly args: unknown[];
}

function recordingSeam(seam: ISqlSeam): IRecordedRun {
  const statements: string[] = [];
  const args: unknown[] = [];
  const record = (statement: string | SqlStatement): void => {
    statements.push(typeof statement === "string" ? statement : statement.sql);
    args.push(typeof statement === "string" ? [] : (statement.args ?? []));
  };
  const runner: ISqlRunner = {
    ...seam.runner,
    execute(db, statement) {
      record(statement);
      return seam.runner.execute(db, statement);
    },
    batch(db, batched) {
      for (const statement of batched) record(statement);
      return seam.runner.batch(db, batched);
    },
  };
  return { seam: { db: seam.db, runner }, statements, args };
}

async function booted(): Promise<ISqlSeam> {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(
    ScratchStore.boot(seam, [DICTIONARY_DDL, HITS_DDL, DIAG_DDL, SEED_HITS]),
  );
  return seam;
}

test("order: the intern statement runs before the row statement", async () => {
  const { seam, statements } = recordingSeam(await booted());
  await firstValueFrom(
    intern_then_execute(seam, [INTERN_SQL], { sql: INSERT_SQL, args: [2] }),
  );
  assert.deepEqual(
    statements,
    [INTERN_SQL, INSERT_SQL],
    "the intern statement must run before the row statement",
  );
});

test("order: the intern statement carries the row statement's bind args", async () => {
  const { seam, args } = recordingSeam(await booted());
  await firstValueFrom(
    intern_then_execute(seam, [INTERN_SQL], { sql: INSERT_SQL, args: [2] }),
  );
  assert.deepEqual(
    args,
    [[2], [2]],
    "the intern statement must carry the row statement's bind args",
  );
});

test("rows: every built string reaches the head as a resolvable id", async () => {
  const seam = await booted();
  await firstValueFrom(
    intern_then_execute(seam, [INTERN_SQL], { sql: INSERT_SQL, args: [2] }),
  );
  const decoded = await firstValueFrom(
    seam.runner.execute(seam.db, {
      sql: `SELECT d."path", (SELECT s."content" FROM "__str" s WHERE s."__id" = d."message") AS "message" FROM "diag" d ORDER BY d."path"`,
      args: [],
    }),
  );
  assert.deepEqual(
    decoded.rows.map((row) => [row["path"], row["message"]]),
    [["a.rs", "3 hits"], ["b.rs", "7 hits"], ["c.rs", "3 hits"]],
    "no row may be lost to an unseeded dictionary lookup",
  );
  const dictionary = await firstValueFrom(
    seam.runner.scalar(seam.db, { sql: `SELECT count(*) FROM "__str"`, args: [] }),
  );
  assert.equal(dictionary, 2, "the duplicate built string must intern once");
});

test("cost: an absent intern list runs exactly the row statement", async () => {
  const { seam, statements } = recordingSeam(await booted());
  await firstValueFrom(
    intern_then_execute(seam, undefined, { sql: INSERT_SQL, args: [2] }),
  );
  assert.deepEqual(
    statements,
    [INSERT_SQL],
    `an absent intern list must run exactly 1 statement, got ${statements.length}`,
  );
});

test("cost: an empty intern list runs exactly the row statement", async () => {
  const { seam, statements } = recordingSeam(await booted());
  await firstValueFrom(
    intern_then_execute(seam, [], { sql: INSERT_SQL, args: [2] }),
  );
  assert.deepEqual(
    statements,
    [INSERT_SQL],
    `an empty intern list must run exactly 1 statement, got ${statements.length}`,
  );
});
