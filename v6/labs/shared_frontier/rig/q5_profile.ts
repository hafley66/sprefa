/**
 * Q5: where time goes in the worst arm-B cell (N=1024, k=N).
 *
 * Prints the insert/read/delete split with statement counts, median of 5, then
 * the same workload runs once more so a `--cpu-prof` wrapper captures it.
 * `rig/q5_summarize.ts` reads the resulting .cpuprofile.
 */

import { markdownTable, median, openMemory, round } from "./common.ts";
import { bootDdl, durableInsertSql, frontierDeleteSql, frontierInsertSql, frontierReadSql, touched } from "./schema.ts";

const RELATIONS = 1024;
const TICKS = 50;
const RUNS = 5;

async function run(): Promise<{ insert: number; read: number; remove: number; counts: [number, number, number] }> {
  const db = openMemory();
  for (const statement of bootDdl("B", RELATIONS)) await db.execute(statement);
  let insert = 0;
  let read = 0;
  let remove = 0;
  const counts: [number, number, number] = [0, 0, 0];
  let rowKey = 0;
  for (let tick = 1; tick <= TICKS; tick += 1) {
    const indices = touched(RELATIONS, RELATIONS, tick);

    let phase = performance.now();
    for (const index of indices) {
      rowKey += 1;
      const inserted = await db.execute({ sql: durableInsertSql(index), args: [rowKey, rowKey * 2, rowKey * 3] });
      await db.execute({ sql: frontierInsertSql("B", index), args: [index, Number(inserted.lastInsertRowid), tick] });
      counts[0] += 2;
    }
    insert += performance.now() - phase;

    phase = performance.now();
    for (const index of indices) {
      await db.execute({ sql: frontierReadSql("B", index), args: [index, tick] });
      counts[1] += 1;
    }
    read += performance.now() - phase;

    phase = performance.now();
    await db.execute({ sql: frontierDeleteSql("B", 0), args: [tick] });
    counts[2] += 1;
    remove += performance.now() - phase;
  }
  return { insert, read, remove, counts };
}

const samples: Awaited<ReturnType<typeof run>>[] = [];
for (let attempt = 0; attempt < RUNS; attempt += 1) samples.push(await run());
const counts = samples[0].counts;
const insert = median(samples.map((sample) => sample.insert));
const read = median(samples.map((sample) => sample.read));
const remove = median(samples.map((sample) => sample.remove));
const total = insert + read + remove;

console.log(`### Q5a. Arm B, N=${RELATIONS}, k=N, ${TICKS} ticks, median of ${RUNS}\n`);
console.log(
  markdownTable(
    ["phase", "statements", "median ms", "share of tick", "us per statement"],
    [
      ["insert (durable + frontier)", String(counts[0]), round(insert, 1).toFixed(1), `${round((100 * insert) / total, 1).toFixed(1)}%`, round((insert * 1000) / counts[0], 2).toFixed(2)],
      ["read (frontier join durable)", String(counts[1]), round(read, 1).toFixed(1), `${round((100 * read) / total, 1).toFixed(1)}%`, round((read * 1000) / counts[1], 2).toFixed(2)],
      ["delete (one per tick)", String(counts[2]), round(remove, 1).toFixed(1), `${round((100 * remove) / total, 1).toFixed(1)}%`, round((remove * 1000) / counts[2], 2).toFixed(2)],
      ["total", String(counts[0] + counts[1] + counts[2]), round(total, 1).toFixed(1), "100.0%", round((total * 1000) / (counts[0] + counts[1] + counts[2]), 2).toFixed(2)],
    ],
  ),
);
