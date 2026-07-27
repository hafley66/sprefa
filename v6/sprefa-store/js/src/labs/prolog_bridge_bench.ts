/**
 * prolog_bridge_bench.ts — bench entry for the prolog frontend bridge. Hydrates
 * the `Program` JSON that v6/prolog/src/emit_ast.pl emitted and runs it through
 * the REAL evaluator (evalProgramSql: strata, semi-naive deltas, the works)
 * over a :memory: libsql client, on the shared benchgraph workload.
 *
 * Phases match the harness: setup = tables + facts + full derive with roots
 * {0,1}; retract = delete root 0, clear the IDB table, re-derive. Prints the
 * harness CSV line on stdout.
 *
 * Usage: node src/labs/prolog_bridge_bench.ts <program.json> <layers> <width>
 * (async/await is the local idiom for this driver seam, as in lowerSql.test.ts.)
 */

import { readFileSync } from "node:fs";
import { firstValueFrom } from "rxjs";
import { createClient } from "@libsql/client";

import type { Program } from "../lower/ast.ts";
import { evalProgramSql } from "../lower/lowerSql.ts";
import type { RelTable, RelTables } from "../lower/types.ts";

const [programPath, layersArg, widthArg] = process.argv.slice(2);
const program = JSON.parse(readFileSync(programPath, "utf8")) as Program;
const layers = Number(layersArg ?? 2);
const width = Number(widthArg ?? 200);

// benchgraph::gen mirror (src/measure.rs): nodes 0,1 roots, layered DAG
const edges: [number, number][] = [];
for (let w = 0; w < width; w++) {
  const id = 2 + w;
  edges.push([0, id]);
  if (w % 3 === 0) edges.push([1, id]);
}
for (let l = 1; l < layers; l++) {
  for (let w = 0; w < width; w++) {
    const id = 2 + l * width + w;
    const prev = 2 + (l - 1) * width;
    edges.push([prev + w, id], [prev + (w + 1) % width, id]);
  }
}

const db = createClient({ url: ":memory:" });
const tables = new Map<string, RelTable>();

async function count(rel: string): Promise<number> {
  const res = await db.execute(`SELECT count(*) FROM t_${rel}`);
  return Number(res.rows[0][0]);
}

async function main(): Promise<void> {
  const t0 = performance.now();
  for (const decl of program.rels) {
    tables.set(decl.name, { table: `t_${decl.name}`, columns: decl.columns });
    await db.executeMultiple(
      `CREATE TABLE t_${decl.name}(${decl.columns.join(", ")}, PRIMARY KEY (${decl.columns.join(", ")})) WITHOUT ROWID`,
    );
  }
  for (let i = 0; i < edges.length; i += 400) {
    const vals = edges
      .slice(i, i + 400)
      .map(([p, c]) => `(${p},${c})`)
      .join(",");
    await db.executeMultiple(`INSERT INTO t_edge(parent, child) VALUES ${vals}`);
  }
  await db.executeMultiple(`INSERT INTO t_root(node) VALUES (0),(1)`);
  await firstValueFrom(evalProgramSql(db, program, tables as RelTables));
  const before = await count("reach");
  const setupMs = performance.now() - t0;

  const t1 = performance.now();
  await db.executeMultiple(`DELETE FROM t_root WHERE node = 0`);
  await db.executeMultiple(`DELETE FROM t_reach`);
  await firstValueFrom(evalProgramSql(db, program, tables as RelTables));
  const after = await count("reach");
  const retractMs = performance.now() - t1;

  const nodes = 2 + layers * width;
  console.log(
    `CSV,swi-js,${nodes},${edges.length},${before - after},${setupMs.toFixed(3)},${retractMs.toFixed(3)}`,
  );
  db.close();
}

main().catch((failure: unknown) => {
  console.error(failure);
  process.exit(1);
});
