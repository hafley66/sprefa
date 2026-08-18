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

/** Every stored rel here is `rel X(name: text)`, so one shape digest covers all
 *  five; a DERIVED rel takes none. `person` folds onto `Person` in SQLite even
 *  with the digest, so it keeps the deterministic `_2` collision suffix. */
const SHAPE = "7a5ef237b7b9";
const ENTRY = "0_module_storage_runtime";
const PERSON_TABLE = `${ENTRY}_Person_${SHAPE}`;
const PERSON_LOWER_TABLE = `${ENTRY}_person_${SHAPE}_2`;
const SOURCE_TABLE = `${ENTRY}_source_${SHAPE}`;
const IMPORTED_TABLE = `${ENTRY}_imported`;
const DERIVED_TABLE = `${ENTRY}_derived`;
const FIRST_TABLE = `a_model_First_${SHAPE}`;
const SECOND_TABLE = `b_model_Second_${SHAPE}`;

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
      ["First", FIRST_TABLE],
      ["Person", PERSON_TABLE],
      ["Second", SECOND_TABLE],
      ["derived", DERIVED_TABLE],
      ["imported", IMPORTED_TABLE],
      ["person", PERSON_LOWER_TABLE],
      ["source", SOURCE_TABLE],
    ],
  );

  const person = physical.get("Person");
  const imported = physical.get("imported");
  const derived = physical.get("derived");
  assert.ok(person);
  assert.ok(imported);
  assert.ok(derived);
  assert.ok((person.arrival_add_sql ?? "").includes(PERSON_TABLE));
  assert.equal(person.delta_table_name, `__delta_${PERSON_TABLE}`);
  assert.equal(person.frontier_table_name, `__frontier_${PERSON_TABLE}`);
  assert.equal(person.next_frontier_table_name, `__next_frontier_${PERSON_TABLE}`);

  const imported_rule = incremental_plan.levels.find((level) => level.head_rel === "imported");
  const derived_rule = incremental_plan.levels.find((level) => level.head_rel === "derived");
  assert.ok(imported_rule);
  assert.ok(derived_rule);
  assert.ok((imported_rule.insert_sql ?? "").includes(`__frontier_${FIRST_TABLE}`));
  assert.ok((imported_rule.recompute_sql ?? "").includes(FIRST_TABLE));
  assert.ok((imported_rule.insert_sql ?? "").includes(IMPORTED_TABLE));
  assert.ok((derived_rule.insert_sql ?? "").includes(`__frontier_${IMPORTED_TABLE}`));
  assert.ok((derived_rule.recompute_sql ?? "").includes(IMPORTED_TABLE));
  assert.ok((derived_rule.insert_sql ?? "").includes(DERIVED_TABLE));

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
    PERSON_TABLE,
    DERIVED_TABLE,
    IMPORTED_TABLE,
    PERSON_LOWER_TABLE,
    SOURCE_TABLE,
    "__str",
    FIRST_TABLE,
    SECOND_TABLE,
  ]);
  for (const name of ["First", "Person", "Second", "derived", "imported", "person", "source"]) {
    assert.equal(public_tables.includes(name), false, `unprefixed table leaked: ${name}`);
  }
});
