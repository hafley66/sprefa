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

type EmittedProgram = IGenProgram & {
  readonly boot: readonly IBootStatement[];
  readonly final_select: Readonly<Record<string, string>>;
};

type EmittedModule = {
  readonly program: EmittedProgram;
  readonly incremental_plan: IIncrementalProgramPlan;
};

const TSV2 = fileURLToPath(new URL("..", import.meta.url));
const V6 = join(TSV2, "..");
const ROOT = join(V6, "..");
const SOURCE = join(TSV2, "goldens", "relation_id_access", "0_relation_id_access.dl6");
const COMPILE = join(V6, "prolog", "compile", "scripts", "compile_dl6.sh");

async function load_compiled_module(): Promise<EmittedModule> {
  const generated_dir = join(TSV2, "gen_emitted");
  mkdirSync(generated_dir, { recursive: true });
  const generated = join(generated_dir, `relation_id_access_${process.pid}.ts`);
  execFileSync(COMPILE, [SOURCE, generated], { cwd: ROOT, encoding: "utf8" });
  try {
    return await import(`${pathToFileURL(generated).href}?relation-id-access=${Date.now()}`) as EmittedModule;
  } finally {
    rmSync(generated, { force: true });
  }
}

async function final_rows(
  seam: ReturnType<typeof ScratchStore.open>,
  program: EmittedProgram,
  rel: string,
): Promise<readonly Record<string, unknown>[]> {
  const sql = program.final_select[rel];
  assert.ok(sql, `final select for ${rel}`);
  return (await firstValueFrom(seam.runner.execute(seam.db, sql))).rows;
}

test("Revision.id compiles to an integer endpoint without following Revision", async () => {
  const { program, incremental_plan } = await load_compiled_module();
  const relation = new Map(incremental_plan.relations.map((item) => [item.rel, item]));
  const id_only = relation.get("IdOnly");
  const value_only = relation.get("ValueOnly");
  const both = relation.get("Both");
  const dot_id = relation.get("DotId");
  const revision_batch = relation.get("RevisionBatch");
  assert.ok(id_only);
  assert.ok(value_only);
  assert.ok(both);
  assert.ok(dot_id);
  assert.ok(revision_batch);
  assert.deepEqual(id_only.column_types, ["relation_id"]);
  assert.deepEqual(value_only.column_types, ["ref"]);
  assert.deepEqual(both.column_types, ["ref", "relation_id"]);
  assert.deepEqual(revision_batch.column_types, ["list"]);

  const id_only_sql = program.final_select.IdOnly;
  const value_only_sql = program.final_select.ValueOnly;
  const both_sql = program.final_select.Both;
  const dot_id_sql = program.final_select.DotId;
  const listed_id_sql = program.final_select.ListedId;
  const listed_value_sql = program.final_select.ListedValue;
  assert.ok(id_only_sql);
  assert.ok(value_only_sql);
  assert.ok(both_sql);
  assert.ok(dot_id_sql);
  assert.ok(listed_id_sql);
  assert.ok(listed_value_sql);
  const revision_view = /__ref_0_relation_id_access_Revision/g;
  assert.equal((id_only_sql.match(revision_view) ?? []).length, 0);
  assert.equal((value_only_sql.match(revision_view) ?? []).length, 1);
  assert.equal((both_sql.match(revision_view) ?? []).length, 1);
  assert.match(dot_id_sql, /^SELECT t\."revision_id"/);
  assert.equal((listed_id_sql.match(revision_view) ?? []).length, 0);
  assert.equal((listed_value_sql.match(revision_view) ?? []).length, 1);

  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(
    ScratchStore.boot(seam, program.ddl).pipe(concatMap(() => BootRunner.run(seam, program.boot))),
  );
  await firstValueFrom(
    TickFold.run(program, seam, [[
      { rel: "Revision", sign: "add", row: ["r1"] },
      { rel: "Revision", sign: "add", row: ["r2"] },
      { rel: "File", sign: "add", row: [{ oid: "r1" } as unknown as string, "src/main.dl6"] },
      {
        rel: "Holder",
        sign: "add",
        row: [{ revision: { oid: "r1" }, path: "src/main.dl6" } as unknown as string],
      },
      { rel: "__gen__list_id_Revision_a140a3db2b035729", sign: "add", row: ["[2,1,2]"] },
      { rel: "__gen__list_id_Revision_a140a3db2b035729__member", sign: "add", row: [1, 0, 2] },
      { rel: "__gen__list_id_Revision_a140a3db2b035729__member", sign: "add", row: [1, 1, 1] },
      { rel: "__gen__list_id_Revision_a140a3db2b035729__member", sign: "add", row: [1, 2, 2] },
      { rel: "RevisionBatch", sign: "add", row: [1] },
    ]]).pipe(toArray()),
  );
  assert.deepEqual(await final_rows(seam, program, "IdOnly"), [{ revision_id: 1 }]);
  const value_only_rows = await final_rows(seam, program, "ValueOnly");
  const both_rows = await final_rows(seam, program, "Both");
  assert.deepEqual(value_only_rows.map((row) => ({ revision: JSON.parse(row.revision as string) })), [{ revision: { oid: "r1" } }]);
  assert.deepEqual(both_rows.map((row) => ({
    revision: JSON.parse(row.revision as string),
    revision_id: row.revision_id,
  })), [{ revision: { oid: "r1" }, revision_id: 1 }]);
  assert.deepEqual(await final_rows(seam, program, "DotId"), [{ revision_id: 1 }]);
  assert.deepEqual(await final_rows(seam, program, "ListedId"), [
    { revision_id: 1 },
    { revision_id: 2 },
  ]);
  assert.deepEqual(
    (await final_rows(seam, program, "ListedValue")).map((row) => JSON.parse(row.revision as string)),
    [{ oid: "r1" }, { oid: "r2" }],
  );
  assert.deepEqual(await final_rows(seam, program, "RevisionBatch"), [{ revisions: "[2,1,2]" }]);
  const member_rows = await firstValueFrom(seam.runner.execute(
    seam.db,
    'SELECT "idx", "value" FROM "0_relation_id_access___gen__list_id_Revision_a140a3db2b035729__member" ORDER BY "idx"',
  ));
  assert.deepEqual(member_rows.rows, [
    { idx: 0, value: 2 },
    { idx: 1, value: 1 },
    { idx: 2, value: 2 },
  ]);
});
