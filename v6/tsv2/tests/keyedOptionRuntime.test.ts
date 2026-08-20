import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { join } from "node:path";
import { test } from "node:test";

import { concatMap, firstValueFrom, toArray } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalRow, IRowValue, IServedProgram } from "../runtime/types.ts";

type Module = { readonly program: IServedProgram };
const TSV2 = fileURLToPath(new URL("..", import.meta.url));
const V6 = join(TSV2, "..");
const ROOT = join(V6, "..");
const SOURCE = join(V6, "dl", "fixtures", "keyed-option-relation-runtime.dl6");
const COMPILE = join(V6, "prolog", "compile", "scripts", "compile_dl6.sh");

async function load(): Promise<Module> {
  const dir = join(TSV2, "gen_emitted"); mkdirSync(dir, { recursive: true });
  const file = join(dir, `keyed_option_relation_runtime_${process.pid}.ts`);
  execFileSync(COMPILE, [SOURCE, file], { cwd: ROOT, encoding: "utf8" });
  try { return await import(`${pathToFileURL(file).href}?keyed-option=${Date.now()}`) as Module; }
  finally { rmSync(file, { force: true }); }
}

/** `id: key(option(Person))` declares a REFERENCE column: the arrival carries
 *  the `__opt_Person` instance id, and the instance itself arrives as its own
 *  variant row. `rel_column_types` says `int` for that column, which is the
 *  receipt that the compiler already spells it as a reference. */
function option_instance(sign: "add" | "del", id: number, person: Record<string, IRowValue>): IArrivalRow {
  return { rel: "__opt_Person_some", sign, row: [id, person] };
}

function keyed(sign: "add" | "del", id: number, body: string): IArrivalRow {
  return { rel: "KeyedRelationOption", sign, row: [id, body] };
}

test("a keyed option column carries the instance reference and keeps its keyed replacement", async () => {
  const { program } = await load();
  assert.deepEqual(program.rel_column_types?.KeyedRelationOption, ["int", "text"]);
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl).pipe(concatMap(() => BootRunner.run(seam, program.boot))));
  const lines = await firstValueFrom(TickFold.run(program, seam, [
    [option_instance("add", 1, { id: 1, name: "Ada" }), keyed("add", 1, "old")],
    [option_instance("add", 1, { name: "Ada", id: 1 }), keyed("add", 1, "new")],
    [keyed("del", 1, "old")],
    [keyed("del", 1, "new")],
  ]).pipe(toArray()));
  const ticks = lines.map((line) => JSON.parse(line) as { deltas: Record<string, { add: unknown[][]; del: unknown[][] }> });
  assert.deepEqual(ticks[0]?.deltas.KeyedRelationOption, { add: [[1, "old"]], del: [] });
  // The reversed-key struct is the same Person row, so the option instance
  // stays one row and only the parent body replaces.
  assert.deepEqual(ticks[1]?.deltas.__opt_Person_some, undefined);
  assert.deepEqual(ticks[1]?.deltas.KeyedRelationOption, { add: [[1, "new"]], del: [[1, "old"]] });
  assert.equal(ticks[2]?.deltas.KeyedRelationOption, undefined);
  assert.deepEqual(ticks[3]?.deltas.KeyedRelationOption, { add: [], del: [[1, "new"]] });
});
