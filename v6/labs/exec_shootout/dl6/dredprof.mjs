// Per-phase / per-round profile of dred.mjs's delete_batch worst case.
// Usage: node dredprof.mjs [gridSide] [batch]   (defaults 50, 100)
import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";

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

function insertRows(db, table, rows) {
  const statement = db.prepare(`INSERT OR IGNORE INTO ${table} VALUES (?, ?)`);
  db.transaction(() => { for (const row of rows) statement.run(row); })();
}

function buildClosure(db) {
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
}

const phases = new Map();
function timed(label, fn) {
  const startedAt = performance.now();
  const changes = fn();
  const ms = performance.now() - startedAt;
  const slot = phases.get(label) ?? { ms: 0, rows: 0, calls: 0 };
  slot.ms += ms;
  slot.rows += changes ?? 0;
  slot.calls += 1;
  phases.set(label, slot);
  return changes;
}

const gridSide = Number(process.argv[2] ?? 50);
const batch = Number(process.argv[3] ?? 100);
const edges = gridEdges(gridSide);
const db = openDb();
insertRows(db, "edge", edges);

const batchEdges = [];
for (let index = 0; index < batch; index++) {
  const a = (index * 7919) % (gridSide * gridSide);
  const b = (index * 104729 + 13) % (gridSide * gridSide);
  if (a !== b) batchEdges.push([Math.min(a, b), Math.max(a, b)]);
}
insertRows(db, "edge", batchEdges);
buildClosure(db);
const headBefore = db.prepare(`SELECT count(*) AS n FROM reachable`).all()[0].n;

db.transaction(() => {
  const statement = db.prepare(`DELETE FROM edge WHERE source = ? AND target = ?`);
  for (const edge of batchEdges) statement.run(edge);
})();
insertRows(db, "delta_edge", batchEdges);

console.log(`# grid ${gridSide}x${gridSide}, head=${headBefore}, deleting ${batchEdges.length} edges`);

const overDeleteSql = (frontier, next) =>
  `INSERT OR IGNORE INTO ${next}
   SELECT f.source, e.target FROM ${frontier} f
   JOIN edge e ON e.source = f.target
   JOIN reachable h ON h.source = f.source AND h.target = e.target
   WHERE NOT EXISTS (SELECT 1 FROM cone c
                     WHERE c.source = f.source AND c.target = e.target)`;

{
  const schemaOnly = openDb();
  for (const row of schemaOnly.prepare(`EXPLAIN QUERY PLAN ${overDeleteSql("ping", "pong")}`).all()) {
    console.log(`EQP over_delete_hop\t${row.detail}`);
  }
  schemaOnly.close();
}

const rounds = [];
const wholeStartedAt = performance.now();
db.transaction(() => {
  db.exec(`DELETE FROM ping; DELETE FROM pong; DELETE FROM cone;`);
  timed("seed_base_arm", () => db.prepare(
    `INSERT OR IGNORE INTO ping
     SELECT d.source, d.target FROM delta_edge d
     JOIN reachable h ON h.source = d.source AND h.target = d.target`).run().changes);
  timed("seed_recursive_arm", () => db.prepare(
    `INSERT OR IGNORE INTO ping
     SELECT r.source, d.target FROM delta_edge d
     JOIN reachable r ON r.target = d.source
     JOIN reachable h ON h.source = r.source AND h.target = d.target`).run().changes);
  timed("seed_to_cone", () => db.prepare(
    `INSERT INTO cone SELECT source, target FROM ping`).run().changes);

  let frontier = "ping";
  let next = "pong";
  for (;;) {
    timed("hop_clear_next", () => db.prepare(`DELETE FROM ${next}`).run().changes);
    const roundStartedAt = performance.now();
    const suspected = timed("hop_generate", () => db.prepare(overDeleteSql(frontier, next)).run().changes);
    const hopMs = performance.now() - roundStartedAt;
    if (suspected === 0) break;
    timed("hop_to_cone", () => db.prepare(
      `INSERT OR IGNORE INTO cone SELECT source, target FROM ${next}`).run().changes);
    rounds.push({ suspected, hopMs });
    [frontier, next] = [next, frontier];
  }

  timed("delete_cone_from_head", () => db.prepare(
    `DELETE FROM reachable WHERE (source, target) IN (SELECT source, target FROM cone)`).run().changes);

  db.exec(`DELETE FROM ping; DELETE FROM pong;`);
  timed("rederive_base_arm", () => db.prepare(
    `INSERT OR IGNORE INTO ping
     SELECT c.source, c.target FROM cone c
     JOIN edge e ON e.source = c.source AND e.target = c.target`).run().changes);
  timed("rederive_recursive_arm", () => db.prepare(
    `INSERT OR IGNORE INTO ping
     SELECT c.source, c.target FROM cone c
     JOIN edge e ON e.target = c.target
     JOIN reachable r ON r.source = c.source AND r.target = e.source`).run().changes);
  timed("rederive_commit", () => db.prepare(
    `INSERT OR IGNORE INTO reachable SELECT source, target FROM ping`).run().changes);
  timed("rederive_uncone", () => db.prepare(
    `DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ping)`).run().changes);

  frontier = "ping";
  next = "pong";
  for (;;) {
    timed("revive_clear_next", () => db.prepare(`DELETE FROM ${next}`).run().changes);
    const revived = timed("revive_generate", () => db.prepare(
      `INSERT OR IGNORE INTO ${next}
       SELECT f.source, e.target FROM ${frontier} f
       JOIN edge e ON e.source = f.target
       JOIN cone c ON c.source = f.source AND c.target = e.target`).run().changes);
    if (revived === 0) break;
    timed("revive_commit", () => db.prepare(
      `INSERT OR IGNORE INTO reachable SELECT source, target FROM ${next}`).run().changes);
    timed("revive_uncone", () => db.prepare(
      `DELETE FROM cone WHERE (source, target) IN (SELECT source, target FROM ${next})`).run().changes);
    [frontier, next] = [next, frontier];
  }
})();
const wholeMs = performance.now() - wholeStartedAt;

const coneLeft = db.prepare(`SELECT count(*) AS n FROM cone`).all()[0].n;
console.log(`# total ${Math.round(wholeMs)} ms, true retractions ${coneLeft}`);
console.log(`# phase\tms\trows\tcalls`);
const ranked = [...phases.entries()].sort((a, b) => b[1].ms - a[1].ms);
for (const [label, slot] of ranked) {
  console.log(`${label}\t${Math.round(slot.ms)}\t${slot.rows}\t${slot.calls}`);
}
const fattest = rounds.map((round, index) => ({ index, ...round }))
  .sort((a, b) => b.hopMs - a.hopMs).slice(0, 8);
console.log(`# fattest over-delete rounds of ${rounds.length}`);
console.log(`# round\tsuspected\tms`);
for (const round of fattest) {
  console.log(`${round.index}\t${round.suspected}\t${Math.round(round.hopMs)}`);
}
db.close();
