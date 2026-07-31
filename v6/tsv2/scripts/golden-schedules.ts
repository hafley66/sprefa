/**
 * golden-schedules.ts — write the four arrival schedules golden-flex.dl6 is
 * graded on, as the plain `IArrivalBatch[]` JSON both doors read.
 *
 * THE POINT IS CARDINALITY. The same nine-tick shape is emitted at 0 rows, 1
 * row and 100 rows per input rel, so every rule in the program is exercised
 * against an empty world, a singleton world and a world where every join,
 * aggregate group, retention window and departure frontier has real width. A
 * fixture corpus that only ever feeds one or two rows cannot tell a correct
 * rule from one that happens to be right at n=1.
 *
 * TICK SHAPE (identical at every cardinality; a batch is simply empty at 0):
 *   1  quarantined('weed')                the anti-join's one row, always
 *   2  tree + sensor                      the value plane arrives whole
 *   3  pick_event                         triggers every edge rule at once
 *   4  __host_response_weigh              the host's answers (see below)
 *   5  interval(1, bucket)                the bind row
 *   6  grade_ripe / grade_green / grade_bruised   the enum's variant rels
 *   7  retire_event                       the edge-arm match block
 *   8  DELETE half the trees              departures: finalize/1 fires here
 *   9  (empty)                            the settle tick departures land in
 *
 * THE HOST ANSWERS ARE SYNTHESIZED, NOT GUESSED. `weigh`'s witness digest is
 * built by the emitted SQL as
 *   'witness|weigh' || '|tree_id:int=' || tree_id || '|grams:int=' || grams
 * and its template is `printf '%s' "tree-{tree_id}-at-{grams}g"`. Reproducing
 * both here is what lets the NON-SERVED legs (oracle + both emitter modes)
 * grade the host-fed rules; the served leg runs the real subprocess and its
 * schedule is captured off the wire instead, so the two legs cross-check each
 * other's idea of the witness text.
 *
 * `grams` is `sum(sugar)` per tree and each tree gets exactly one pick, so the
 * demanded grams equal that pick's sugar -- stated because it is the one place
 * this generator has to know a rule's arithmetic.
 *
 * Usage: node --experimental-transform-types scripts/golden-schedules.ts <outDir>
 */

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { StructPlane } from "../runtime/structPlane.ts";
import type { IArrivalBatch, IArrivalRow } from "../runtime/types.ts";

/**
 * FINDING, measured here: `IRowValue` in runtime/types.ts is
 * `string | number | boolean`, so the declared arrival-row type CANNOT express a
 * struct value -- while the served engine demonstrably both accepts one and
 * prints one (tests/serveHost.test.ts asserts a `{ end: 42, start: 17 }` column
 * in a real tick-log delta). The header and the runtime disagree; this is the
 * same class as the sh/bind type-vocabulary gap and the JSON-schedule object
 * gap, and it is recorded rather than papered: the one cast below is the only
 * place this generator leaves the declared types, and it is named.
 */
type StructValue = { readonly [key: string]: number | string | StructValue };

/** One tree's whole arrival set, derived from its index so every cardinality
 *  uses the same generator and 100 rows are not 100 hand-written literals. */
function treeRow(index: number): IArrivalRow {
  const species = index % 3 === 0 ? "apple" : index % 3 === 1 ? "pear" : "weed";
  const site: StructValue = { label: `patch-${index % 4}`, at: { row: index % 5, col: index % 7 } };
  return { rel: "tree", sign: "add", row: [index, species, site as unknown as string] };
}

function orchardDocument(index: number): Readonly<Record<string, unknown>> {
  const species = index % 3 === 0 ? "apple" : index % 3 === 1 ? "pear" : "weed";
  const stars = 4 + (index % 6);
  return {
    empty: {},
    items: [
      { fruit: species, stars },
      { fruit: "pear", stars: stars + 1 },
    ],
    nested: { box: { leaf: index } },
    species,
    stars,
    tags: { blue: index + 1, red: index },
  };
}

function orchardJsonRow(index: number): IArrivalRow {
  return {
    rel: "orchard_json",
    sign: "add",
    row: [index, StructPlane.canonicalText(orchardDocument(index))],
  };
}

function orchardListRow(index: number): IArrivalRow {
  return {
    rel: "orchard_list",
    sign: "add",
    row: [index, StructPlane.canonicalText(["red", `tree-${index}`])],
  };
}

function orchardTagSourceRows(index: number): readonly IArrivalRow[] {
  return [
    { rel: "orchard_tag_source", sign: "add", row: [index, "red"] },
    { rel: "orchard_tag_source", sign: "add", row: [index, `tree-${index}`] },
  ];
}

function sugarOf(index: number): number {
  return 10 + (index % 9);
}

function pickRow(index: number): IArrivalRow {
  const kilos = index % 2 === 0 ? 2.5 : 0.75;
  return { rel: "pick_event", sign: "add", row: [index, index % 2 === 0 ? "ada" : "bob", kilos, sugarOf(index)] };
}

function hostAnswerRow(index: number): IArrivalRow {
  const grams = sugarOf(index);
  return {
    rel: "__host_response_weigh",
    sign: "add",
    row: [`witness|weigh|tree_id:int=${index}|grams:int=${grams}`, 0, index, grams, `tree-${index}-at-${grams}g`],
  };
}

function gradeRow(index: number): IArrivalRow {
  if (index % 3 === 0) return { rel: "grade_ripe", sign: "add", row: [index, sugarOf(index)] };
  if (index % 3 === 1) return { rel: "grade_green", sign: "add", row: [index, index % 5] };
  return { rel: "grade_bruised", sign: "add", row: [index, "hail"] };
}

function indices(count: number): readonly number[] {
  return Array.from({ length: count }, (_unused, offset) => offset + 1);
}

export function scheduleFor(count: number): readonly IArrivalBatch[] {
  const all = indices(count);
  const half = all.filter((index) => index % 2 === 1);
  return [
    [{ rel: "quarantined", sign: "add", row: ["weed"] }],
    [
      ...all.map(treeRow),
      ...all.map((index) => ({ rel: "sensor", sign: "add", row: [index, index % 4 !== 0] }) as IArrivalRow),
      ...all.map(orchardJsonRow),
      ...all.map(orchardListRow),
    ],
    all.flatMap((index) => [pickRow(index), ...orchardTagSourceRows(index)]),
    all.map(hostAnswerRow),
    [{ rel: "interval", sign: "add", row: [1, 1_800_000] }],
    all.map(gradeRow),
    all.map((index) => ({ rel: "retire_event", sign: "add", row: [index] }) as IArrivalRow),
    half.map((index) => ({ ...treeRow(index), sign: "del" }) as IArrivalRow),
    [],
  ];
}

/** The perturbed run: the many-row schedule plus one tick naming a tree the
 *  generator never produced. A runner that replayed a canned answer instead of
 *  computing deltas from the rules cannot follow this. */
export function perturbedSchedule(count: number): readonly IArrivalBatch[] {
  const extra = 999;
  return [
    ...scheduleFor(count),
    [treeRow(extra), { rel: "sensor", sign: "add", row: [extra, true] }, orchardJsonRow(extra), orchardListRow(extra)],
    [pickRow(extra), ...orchardTagSourceRows(extra)],
    [hostAnswerRow(extra), gradeRow(extra)],
    [{ ...treeRow(extra), sign: "del" }],
    [],
  ];
}

function main(): void {
  const outDir = process.argv[2];
  if (outDir === undefined) {
    process.stderr.write("usage: golden-schedules.ts <outDir>\n");
    process.exitCode = 2;
    return;
  }
  mkdirSync(outDir, { recursive: true });
  const written: string[] = [];
  for (const [name, schedule] of [
    ["zero", scheduleFor(0)],
    ["one", scheduleFor(1)],
    ["many", scheduleFor(100)],
    ["perturbed", perturbedSchedule(100)],
  ] as const) {
    const path = join(outDir, `golden-flex.${name}.json`);
    writeFileSync(path, `${JSON.stringify(schedule)}\n`, "utf8");
    written.push(path);
  }
  process.stdout.write(`${written.join("\n")}\n`);
}

if (process.argv[1]?.endsWith("golden-schedules.ts")) main();
