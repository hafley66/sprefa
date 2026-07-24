/**
 * txn.test.ts — the with_txn bracket (the restored Rust all-or-nothing crash bracket
 * for the interactive round loops; see the engine.ts file header for why it must be
 * single-statement `execute` calls, never `client.transaction()`/`executeMultiple`).
 * Proves: commit persists, a mid-body throw rolls back, the one pinned connection (and
 * its TEMP tables) survives both paths, and a bracketed retract still cascades.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { createClient } from "@libsql/client";

import { cascade } from "../../src/engine/engine.ts";
import { RelStore } from "../../src/engine/lib.ts";

test("with_txn: commit persists, a throw rolls back, the connection survives", async () => {
  const db = createClient({ url: ":memory:", intMode: "bigint" });
  await db.execute("CREATE TABLE t(x INTEGER)");
  await db.execute("CREATE TEMP TABLE tt(x INTEGER)");

  await cascade.with_txn(db, async () => {
    await db.execute("INSERT INTO t(x) VALUES (1)");
    await db.execute("INSERT INTO t(x) VALUES (2)");
  });
  let res = await db.execute("SELECT count(*) FROM t");
  assert.strictEqual(Number(res.rows[0]![0]), 2, "committed rows persist");

  await assert.rejects(
    cascade.with_txn(db, async () => {
      await db.execute("INSERT INTO t(x) VALUES (3)");
      throw new Error("boom");
    }),
    /boom/,
  );
  res = await db.execute("SELECT count(*) FROM t");
  assert.strictEqual(Number(res.rows[0]![0]), 2, "the failed bracket rolled back");

  // The same connection is still usable and still owns its TEMP tables (:memory: +
  // TEMP state is exactly what a swapped connection would have lost).
  await db.execute("INSERT INTO tt(x) VALUES (9)");
  res = await db.execute("SELECT count(*) FROM tt");
  assert.strictEqual(Number(res.rows[0]![0]), 1, "the TEMP table survived the bracket");
  db.close();
});

test("with_txn: the bracketed retract still cascades (chain kill)", async () => {
  const store = await RelStore.attach(createClient({ url: ":memory:", intMode: "bigint" }));
  // Chain 1 -> 2 -> 3, single support each: retracting the root kills all three.
  await store.add_rows([
    [1, 1, 1],
    [1, 2, 1],
    [1, 3, 1],
  ]);
  await store.add_deps([
    [1, 1, 1, 2],
    [1, 2, 1, 3],
  ]);
  const rounds = await store.retract([[1, 1]]);
  assert.ok(rounds >= 1, "the cascade ran at least one round");
  assert.strictEqual(await store.alive(), 0, "the whole chain died");
  store.conn().close();
});
