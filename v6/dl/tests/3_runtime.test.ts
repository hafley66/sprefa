/**
 * tests/3_runtime.test.ts - M2 gate: DlRuntime commit()/rows()/deltas$ over the
 * SQLite tick store. One golden (add/noop/remove) proves idempotence + weight-retract
 * end to end; the rest are unit tests reaching what the golden can't isolate (the diff
 * combinatorics' noop case, retention 0/1, deltas$ fan-out, settle-before-resolve).
 *
 * M11 tail (2026-07-25, the lowerSql swap): recursion, body literals through the string
 * dictionary, and stratified negation, all at the DlRuntime level. The three recursion
 * tests could not have been written before the swap — boot() threw on a recursive
 * stratum — and the literal test is the rail for `internProgramForStorage` (bypass it
 * and that test alone goes red, verified 2026-07-25).
 *
 * F1 SABOTAGE RECEIPT (2026-07-28 cleanup audit): the pre-existing "rowsForPath ==
 * rows().filter(path)" test asserts a SET-EQUALITY property that the O(n^2) shape the
 * ingest-perf arc removed (selectAll + a JS .filter) ALSO satisfies -- the audit's own
 * probe reverted rowsForPath$ to exactly that shape and every 3_runtime/4_ingest test
 * stayed green. The new "F1: rowsForPath compiles a WHERE-scoped SELECT..." test
 * instead watches the SQL TEXT over the sql trace seam (0_trace.ts's `rawSql`,
 * PERF_CHANNEL_NAMES.sql — a channel a test may subscribe to directly, independent of
 * DL_PERF_LOG/installFromEnv, per diagnostics_channel's process-global-registry
 * design). Probe run in this worktree: swapped rowsForPath$'s body for
 * `selectAll(...).pipe(map(rows => rows.filter(row => row.path === path)))` (the exact
 * shape the audit used) -- the F1 test went RED, captured statement
 * `SELECT path,kind FROM rel_node` (no WHERE, the unscoped decode view). Restored the
 * real `selectByColumn` body -- F1 test green again, statement
 * `SELECT path,kind FROM relbase_node WHERE path = <id>`. rawSql is a NEW seam (F1's
 * own addition, 0_trace.ts + 0_types.ts's IPerfTrace): every SQL statement
 * 3_runtime.ts's `execute$` runs now also publishes its raw text on `sprefa:sql` with
 * no `tick`, which `onSqlMessage`'s tick-keyed aggregator explicitly ignores (hardened
 * in the same change) so this can never pollute a real DL_PERF_LOG JSONL line.
 *
 * COUNT-LAYER RECEIPT (2026-07-28, CLAUDE.md's "any path that was ever O(n^2) gets a
 * COUNT/PLAN test" law): F1's own test only ever watched the SQL TEXT; it never
 * proved that text actually EXECUTES as an index lookup rather than a scan, nor that
 * the row count returned stays flat as the total table grows. Two tests below close
 * that: "F1-COUNT: rowsForPath's compiled SELECT plans..." runs the exact statement
 * F1 captures through `EXPLAIN QUERY PLAN`, on a second short-lived connection to the
 * same db file (1_helpers_db.ts's `deltaDump` pattern), and asserts every plan step is
 * a SEARCH, never a SCAN. "rowsForPath returns only the requested path's rows out of
 * many..." seeds 50 distinct paths (100 rows) and reruns the same plan check at that
 * size, so the proof holds with real row-count pressure, not just the 3-path fixture.
 * SABOTAGE (same probe as F1's own receipt above, reapplied here): swapping
 * `rowsForPath$`'s body back to `selectAll(...).pipe(map(rows => rows.filter(...)))`
 * flipped BOTH new tests RED, in both cases: the plan's top step became
 * `SCAN relbase_node` (a whole-table scan through the view's underlying table), where
 * the real body produces `SEARCH relbase_node USING COVERING INDEX
 * sqlite_autoindex_relbase_node_1 (path=?)`. Restoring the real body turned both
 * green again. tests/4_ingest.test.ts carries the third layer (statement COUNT, not
 * just plan shape): rowsForPath fires exactly `spineDeclsLocal.length`
 * times per `ingestFile` call, constant at 5 files and at 20 files even though the
 * table's total row count keeps growing underneath it.
 */
import assert from "node:assert/strict";
import diagnostics_channel from "node:diagnostics_channel";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { open_db } from "sprefa-store-engine/src/engine/lib.ts";

import { diffDerivedRel, DlRuntime, execute$ } from "../src/3_runtime.ts";
import { PERF_CHANNEL_NAMES } from "../src/0_trace.ts";
import type { DeltaEvent, Row } from "../tasks.d.ts";
import {
  bootFixture,
  bootParentFixture,
  buildAncestorProgram,
  buildLiteralAndNegationProgram,
  cleanupDbFile,
  deltaDump,
  edbBatch,
  fakeBridgeOk,
  freshDbPath,
  rowsOf,
  singleEdbRelProgram,
} from "./1_helpers_db.ts";

const GOLDEN_PATH = path.join(import.meta.dirname, "..", "fixtures", "golden", "runtime.add-noop-remove.json");

function byJson(a: unknown, b: unknown): number {
  const left = JSON.stringify(a);
  const right = JSON.stringify(b);
  return left < right ? -1 : left > right ? 1 : 0;
}

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
  const { rt, dbPath } = await bootFixture(
    singleEdbRelProgram("scratch", ["value"]),
    undefined,
    { scratch: 0 },
    { scratch: ["int"] },
  );
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
  const { rt, dbPath } = await bootFixture(
    singleEdbRelProgram("latest", ["value"]),
    undefined,
    { latest: 1 },
    { latest: ["int"] },
  );
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

test("deltas$ reports the same inserts/retracts the tables saw", async () => {
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

    // no timer/sleep: by the time commit()'s promise resolves the derived fixpoint has
    // already run, because evalProgramSql is awaited inside the tick's own concatMap
    // stage (applyDerivedTxn) before reportsSubject emits the report commit() waits on.
    const grandparentRows = await rt.rows("grandparent");
    assert.deepEqual(grandparentRows, [{ grandchild_name: "alice", grandparent_name: "carol" }]);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

// ─────────────────────────────────────────────────────────────────────────────
// rowsForPath (ingest-perf arc): the WHERE-scoped read 4_ingest.ts's per-file diff
// uses instead of `rows()` + a JS filter. Proof: for a rel with several distinct
// `path` values committed in one db, `rowsForPath(rel, path)` returns exactly the
// same row set as `(await rows(rel)).filter(row => row.path === path)`, for every
// path, and refuses cleanly outside its contract (unknown rel, no `path` column).
// ─────────────────────────────────────────────────────────────────────────────

test("rowsForPath == rows().filter(path) for every distinct path in a multi-file db", async () => {
  const { rt, dbPath } = await bootFixture(singleEdbRelProgram("node", ["path", "kind"]));
  try {
    const paths = ["a.ts", "b.ts", "c.ts"];
    await rt.commit(
      edbBatch({
        node: [
          { path: "a.ts", kind: "call" },
          { path: "a.ts", kind: "ident" },
          { path: "b.ts", kind: "call" },
          { path: "c.ts", kind: "string" },
        ],
      }),
    );

    const allRows = await rt.rows("node");
    for (const path of paths) {
      const scoped = (await rt.rowsForPath("node", path)).sort(byJson);
      const unscoped = allRows.filter((row) => row.path === path).sort(byJson);
      assert.deepEqual(scoped, unscoped, `mismatch for path '${path}'`);
    }

    // A path with zero committed rows: both reads agree on empty.
    assert.deepEqual(await rt.rowsForPath("node", "never-ingested.ts"), []);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("rowsForPath throws on an unknown rel", async () => {
  const { rt, dbPath } = await bootFixture(singleEdbRelProgram("node", ["path", "kind"]));
  try {
    await assert.rejects(() => rt.rowsForPath("no_such_rel", "a.ts"), /unknown rel/);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("rowsForPath throws when the rel has no `path` column", async () => {
  const { rt, dbPath } = await bootParentFixture();
  try {
    await assert.rejects(() => rt.rowsForPath("parent", "a.ts"), /no column 'path'/);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

// F1 (audit finding): the previous test proves a SET-EQUALITY property that the exact
// O(n^2) shape the perf arc removed (selectAll + a JS filter) also has -- reverting
// rowsForPath$ to that shape left every 3_runtime/4_ingest test green (the audit's own
// probe). This test instead watches the SQL TEXT rowsForPath actually runs, over the
// sql trace seam (0_trace.ts's rawSql, PERF_CHANNEL_NAMES.sql): a WHERE-scoped read
// against relbase_<rel> is a different, discriminating string from an unscoped
// `rel_<rel>` decode-view scan. Sabotage receipt in this file's header docblock.
test("F1: rowsForPath compiles a WHERE-scoped SELECT against relbase_<rel>, never an unscoped whole-table scan", async () => {
  const { rt, dbPath } = await bootFixture(singleEdbRelProgram("node", ["path", "kind"]));
  const sqlChannel = diagnostics_channel.channel(PERF_CHANNEL_NAMES.sql);
  try {
    await rt.commit(
      edbBatch({
        node: [
          { path: "a.ts", kind: "call" },
          { path: "b.ts", kind: "call" },
        ],
      }),
    );

    const statements: string[] = [];
    const listener = (message: unknown): void => {
      const { sql } = message as { sql?: unknown };
      if (typeof sql === "string") statements.push(sql);
    };
    sqlChannel.subscribe(listener);
    try {
      await rt.rowsForPath("node", "a.ts");
    } finally {
      sqlChannel.unsubscribe(listener);
    }

    assert.equal(statements.length, 1, `expected exactly one SQL statement, saw ${statements.length}: ${statements.join(" | ")}`);
    assert.match(statements[0]!, /FROM relbase_node WHERE path = \d+/);
    assert.ok(
      !/FROM rel_node\b/.test(statements[0]!),
      `rowsForPath must not read through the unscoped rel_<rel> decode view; saw: ${statements[0]}`,
    );
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

/** Runs `sql` through `EXPLAIN QUERY PLAN` on a fresh short-lived connection to
 *  `dbPath` (same driver seam, same pattern as 1_helpers_db.ts's `deltaDump`: open,
 *  read, close before returning) and returns each step's `detail` text. */
async function explainQueryPlan(dbPath: string, sql: string): Promise<string[]> {
  const db = open_db(`file:${dbPath}`);
  try {
    const result = await db.execute(`EXPLAIN QUERY PLAN ${sql}`);
    return result.rows.map((row) => String(row.detail));
  } finally {
    db.close();
  }
}

// COUNT-LAYER test 1 (CLAUDE.md's O(n^2)-path law, sabotage receipt in this file's
// header docblock): F1 above only ever watched the SQL TEXT. This test watches
// whether SQLite actually EXECUTES that text as an index lookup: it runs the exact
// statement rowsForPath compiles through EXPLAIN QUERY PLAN and asserts every step
// is a SEARCH -- never a SCAN, which is the plan shape the O(n^2) unscoped-read-then-
// JS-filter body would produce (a bare `selectAll` has no WHERE clause for the
// planner to push into an index seek at all).
test("F1-COUNT: rowsForPath's compiled SELECT plans as SEARCH against relbase_node, never SCAN", async () => {
  const { rt, dbPath } = await bootFixture(singleEdbRelProgram("node", ["path", "kind"]));
  const sqlChannel = diagnostics_channel.channel(PERF_CHANNEL_NAMES.sql);
  try {
    await rt.commit(
      edbBatch({
        node: [
          { path: "a.ts", kind: "call" },
          { path: "b.ts", kind: "call" },
        ],
      }),
    );

    const statements: string[] = [];
    const listener = (message: unknown): void => {
      const { sql } = message as { sql?: unknown };
      if (typeof sql === "string") statements.push(sql);
    };
    sqlChannel.subscribe(listener);
    try {
      await rt.rowsForPath("node", "a.ts");
    } finally {
      sqlChannel.unsubscribe(listener);
    }
    assert.equal(statements.length, 1, `expected exactly one SQL statement, saw ${statements.length}`);

    const planSteps = await explainQueryPlan(dbPath, statements[0]!);
    assert.ok(planSteps.length > 0, "EXPLAIN QUERY PLAN returned no steps at all");
    assert.ok(
      planSteps.some((detail) => /^SEARCH relbase_node USING (COVERING )?INDEX/.test(detail)),
      `expected an index SEARCH step against relbase_node; got: ${JSON.stringify(planSteps)}`,
    );
    assert.ok(
      !planSteps.some((detail) => /^SCAN\b/.test(detail)),
      `rowsForPath's plan must never SCAN a whole table; got: ${JSON.stringify(planSteps)}`,
    );
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

// COUNT-LAYER test 3 (rows-touched bound): seeds MANY distinct paths so the "rows
// returned" and "rows the plan structurally could have visited" properties both get
// checked under real row-count pressure, not just the 2-path fixture above.
test("rowsForPath returns only the requested path's rows out of many, and its plan proves it never visited the rest", async () => {
  const { rt, dbPath } = await bootFixture(singleEdbRelProgram("node", ["path", "kind"]));
  const sqlChannel = diagnostics_channel.channel(PERF_CHANNEL_NAMES.sql);
  try {
    const PATH_COUNT = 50;
    const seedRows: Row[] = [];
    for (let index = 0; index < PATH_COUNT; index++) {
      seedRows.push({ path: `file_${index}.ts`, kind: "call" });
      seedRows.push({ path: `file_${index}.ts`, kind: "ident" });
    }
    await rt.commit(edbBatch({ node: seedRows }));

    const targetPath = "file_25.ts";
    const statements: string[] = [];
    const listener = (message: unknown): void => {
      const { sql } = message as { sql?: unknown };
      if (typeof sql === "string") statements.push(sql);
    };
    sqlChannel.subscribe(listener);
    let scoped: Row[];
    try {
      scoped = await rt.rowsForPath("node", targetPath);
    } finally {
      sqlChannel.unsubscribe(listener);
    }

    // Rows-touched: exactly the 2 rows for this one path out of 100 committed rows
    // across 50 paths, never a row from any other path.
    assert.deepEqual(
      scoped.slice().sort(byJson),
      [
        { path: targetPath, kind: "call" },
        { path: targetPath, kind: "ident" },
      ].sort(byJson),
    );
    assert.ok(
      scoped.every((row) => row.path === targetPath),
      `expected every returned row to belong to '${targetPath}'; saw: ${JSON.stringify(scoped)}`,
    );

    // Plan proof, same technique as F1-COUNT above, at this larger size: a SEARCH,
    // never a SCAN -- structurally, the query never touched the other 49 paths.
    assert.equal(statements.length, 1);
    const planSteps = await explainQueryPlan(dbPath, statements[0]!);
    assert.ok(
      !planSteps.some((detail) => /^SCAN\b/.test(detail)),
      `rowsForPath must never SCAN with ${PATH_COUNT} distinct paths present; got: ${JSON.stringify(planSteps)}`,
    );
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

// ─────────────────────────────────────────────────────────────────────────────
// M11 (2026-07-25): the SQL fixpoint IS the evaluator. Everything below was
// structurally unreachable before the swap — DlRuntime.boot threw
// "recursive strata not in this slice" on any recursive program, so the engine's
// headline feature (recursion) had no dl-level test at all.
// ─────────────────────────────────────────────────────────────────────────────

test("recursive stratum: transitive closure over the parent chain settles inside one tick", async () => {
  const { rt, dbPath } = await bootFixture(buildAncestorProgram());
  try {
    const report = await rt.commit(edbBatch({ parent: PARENT_ROWS }));
    // 3 base edges + 2 two-hop + 1 three-hop = 6 closure rows, from 3 EDB rows.
    assert.deepEqual(report.changed, [
      ["ancestor", 6],
      ["parent", 3],
    ]);
    assert.deepEqual(await rowsOf(rt, "ancestor"), [
      { descendant_name: "alice", ancestor_name: "bob" },
      { descendant_name: "alice", ancestor_name: "carol" },
      { descendant_name: "alice", ancestor_name: "dana" },
      { descendant_name: "bob", ancestor_name: "carol" },
      { descendant_name: "bob", ancestor_name: "dana" },
      { descendant_name: "carol", ancestor_name: "dana" },
    ]);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("recursive stratum RETRACTS: killing the middle link retracts all 4 rows it supported, through weights", async () => {
  const { rt, dbPath } = await bootFixture(buildAncestorProgram());
  try {
    await rt.commit(edbBatch({ parent: PARENT_ROWS }));

    // bob -> carol is the sole support for 4 of the 6 closure rows: (alice,carol),
    // (alice,dana), (bob,carol), (bob,dana). The 2 survivors (alice,bob) and
    // (carol,dana) are supported by edges the retraction never touched — a full
    // recompute that "worked" by clearing the rel would show -6, not -4.
    const report = await rt.commit(edbBatch({}, { parent: [{ child_name: "bob", parent_name: "carol" }] }));
    assert.deepEqual(report.changed, [
      ["ancestor", -4],
      ["parent", -1],
    ]);
    assert.deepEqual(await rowsOf(rt, "ancestor"), [
      { descendant_name: "alice", ancestor_name: "bob" },
      { descendant_name: "carol", ancestor_name: "dana" },
    ]);

    // ...and the Z-set log carries it: exactly 4 weight -1 ancestor rows on tick 2,
    // each one the digest of a row that was +1 on tick 1 (retraction through weights,
    // not a table rewrite).
    const deltas = await deltaDump(dbPath);
    const inserted = deltas.filter((d) => d.rel === "ancestor" && d.tick === 1 && d.weight === 1);
    const retracted = deltas.filter((d) => d.rel === "ancestor" && d.tick === 2 && d.weight === -1);
    assert.equal(inserted.length, 6);
    assert.equal(retracted.length, 4);
    const insertedDigests = new Set(inserted.map((d) => d.row_digest));
    for (const entry of retracted) {
      assert.ok(insertedDigests.has(entry.row_digest), `retracted digest ${entry.row_digest} was never inserted`);
    }
    // Net weight per digest is 0 for the 4 dead rows, +1 for the 2 survivors.
    const netByDigest = new Map<number, number>();
    for (const entry of deltas.filter((d) => d.rel === "ancestor")) {
      netByDigest.set(entry.row_digest, (netByDigest.get(entry.row_digest) ?? 0) + entry.weight);
    }
    assert.equal([...netByDigest.values()].filter((w) => w === 0).length, 4);
    assert.equal([...netByDigest.values()].filter((w) => w === 1).length, 2);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("recursive stratum re-adds: restoring the retracted link re-derives every row it supported", async () => {
  const { rt, dbPath } = await bootFixture(buildAncestorProgram());
  const middleLink = { child_name: "bob", parent_name: "carol" };
  try {
    await rt.commit(edbBatch({ parent: PARENT_ROWS }));
    await rt.commit(edbBatch({}, { parent: [middleLink] }));
    const report = await rt.commit(edbBatch({ parent: [middleLink] }));
    assert.deepEqual(report.changed, [
      ["ancestor", 4],
      ["parent", 1],
    ]);
    assert.equal((await rt.rows("ancestor")).length, 6);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("a text literal in a rule body matches through the string dictionary (the interning crossing)", async () => {
  const { rt, dbPath } = await bootFixture(buildLiteralAndNegationProgram());
  try {
    await rt.commit(edbBatch({ parent: PARENT_ROWS }));
    // parent(child_name, "bob") selects one row. The literal is a surface STRING while
    // relbase_parent.parent_name holds a dictionary id, so an un-interned literal
    // compiles to `parent_name = 'bob'` and matches nothing — this asserting [] instead
    // of alice is exactly the failure internProgramForStorage exists to prevent.
    assert.deepEqual(await rowsOf(rt, "child_of_bob"), [{ child_name: "alice" }]);

    // ...and it retracts with its support.
    await rt.commit(edbBatch({}, { parent: [{ child_name: "alice", parent_name: "bob" }] }));
    assert.deepEqual(await rowsOf(rt, "child_of_bob"), []);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("stratified negation: root(name) <- parent(_, name), !parent(name, _)", async () => {
  const { rt, dbPath } = await bootFixture(buildLiteralAndNegationProgram());
  try {
    await rt.commit(edbBatch({ parent: PARENT_ROWS }));
    // dana is a parent_name and never a child_name: the only root of the chain.
    assert.deepEqual(await rowsOf(rt, "root"), [{ name: "dana" }]);

    // Give dana a parent: the negated atom now matches, so the root row retracts and
    // the new top (eve) takes its place — negation reacting in both directions.
    await rt.commit(edbBatch({ parent: [{ child_name: "dana", parent_name: "eve" }] }));
    assert.deepEqual(await rowsOf(rt, "root"), [{ name: "eve" }]);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

// ─────────────────────────────────────────────────────────────────────────────
// The store's Z-set fact plane, wired (2026-07-25). Every fact now has an integer
// address, `cascade.key(rel_tag, row_id)`, so `cx_row`/`cx_dep` can hold the model and
// its support graph, and `cascade.retract_dred` can retract WITHOUT recomputing.
//
// These tests are the gate before the tick is allowed to stop recomputing: the graph's
// answer is checked against the recompute answer on the same input, cyclic case included.
// ─────────────────────────────────────────────────────────────────────────────

test("support graph IS sound for a single-atom rule: the row dies with its only parent", async () => {
  const { rt, dbPath } = await bootFixture(buildLiteralAndNegationProgram());
  try {
    await rt.commit(edbBatch({ parent: PARENT_ROWS }));
    assert.deepEqual(await rowsOf(rt, "child_of_bob"), [{ child_name: "alice" }]);

    // `child_of_bob(child_name) <- parent(child_name, "bob")` has ONE positive body atom,
    // so a binary edge expresses its support exactly: that parent row alone derives the
    // head row. Retracting it leaves the head with no live parent, and DRed's rederive
    // phase finds nothing to bring it back.
    const { dead } = await rt.retractThroughSupport("parent", [{ child_name: "alice", parent_name: "bob" }]);
    assert.deepEqual(
      dead.map((fact) => fact.rel).sort(),
      ["child_of_bob", "parent"],
    );
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("KNOWN LIMIT: conjunctive (join) support is a hypergraph, cx_dep is a binary graph", async () => {
  const { rt, dbPath } = await bootFixture(buildAncestorProgram());
  try {
    await rt.commit(edbBatch({ parent: PARENT_ROWS }));
    const { dead } = await rt.retractThroughSupport("parent", [{ child_name: "bob", parent_name: "carol" }]);
    const deadAncestors = dead
      .filter((fact) => fact.rel === "ancestor")
      .map((fact) => `${String(fact.row.descendant_name)}->${String(fact.row.ancestor_name)}`)
      .sort();

    // RECOMPUTE says 4 rows die here (proven by the "recursive stratum RETRACTS" test
    // above: ["ancestor", -4]). The support graph says 1. This test pins the GAP, so the
    // day it closes we find out from the suite instead of from a wrong answer in
    // production.
    //
    // Why: `ancestor(d,a) <- parent(d,mid), ancestor(mid,a)` needs BOTH body rows. Emitted
    // as two independent binary edges, each one reads as "this parent alone suffices". DRed
    // kills the forward cone and then rederives anything still reachable from a survivor,
    // so ancestor(alice,carol) comes back off the surviving parent(alice,bob) edge even
    // though its co-body ancestor(bob,carol) is dead. cascade's own tests only ever load
    // plain reachability graphs (tests/engine/golden.test.ts: rows + edges, R->A->B), so
    // the binary model was never wrong for cascade; it is wrong for a JOIN.
    //
    // The tick therefore still recomputes. Closing this needs a decision on encoding
    // (reify each derivation as its own node and count live derivations per head) or on
    // scope (use the fact plane only for the disjunctive, tree-shaped cascades it fits).
    assert.deepEqual(deadAncestors, ["bob->carol"]);
  } finally {
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("support coverage is reported, not assumed: negation and aggregate heads carry no edges", async () => {
  const covered = await bootFixture(buildAncestorProgram());
  try {
    // Pure positive joins: fully covered, so the graph may be trusted for every head.
    assert.deepEqual(covered.rt.supportCoverageGaps(), []);
  } finally {
    await covered.rt.dispose();
    cleanupDbFile(covered.dbPath);
  }

  const gapped = await bootFixture(buildLiteralAndNegationProgram());
  try {
    const gaps = gapped.rt.supportCoverageGaps();
    // `root` has a negated body predicate: a row appearing in the negated rel DESTROYS a
    // derivation rather than adding one, so counting/DRed over that edge is unsound and
    // the pass refuses to emit it. `child_of_bob` is a pure positive join and is covered.
    assert.deepEqual(
      gaps.map((gap) => gap.head),
      ["root"],
    );
    assert.match(gaps[0]!.reason, /negated body predicate/);
  } finally {
    await gapped.rt.dispose();
    cleanupDbFile(gapped.dbPath);
  }
});

// ─────────────────────────────────────────────────────────────────────────────
// F7 (docs/failure-modes.md class 36): a non-finite number reaching the storage-plane
// splice site used to become the literal text "NaN" inside SQL, unquoted, which SQLite
// reads as a column reference and fails with the unrelated-looking "no such column:
// NaN" -- 4_hosts.test.ts's regression tests fix the ghcacher-shaped ROOT CAUSE (a
// host-output-parsing bug that could produce that NaN in the first place); the two
// tests below are the defense-in-depth layer named in the incident's fix requirements:
// a guard at the encode splice site (named rel + column, before any SQL runs), and
// execute$ carrying the failing statement's own text on ANY SQL failure, not just this
// one -- the incident's own postmortem could not see the statement at all.
// ─────────────────────────────────────────────────────────────────────────────

/** No arbitrary sleeps: retries `check` on a short interval until it returns a defined
 *  value or the attempt budget runs out. */
async function waitForDefined<T>(check: () => T | undefined, attempts = 50, intervalMs = 20): Promise<T> {
  for (let attempt = 0; attempt < attempts; attempt++) {
    const result = check();
    if (result !== undefined) return result;
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(`waitForDefined: condition not met after ${attempts * intervalMs}ms`);
}

test("commit's guard rejects a non-finite number before it reaches SQL text, naming the rel and column", async () => {
  // Booted by hand rather than bootFixture: an exception raised inside the tick
  // pipeline is architecturally FATAL to the shared deltas$ stream (main.ts's own
  // documented behavior -- "a fault raised later by the tick loop... stays on the
  // stream," confirmed in the F7 incident). Since the F2 fix (see the dedicated F2
  // test below) the SAME fault also settles the in-flight commit() as a rejection,
  // via reportsSubject keyed by commit id -- this test still asserts the guard's
  // message off deltas$ directly, which is the distinct thing it was written to pin
  // (the error text itself, independent of which channel delivers it).
  const dbPath = freshDbPath();
  const bridgeOk = fakeBridgeOk(singleEdbRelProgram("status_rel", ["status"]), undefined, undefined, {
    status_rel: ["int"],
  });
  const rt = await DlRuntime.boot({ dbPath, bridge: bridgeOk });
  let capturedError: unknown;
  const subscription = rt.deltas$.subscribe({
    error: (failure: unknown) => {
      capturedError = failure;
    },
  });
  try {
    // commit()'s own promise now also rejects with this same error (F2); swallowed
    // here since the guard's error (asserted below, off deltas$) is what this test is
    // actually about.
    rt.commit(edbBatch({ status_rel: [{ status: Number.NaN }] })).catch(() => {});
    const failure = await waitForDefined(() => capturedError);
    assert.ok(failure instanceof Error);
    assert.match(failure.message, /status_rel/);
    assert.match(failure.message, /column 'status'/);
  } finally {
    subscription.unsubscribe();
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

/** Races `promise` against a short timeout so a regression (the pending commit()
 *  hangs forever, per the F2 incident's 600s manual probe) fails this test fast
 *  instead of stalling the whole suite. */
async function withTimeout<Value>(promise: Promise<Value>, ms: number, label: string): Promise<Value> {
  let timer: ReturnType<typeof setTimeout>;
  const timeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label}: timed out after ${ms}ms`)), ms);
  });
  try {
    return await Promise.race([promise, timeout]);
  } finally {
    clearTimeout(timer!);
  }
}

test("F2: a tick-pipeline fault rejects the in-flight commit() with the original error, instead of hanging forever", async () => {
  // Same fault shape as the guard test above (a non-finite number reaching
  // encodeSurfaceRowByColumns), but this time the assertion is on commit()'s OWN
  // promise, not on a side-channel deltas$ subscription: the failing mode this
  // guards is a commit() call that neither resolves NOR rejects, because the
  // fault travelled on deltas$ and reportsSubject never heard about it (F2,
  // docs/failure-modes.md-shaped incident, 3_runtime.ts:969-1000). Pre-fix this
  // test times out at the withTimeout ceiling instead of passing; the audit's
  // manual probe hung a full 600s on the same shape.
  const dbPath = freshDbPath();
  const bridgeOk = fakeBridgeOk(singleEdbRelProgram("status_rel", ["status"]), undefined, undefined, {
    status_rel: ["int"],
  });
  const rt = await DlRuntime.boot({ dbPath, bridge: bridgeOk });
  // The tick loop only turns while something subscribes deltas$ (class header law); the
  // pipeline fault also terminates deltas$ itself, so swallow that terminal error here --
  // it is asserted through commit()'s rejection below, not through this subscription.
  const subscription = rt.deltas$.subscribe({ error: () => {} });
  try {
    await assert.rejects(
      () =>
        withTimeout(
          rt.commit(edbBatch({ status_rel: [{ status: Number.NaN }] })),
          5_000,
          "commit() after a tick-pipeline fault",
        ),
      (failure: unknown) => {
        assert.ok(failure instanceof Error);
        assert.match(failure.message, /status_rel/);
        assert.match(failure.message, /column 'status'/);
        return true;
      },
    );
  } finally {
    subscription.unsubscribe();
    await rt.dispose();
    cleanupDbFile(dbPath);
  }
});

test("execute$ carries the failing statement's own text (truncated) in the thrown error", async () => {
  const dbPath = path.join(os.tmpdir(), `dl-execute-diag-${Date.now()}.sqlite`);
  const db = open_db(`file:${dbPath}`);
  try {
    const longBogusSql = `SELECT ${"x".repeat(2000)} FROM no_such_table_at_all`;
    await assert.rejects(
      () => firstValueFrom(execute$(db, longBogusSql)),
      (failure: unknown) => {
        assert.ok(failure instanceof Error);
        assert.match(failure.message, /execute\$ failed on statement:/);
        assert.match(failure.message, /SELECT x{10,}/);
        // Truncated, not the whole 2000+-char statement dumped into the error text.
        assert.ok(failure.message.length < longBogusSql.length);
        assert.ok(failure.cause !== undefined, "the original driver failure should ride as `cause`");
        return true;
      },
    );
  } finally {
    db.close();
    cleanupDbFile(dbPath);
  }
});
