/**
 * tests/3_runtime.test.ts - M2 gate: DlRuntime commit()/rows()/deltas$ over the
 * SQLite tick store. One golden (add/noop/remove) proves idempotence + weight-retract
 * end to end; the rest are unit tests reaching what the golden can't isolate (the diff
 * combinatorics' noop case, retention 0/1, deltas$ fan-out, the sync-settle invariant).
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";

import { diffDerivedRel } from "../src/3_runtime.ts";
import type { DeltaEvent } from "../tasks.d.ts";
import {
  bootFixture,
  bootParentFixture,
  cleanupDbFile,
  deltaDump,
  edbBatch,
  singleEdbRelProgram,
} from "./1_helpers_db.ts";

const GOLDEN_PATH = path.join(import.meta.dirname, "..", "fixtures", "golden", "runtime.add-noop-remove.json");

// The three parent rows every M2 test scenario re-uses (chain: alice -> bob -> carol -> dana).
const PARENT_ROWS = [
  { child_name: "alice", parent_name: "bob" },
  { child_name: "bob", parent_name: "carol" },
  { child_name: "carol", parent_name: "dana" },
];

test("golden add/noop/remove: idempotent re-commit is zero deltas, removal retracts through weights", async () => {
  const { rt, dbPath } = await bootParentFixture();
  try {
    const report1 = await rt.commit(edbBatch({ parent: PARENT_ROWS }));
    assert.equal(report1.tick, 1);
    assert.deepEqual(report1.changed, [
      ["grandparent", 2],
      ["parent", 3],
    ]);

    // idempotent re-commit of the identical rows: the pre-check recognizes every
    // candidate already exists, so both the EDB write and the derived diff are zero.
    const report2 = await rt.commit(edbBatch({ parent: PARENT_ROWS }));
    assert.equal(report2.tick, 2);
    assert.deepEqual(report2.changed, []);

    // retract the middle link: both derived grandparent rows depended on it.
    const report3 = await rt.commit(edbBatch({}, { parent: [{ child_name: "bob", parent_name: "carol" }] }));
    assert.equal(report3.tick, 3);
    assert.deepEqual(report3.changed, [
      ["grandparent", -2],
      ["parent", -1],
    ]);

    const snapshot = [report1, report2, report3, await deltaDump(dbPath)];

    if (process.env.REGEN_GOLDEN === "1") {
      fs.mkdirSync(path.dirname(GOLDEN_PATH), { recursive: true });
      fs.writeFileSync(GOLDEN_PATH, `${JSON.stringify(snapshot, null, 2)}\n`);
    }
    const expected: unknown = JSON.parse(fs.readFileSync(GOLDEN_PATH, "utf8"));
    assert.deepEqual(snapshot, expected);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("derived diff enumerates the membership combinatorics: in-old/in-new = noop, in-old-only = retract, in-new-only = insert", () => {
  const inBoth = { name: "shared" };
  const oldOnly = { name: "retracted" };
  const newOnly = { name: "inserted" };

  const diff = diffDerivedRel([inBoth, oldOnly], [inBoth, newOnly]);

  assert.deepEqual(diff.insert, [newOnly]); // in-new-only
  assert.deepEqual(diff.retract, [oldOnly]); // in-old-only
  // inBoth appears in neither result: the noop case the golden can't isolate.
  assert.ok(!diff.insert.includes(inBoth) && !diff.retract.includes(inBoth));
});

test("rel(0) scratch dies with its tick", async () => {
  const { rt, dbPath } = await bootFixture(singleEdbRelProgram("scratch", ["value"]), undefined, { scratch: 0 });
  try {
    const report = await rt.commit(edbBatch({ scratch: [{ value: 1 }, { value: 2 }] }));
    assert.deepEqual(report.changed, [["scratch", 2]]);

    assert.deepEqual(await rt.rows("scratch"), []);

    const deltas = await deltaDump(dbPath);
    assert.deepEqual(
      deltas.map((entry) => ({ rel: entry.rel, tick: entry.tick, weight: entry.weight })),
      [
        { rel: "scratch", tick: 1, weight: 1 },
        { rel: "scratch", tick: 1, weight: 1 },
      ],
    );
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("rel(1) keeps only the newest row", async () => {
  const { rt, dbPath } = await bootFixture(singleEdbRelProgram("latest", ["value"]), undefined, { latest: 1 });
  try {
    await rt.commit(edbBatch({ latest: [{ value: 1 }] }));
    assert.deepEqual(await rt.rows("latest"), [{ value: 1 }]);

    await rt.commit(edbBatch({ latest: [{ value: 2 }] }));
    const rows = await rt.rows("latest");
    assert.equal(rows.length, 1);
    assert.deepEqual(rows, [{ value: 2 }]);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("deltas$ carries the same inserts/retracts the tables saw", async () => {
  const { rt, dbPath } = await bootParentFixture();
  try {
    const events: DeltaEvent[] = [];
    const subscription = rt.deltas$.subscribe((event) => events.push(event));
    try {
      await rt.commit(edbBatch({ parent: PARENT_ROWS }));
      await rt.commit(edbBatch({ parent: PARENT_ROWS }));
      await rt.commit(edbBatch({}, { parent: [{ child_name: "bob", parent_name: "carol" }] }));
    } finally {
      subscription.unsubscribe();
    }

    const tick3GrandparentRetract = events.find((event) => event.tick === 3 && event.rel === "grandparent");
    assert.ok(tick3GrandparentRetract, "expected a grandparent DeltaEvent on tick 3");
    assert.equal(tick3GrandparentRetract?.inserts.length, 0);
    assert.equal(tick3GrandparentRetract?.retracts.length, 2);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("sync-settle: commit resolves with derived rows already queryable", async () => {
  const { rt, dbPath } = await bootParentFixture();
  try {
    await rt.commit(
      edbBatch({
        parent: [
          { child_name: "alice", parent_name: "bob" },
          { child_name: "bob", parent_name: "carol" },
        ],
      }),
    );

    // no timer/sleep: by the time commit()'s promise resolves, the derived fixpoint
    // has already settled synchronously (the sync-settle assertion inside
    // diffAgainstTables would have thrown otherwise).
    const grandparentRows = await rt.rows("grandparent");
    assert.deepEqual(grandparentRows, [{ grandchild_name: "alice", grandparent_name: "carol" }]);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});
