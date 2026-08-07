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
 *      + dispatch_manifest + leg 1        the composition scenario opens
 *   3  pick_event                         triggers every edge rule at once
 *      + leg 2                            the fold's second link
 *   4  __host_response_weigh              the host's answers (see below)
 *   5  interval(1, bucket)                the bind row
 *   6  grade_ripe / grade_green / grade_bruised   the enum's variant rels
 *      + dispatch variant                 the second enum's variant rels
 *      + dispatch_ack + dispatch_seal     the two `any` triggers, one tick
 *   7  retire_event                       the edge-arm match block
 *   8  DELETE half the trees              departures: finalize/1 fires here
 *   9  (empty)                            the settle tick departures land in
 *
 * THE FOLD IS TWO LINKS, ON TWO TICKS, AND BOTH NUMBERS ARE MEASURED WALLS.
 * ONE LINK PER TICK: a chain pushed inside a single batch runs to fixpoint in
 * the oracle and ONE round in the emitter, and later ticks never finish it.
 * TWO LINKS TOTAL: a third leg was written here first and never landed in the
 * emitter, on tick 6, 7 or 9 alike, while the oracle had it every time. A
 * two-rule program with the same fold shape chains to depth 4 with both doors
 * identical, so the wall belongs to something this program has and that one
 * does not; a negated level body (the reconcile-every-tick trigger) was the
 * first guess and it does NOT reproduce it. COMPOSE.md finding 1 carries the
 * diffs.
 *
 * THE `dispatch` VARIANT CONTENT IS UNIQUE PER INDEX. An enum variant rel is
 * keyed on its CONTENT columns, so two ids carrying equal content replace each
 * other -- which is what `grade` does above on purpose. Here the tag is joined
 * back to a per-id json route, so a replacement would silently shrink the
 * 100-row leg to a handful of tickets and grade the composition on almost
 * nothing.
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
function tree_row(index: number): IArrivalRow {
  const species = index % 3 === 0 ? "apple" : index % 3 === 1 ? "pear" : "weed";
  const site: StructValue = { label: `patch-${index % 4}`, at: { row: index % 5, col: index % 7 } };
  return { rel: "tree", sign: "add", row: [index, species, site as unknown as string] };
}

function orchard_document(index: number): Readonly<Record<string, unknown>> {
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

function orchard_json_row(index: number): IArrivalRow {
  return {
    rel: "orchard_json",
    sign: "add",
    row: [index, StructPlane.canonical_text(orchard_document(index))],
  };
}

function orchard_list_row(index: number): IArrivalRow {
  return {
    rel: "orchard_list",
    sign: "add",
    row: [index, StructPlane.canonical_text(["red", `tree-${index}`])],
  };
}

function orchard_tag_source_rows(index: number): readonly IArrivalRow[] {
  return [
    { rel: "orchard_tag_source", sign: "add", row: [index, "red"] },
    { rel: "orchard_tag_source", sign: "add", row: [index, `tree-${index}`] },
  ];
}

function sugar_of(index: number): number {
  return 10 + (index % 9);
}

function pick_row(index: number): IArrivalRow {
  const kilos = index % 2 === 0 ? 2.5 : 0.75;
  return { rel: "pick_event", sign: "add", row: [index, index % 2 === 0 ? "ada" : "bob", kilos, sugar_of(index)] };
}

function host_answer_row(index: number): IArrivalRow {
  const grams = sugar_of(index);
  return {
    rel: "__host_response_weigh",
    sign: "add",
    row: [`witness|weigh|tree_id:int=${index}|grams:int=${grams}`, 0, index, grams, `tree-${index}-at-${grams}g`],
  };
}

function grade_row(index: number): IArrivalRow {
  if (index % 3 === 0) return { rel: "grade_ripe", sign: "add", row: [index, sugar_of(index)] };
  if (index % 3 === 1) return { rel: "grade_green", sign: "add", row: [index, index % 5] };
  return { rel: "grade_bruised", sign: "add", row: [index, "hail"] };
}

/** Only the rail variant carries hops above two, which is what keeps the
 *  dispatch match block's three guards disjoint per ticket. */
function hops_of(index: number): number {
  return index % 3 === 2 ? 3 + (index % 2) : 1 + (index % 2);
}

function dispatch_manifest_row(index: number): IArrivalRow {
  const route_name = index % 3 === 0 ? "north" : index % 3 === 1 ? "south" : "east";
  return {
    rel: "dispatch_manifest",
    sign: "add",
    row: [
      index,
      StructPlane.canonical_text({
        crates: [index, index + 1],
        route: { hops: hops_of(index), name: route_name },
      }),
    ],
  };
}

/** Leg `step` of dispatch `index`: leg ids are index*10+step so the previous
 *  leg is nameable without a lookup, and step 1 points at 0 to say "first". */
function dispatch_leg_row(index: number, step: number): IArrivalRow {
  const origin: StructValue = { label: `patch-${index % 4}`, at: { row: index % 5, col: index % 7 } };
  return {
    rel: "dispatch_leg",
    sign: "add",
    row: [
      index * 10 + step,
      index,
      step === 1 ? 0 : index * 10 + (step - 1),
      index + step,
      origin as unknown as string,
    ],
  };
}

function dispatch_variant_row(index: number): IArrivalRow {
  if (index % 3 === 0) return { rel: "dispatch_air", sign: "add", row: [index, index] };
  if (index % 3 === 1) return { rel: "dispatch_road", sign: "add", row: [index, 1_000 + index] };
  return { rel: "dispatch_rail", sign: "add", row: [index, 2_000 + index] };
}

function indices(count: number): readonly number[] {
  return Array.from({ length: count }, (_unused, offset) => offset + 1);
}

export function schedule_for(count: number): readonly IArrivalBatch[] {
  const all = indices(count);
  const half = all.filter((index) => index % 2 === 1);
  return [
    [{ rel: "quarantined", sign: "add", row: ["weed"] }],
    [
      ...all.map(tree_row),
      ...all.map((index) => ({ rel: "sensor", sign: "add", row: [index, index % 4 !== 0] }) as IArrivalRow),
      ...all.map(orchard_json_row),
      ...all.map(orchard_list_row),
      ...all.map(dispatch_manifest_row),
      ...all.map((index) => dispatch_leg_row(index, 1)),
    ],
    all.flatMap((index) => [pick_row(index), ...orchard_tag_source_rows(index), dispatch_leg_row(index, 2)]),
    all.map(host_answer_row),
    [{ rel: "interval", sign: "add", row: [1, 1_800_000] }],
    all.flatMap((index) => [
      grade_row(index),
      dispatch_variant_row(index),
      { rel: "dispatch_ack", sign: "add", row: [index] } as IArrivalRow,
      { rel: "dispatch_seal", sign: "add", row: [index] } as IArrivalRow,
    ]),
    all.map((index) => ({ rel: "retire_event", sign: "add", row: [index] }) as IArrivalRow),
    half.map((index) => ({ ...tree_row(index), sign: "del" }) as IArrivalRow),
    [],
  ];
}

/** The perturbed run: the many-row schedule plus one tick naming a tree the
 *  generator never produced. A runner that replayed a canned answer instead of
 *  computing deltas from the rules cannot follow this. */
export function perturbed_schedule(count: number): readonly IArrivalBatch[] {
  const extra = 999;
  return [
    ...schedule_for(count),
    [
      tree_row(extra),
      { rel: "sensor", sign: "add", row: [extra, true] },
      orchard_json_row(extra),
      orchard_list_row(extra),
      dispatch_manifest_row(extra),
      dispatch_leg_row(extra, 1),
    ],
    [pick_row(extra), ...orchard_tag_source_rows(extra), dispatch_leg_row(extra, 2)],
    [
      host_answer_row(extra),
      grade_row(extra),
      dispatch_variant_row(extra),
      { rel: "dispatch_ack", sign: "add", row: [extra] },
      { rel: "dispatch_seal", sign: "add", row: [extra] },
    ],
    [{ ...tree_row(extra), sign: "del" }],
    [],
  ];
}

function main(): void {
  const out_dir = process.argv[2];
  if (out_dir === undefined) {
    process.stderr.write("usage: golden-schedules.ts <outDir>\n");
    process.exitCode = 2;
    return;
  }
  mkdirSync(out_dir, { recursive: true });
  const written: string[] = [];
  for (const [name, schedule] of [
    ["zero", schedule_for(0)],
    ["one", schedule_for(1)],
    ["many", schedule_for(100)],
    ["perturbed", perturbed_schedule(100)],
  ] as const) {
    const path = join(out_dir, `golden-flex.${name}.json`);
    writeFileSync(path, `${JSON.stringify(schedule)}\n`, "utf8");
    written.push(path);
  }
  process.stdout.write(`${written.join("\n")}\n`);
}

if (process.argv[1]?.endsWith("golden-schedules.ts")) main();
