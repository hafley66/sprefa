/**
 * txn.test.ts — the `SqlRunner.inTransaction` bracket (the restored Rust all-or-nothing crash
 * bracket for the interactive round loops; see the engine.ts file header for why it must be
 * single-statement `execute` calls, never `client.transaction()`/`executeMultiple`).
 * Proves: commit persists, a mid-body error rolls back, the one pinned connection (and its
 * TEMP tables) survives both paths, and a retract still cascades under its own bracket.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { createClient } from "@libsql/client";
import { concatMap, firstValueFrom, throwError } from "rxjs";

import { RelStore } from "../../src/engine/lib.ts";
import { SqlRunner } from "../../src/engine/sqlRunner.ts";

test("inTransaction: a committed body persists", async () => {
  const db = createClient({ url: ":memory:", intMode: "bigint" });
  await firstValueFrom(SqlRunner.run(db, "CREATE TABLE t(x INTEGER)"));

  await firstValueFrom(
    SqlRunner.inTransaction(db, () =>
      SqlRunner.run(db, "INSERT INTO t(x) VALUES (1)").pipe(
        concatMap(() => SqlRunner.run(db, "INSERT INTO t(x) VALUES (2)")),
      ),
    ),
  );

  assert.strictEqual(await firstValueFrom(SqlRunner.scalar(db, "SELECT count(*) FROM t")), 2);
  db.close();
});

test("inTransaction: an erroring body rolls back and the connection keeps its TEMP tables", async () => {
  const db = createClient({ url: ":memory:", intMode: "bigint" });
  await firstValueFrom(SqlRunner.run(db, "CREATE TABLE t(x INTEGER)"));
  await firstValueFrom(SqlRunner.run(db, "CREATE TEMP TABLE tt(x INTEGER)"));
  await firstValueFrom(SqlRunner.run(db, "INSERT INTO t(x) VALUES (1)"));

  await assert.rejects(
    firstValueFrom(
      SqlRunner.inTransaction(db, () =>
        SqlRunner.run(db, "INSERT INTO t(x) VALUES (3)").pipe(
          concatMap(() => throwError(() => new Error("boom"))),
        ),
      ),
    ),
    /boom/,
  );
  assert.strictEqual(
    await firstValueFrom(SqlRunner.scalar(db, "SELECT count(*) FROM t")),
    1,
    "the failed bracket rolled back",
  );

  // The same connection is still usable and still owns its TEMP tables (:memory: +
  // TEMP state is exactly what a swapped connection would have lost).
  await firstValueFrom(SqlRunner.run(db, "INSERT INTO tt(x) VALUES (9)"));
  assert.strictEqual(
    await firstValueFrom(SqlRunner.scalar(db, "SELECT count(*) FROM tt")),
    1,
    "the TEMP table survived the bracket",
  );
  db.close();
});

test("inTransaction: the bracketed retract still cascades (chain kill)", async () => {
  const store = await firstValueFrom(RelStore.attach(createClient({ url: ":memory:", intMode: "bigint" })));
  // Chain 1 -> 2 -> 3, single support each: retracting the root kills all three.
  await firstValueFrom(
    store.add_rows([
      [1, 1, 1],
      [1, 2, 1],
      [1, 3, 1],
    ]),
  );
  await firstValueFrom(
    store.add_deps([
      [1, 1, 1, 2],
      [1, 2, 1, 3],
    ]),
  );
  // `cascade.retract` opens its OWN `inTransaction` bracket; a second one here would be a
  // nested BEGIN, which SQLite rejects.
  const rounds = await firstValueFrom(store.retract([[1, 1]]));
  assert.ok(rounds >= 1, "the cascade ran at least one round");
  assert.strictEqual(await firstValueFrom(store.alive()), 0, "the whole chain died");
  store.conn().close();
});
