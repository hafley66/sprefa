/** Writes keyed_replace, two_hop_join, and cross_join schedules. */

import { writeFileSync } from "node:fs";

/** One arrival row consumed by both adapters. */
interface IArrivalRow {
  readonly rel: string;
  readonly sign: "add" | "del";
  readonly row: readonly (string | number)[];
}

type Shape = "keyed_replace" | "two_hop_join" | "cross_join";

const BATCH = 100;

/** keyed_replace rewrites 100 keys per tick. */
function keyedReplaceSchedule(rows: number): IArrivalRow[][] {
  const ticks: IArrivalRow[][] = [];
  for (let tick = 0; tick < rows / BATCH; tick++) {
    const batch: IArrivalRow[] = [];
    for (let key = 0; key < BATCH; key++) {
      batch.push({ rel: "change", sign: "add", row: [`k${key}`, `v${tick * 1000 + key}`] });
    }
    ticks.push(batch);
  }
  return ticks;
}

/** two_hop_join seeds link rows, then emits 100 c rows per tick. */
function twoHopJoinSchedule(rows: number): IArrivalRow[][] {
  const seed: IArrivalRow[] = [];
  for (let key = 0; key < BATCH; key++) seed.push({ rel: "b_link", sign: "add", row: [`k${key}`, `m${key}`] });
  for (let key = 0; key < BATCH; key++) seed.push({ rel: "a_link", sign: "add", row: [`m${key}`, `o${key}`] });

  const ticks: IArrivalRow[][] = [seed];
  for (let tick = 0; tick < rows / BATCH; tick++) {
    const batch: IArrivalRow[] = [];
    for (let offset = 0; offset < BATCH; offset++) {
      batch.push({ rel: "c", sign: "add", row: [`k${offset}`, `p${tick * BATCH + offset}`] });
    }
    ticks.push(batch);
  }
  return ticks;
}

/** cross_join emits all left arrivals before all right arrivals. */
function crossJoinSchedule(rows: number): IArrivalRow[][] {
  const ticks: IArrivalRow[][] = [];
  for (const rel of ["left_value", "right_value"] as const) {
    const prefix = rel === "left_value" ? "l" : "r";
    for (let tick = 0; tick < rows / BATCH; tick++) {
      const batch: IArrivalRow[] = [];
      for (let offset = 0; offset < BATCH; offset++) {
        batch.push({ rel, sign: "add", row: [`${prefix}${tick * BATCH + offset}`] });
      }
      ticks.push(batch);
    }
  }
  return ticks;
}

const BUILDERS: Readonly<Record<Shape, (rows: number) => IArrivalRow[][]>> = {
  keyed_replace: keyedReplaceSchedule,
  two_hop_join: twoHopJoinSchedule,
  cross_join: crossJoinSchedule,
};

function main(): void {
  const [shape, rowsText, outPath] = process.argv.slice(2);
  const build = shape === undefined ? undefined : BUILDERS[shape as Shape];
  const rows = Number(rowsText);
  if (build === undefined || !Number.isInteger(rows) || rows < BATCH || rows % BATCH !== 0 || outPath === undefined) {
    process.stderr.write("usage: schedule-gen.ts <keyed_replace|two_hop_join|cross_join> <rows: multiple of 100, >=100> <outPath>\n");
    process.exitCode = 2;
    return;
  }
  writeFileSync(outPath, JSON.stringify(build(rows)));
  process.stderr.write(`wrote ${outPath}\n`);
}

main();
