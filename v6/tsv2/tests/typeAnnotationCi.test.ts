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
import type { IBootStatement, IGenProgram, IIncrementalProgramPlan } from "../runtime/types.ts";

type Module = { program: IGenProgram & { boot: readonly IBootStatement[]; final_select: Readonly<Record<string, string>> }; incremental_plan: IIncrementalProgramPlan };
const TSV2 = fileURLToPath(new URL("..", import.meta.url));
const V6 = join(TSV2, "..");
const ROOT = join(V6, "..");
const SOURCE = join(V6, "dl", "fixtures", "type-annotation-ci.dl6");
const COMPILE = join(V6, "prolog", "compile", "scripts", "compile_dl6.sh");

async function load(): Promise<Module> {
  const dir = join(TSV2, "gen_emitted"); mkdirSync(dir, { recursive: true });
  const file = join(dir, `type_annotation_ci_${process.pid}.ts`);
  execFileSync(COMPILE, [SOURCE, file], { cwd: ROOT, encoding: "utf8" });
  try { return await import(`${pathToFileURL(file).href}?annotation-ci=${Date.now()}`) as Module; }
  finally { rmSync(file, { force: true }); }
}

test("annotation key and legacy key execute identical SQLite replacements", async () => {
  const { program, incremental_plan } = await load();
  for (const name of ["key", "configure", "first", "second", "optional"]) {
    assert.equal(incremental_plan.relations.some((relation) => relation.rel === name), false);
  }
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl).pipe(concatMap(() => BootRunner.run(seam, program.boot))));
  await firstValueFrom(TickFold.run(program, seam, [[
    { rel: "LegacyKey", sign: "add", row: [1, "old"] }, { rel: "LegacyKey", sign: "add", row: [1, "new"] },
    { rel: "AnnotationKey", sign: "add", row: [1, "old"] }, { rel: "AnnotationKey", sign: "add", row: [1, "new"] },
  ]]).pipe(toArray()));
  for (const rel of ["LegacyKey", "AnnotationKey"]) {
    const result = await firstValueFrom(seam.runner.execute(seam.db, program.final_select[rel]!));
    assert.deepEqual(result.rows.map((row) => Object.values(row)), [[1, "new"]]);
  }
});
