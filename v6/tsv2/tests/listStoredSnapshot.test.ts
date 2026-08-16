/**
 * listStoredSnapshot.test.ts — the read_stored_snapshot pin for a live
 * list(T) column. The emitted `read_stored_snapshot` reads the RAW base table
 * (not the `__list_` view), so a list column's value there is the interned
 * surrogate INTEGER id, not the array text `read_snapshot` decodes. Before
 * `rel_stored_column_types` existed the stored read inherited `rel_column_types`
 * ("list") and `row_value_from_sql` threw `list column crossed SQLite with <id>`
 * on the first non-empty row. That path is latent on any list(T) column that
 * ever holds a row; golden-flex's `tree_bundle(tree_id, sites: list(patch))`
 * is the one column in the corpus with both a list type and an ordered program
 * (so read_stored_snapshot is emitted), and golden-schedules.ts never seeds it,
 * which is why the corpus passed before the fix.
 *
 * The pin is behavioral: arrive one tree_bundle row and assert the tick folds
 * clean and the decoded boundary delta is the empty list the no-member seed
 * renders. Reverting the stored-type fix throws here instead.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { concatMap, firstValueFrom, toArray } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, ISqlSeam } from "../runtime/types.ts";

import { program } from "../gen_emitted/golden-flex.ts";

async function booted(): Promise<ISqlSeam> {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(
    ScratchStore.boot(seam, program.ddl).pipe(concatMap(() => BootRunner.run(seam, program.boot))),
  );
  return seam;
}

test("read_stored_snapshot reads a live list column as its surrogate id", async () => {
  const seam = await booted();
  const schedule: readonly IArrivalBatch[] = [
    [{ rel: "tree_bundle", sign: "add", row: [1, 42] }],
  ];
  const lines = await firstValueFrom(TickFold.run(program, seam, schedule).pipe(toArray()));
  const tree_line = lines.find((line) => line.includes('"tree_bundle"'));
  assert.ok(tree_line, `tree_bundle delta missing from the tick log:\n${lines.join("\n")}`);
  assert.match(
    tree_line,
    /"tree_bundle":\{"add":\[\[1,\[\]\]\],"del":\[\]\}/,
    `tree_bundle decoded as the empty list, got: ${tree_line}`,
  );
});
