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
import type {
  IBootStatement,
  IGenProgram,
  IIncrementalProgramPlan,
} from "../runtime/types.ts";

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
const SOURCE = join(TSV2, "goldens", "module_storage_runtime", "0_module_storage_runtime.dl6");
const COMPILE = join(V6, "prolog", "compile", "scripts", "compile_dl6.sh");

function normalized_rows(rows: readonly Record<string, unknown>[]): string[][] {
  return rows.map((row) => Object.values(row).map((value) => String(value)));
}

async function load_compiled_module(): Promise<EmittedModule> {
  const generated_dir = join(TSV2, "gen_emitted");
  mkdirSync(generated_dir, { recursive: true });
  const generated = join(generated_dir, `module_storage_runtime_e2e_${process.pid}.ts`);
  execFileSync(COMPILE, [SOURCE, generated], { cwd: ROOT, encoding: "utf8" });
  try {
    return await import(`${pathToFileURL(generated).href}?module-storage-e2e=${Date.now()}`) as EmittedModule;
  } finally {
    rmSync(generated, { force: true });
  }
}

test("module storage names execute through the TypeScript SQLite runtime", async () => {
  const { program, incremental_plan } = await load_compiled_module();
  const physical = new Map(incremental_plan.relations.map((relation) => [relation.rel, relation]));

  assert.deepEqual(
    [...physical].map(([rel, plan]) => [rel, plan.table_name]),
    [
      ["First", "a_model_First"],
      ["Person", "0_module_storage_runtime_Person"],
      ["Second", "b_model_Second"],
      ["derived", "0_module_storage_runtime_derived"],
      ["imported", "0_module_storage_runtime_imported"],
      ["person", "0_module_storage_runtime_person_2"],
      ["source", "0_module_storage_runtime_source"],
    ],
  );

  const person = physical.get("Person");
  const imported = physical.get("imported");
  const derived = physical.get("derived");
  assert.ok(person);
  assert.ok(imported);
  assert.ok(derived);
  assert.match(person.arrival_add_sql ?? "", /0_module_storage_runtime_Person/);
  assert.match(person.delta_table_name, /^__delta_0_module_storage_runtime_Person$/);
  assert.match(person.frontier_table_name, /^__frontier_0_module_storage_runtime_Person$/);
  assert.match(person.next_frontier_table_name, /^__next_frontier_0_module_storage_runtime_Person$/);

  const imported_rule = incremental_plan.levels.find((level) => level.head_rel === "imported");
  const derived_rule = incremental_plan.levels.find((level) => level.head_rel === "derived");
  assert.ok(imported_rule);
  assert.ok(derived_rule);
  assert.match(imported_rule.insert_sql ?? "", /__frontier_a_model_First/);
  assert.match(imported_rule.recompute_sql ?? "", /a_model_First/);
  assert.match(imported_rule.insert_sql ?? "", /0_module_storage_runtime_imported/);
  assert.match(derived_rule.insert_sql ?? "", /__frontier_0_module_storage_runtime_imported/);
  assert.match(derived_rule.recompute_sql ?? "", /0_module_storage_runtime_imported/);
  assert.match(derived_rule.insert_sql ?? "", /0_module_storage_runtime_derived/);

  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(
    ScratchStore.boot(seam, program.ddl).pipe(concatMap(() => BootRunner.run(seam, program.boot))),
  );
  const lines = await firstValueFrom(
    TickFold.run(program, seam, [[
      { rel: "Person", sign: "add", row: ["alice"] },
      { rel: "person", sign: "add", row: ["alice"] },
      { rel: "First", sign: "add", row: ["alice"] },
      { rel: "Second", sign: "add", row: ["bob"] },
    ]]).pipe(toArray()),
  );

  assert.deepEqual(lines, [
    '{"tick":1,"deltas":{"First":{"add":[["alice"]],"del":[]},"Person":{"add":[["alice"]],"del":[]},"Second":{"add":[["bob"]],"del":[]},"derived":{"add":[["alice"]],"del":[]},"imported":{"add":[["alice"]],"del":[]},"person":{"add":[["alice"]],"del":[]}}}',
  ]);

  const final_rows: Record<string, string[][]> = {};
  for (const [rel, sql] of Object.entries(program.final_select)) {
    const result = await firstValueFrom(seam.runner.execute(seam.db, sql));
    final_rows[rel] = normalized_rows(result.rows);
  }
  assert.deepEqual(final_rows, {
    First: [["alice"]],
    Person: [["alice"]],
    Second: [["bob"]],
    derived: [["alice"]],
    imported: [["alice"]],
    person: [["alice"]],
    source: [["direct"]],
  });

  const public_tables = (await firstValueFrom(
    seam.runner.execute(seam.db, `SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name`),
  )).rows.map((row) => String(row.name));
  assert.deepEqual(public_tables, [
    "0_module_storage_runtime_Person",
    "0_module_storage_runtime_derived",
    "0_module_storage_runtime_imported",
    "0_module_storage_runtime_person_2",
    "0_module_storage_runtime_source",
    "__str",
    "a_model_First",
    "b_model_Second",
  ]);
  for (const name of ["First", "Person", "Second", "derived", "imported", "person", "source"]) {
    assert.equal(public_tables.includes(name), false, `unprefixed table leaked: ${name}`);
  }
});
