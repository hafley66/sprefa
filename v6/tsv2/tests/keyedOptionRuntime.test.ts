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
import type { IArrivalBatch, IRowValue, IServedProgram } from "../runtime/types.ts";

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

function keyed(sign: "add" | "del", value: Record<string, IRowValue>, body: string): IArrivalBatch {
  return [{ rel: "KeyedRelationOption", sign, row: [{ tag: "some", value }, body] }];
}

test("generated keyed relation options normalize before validation and preserve public retractions", async () => {
  const { program } = await load();
  assert.equal(program.enum_ref_columns?.KeyedRelationOption?.[0]?.endpoint_index, null);
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl).pipe(concatMap(() => BootRunner.run(seam, program.boot))));
  const lines = await firstValueFrom(TickFold.run(program, seam, [
    keyed("add", { id: 1, name: "Ada" }, "old"),
    keyed("add", { name: "Ada", id: 1 }, "new"),
    keyed("del", { id: 1, name: "Ada" }, "old"),
    keyed("del", { id: 1, name: "Ada" }, "new"),
  ]).pipe(toArray()));
  const ticks = lines.map((line) => JSON.parse(line) as { deltas: Record<string, { add: unknown[][]; del: unknown[][] }> });
  assert.deepEqual(ticks[0]?.deltas.KeyedRelationOption, {
    add: [[{ tag: "some", value: { id: 1, name: "Ada" } }, "old"]], del: [],
  });
  assert.deepEqual(ticks[1]?.deltas.KeyedRelationOption, {
    add: [[{ tag: "some", value: { id: 1, name: "Ada" } }, "new"]],
    del: [[{ tag: "some", value: { id: 1, name: "Ada" } }, "old"]],
  });
  assert.equal(ticks[2]?.deltas.KeyedRelationOption, undefined);
  assert.deepEqual(ticks[3]?.deltas.KeyedRelationOption, {
    add: [], del: [[{ tag: "some", value: { id: 1, name: "Ada" } }, "new"]],
  });
});
