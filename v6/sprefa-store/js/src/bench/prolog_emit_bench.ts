/**
 * prolog_emit_bench.ts — bench entry for the prolog frontend's literal-TS
 * emission (v6/prolog/src/emit_ts.pl). Imports the emitted module — readable
 * TS built from ast.ts's own constructor helpers, no JSON hydration — and
 * runs it through the REAL evaluator (evalProgramSql: strata, semi-naive
 * deltas, the works) over a :memory: libsql client, on the shared benchgraph
 * workload.
 *
 * Phases match the harness: setup = tables + facts + full derive with roots
 * {0,1}; retract = delete root 0, clear the IDB table, re-derive. Prints the
 * harness CSV line on stdout.
 *
 * Usage: node src/labs/prolog_emit_bench.ts <program-module.ts> <layers> <width>
 * (async/await is the local idiom for this driver seam, as in lowerSql.test.ts.)
 */

import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { firstValueFrom } from "rxjs";
import { createClient } from "@libsql/client";

import type { Program } from "../lower/ast.ts";
import { evalProgramSql } from "../lower/lowerSql.ts";
import type { RelTable, RelTables } from "../lower/types.ts";

const [modulePath, layersArg, widthArg] = process.argv.slice(2);
if (modulePath === undefined) throw new Error("usage: prolog_emit_bench.ts <program-module.ts> <layers> <width>");
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
  return Number(res.rows[0]?.[0]);
}

async function main(): Promise<void> {
  const { program } = (await import(pathToFileURL(resolve(modulePath!)).href)) as { program: Program };

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
    `CSV,swi-emit,${nodes},${edges.length},${before - after},${setupMs.toFixed(3)},${retractMs.toFixed(3)}`,
  );
  db.close();
}

main().catch((failure: unknown) => {
  console.error(failure);
  process.exit(1);
});
