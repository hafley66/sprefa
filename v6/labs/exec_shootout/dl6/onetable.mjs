// G1 probe: one bitemporal table vs the emitted multi-table staging shape.
// Usage: node onetable.mjs [gridSide]   (default 40)
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

function checksumRows(rows) {
  let hash = 0n;
  for (const row of rows) hash ^= fnv1a64(row.source, row.target);
  return `${rows.length}:${hash.toString(16).padStart(16, "0")}`;
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

function baseDb(edges) {
  const db = new Database(":memory:");
  db.exec(`
    PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA temp_store=MEMORY;
    CREATE TABLE edge (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
  `);
  const statement = db.prepare(`INSERT OR IGNORE INTO edge VALUES (?, ?)`);
  db.transaction(() => { for (const edge of edges) statement.run(edge); })();
  return db;
}

// A: the emitted shape, one tick. Closure into a next-table, then the tail:
// head fill + delta + two mailboxes + arrival scratch. 7 writes per row.
function multiTableTick(db) {
  db.exec(`
    CREATE TABLE reachable (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE TEMP TABLE next_truth (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE TEMP TABLE wave_a (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE TEMP TABLE wave_b (source INTEGER NOT NULL, target INTEGER NOT NULL,
      PRIMARY KEY (source, target)) WITHOUT ROWID;
    CREATE TEMP TABLE born_scratch (source INTEGER, target INTEGER);
    CREATE TEMP TABLE delta_log (sign INTEGER, sequence INTEGER, source INTEGER, target INTEGER);
    CREATE TEMP TABLE mailbox_now (phase INTEGER, sequence INTEGER, source INTEGER, target INTEGER);
    CREATE TEMP TABLE mailbox_next (phase INTEGER, sequence INTEGER, source INTEGER, target INTEGER);
  `);
  const startedAt = performance.now();
  db.transaction(() => {
    db.exec(`INSERT OR IGNORE INTO wave_a SELECT source, target FROM edge`);
    db.exec(`INSERT OR IGNORE INTO next_truth SELECT source, target FROM wave_a`);
    let frontier = "wave_a";
    let next = "wave_b";
    for (;;) {
      db.exec(`DELETE FROM ${next}`);
      const derived = db.prepare(
        `INSERT OR IGNORE INTO ${next}
         SELECT f.source, e.target FROM ${frontier} f
         JOIN edge e ON e.source = f.target
         WHERE NOT EXISTS (SELECT 1 FROM next_truth n
                           WHERE n.source = f.source AND n.target = e.target)`).run().changes;
      if (derived === 0) break;
      db.exec(`INSERT OR IGNORE INTO next_truth SELECT source, target FROM ${next}`);
      [frontier, next] = [next, frontier];
    }
    db.exec(`
      INSERT INTO born_scratch SELECT n.source, n.target FROM next_truth n
        LEFT JOIN reachable h ON h.source = n.source AND h.target = n.target
        WHERE h.source IS NULL;
      INSERT INTO delta_log SELECT 1, rowid - 1, source, target FROM born_scratch;
      INSERT INTO mailbox_now SELECT 2, rowid - 1, source, target FROM born_scratch;
      INSERT INTO mailbox_next SELECT 1, rowid - 1, source, target FROM born_scratch;
      INSERT OR IGNORE INTO reachable SELECT source, target FROM next_truth;
    `);
  })();
  const fixpointMs = performance.now() - startedAt;
  const events = db.prepare(`SELECT source, target FROM delta_log WHERE sign = 1`).all();
  const rows = db.prepare(`SELECT source, target FROM reachable`).all();
  return { fixpointMs, events: checksumRows(events), state: checksumRows(rows) };
}

// B: G1 shape. ONE table with born/died columns; live-set and events are
// INDEX views of the same rows. Wavefront = the rows born in round R.
function oneTableTick(db) {
  db.exec(`
    CREATE TABLE reachable (source INTEGER NOT NULL, target INTEGER NOT NULL,
      born_tick INTEGER NOT NULL, born_round INTEGER NOT NULL, died_tick INTEGER);
    CREATE UNIQUE INDEX reachable_live ON reachable (source, target) WHERE died_tick IS NULL;
    CREATE INDEX reachable_born ON reachable (born_tick);
    CREATE INDEX reachable_wave ON reachable (born_round);
  `);
  const tick = 1;
  const startedAt = performance.now();
  db.transaction(() => {
    db.exec(`INSERT INTO reachable (source, target, born_tick, born_round)
             SELECT source, target, ${tick}, 0 FROM edge`);
    const hop = db.prepare(
      `INSERT INTO reachable (source, target, born_tick, born_round)
       SELECT DISTINCT f.source, e.target, ${tick}, ?
       FROM reachable f JOIN edge e ON e.source = f.target
       WHERE f.born_round = ? AND f.died_tick IS NULL
         AND NOT EXISTS (SELECT 1 FROM reachable n
                         WHERE n.source = f.source AND n.target = e.target
                           AND n.died_tick IS NULL)`);
    for (let round = 1; ; round++) {
      if (hop.run([round, round - 1]).changes === 0) break;
    }
  })();
  const fixpointMs = performance.now() - startedAt;
  const events = db.prepare(
    `SELECT source, target FROM reachable WHERE born_tick = ${tick}`).all();
  const rows = db.prepare(
    `SELECT source, target FROM reachable WHERE died_tick IS NULL`).all();
  return { fixpointMs, events: checksumRows(events), state: checksumRows(rows) };
}

const gridSide = Number(process.argv[2] ?? 40);
const edges = gridEdges(gridSide);
console.log(`# grid ${gridSide}x${gridSide}, ${edges.length} edges`);
console.log(`# shape\tfixpoint_ms\tevents_checksum\tstate_checksum`);

for (const [name, run] of [["A_multi_table", multiTableTick], ["B_one_table", oneTableTick]]) {
  const db = baseDb(edges);
  const result = run(db);
  console.log(`${name}\t${Math.round(result.fixpointMs)}\t${result.events}\t${result.state}`);
  db.close();
}
