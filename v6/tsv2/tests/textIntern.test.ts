/**
 * textIntern.test.ts — the COUNT rail for the ingest door's text intern
 * (interning contract §6.4). Tick-log grading cannot see statement COUNT, and
 * a per-row door would be byte-identical and quadratic.
 *
 * SABOTAGE RECEIPTS (each edit made, this file run, then reverted; the quoted
 * text is what the run printed):
 *   a. textPlane.ts `intern` rewritten to run one INSERT + one SELECT per
 *      arriving value (the N+1 shape this file exists for). 4 of 8 RED:
 *      "50 distinct values across 1 rel must intern in 2 statements, got 100".
 *   b. textPlane.ts's `values.length === 0` early return deleted. 2 of 8 RED:
 *      "a batch with no interned column must run 0 statements, got 2".
 *   c. TextPlane.intern made to return `arrivals` unchanged. 2 of 8 RED:
 *      "an interned position must hand a number downstream, got string 'a.rs'".
 *
 * NAMED BLIND SPOT: the ORDER of the door against StructPlane is asserted in
 * the emitted module's text (plunit `text_intern_runs_before_struct_intern`),
 * not here; this file drives TextPlane directly and never sees the wiring.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";
import type { ISqlRunner } from "sprefa-store-engine/src/engine/types.ts";

import { ScratchStore } from "../runtime/scratchStore.ts";
import { TextPlane } from "../runtime/textPlane.ts";
import type {
  IArrivalBatch,
  IArrivalRow,
  ISqlSeam,
  ITextInternPlan,
  SqlStatement,
} from "../runtime/types.ts";

const DICTIONARY_DDL = `CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`;

const PLAN: ITextInternPlan = {
  internSql: `INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each(?) i`,
  lookupSql:
    `SELECT s."content" AS "__lookup", s."__id" AS "__id" FROM json_each(?) i JOIN "__str" s ON s."content" = i.value`,
  relColumns: {
    edge_one: [true, false],
    edge_two: [true, true],
    edge_three: [false, true],
    edge_four: [true],
  },
};

function countingSeam(seam: ISqlSeam): { seam: ISqlSeam; statements: string[] } {
  const statements: string[] = [];
  const record = (statement: string | SqlStatement): void => {
    statements.push(typeof statement === "string" ? statement : statement.sql);
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
    executeMultiple(db, sql) {
      for (const part of sql.split(";\n")) record(part);
      return seam.runner.executeMultiple(db, sql);
    },
  };
  return { seam: { db: seam.db, runner }, statements };
}

async function dictionarySeam(): Promise<ISqlSeam> {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, [DICTIONARY_DDL]));
  return seam;
}

/** `distinct` values spread over `rels` relations, one row each. */
function batchOf(distinct: number, rels: readonly string[]): IArrivalBatch {
  return Array.from({ length: distinct }, (_unused, index): IArrivalRow => ({
    rel: rels[index % rels.length]!,
    sign: "add",
    row: rels[index % rels.length] === "edge_two"
      ? [`left_${index}`, `right_${index}`]
      : [`value_${index}`, 0],
  }));
}

// ── COST: two statements, whatever N and M are ───────────────────────────────

for (const distinct of [1, 3, 50]) {
  for (const rels of [["edge_one"], ["edge_one", "edge_two", "edge_three", "edge_four"]]) {
    test(`count: ${distinct} distinct values across ${rels.length} rel(s) intern in 2 statements`, async () => {
      const base = await dictionarySeam();
      const { seam, statements } = countingSeam(base);
      await firstValueFrom(TextPlane.intern(seam, PLAN, batchOf(distinct, rels)));
      assert.equal(
        statements.length,
        2,
        `${distinct} distinct values across ${rels.length} rel(s) must intern in 2 statements, got ${statements.length}`,
      );
    });
  }
}

test("count: an empty batch runs 0 statements", async () => {
  const base = await dictionarySeam();
  const { seam, statements } = countingSeam(base);
  await firstValueFrom(TextPlane.intern(seam, PLAN, []));
  assert.equal(statements.length, 0, `an empty batch must run 0 statements, got ${statements.length}`);
});

test("count: a batch with no interned column runs 0 statements", async () => {
  const base = await dictionarySeam();
  const { seam, statements } = countingSeam(base);
  const arrivals: IArrivalBatch = [{ rel: "not_interned", sign: "add", row: ["a.rs", 1] }];
  await firstValueFrom(TextPlane.intern(seam, PLAN, arrivals));
  assert.equal(
    statements.length,
    0,
    `a batch with no interned column must run 0 statements, got ${statements.length}`,
  );
});

// ── the batch handed downstream carries ids, never strings ───────────────────

test("no string survives in an interned position", async () => {
  const seam = await dictionarySeam();
  const arrivals: IArrivalBatch = [
    { rel: "edge_one", sign: "add", row: ["a.rs", 7] },
    { rel: "edge_two", sign: "add", row: ["a.rs", "b.rs"] },
    { rel: "not_interned", sign: "add", row: ["untouched", 1] },
  ];
  const interned = await firstValueFrom(TextPlane.intern(seam, PLAN, arrivals));
  for (const arrival of interned) {
    const flags = PLAN.relColumns[arrival.rel];
    if (flags === undefined) continue;
    arrival.row.forEach((value, index) => {
      if (flags[index] !== true) return;
      assert.equal(
        typeof value,
        "number",
        `an interned position must hand a number downstream, got ${typeof value} ${JSON.stringify(value)}`,
      );
    });
  }
  assert.deepEqual(interned[2]!.row, ["untouched", 1]);
});

test("one value shared by two rels takes one id", async () => {
  const seam = await dictionarySeam();
  const arrivals: IArrivalBatch = [
    { rel: "edge_one", sign: "add", row: ["shared.rs", 1] },
    { rel: "edge_four", sign: "add", row: ["shared.rs"] },
  ];
  const interned = await firstValueFrom(TextPlane.intern(seam, PLAN, arrivals));
  assert.equal(interned[0]!.row[0], interned[1]!.row[0]);
});

test("a null in an interned position is a named unsupported construct, never a dictionary row", async () => {
  const seam = await dictionarySeam();
  const arrivals = [
    { rel: "edge_one", sign: "add", row: [null, 1] },
  ] as unknown as IArrivalBatch;
  await assert.rejects(
    () => firstValueFrom(TextPlane.intern(seam, PLAN, arrivals)),
    /text_intern_null\(edge_one, 0\)/,
  );
});
