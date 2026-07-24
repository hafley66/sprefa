/**
 * reactor-claims.test.ts — VALIDATE every architecture guess from the reactor writeup,
 * with real measurements, using @libsql/client (async, native-dispatch) as the store.
 * Each test prints its receipt (numbers) then asserts. Where a guess is only partly
 * true, the test says so in its assertion + a NOTE. Run with --expose-gc for the heap
 * claims (the test still runs without it, thresholds just loosen).
 *
 * The guesses under test (from the reactor architecture answer):
 *   G1  the rxjs graph carries SIGNALS not ROWS -> heap cost is materialization, not count.
 *   G2  resident set tracks SQLite's cache, NOT the db file size (the 877MB-in-130MB claim).
 *   G3  a heavy query BLOCKS the loop under sync (better-sqlite3), FREE under async (libsql).
 *   G4  switchMap drops the STALE rederive result (cancel-stale) — result-level, see NOTE.
 *   G5  WAL lets a reader proceed concurrently with an open writer (no SQLITE_BUSY).
 *   G6  combineLatest(dirty) -> switchMap(from(sql join)) is a correct reactive derived rel.
 *   G7  cold-by-default: an unsubscribed rel runs ZERO queries.
 *   G8  the expand frontier respects dep-order AND mergeMap caps concurrency to the budget.
 *   G9  a SQL semi-naive fixpoint keeps the closure in SQLite, heap stays flat as it grows.
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { rmSync } from "node:fs";
import { randomUUID } from "node:crypto";

import {
  EMPTY,
  of,
  from,
  Subject,
  ReplaySubject,
  combineLatest,
  expand,
  mergeMap,
  switchMap,
  map,
  toArray,
  firstValueFrom,
  lastValueFrom,
} from "rxjs";
import { createClient, type Client } from "@libsql/client";
import Database from "better-sqlite3";

// ── measurement helpers ──────────────────────────────────────────────────────
const gc = (): void => {
  const g = (globalThis as { gc?: () => void }).gc;
  if (g) g();
};
const heapMB = (): number => process.memoryUsage().heapUsed / 1048576;
const rssMB = (): number => process.memoryUsage().rss / 1048576;
const num = (v: unknown): number => Number(v);

function memClient(): Client {
  return createClient({ url: ":memory:" });
}

/** Insert N (a,b) integer rows into a fresh table via batched libsql. */
async function seedIntTable(client: Client, n: number): Promise<void> {
  await client.execute("CREATE TABLE t(a INTEGER, b INTEGER)");
  const CHUNK = 5000;
  for (let base = 0; base < n; base += CHUNK) {
    const stmts = [];
    for (let i = base; i < Math.min(base + CHUNK, n); i++) {
      stmts.push({ sql: "INSERT INTO t VALUES (?, ?)", args: [i % 1000, i % 777] });
    }
    await client.batch(stmts);
  }
}

// =============================================================================
// G1 — signals not rows: heap cost is MATERIALIZATION, not row count.
// =============================================================================

test("G1: carrying a count-signal is O(1) heap; materializing rows is O(N)", async () => {
  const client = memClient();
  const N = 300_000;
  await seedIntTable(client, N);

  gc();
  const h0 = heapMB();
  const countRs = await client.execute("SELECT count(*) AS n FROM t"); // signal-shaped
  const afterCount = heapMB();
  assert.equal(num(countRs.rows[0]!.n), N);

  gc();
  const h1 = heapMB();
  const allRs = await client.execute("SELECT a, b FROM t"); // materialize ALL rows into heap
  const afterAll = heapMB();
  assert.equal(allRs.rows.length, N);

  const countDelta = afterCount - h0;
  const materializeDelta = afterAll - h1;
  console.log(`  [G1] count-signal heapDelta=${countDelta.toFixed(1)}MB  materialize-${N}-rows heapDelta=${materializeDelta.toFixed(1)}MB`);

  // The signal is cheap; the full pull is many MB and dwarfs it. THIS is why the graph
  // moves dirty-ticks, not rowsets.
  assert.ok(materializeDelta > 10, `materializing ${N} rows should cost >10MB, got ${materializeDelta.toFixed(1)}`);
  assert.ok(countDelta < materializeDelta / 4, `count-signal (${countDelta.toFixed(1)}MB) should be a fraction of materialize (${materializeDelta.toFixed(1)}MB)`);
  client.close();
});

// =============================================================================
// G2 — resident set tracks the CACHE, not the db file size.
// =============================================================================

test("G2: a scan of a db far larger than its page cache does NOT load the whole file", async () => {
  const path = join(tmpdir(), `sprefa-g2-${randomUUID()}.db`);
  try {
    const client = createClient({ url: `file:${path}` });
    // Build a db meaningfully larger than the cache we will set: ~64MB of payload.
    await client.execute("CREATE TABLE big(id INTEGER, payload TEXT)");
    const payload = "x".repeat(512);
    const ROWS = 120_000; // ~64MB
    const CHUNK = 4000;
    for (let base = 0; base < ROWS; base += CHUNK) {
      const stmts = [];
      for (let i = base; i < Math.min(base + CHUNK, ROWS); i++) {
        stmts.push({ sql: "INSERT INTO big VALUES (?, ?)", args: [i, payload] });
      }
      await client.batch(stmts);
    }
    const pageCount = num((await client.execute("PRAGMA page_count")).rows[0]!.page_count);
    const pageSize = num((await client.execute("PRAGMA page_size")).rows[0]!.page_size);
    const fileMB = (pageCount * pageSize) / 1048576;
    client.close();

    // Reopen with a TINY cache (4MB) and full-scan. RSS delta must stay far under the file.
    const reader = createClient({ url: `file:${path}` });
    await reader.execute("PRAGMA cache_size = -4096"); // 4 MiB
    gc();
    const r0 = rssMB();
    const scan = await reader.execute("SELECT count(*) AS n FROM big WHERE length(payload) > 0");
    const r1 = rssMB();
    assert.equal(num(scan.rows[0]!.n), ROWS);
    const rssDelta = r1 - r0;
    console.log(`  [G2] fileSize=${fileMB.toFixed(0)}MB  cache=4MB  scan rssDelta=${rssDelta.toFixed(1)}MB`);
    reader.close();

    // The scan touches every page but keeps only the cache resident: delta << file size.
    assert.ok(rssDelta < fileMB / 2, `scan rssDelta ${rssDelta.toFixed(1)}MB should be << file ${fileMB.toFixed(0)}MB`);
  } finally {
    rmSync(path, { force: true });
    rmSync(`${path}-wal`, { force: true });
    rmSync(`${path}-shm`, { force: true });
  }
});

// =============================================================================
// G3 — heavy query blocks the loop under SYNC, free under ASYNC. Head to head.
// =============================================================================

function heartbeat(): { stop: () => number } {
  let last = performance.now();
  let maxGap = 0;
  const timer = setInterval(() => {
    const now = performance.now();
    if (now - last > maxGap) maxGap = now - last;
    last = now;
  }, 10);
  return {
    stop: () => {
      clearInterval(timer);
      return maxGap;
    },
  };
}

const HEAVY = "SELECT count(*) AS n FROM t x JOIN t y ON x.a = y.b";

test("G3: async (libsql) keeps the event loop live during a heavy query; sync (better) blocks it", async () => {
  const N = 160_000;
  // libsql (async)
  const client = memClient();
  await seedIntTable(client, N);
  let hb = heartbeat();
  const t0 = performance.now();
  const r1 = await client.execute(HEAVY);
  const libDur = performance.now() - t0;
  const libGap = hb.stop();
  client.close();

  // better-sqlite3 (sync) — build the same data, run the same query on the main thread.
  const db = new Database(":memory:");
  db.exec("CREATE TABLE t(a INTEGER, b INTEGER)");
  const ins = db.prepare("INSERT INTO t VALUES (?, ?)");
  db.transaction(() => {
    for (let i = 0; i < N; i++) ins.run(i % 1000, i % 777);
  })();
  hb = heartbeat();
  const t1 = performance.now();
  const r2 = db.prepare(HEAVY).get() as { n: number };
  const betDur = performance.now() - t1;
  await new Promise((res) => setTimeout(res, 40)); // let post-block beats through
  const betGap = hb.stop();
  db.close();

  console.log(`  [G3] libsql  query=${libDur.toFixed(0)}ms maxGap=${libGap.toFixed(1)}ms | better query=${betDur.toFixed(0)}ms maxGap=${betGap.toFixed(1)}ms`);
  assert.equal(num(r1.rows[0]!.n), r2.n);
  assert.ok(libDur > 120, `expected a genuinely heavy query, got ${libDur.toFixed(0)}ms`);
  // Async: the loop kept ticking (gap far under the query time).
  assert.ok(libGap < libDur / 3, `libsql should not block the loop (gap ${libGap.toFixed(0)}ms vs query ${libDur.toFixed(0)}ms)`);
  // Sync: the loop was frozen for ~the whole query.
  assert.ok(betGap > betDur / 2, `better-sqlite3 should block the loop (gap ${betGap.toFixed(0)}ms vs query ${betDur.toFixed(0)}ms)`);
});

// =============================================================================
// G4 — switchMap cancel-stale: only the LATEST rederive result is delivered.
// =============================================================================

test("G4: switchMap drops the stale rederive when inputs move again mid-flight", async () => {
  const input = new Subject<string>();
  const delivered: string[] = [];

  // A rel re-derive modelled as an async op whose duration depends on the key: the FIRST
  // (stale) key resolves SLOWER than the second, so without cancel-stale it would arrive
  // last and clobber. switchMap must drop it.
  const durations: Record<string, number> = { first: 60, second: 10 };
  const rel$ = input.pipe(
    switchMap((key) => from(new Promise<string>((res) => setTimeout(() => res(`derived(${key})`), durations[key])))),
  );
  const sub = rel$.subscribe((v) => delivered.push(v));

  input.next("first");
  await new Promise((res) => setTimeout(res, 5));
  input.next("second"); // switch before "first" resolves
  await new Promise((res) => setTimeout(res, 120));
  sub.unsubscribe();

  console.log(`  [G4] delivered=${JSON.stringify(delivered)}`);
  // Only the latest survives; the slow stale "first" is cancelled at the result level.
  assert.deepEqual(delivered, ["derived(second)"]);
  // NOTE: switchMap cancels the RESULT delivery. It does NOT abort an in-flight SQL query
  // inside the store — that keeps running to completion on its thread. True mid-query abort
  // needs a cancel token / reqid check at the store boundary (Layer-1 reconcile already has one).
});

// =============================================================================
// G5 — WAL: a reader proceeds while a writer holds an open transaction.
// =============================================================================

test("G5: WAL lets a second connection read during an open write transaction", async () => {
  const path = join(tmpdir(), `sprefa-g5-${randomUUID()}.db`);
  try {
    const writer = createClient({ url: `file:${path}` });
    await writer.execute("PRAGMA journal_mode = WAL");
    await writer.execute("CREATE TABLE kv(k TEXT PRIMARY KEY, v INTEGER)");
    await writer.execute({ sql: "INSERT INTO kv VALUES ('x', ?)", args: [1] });

    const reader = createClient({ url: `file:${path}` });

    // Open a write transaction and mutate, but do NOT commit yet.
    const tx = await writer.transaction("write");
    await tx.execute({ sql: "UPDATE kv SET v = ? WHERE k = 'x'", args: [999] });

    // The reader must still get an answer (WAL snapshot = the pre-commit value), not SQLITE_BUSY.
    const midRead = await reader.execute("SELECT v FROM kv WHERE k = 'x'");
    const midValue = num(midRead.rows[0]!.v);

    await tx.commit();
    const postRead = await reader.execute("SELECT v FROM kv WHERE k = 'x'");
    const postValue = num(postRead.rows[0]!.v);

    console.log(`  [G5] read-during-open-write=${midValue}  read-after-commit=${postValue}`);
    assert.equal(midValue, 1, "reader sees the pre-commit snapshot (WAL), no block/error");
    assert.equal(postValue, 999, "reader sees the committed value after commit");
    writer.close();
    reader.close();
  } finally {
    rmSync(path, { force: true });
    rmSync(`${path}-wal`, { force: true });
    rmSync(`${path}-shm`, { force: true });
  }
});

// =============================================================================
// G6 — the lowering shape: combineLatest(dirty) -> switchMap(from(sql join)) is reactive.
// =============================================================================

test("G6: combineLatest(dirty) + switchMap(from(join)) re-derives when EDB changes", async () => {
  const client = memClient();
  await client.execute("CREATE TABLE edge(x TEXT, y TEXT)");
  await client.execute("CREATE TABLE label(ep TEXT, name TEXT)");
  await client.batch([
    { sql: "INSERT INTO edge VALUES ('a','b')", args: [] },
    { sql: "INSERT INTO label VALUES ('a','tui')", args: [] },
  ]);

  const edgeDirty$ = new ReplaySubject<number>(1);
  const labelDirty$ = new ReplaySubject<number>(1);
  const JOIN = "SELECT e.x AS x, e.y AS y, l.name AS name FROM edge e JOIN label l ON e.x = l.ep ORDER BY x, y, name";

  const watchLabel$ = combineLatest([edgeDirty$, labelDirty$]).pipe(
    switchMap(() => from(client.execute(JOIN))),
    map((rs) => rs.rows.map((r) => `${r.x}-${r.y}-${r.name}`)),
  );

  const seen: string[][] = [];
  const sub = watchLabel$.subscribe((rows) => seen.push(rows));

  edgeDirty$.next(1);
  labelDirty$.next(1);
  await new Promise((res) => setTimeout(res, 20));

  // EDB change: add a second labelled edge, signal dirty.
  await client.batch([
    { sql: "INSERT INTO edge VALUES ('a','c')", args: [] },
  ]);
  edgeDirty$.next(2);
  await new Promise((res) => setTimeout(res, 20));
  sub.unsubscribe();

  const latest = seen[seen.length - 1]!;
  console.log(`  [G6] emissions=${seen.length}  latest=${JSON.stringify(latest)}`);
  assert.ok(seen.length >= 2, "should re-emit on the dirty tick");
  assert.deepEqual(latest, ["a-b-tui", "a-c-tui"], "derived rel reflects the new edge");
  client.close();
});

// =============================================================================
// G7 — cold-by-default: an unsubscribed rel runs ZERO queries.
// =============================================================================

test("G7: building the Observable graph runs no query until something subscribes", async () => {
  const client = memClient();
  await client.execute("CREATE TABLE t(a INTEGER)");
  await client.execute("INSERT INTO t VALUES (1)");

  let queries = 0;
  const facts$ = from(Promise.resolve()).pipe(
    switchMap(() => {
      queries++;
      return from(client.execute("SELECT a FROM t"));
    }),
  );

  console.log(`  [G7] queries after build (no subscribe) = ${queries}`);
  assert.equal(queries, 0, "cold: no query ran at graph-build time");

  await lastValueFrom(facts$);
  console.log(`  [G7] queries after subscribe = ${queries}`);
  assert.equal(queries, 1, "subscribing runs the query exactly once");
  client.close();
});

// =============================================================================
// G8 — expand frontier: dep-order respected AND concurrency capped to the budget.
// =============================================================================

/** A minimal ready-set scheduler: expand over the levels of a rel DAG, mergeMap caps
 *  in-flight rederives to `budget`. Emits the batch of rels finished per level. */
function runFrontier(
  deps: Map<string, string[]>, // rel -> the rels it depends on (must finish first)
  budget: number,
  rederive: (rel: string) => Promise<void>,
) {
  const indeg = new Map<string, number>();
  const dependents = new Map<string, string[]>();
  for (const [rel, ds] of deps) {
    indeg.set(rel, ds.length);
    for (const d of ds) {
      const list = dependents.get(d) ?? [];
      list.push(rel);
      dependents.set(d, list);
    }
  }
  const seed = [...deps.keys()].filter((rel) => (indeg.get(rel) ?? 0) === 0);
  return of(seed).pipe(
    expand((batch) =>
      from(batch).pipe(
        mergeMap((rel) => from(rederive(rel)).pipe(map(() => rel)), budget), // ← the budget cap
        toArray(),
        map((finished) => {
          const newlyReady: string[] = [];
          for (const rel of finished) {
            for (const dep of dependents.get(rel) ?? []) {
              const left = (indeg.get(dep) ?? 0) - 1;
              indeg.set(dep, left);
              if (left === 0) newlyReady.push(dep);
            }
          }
          return newlyReady;
        }),
        mergeMap((nr) => (nr.length ? of(nr) : EMPTY)),
      ),
    ),
  );
}

test("G8: the expand frontier runs deps before dependents and never exceeds the budget", async () => {
  // DAG: a,b -> c ; c -> d ; e independent. So valid order gates c after a&b, d after c.
  const deps = new Map<string, string[]>([
    ["a", []],
    ["b", []],
    ["e", []],
    ["c", ["a", "b"]],
    ["d", ["c"]],
  ]);
  const BUDGET = 2;

  const started: Record<string, number> = {};
  const ended: Record<string, number> = {};
  let clock = 0;
  let inFlight = 0;
  let maxInFlight = 0;

  const rederive = (rel: string): Promise<void> => {
    started[rel] = clock++;
    inFlight++;
    if (inFlight > maxInFlight) maxInFlight = inFlight;
    return new Promise((res) =>
      setTimeout(() => {
        ended[rel] = clock++;
        inFlight--;
        res();
      }, 15),
    );
  };

  await firstValueFrom(runFrontier(deps, BUDGET, rederive).pipe(toArray()));

  console.log(`  [G8] maxInFlight=${maxInFlight} (budget ${BUDGET})  started=${JSON.stringify(started)}`);
  // dep-order: c starts after a AND b ended; d starts after c ended.
  assert.ok(started["c"]! > ended["a"]! && started["c"]! > ended["b"]!, "c waits for a and b");
  assert.ok(started["d"]! > ended["c"]!, "d waits for c");
  // budget: never more than BUDGET rederives in flight at once.
  assert.ok(maxInFlight <= BUDGET, `maxInFlight ${maxInFlight} exceeded budget ${BUDGET}`);
});

// =============================================================================
// G9 — a SQL semi-naive fixpoint keeps the closure in SQLite; heap stays flat.
// =============================================================================

async function sqlTransitiveClosureCount(client: Client, edges: readonly [number, number][]): Promise<number> {
  await client.execute("CREATE TABLE edge(x INTEGER, y INTEGER)");
  await client.execute("CREATE TABLE path(x INTEGER, y INTEGER, PRIMARY KEY(x, y)) WITHOUT ROWID");
  const CHUNK = 4000;
  for (let base = 0; base < edges.length; base += CHUNK) {
    await client.batch(
      edges.slice(base, base + CHUNK).map(([x, y]) => ({ sql: "INSERT INTO edge VALUES (?, ?)", args: [x, y] })),
    );
  }
  await client.execute("INSERT OR IGNORE INTO path SELECT x, y FROM edge");
  // the delta loop: rows never leave SQLite; only the affected-count crosses into JS.
  for (;;) {
    const rs = await client.execute("INSERT OR IGNORE INTO path SELECT p.x, e.y FROM path p JOIN edge e ON p.y = e.x");
    if (num(rs.rowsAffected) === 0) break;
  }
  const c = await client.execute("SELECT count(*) AS n FROM path");
  return num(c.rows[0]!.n);
}

test("G9: SQL fixpoint keeps a large closure on disk; JS heap stays flat", async () => {
  const client = memClient();
  // A layered DAG whose closure is large: fully-connected layer-to-layer, so the closure
  // is width^2 * C(layers,2) pairs. 10 layers x 10 wide = 100 * 45 = 4500 pairs.
  const layers = 10;
  const width = 10;
  const edges: [number, number][] = [];
  const id = (l: number, i: number): number => l * width + i;
  for (let l = 0; l < layers - 1; l++) {
    for (let i = 0; i < width; i++) for (let j = 0; j < width; j++) edges.push([id(l, i), id(l + 1, j)]);
  }

  gc();
  const h0 = heapMB();
  const closureSize = await sqlTransitiveClosureCount(client, edges);
  gc();
  const h1 = heapMB();
  const heapDelta = h1 - h0;

  console.log(`  [G9] closureSize=${closureSize} pairs  edges=${edges.length}  heapDelta=${heapDelta.toFixed(1)}MB`);
  assert.ok(closureSize > 3000, `expected a large closure, got ${closureSize}`);
  // The whole closure lives in SQLite; heap only ever held the per-pass affected-count.
  assert.ok(heapDelta < 8, `fixpoint heap delta ${heapDelta.toFixed(1)}MB should stay flat (closure is on disk)`);
  client.close();
});
