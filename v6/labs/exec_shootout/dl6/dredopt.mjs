// Measured DRed variants against dredprof.mjs's worst case (delete_batch).
// Usage: node dredopt.mjs [gridSide] [batch]   (defaults 50, 100)
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
      round INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE INDEX cone_by_round ON cone (round);
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

function insertRows(db, table, rows) {
  const statement = db.prepare(`INSERT OR IGNORE INTO ${table} (source, target) VALUES (?, ?)`);
  db.transaction(() => { for (const row of rows) statement.run(row); })();
}

function fullRecompute(db) {
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

// A: dred.mjs as committed (ping/pong walk, head probe, up-front cone delete).
function variantShipped(db) {
  db.transaction(() => {
    db.exec(`DELETE FROM ping; DELETE FROM pong; DELETE FROM cone;`);
    db.exec(`
      INSERT OR IGNORE INTO ping
      SELECT d.source, d.target FROM delta_edge d
      JOIN reachable h ON h.source = d.source AND h.target = d.target;
      INSERT OR IGNORE INTO ping
      SELECT r.source, d.target FROM delta_edge d
      JOIN reachable r ON r.target = d.source
      JOIN reachable h ON h.source = r.source AND h.target = d.target;
      INSERT INTO cone (source, target) SELECT source, target FROM ping;
    `);
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
      db.exec(`INSERT OR IGNORE INTO cone (source, target) SELECT source, target FROM ${next}`);
      [frontier, next] = [next, frontier];
    }
    db.exec(`DELETE FROM reachable WHERE (source, target) IN (SELECT source, target FROM cone)`);
    db.exec(`
      DELETE FROM ping; DELETE FROM pong;
      INSERT OR IGNORE INTO ping
      SELECT c.source, c.target FROM cone c
      JOIN edge e ON e.source = c.source AND e.target = c.target;
      INSERT OR IGNORE INTO ping
      SELECT c.source, c.target FROM cone c
      JOIN edge e ON e.target = c.target
      JOIN reachable r ON r.source = c.source AND r.target = e.source;
      INSERT OR IGNORE INTO reachable SELECT source, target FROM ping;
      DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ping);
    `);
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
      db.exec(`
        INSERT OR IGNORE INTO reachable SELECT source, target FROM ${next};
        DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ${next});
      `);
      [frontier, next] = [next, frontier];
    }
  })();
}

// B: over-delete writes cone DIRECTLY (round column = frontier), no pong copy,
// no head probe (old head is a fixpoint, the probe is a tautology).
function overDeleteFused(db) {
  db.exec(`
    INSERT OR IGNORE INTO cone (source, target, round)
    SELECT d.source, d.target, 0 FROM delta_edge d
    JOIN reachable h ON h.source = d.source AND h.target = d.target;
    INSERT OR IGNORE INTO cone (source, target, round)
    SELECT r.source, d.target, 0 FROM delta_edge d
    JOIN reachable r ON r.target = d.source
    JOIN reachable h ON h.source = r.source AND h.target = d.target;
  `);
  const hop = db.prepare(
    `INSERT OR IGNORE INTO cone (source, target, round)
     SELECT f.source, e.target, ? FROM cone f
     JOIN edge e ON e.source = f.target
     WHERE f.round = ?`);
  for (let round = 1; ; round++) {
    if (hop.run([round, round - 1]).changes === 0) break;
  }
}

// C: head rows never leave up front; rederive treats "alive" as
// in-head-and-not-suspect; only the final cone is deleted.
function rederiveDeferred(db) {
  db.exec(`
    DELETE FROM ping; DELETE FROM pong;
    INSERT OR IGNORE INTO ping
    SELECT c.source, c.target FROM cone c
    JOIN edge e ON e.source = c.source AND e.target = c.target;
    INSERT OR IGNORE INTO ping
    SELECT c.source, c.target FROM cone c
    JOIN edge e ON e.target = c.target
    JOIN reachable r ON r.source = c.source AND r.target = e.source
    WHERE NOT EXISTS (SELECT 1 FROM cone x
                      WHERE x.source = r.source AND x.target = r.target);
    DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ping);
  `);
  let frontier = "ping";
  let next = "pong";
  for (;;) {
    db.exec(`DELETE FROM ${next}`);
    const revived = db.prepare(
      `INSERT OR IGNORE INTO ${next}
       SELECT f.source, e.target FROM ${frontier} f
       JOIN edge e ON e.source = f.target
       JOIN cone c ON c.source = f.source AND c.target = e.target`).run().changes;
    if (revived === 0) break;
    db.exec(`DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ${next})`);
    [frontier, next] = [next, frontier];
  }
  db.exec(`DELETE FROM reachable WHERE (source, target) IN (SELECT source, target FROM cone)`);
}

function variantFusedDeferred(db) {
  db.transaction(() => {
    db.exec(`DELETE FROM ping; DELETE FROM pong; DELETE FROM cone;`);
    overDeleteFused(db);
    rederiveDeferred(db);
  })();
}


// B: A minus the tautological head probe in the over-delete hop.
function variantLeanHop(db) {
  db.transaction(() => {
    db.exec(`DELETE FROM ping; DELETE FROM pong; DELETE FROM cone;`);
    seedAndWalk(db, false);
    db.exec(`DELETE FROM reachable WHERE (source, target) IN (SELECT source, target FROM cone)`);
    rederiveShipped(db);
  })();
}

// B+C: lean hop, and head rows only leave at the end (true retractions).
function variantLeanDefer(db) {
  db.transaction(() => {
    db.exec(`DELETE FROM ping; DELETE FROM pong; DELETE FROM cone;`);
    seedAndWalk(db, false);
    rederiveDeferred(db);
  })();
}

function seedAndWalk(db, withHeadProbe) {
  db.exec(`
    INSERT OR IGNORE INTO ping
    SELECT d.source, d.target FROM delta_edge d
    JOIN reachable h ON h.source = d.source AND h.target = d.target;
    INSERT OR IGNORE INTO ping
    SELECT r.source, d.target FROM delta_edge d
    JOIN reachable r ON r.target = d.source
    JOIN reachable h ON h.source = r.source AND h.target = d.target;
    INSERT INTO cone (source, target) SELECT source, target FROM ping;
  `);
  const headProbe = withHeadProbe
    ? `JOIN reachable h ON h.source = f.source AND h.target = e.target`
    : ``;
  let frontier = "ping";
  let next = "pong";
  for (;;) {
    db.exec(`DELETE FROM ${next}`);
    const suspected = db.prepare(
      `INSERT OR IGNORE INTO ${next}
       SELECT f.source, e.target FROM ${frontier} f
       JOIN edge e ON e.source = f.target
       ${headProbe}
       WHERE NOT EXISTS (SELECT 1 FROM cone c
                         WHERE c.source = f.source AND c.target = e.target)`).run().changes;
    if (suspected === 0) break;
    db.exec(`INSERT OR IGNORE INTO cone (source, target) SELECT source, target FROM ${next}`);
    [frontier, next] = [next, frontier];
  }
}

function rederiveShipped(db) {
  db.exec(`
    DELETE FROM ping; DELETE FROM pong;
    INSERT OR IGNORE INTO ping
    SELECT c.source, c.target FROM cone c
    JOIN edge e ON e.source = c.source AND e.target = c.target;
    INSERT OR IGNORE INTO ping
    SELECT c.source, c.target FROM cone c
    JOIN edge e ON e.target = c.target
    JOIN reachable r ON r.source = c.source AND r.target = e.source;
    INSERT OR IGNORE INTO reachable SELECT source, target FROM ping;
    DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ping);
  `);
  let frontier = "ping";
  let next = "pong";
  for (;;) {
    db.exec(`DELETE FROM ${next}`);    const revived = db.prepare(
      `INSERT OR IGNORE INTO ${next}
       SELECT f.source, e.target FROM ${frontier} f
       JOIN edge e ON e.source = f.target
       JOIN cone c ON c.source = f.source AND c.target = e.target`).run().changes;
    if (revived === 0) break;
    db.exec(`
      INSERT OR IGNORE INTO reachable SELECT source, target FROM ${next};
      DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ${next});
    `);
    [frontier, next] = [next, frontier];
  }
}


// E: B plus a mid-walk bail: cone count is free (sum of changes); past the
// threshold the walk abandons and the tick falls back to one full rebuild.
function variantLeanBail(db) {
  const headCount = db.prepare(`SELECT count(*) AS n FROM reachable`).all()[0].n;
  const threshold = Math.floor(headCount / 4);
  let bailed = false;
  db.transaction(() => {
    db.exec(`DELETE FROM ping; DELETE FROM pong; DELETE FROM cone;`);
    let coneCount = db.prepare(`
      INSERT OR IGNORE INTO ping
      SELECT d.source, d.target FROM delta_edge d
      JOIN reachable h ON h.source = d.source AND h.target = d.target`).run().changes;
    coneCount = db.prepare(`
      INSERT OR IGNORE INTO ping
      SELECT r.source, d.target FROM delta_edge d
      JOIN reachable r ON r.target = d.source
      JOIN reachable h ON h.source = r.source AND h.target = d.target`).run().changes + coneCount;
    db.exec(`INSERT INTO cone (source, target) SELECT source, target FROM ping`);
    let frontier = "ping";
    let next = "pong";
    for (;;) {
      if (coneCount > threshold) { bailed = true; break; }
      db.exec(`DELETE FROM ${next}`);
      const suspected = db.prepare(
        `INSERT OR IGNORE INTO ${next}
         SELECT f.source, e.target FROM ${frontier} f
         JOIN edge e ON e.source = f.target
         WHERE NOT EXISTS (SELECT 1 FROM cone c
                           WHERE c.source = f.source AND c.target = e.target)`).run().changes;
      if (suspected === 0) break;
      coneCount += suspected;
      db.exec(`INSERT OR IGNORE INTO cone (source, target) SELECT source, target FROM ${next}`);
      [frontier, next] = [next, frontier];
    }
    if (!bailed) {
      db.exec(`DELETE FROM reachable WHERE (source, target) IN (SELECT source, target FROM cone)`);
      rederiveShipped(db);
    }
  })();
  if (bailed) fullRecompute(db);
}

const gridSide = Number(process.argv[2] ?? 50);
const batch = Number(process.argv[3] ?? 100);
const baseEdges = gridEdges(gridSide);
const batchEdges = [];
for (let index = 0; index < batch; index++) {
  const a = (index * 7919) % (gridSide * gridSide);
  const b = (index * 104729 + 13) % (gridSide * gridSide);
  if (a !== b) batchEdges.push([Math.min(a, b), Math.max(a, b)]);
}

function scenario(name, runVariant) {
  const db = openDb();
  insertRows(db, "edge", baseEdges);
  insertRows(db, "edge", batchEdges);
  fullRecompute(db);
  db.transaction(() => {
    const statement = db.prepare(`DELETE FROM edge WHERE source = ? AND target = ?`);
    for (const edge of batchEdges) statement.run(edge);
  })();
  insertRows(db, "delta_edge", batchEdges);
  const startedAt = performance.now();
  runVariant(db);
  const ms = performance.now() - startedAt;
  const left = checksum(db);
  const twin = openDb();
  insertRows(twin, "edge", db.prepare(`SELECT source, target FROM edge`).all()
    .map((row) => [row.source, row.target]));
  const oracleMs = fullRecompute(twin);
  const right = checksum(twin);
  twin.close();
  const verdict = left === right ? "MATCH" : `MISMATCH ${left} vs ${right}`;
  console.log(`${name}\t${Math.round(ms)}\t${verdict}\t(full recompute ${Math.round(oracleMs)} ms)`);
  db.close();
  if (left !== right) process.exit(1);
}

console.log(`# grid ${gridSide}x${gridSide}, batch=${batchEdges.length}`);
console.log(`# variant\tms\treferee`);
scenario("A_shipped", variantShipped);
scenario("B_lean_hop", variantLeanHop);
scenario("E_lean_bail_25pct", variantLeanBail);
