/**
 * schedule-gen.ts — writes the scale schedules for the keyed_replace /
 * two_hop_join / cross_join programs at a given row count.
 *
 * Transcribed from v6/sprefa-store/bench/scale-gen.pl's s1_schedule/2,
 * s2_schedule/2 and s3_schedule/2 so the arrival stream is the same shape the
 * existing SCALE.md numbers were taken over: 100 arrivals per tick,
 * TickCount = Rows / 100. The one deliberate difference is two_hop_join's link rows,
 * which the fixture supplies as `Initial` (boot) and which the text door
 * cannot express as bare facts -- they arrive here as tick 1 instead. See
 * programs/two_hop_join.dl6's header.
 *
 * This is plain array code that returns arrays and writes one file. It is not
 * an Observable: nothing here is async above the single fs write.
 *
 * Usage: node --experimental-transform-types schedule-gen.ts <shape> <rows> <outPath>
 */

import { writeFileSync } from "node:fs";

/** One arrival row, the shape both doors read (CONTRACT.md section 2.3). */
interface IArrivalRow {
  readonly rel: string;
  readonly sign: "add" | "del";
  readonly row: readonly (string | number)[];
}

type Shape = "keyed_replace" | "two_hop_join" | "cross_join";

const BATCH = 100;

/** keyed_replace: 100 keys, every tick rewrites all of them with a fresh value. */
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

/**
 * two_hop_join: one seeding tick of 100 b_link + 100 a_link rows, then `rows`/100 ticks
 * of 100 `c` arrivals each. scale-gen.pl builds the identical link rows in
 * lookup_rows/1 (k<i> -> m<i>, m<i> -> o<i>) but delivers them as Initial.
 */
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

/** cross_join: all left arrivals, then all right arrivals. Quadratic on purpose. */
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
