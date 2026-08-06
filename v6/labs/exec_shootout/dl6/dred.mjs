// sprefa-store assert/retract_dred (src/engine.rs:407,:454) on real rule columns.
// Usage: node dred.mjs [gridSide] [batch]   (defaults 30, 100)
import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";

const OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV_PRIME = 0x00000100000001b3n;
const MASK_64 = 0xffffffffffffffffn;

function fnv1a64(source, target) {
  const bytes = new Uint8Array(8);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, source, true);
  view.setUint32(4, target, true);
  let hash = OFFSET_BASIS;
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = (hash * FNV_PRIME) & MASK_64;
  }
  return hash;
}

function checksum(db) {
  const rows = db.prepare(`SELECT source, target FROM reachable`).all();
  let hash = 0n;
  for (const row of rows) hash ^= fnv1a64(row.source, row.target);
  return `${rows.length}:${hash.toString(16).padStart(16, "0")}`;
}

function openDb() {
  const db = new Database(":memory:");
  db.exec(`
    PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA temp_store=MEMORY;
    CREATE TABLE edge (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE INDEX edge_by_target ON edge (target, source);
    CREATE TABLE reachable (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE TEMP TABLE ping (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE TEMP TABLE pong (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE TEMP TABLE cone (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE TEMP TABLE delta_edge (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
  `);
  return db;
}

function gridEdges(side) {
  const edges = [];
  for (let row = 0; row < side; row++) {
    for (let column = 0; column < side; column++) {
      const node = row * side + column;
      if (column + 1 < side) edges.push([node, node + 1]);
      if (row + 1 < side) edges.push([node, node + side]);
    }
  }
  return edges;
}

function insertEdges(db, edges) {
  const statement = db.prepare(`INSERT OR IGNORE INTO edge VALUES (?, ?)`);
  db.transaction(() => { for (const edge of edges) statement.run(edge); })();
}

// ORACLE: the shipped shape, one WITH RECURSIVE full recompute of the head.
function oracleRecompute(db) {
  const startedAt = performance.now();
  db.exec(`
    DELETE FROM reachable;
    INSERT INTO reachable
    WITH RECURSIVE closure (source, target) AS (
      SELECT source, target FROM edge
      UNION
      SELECT closure.source, edge.target FROM closure
      JOIN edge ON edge.source = closure.target
    )
    SELECT source, target FROM closure;
  `);
  return performance.now() - startedAt;
}

// ASSERT (engine.rs:407): seeds from the delta only, ping-pong with retirement,
// head absent-check gates every hop. Work is proportional to the NEW cone.
function assertDelta(db) {
  const startedAt = performance.now();
  let rowsTouched = 0;
  let rounds = 0;
  const run = (sql) => { rowsTouched += db.prepare(sql).run().changes; };
  db.transaction(() => {
    db.exec(`DELETE FROM ping; DELETE FROM pong;`);
    run(`INSERT OR IGNORE INTO ping
         SELECT d.source, d.target FROM delta_edge d
         LEFT JOIN reachable h ON h.source = d.source AND h.target = d.target
         WHERE h.source IS NULL`);
    run(`INSERT OR IGNORE INTO ping
         SELECT r.source, d.target FROM delta_edge d
         JOIN reachable r ON r.target = d.source
         LEFT JOIN reachable h ON h.source = r.source AND h.target = d.target
         WHERE h.source IS NULL`);
    run(`INSERT OR IGNORE INTO reachable SELECT source, target FROM ping`);
    let frontier = "ping";
    let next = "pong";
    for (;;) {
      db.exec(`DELETE FROM ${next}`);
      const derived = db.prepare(
        `INSERT OR IGNORE INTO ${next}
         SELECT f.source, e.target FROM ${frontier} f
         JOIN edge e ON e.source = f.target
         LEFT JOIN reachable h ON h.source = f.source AND h.target = e.target
         WHERE h.source IS NULL`).run().changes;
      if (derived === 0) break;
      rowsTouched += derived;
      rounds += 1;
      run(`INSERT OR IGNORE INTO reachable SELECT source, target FROM ${next}`);
      [frontier, next] = [next, frontier];
    }
  })();
  return { ms: performance.now() - startedAt, rowsTouched, rounds };
}

// DRED (engine.rs:454): over-delete the cone, rederive rows a surviving
// derivation still anchors. A dead cycle has no surviving anchor.
function dredDelta(db) {
  const startedAt = performance.now();
  let rowsTouched = 0;
  let rounds = 0;
  const run = (sql) => { rowsTouched += db.prepare(sql).run().changes; };
  db.transaction(() => {
    db.exec(`DELETE FROM ping; DELETE FROM pong; DELETE FROM cone;`);
    // Seeds, one per arm with the deleted atom substituted (head is pre-delete).
    run(`INSERT OR IGNORE INTO ping
         SELECT d.source, d.target FROM delta_edge d
         JOIN reachable h ON h.source = d.source AND h.target = d.target`);
    run(`INSERT OR IGNORE INTO ping
         SELECT r.source, d.target FROM delta_edge d
         JOIN reachable r ON r.target = d.source
         JOIN reachable h ON h.source = r.source AND h.target = d.target`);
    run(`INSERT INTO cone SELECT source, target FROM ping`);
    let frontier = "ping";
    let next = "pong";
    for (;;) {
      db.exec(`DELETE FROM ${next}`);
      const suspected = db.prepare(
        `INSERT OR IGNORE INTO ${next}
         SELECT f.source, e.target FROM ${frontier} f
         JOIN edge e ON e.source = f.target
         JOIN reachable h ON h.source = f.source AND h.target = e.target
         WHERE NOT EXISTS (SELECT 1 FROM cone c
                           WHERE c.source = f.source AND c.target = e.target)`).run().changes;
      if (suspected === 0) break;
      rowsTouched += suspected;
      rounds += 1;
      run(`INSERT OR IGNORE INTO cone SELECT source, target FROM ${next}`);
      [frontier, next] = [next, frontier];
    }
    run(`DELETE FROM reachable WHERE (source, target) IN (SELECT source, target FROM cone)`);
    // Rederive base: cone rows with a derivation whose every atom survived.
    db.exec(`DELETE FROM ping; DELETE FROM pong;`);
    run(`INSERT OR IGNORE INTO ping
         SELECT c.source, c.target FROM cone c
         JOIN edge e ON e.source = c.source AND e.target = c.target`);
    run(`INSERT OR IGNORE INTO ping
         SELECT c.source, c.target FROM cone c
         JOIN edge e ON e.target = c.target
         JOIN reachable r ON r.source = c.source AND r.target = e.source`);
    run(`INSERT OR IGNORE INTO reachable SELECT source, target FROM ping`);
    run(`DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ping)`);
    frontier = "ping";
    next = "pong";
    for (;;) {
      db.exec(`DELETE FROM ${next}`);
      const revived = db.prepare(
        `INSERT OR IGNORE INTO ${next}
         SELECT f.source, e.target FROM ${frontier} f
         JOIN edge e ON e.source = f.target
         JOIN cone c ON c.source = f.source AND c.target = e.target`).run().changes;
      if (revived === 0) break;
      rowsTouched += revived;
      rounds += 1;
      run(`INSERT OR IGNORE INTO reachable SELECT source, target FROM ${next}`);
      run(`DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ${next})`);
      [frontier, next] = [next, frontier];
    }
  })();
  // Rows still in cone are the true retractions; the runtime stages them -1.
  const retracted = db.prepare(`SELECT count(*) AS n FROM cone`).get().n;
  return { ms: performance.now() - startedAt, rowsTouched, rounds, retracted };
}

function stageDelta(db, edges) {
  db.exec(`DELETE FROM delta_edge`);
  const statement = db.prepare(`INSERT OR IGNORE INTO delta_edge VALUES (?, ?)`);
  db.transaction(() => { for (const edge of edges) statement.run(edge); })();
}

function referee(name, incrementalDb, mutate) {
  const twin = openDb();
  insertEdges(twin, incrementalDb.prepare(`SELECT source, target FROM edge`).all()
    .map((row) => [row.source, row.target]));
  const oracleMs = oracleRecompute(twin);
  const left = checksum(incrementalDb);
  const right = checksum(twin);
  const verdict = left === right ? "MATCH" : `MISMATCH ${left} vs ${right}`;
  console.log(`${name}\t${verdict}\toracle_full_recompute_ms=${Math.round(oracleMs)}`);
  twin.close();
  if (left !== right) process.exit(1);
}

const gridSide = Number(process.argv[2] ?? 30);
const batch = Number(process.argv[3] ?? 100);
const edges = gridEdges(gridSide);
console.log(`# grid ${gridSide}x${gridSide}, ${edges.length} edges, batch=${batch}`);
console.log(`# scenario\tms\trows_touched\trounds\tretracted`);

const db = openDb();
insertEdges(db, edges);

// Tick 0: build the closure from scratch, loop vs CTE on the same corpus.
stageDelta(db, edges);
const build = assertDelta(db);
console.log(`build_assert_loop\t${Math.round(build.ms)}\t${build.rowsTouched}\t${build.rounds}`);
{
  const twin = openDb();
  insertEdges(twin, edges);
  console.log(`build_full_cte\t${Math.round(oracleRecompute(twin))}`);
  twin.close();
}
referee("build", db, null);

// Insert tick: one long-jump edge, then a batch.
const jumpEdge = [[0, gridSide * gridSide - 2]];
insertEdges(db, jumpEdge);
stageDelta(db, jumpEdge);
const one = assertDelta(db);
console.log(`insert_one\t${Math.round(one.ms)}\t${one.rowsTouched}\t${one.rounds}`);
referee("insert_one", db, null);

const batchEdges = [];
for (let index = 0; index < batch; index++) {
  const a = (index * 7919) % (gridSide * gridSide);
  const b = (index * 104729 + 13) % (gridSide * gridSide);
  if (a !== b) batchEdges.push([Math.min(a, b), Math.max(a, b)]);
}
insertEdges(db, batchEdges);
stageDelta(db, batchEdges);
const many = assertDelta(db);
console.log(`insert_batch\t${Math.round(many.ms)}\t${many.rowsTouched}\t${many.rounds}`);
referee("insert_batch", db, null);

// Delete tick: take the batch back out, then one structural edge.
db.transaction(() => {
  const statement = db.prepare(`DELETE FROM edge WHERE source = ? AND target = ?`);
  for (const edge of batchEdges) statement.run(edge);
})();
stageDelta(db, batchEdges);
const unbatch = dredDelta(db);
console.log(`delete_batch\t${Math.round(unbatch.ms)}\t${unbatch.rowsTouched}\t${unbatch.rounds}\t${unbatch.retracted}`);
referee("delete_batch", db, null);

const structural = [[0, 1]];
db.prepare(`DELETE FROM edge WHERE source = 0 AND target = 1`).run();
stageDelta(db, structural);
const cut = dredDelta(db);
console.log(`delete_structural\t${Math.round(cut.ms)}\t${cut.rowsTouched}\t${cut.rounds}\t${cut.retracted}`);
referee("delete_structural", db, null);

// Cycle fixture: a cycle fed by one anchor; cutting the anchor must kill the
// whole cycle (counting keeps phantom rows here; DRed must not).
{
  const cyclic = openDb();
  insertEdges(cyclic, [[1, 2], [2, 3], [3, 4], [4, 2], [5, 6]]);
  stageDelta(cyclic, [[1, 2], [2, 3], [3, 4], [4, 2], [5, 6]]);
  assertDelta(cyclic);
  referee("cycle_build", cyclic, null);
  cyclic.prepare(`DELETE FROM edge WHERE source = 1 AND target = 2`).run();
  stageDelta(cyclic, [[1, 2]]);
  const kill = dredDelta(cyclic);
  console.log(`cycle_cut_anchor\t${Math.round(kill.ms)}\t${kill.rowsTouched}\t${kill.rounds}\t${kill.retracted}`);
  referee("cycle_cut_anchor", cyclic, null);
  cyclic.close();
}
db.close();
