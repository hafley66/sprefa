/** The ledger reads the SqlRunner seam's counter, so its per-tick numbers must
 *  come from real statements a TickFold run issued, never from a hand tally. */

import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom, toArray } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { program } from "../gen_emitted/retention_count_prunes_oldest.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import { TickStatementLedger } from "../runtime/tickStatements.ts";

function rows(count: number, prefix: string) {
  return Array.from({ length: count }, (_value, index) => ({
    rel: "event",
    sign: "add" as const,
    row: [`${prefix}_${index}`],
  }));
}

test("the ledger's per-tick statements equal the counter delta across that tick", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  TickStatementLedger.reset();
  const before = stmt_counter.get();
  await firstValueFrom(
    TickFold.run(program, seam, [rows(3, "first"), rows(2, "second")]).pipe(toArray()),
  );
  const spent = stmt_counter.get() - before;
  seam.db.close();

  const entries = TickStatementLedger.entries();
  const total = TickStatementLedger.total();
  assert.deepEqual(
    entries.map((entry) => entry.tick),
    [1, 2],
    "one entry per tick, numbered the way the tick log numbers them",
  );
  assert.equal(total.statements, spent, "the ledger accounts for every statement the fold ran");
  assert.equal(total.tick, 2, "the total's tick field carries the recorded tick count");
});

test("adds and dels are the tick's boundary delta rows", async () => {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  TickStatementLedger.reset();
  const lines = await firstValueFrom(
    TickFold.run(program, seam, [rows(3, "only")]).pipe(toArray()),
  );
  seam.db.close();

  const [first] = TickStatementLedger.entries();
  assert.equal(lines.length, 1);
  assert.deepEqual(
    { adds: first?.adds, dels: first?.dels },
    { adds: 2, dels: 0 },
    "keep(2) leaves two rows at the boundary and retracts none on the first tick",
  );
  assert.ok((first?.statements ?? 0) > 0, "a tick that writes rows runs statements");
});
