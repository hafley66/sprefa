/**
 * goldenFlexServed.test.ts — the golden's E2E leg: golden-flex.dl6 loaded over
 * HTTP, driven over HTTP, read back over HTTP, and diffed against the oracle.
 *
 * The other three legs (scripts/golden-flex.sh) run the compiled module in
 * process against a schedule file. This one goes through the actual server: the
 * program is POSTed as text and compiled by the server's own door, arrivals
 * arrive as POST bodies, and the rows come back from `GET /idb/:rel`. That is
 * the integration surface, and it is where the parts that the in-process legs
 * can only simulate become real -- the sh host actually spawns `printf`, and its
 * answer actually re-enters the engine as an EDB arrival.
 *
 * THE GRADING IS TOTAL, not a prefix. The served run's own consumed schedule is
 * read back off the ticks (`scheduleFromTicks`: an arrival-target rel's boundary
 * delta at a tick IS the world's push at that tick) and replayed through the
 * oracle, so every column is compared -- including the witness digest the host
 * layer minted, which nothing outside the server chose.
 *
 * CROSS-CHECK WORTH NAMING: scripts/golden-schedules.ts SYNTHESIZES the host's
 * witness text for the in-process legs, from the emitted SQL's own concatenation
 * rule. This leg never does: the witness here is whatever the live host layer
 * produced. If the two ever disagreed, the schedule generator's rows would stop
 * joining and the in-process legs would lose their `weighed` rows. The two legs
 * therefore check each other.
 *
 * SABOTAGE RECEIPT (run 2026-07-30, reverted): changing the expected host label
 * to "tree-1-at-12g" (one gram off) turns the /idb assertion red, so the host
 * assertions read the real subprocess output rather than merely observing that
 * some row landed.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  logOfTicks,
  postArrivals,
  postProgram,
  request,
  scheduleFromTicks,
  startServed,
  tickEvents,
} from "./serveHelpers.ts";
import type { IArrivalBatch } from "../runtime/types.ts";

const GOLDEN_ORACLE_PL = fileURLToPath(
  new URL("../../prolog/compile/scripts/golden_oracle.pl", import.meta.url),
);

/**
 * The oracle through the GOLDEN door rather than `dl6_oracle.pl`'s. Two reasons,
 * both measured (and both written up in golden_oracle.pl's own header): that
 * door turns a JSON object arrival into the atom of a SWI dict's printed text,
 * so a struct-typed column cannot cross it at all, and it has no final-state leg.
 */
function goldenOracleLog(programSource: string, schedule: readonly IArrivalBatch[]): string {
  const workDir = mkdtempSync(join(tmpdir(), "golden-flex-oracle-"));
  const programPath = join(workDir, "program.dl6");
  const schedulePath = join(workDir, "schedule.json");
  writeFileSync(programPath, programSource, "utf8");
  writeFileSync(schedulePath, JSON.stringify(schedule), "utf8");
  const run = spawnSync(
    "swipl",
    ["-q", "-l", GOLDEN_ORACLE_PL, "-g", `oracle_ticklog('${programPath}', '${schedulePath}')`, "-g", "halt"],
    { encoding: "utf8" },
  );
  if (run.status !== 0) throw new Error(`golden_oracle failed (${run.status}): ${run.stderr}`);
  return run.stdout;
}

/**
 * FINDING, measured by this receipt: one `ITickOutcome` carries a struct column
 * in TWO different renderings. `outcome.line` (the graded tick log) canonicalizes
 * it -- sorted keys, the ruled `rendered_text` form -- while `outcome.deltas`
 * rows, and `GET /idb/:rel`, hand back the RAW stored text in the arrival's own
 * key order:
 *
 *   line    "tree":{"add":[[1,"pear",{"at":{"col":2,"row":1},"label":"p1"}]]}
 *   deltas  {"rel":"tree","row":[1,"pear","{\"label\":\"p1\",\"at\":{...}}"]}
 *
 * Nothing covered it: the struct fixtures never read `/idb`, and the `/idb`
 * receipts that do use a struct column (serveHost) get their value from a HOST,
 * whose stdout is already canonical. Consequence here: `scheduleFromTicks` --
 * the replay that makes served grading TOTAL -- loses the object, and the oracle
 * refuses the string with type_arrival_shape_mismatch. This revives it so the
 * replay is gradeable; the underlying disagreement is reported, not fixed.
 */
function reviveStructColumns(schedule: readonly IArrivalBatch[]): readonly IArrivalBatch[] {
  const revive = (value: unknown): unknown => {
    if (typeof value !== "string") return value;
    const text = value.trim();
    if (!text.startsWith("{") || !text.endsWith("}")) return value;
    try {
      const parsed: unknown = JSON.parse(text);
      return typeof parsed === "object" && parsed !== null ? parsed : value;
    } catch {
      return value;
    }
  };
  return schedule.map((batch) => batch.map((arrival) => ({ ...arrival, row: arrival.row.map(revive) as never })));
}

const GOLDEN_DL6 = fileURLToPath(new URL("../../dl/fixtures/golden-flex.dl6", import.meta.url));

/** A tree carries a two-deep struct: `tree.site` is a `patch`, whose `at` is a
 *  `plot`. The value arrives whole -- a braces literal cannot build one in a
 *  rule (json_value_expression), which the golden's header records. */
function treeRow(index: number): (string | number | object)[] {
  const species = index % 3 === 0 ? "apple" : index % 3 === 1 ? "pear" : "weed";
  return [index, species, { label: `patch-${index % 4}`, at: { row: index % 5, col: index % 7 } }];
}

interface RowsReply {
  readonly rows: readonly (readonly unknown[])[];
}

async function rowsOf(port: number, rel: string): Promise<RowsReply["rows"]> {
  const reply = await request(port, `/idb/${rel}`, "GET");
  assert.equal(reply.statusCode, 200, `GET /idb/${rel} -> ${reply.statusCode} ${reply.body}`);
  return (JSON.parse(reply.body) as RowsReply).rows;
}

async function waitUntil(predicate: () => Promise<boolean>, what: string, timeoutMs = 20_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!(await predicate())) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 20));
  }
}

test("golden-flex served: the live host runs, and the served tick log matches the oracle replayed on the served schedule", async () => {
  const source = readFileSync(GOLDEN_DL6, "utf8");
  const served = await startServed();
  try {
    const loaded = await postProgram(served.port, source);
    assert.equal(loaded.statusCode, 200, loaded.body);
    const plans = JSON.parse(loaded.body) as {
      readonly hosts: readonly string[];
      readonly binds: readonly { readonly name: string; readonly literals: readonly (string | number)[] }[];
      readonly arrivalTargets: readonly string[];
    };
    // The program's own rules said these; nothing out here chose them.
    assert.deepEqual(plans.hosts, ["weigh"]);
    assert.deepEqual(plans.binds, [{ name: "interval", literals: [1] }]);

    await postArrivals(served.port, [{ rel: "quarantined", sign: "add", row: ["weed"] }]);
    await postArrivals(served.port, [
      { rel: "tree", sign: "add", row: treeRow(1) as never },
      { rel: "tree", sign: "add", row: treeRow(2) as never },
      { rel: "tree", sign: "add", row: treeRow(3) as never },
      { rel: "sensor", sign: "add", row: [1, true] },
      { rel: "sensor", sign: "add", row: [2, true] },
      { rel: "sensor", sign: "add", row: [3, false] },
    ]);
    await postArrivals(served.port, [
      { rel: "pick_event", sign: "add", row: [1, "ada", 2.5, 11] },
      { rel: "pick_event", sign: "add", row: [2, "bob", 0.75, 12] },
    ]);

    // The host is the only asynchronous thing here: two demands, two subprocesses.
    await waitUntil(async () => (await rowsOf(served.port, "weighed")).length === 2, "both weigh answers to land");

    // The value plane, two hops deep, read back over http.
    const trees = await rowsOf(served.port, "tree");
    assert.equal(trees.length, 3);
    // FINDING, measured here: the /idb boundary renders a struct column as the
    // arrival's OWN key order, `{"label":...,"at":{"row":...,"col":...}}`, while
    // the tick log renders the ruled canonical form (sorted keys). Both are
    // strings at this door; only one is canonical. Asserted as-is rather than
    // normalized, so the day the boundary read is canonicalized this line says so.
    assert.deepEqual(trees.find((row) => row[0] === 1), [
      1,
      "pear",
      '{"label":"patch-1","at":{"row":1,"col":1}}',
    ]);
    const patches = await rowsOf(served.port, "patch");
    const plots = await rowsOf(served.port, "plot");
    assert.ok(patches.length > 0, "the depth-1 dictionary rel is public and populated");
    assert.ok(plots.length > 0, "the depth-2 dictionary rel is public and populated");
    assert.deepEqual((await rowsOf(served.port, "tree_label")).find((row) => row[0] === 1), [1, "patch-1"]);

    // The host's real stdout. `weed` is quarantined so tree 3 never becomes
    // pickable, and the two survivors are weighed at sum(sugar) grams.
    const weighed = [...(await rowsOf(served.port, "weighed"))].sort((left, right) => Number(left[0]) - Number(right[0]));
    assert.deepEqual(weighed, [
      [1, 11, "tree-1-at-11g"],
      [2, 12, "tree-2-at-12g"],
    ]);

    // Aggregates, retention, kwargs, negation and the keyed edge head, all read
    // back through the same door.
    assert.deepEqual((await rowsOf(served.port, "pick_stats")).find((row) => row[0] === 1), [1, 1, 11, 11, 11]);
    assert.deepEqual((await rowsOf(served.port, "picked_by_ada")), [[1]]);
    // tree 2 IS the quarantined species, so it is picked but never pickable:
    // `last_picker` samples `latest(pickable(...))` and gets only tree 1, while
    // `weighed` rides `pick_stats` and gets both. Two rules, two answers, one
    // world -- which is the kind of interaction a one-construct fixture cannot
    // produce at all.
    assert.deepEqual(await rowsOf(served.port, "last_picker"), [[1, "ada"]]);
    assert.deepEqual(
      [...(await rowsOf(served.port, "pickable"))].map((row) => row[0]).sort(),
      [1, 3],
      "the quarantined species is excluded, the other two are not",
    );

    // TOTAL grading: replay the server's own consumed schedule through the oracle.
    const outcomes = tickEvents(served.events);
    const replayed = reviveStructColumns(scheduleFromTicks(outcomes, plans.arrivalTargets));
    assert.equal(logOfTicks(outcomes), goldenOracleLog(source, replayed));
  } finally {
    await served.stop();
  }
});
