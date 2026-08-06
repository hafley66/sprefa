import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";
import { program, incrementalPlan } from "./.compiled/reachability.ts";
import { readFileSync } from "node:fs";

const OFFSET = 0xcbf29ce484222325n, PRIME = 0x00000100000001b3n, MASK = 0xffffffffffffffffn;
function fnv(source, target) {
  const bytes = new Uint8Array(8), view = new DataView(bytes.buffer);
  view.setUint32(0, source, true); view.setUint32(4, target, true);
  let hash = OFFSET;
  for (const byte of bytes) { hash ^= BigInt(byte); hash = (hash * PRIME) & MASK; }
  return hash;
}
const edges = readFileSync(process.argv[2], "utf8").split("\n")
  .map((line) => line.trim())
  .filter((line) => line.length > 0 && !line.startsWith("p "))
  .map((line) => line.split(/\s+/).map(Number));

function seeded() {
  const db = new Database(":memory:");
  db.exec("PRAGMA journal_mode=WAL;PRAGMA synchronous=NORMAL;PRAGMA temp_store=MEMORY;");
  for (const ddl of program.ddl) db.exec(ddl);
  const insert = db.prepare(`INSERT OR IGNORE INTO "edge" ("source","target") VALUES (?,?)`);
  db.transaction(() => { for (const edge of edges) insert.run(edge); })();
  return db;
}

function grade(name, db, ms, statements) {
  const rows = db.prepare(`SELECT "source","target" FROM "reachable"`).all();
  let sum = 0n;
  for (const row of rows) sum ^= fnv(row.source, row.target);
  const delta = db.prepare(`SELECT count(*) AS n FROM "__delta_reachable" WHERE "_sign"=1`).get().n;
  const front = db.prepare(`SELECT count(*) AS n FROM "__frontier_reachable"`).get().n;
  console.log(`${name.padEnd(26)} ${String(rows.length).padStart(9)} ${sum.toString(16).padStart(16,"0")} ${String(Math.round(ms)).padStart(6)}ms  stmts=${statements}  delta=${delta} frontier=${front}`);
}

const level = incrementalPlan.levels.find((s) => s.supportSql !== null);
const S = level.supportSql;

// A: as the emitter writes it today, three antijoins over the support table.
function shipped() {
  const db = seeded();
  const started = performance.now();
  db.prepare(S[0]).run(); db.prepare(S[1]).run(); db.prepare(S[2]).run();
  db.prepare(S[3]).run(); db.prepare(S[4]).run(); db.prepare(S[5]).run();
  db.prepare(S[6]).run(2); db.prepare(S[8]).run();
  grade("A shipped", db, performance.now() - started, 8);
}

// B: the antijoin runs once into a scratch table; three cheap scans read it.
function materialized() {
  const db = seeded();
  db.exec(`CREATE TEMP TABLE "__new_reachable" ("source" INTEGER NOT NULL, "target" INTEGER NOT NULL, "__refcount" INTEGER NOT NULL)`);
  const started = performance.now();
  db.prepare(S[0]).run(); db.prepare(S[1]).run(); db.prepare(S[2]).run();
  db.prepare(S[3]).run(); db.prepare(S[4]).run();
  db.prepare(`INSERT INTO "__new_reachable" SELECT n."source", n."target", n."__refcount"
      FROM "__support_next_reachable" n
     WHERE NOT EXISTS (SELECT 1 FROM "reachable" h WHERE n."source"=h."source" AND n."target"=h."target")`).run();
  db.prepare(`INSERT INTO "__delta_reachable" ("_sign","_sequence","source","target")
      SELECT 1, row_number() OVER () - 1, "source","target" FROM "__new_reachable"`).run();
  db.prepare(`INSERT INTO "__frontier_reachable" ("_phase","_sequence","source","target")
      SELECT ?, row_number() OVER () - 1, "source","target" FROM "__new_reachable"`).run(2);
  db.prepare(`INSERT INTO "reachable" ("source","target","__refcount")
      SELECT "source","target","__refcount" FROM "__new_reachable"`).run();
  grade("B new-set materialized", db, performance.now() - started, 9);
}

// C: B without the window function, since nothing orders a set rel by _sequence.
function noWindow() {
  const db = seeded();
  db.exec(`CREATE TEMP TABLE "__new_reachable" ("source" INTEGER NOT NULL, "target" INTEGER NOT NULL, "__refcount" INTEGER NOT NULL)`);
  const started = performance.now();
  db.prepare(S[0]).run(); db.prepare(S[1]).run(); db.prepare(S[2]).run();
  db.prepare(S[3]).run(); db.prepare(S[4]).run();
  db.prepare(`INSERT INTO "__new_reachable" SELECT n."source", n."target", n."__refcount"
      FROM "__support_next_reachable" n
     WHERE NOT EXISTS (SELECT 1 FROM "reachable" h WHERE n."source"=h."source" AND n."target"=h."target")`).run();
  db.prepare(`INSERT INTO "__delta_reachable" ("_sign","_sequence","source","target")
      SELECT 1, "rowid"-1, "source","target" FROM "__new_reachable"`).run();
  db.prepare(`INSERT INTO "__frontier_reachable" ("_phase","_sequence","source","target")
      SELECT ?, "rowid"-1, "source","target" FROM "__new_reachable"`).run(2);
  db.prepare(`INSERT INTO "reachable" ("source","target","__refcount")
      SELECT "source","target","__refcount" FROM "__new_reachable"`).run();
  grade("C + rowid for _sequence", db, performance.now() - started, 9);
}

// D: C, and the head is filled straight from the support table with OR IGNORE,
// so the scratch set feeds only the two staging reads.
function orIgnoreHead() {
  const db = seeded();
  db.exec(`CREATE TEMP TABLE "__new_reachable" ("source" INTEGER NOT NULL, "target" INTEGER NOT NULL, "__refcount" INTEGER NOT NULL)`);
  const started = performance.now();
  db.prepare(S[0]).run(); db.prepare(S[1]).run(); db.prepare(S[2]).run();
  db.prepare(S[3]).run(); db.prepare(S[4]).run();
  db.prepare(`INSERT INTO "__new_reachable" SELECT n."source", n."target", n."__refcount"
      FROM "__support_next_reachable" n
     WHERE NOT EXISTS (SELECT 1 FROM "reachable" h WHERE n."source"=h."source" AND n."target"=h."target")`).run();
  db.prepare(`INSERT INTO "__delta_reachable" ("_sign","_sequence","source","target")
      SELECT 1, "rowid"-1, "source","target" FROM "__new_reachable"`).run();
  db.prepare(`INSERT INTO "__frontier_reachable" ("_phase","_sequence","source","target")
      SELECT ?, "rowid"-1, "source","target" FROM "__new_reachable"`).run(2);
  db.prepare(`INSERT OR IGNORE INTO "reachable" ("source","target","__refcount")
      SELECT "source","target","__refcount" FROM "__support_next_reachable"`).run();
  grade("D + OR IGNORE head fill", db, performance.now() - started, 9);
}


function ddlWithout(dropped) {
  return program.ddl.filter((sql) => !dropped.some((name) => sql.includes(`"${name}"`)));
}

function seededDdl(ddl) {
  const db = new Database(":memory:");
  db.exec("PRAGMA journal_mode=WAL;PRAGMA synchronous=NORMAL;PRAGMA temp_store=MEMORY;");
  for (const sql of ddl) db.exec(sql);
  const insert = db.prepare(`INSERT OR IGNORE INTO "edge" ("source","target") VALUES (?,?)`);
  db.transaction(() => { for (const edge of edges) insert.run(edge); })();
  return db;
}

function fastPath(db, label) {
  db.exec(`CREATE TEMP TABLE "__new_reachable" ("source" INTEGER NOT NULL, "target" INTEGER NOT NULL, "__refcount" INTEGER NOT NULL)`);
  const started = performance.now();
  db.prepare(S[0]).run(); db.prepare(S[1]).run(); db.prepare(S[2]).run();
  db.prepare(S[3]).run(); db.prepare(S[4]).run();
  db.prepare(`INSERT INTO "__new_reachable" SELECT n."source", n."target", n."__refcount"
      FROM "__support_next_reachable" n
     WHERE NOT EXISTS (SELECT 1 FROM "reachable" h WHERE n."source"=h."source" AND n."target"=h."target")`).run();
  db.prepare(`INSERT INTO "__delta_reachable" ("_sign","_sequence","source","target")
      SELECT 1, "rowid"-1, "source","target" FROM "__new_reachable"`).run();
  db.prepare(`INSERT INTO "__frontier_reachable" ("_phase","_sequence","source","target")
      SELECT ?, "rowid"-1, "source","target" FROM "__new_reachable"`).run(2);
  db.prepare(`INSERT OR IGNORE INTO "reachable" ("source","target","__refcount")
      SELECT "source","target","__refcount" FROM "__support_next_reachable"`).run();
  grade(label, db, performance.now() - started, 9);
}

// E: the _sign index costs a btree write per staged row and indexes 2 values.
function noSignIndex() {
  fastPath(seededDdl(ddlWithout(["__delta_reachable_sign"])), "E - _sign index");
}

// F: the _phase index on a frontier that one statement scans whole.
function noPhaseIndex() {
  fastPath(seededDdl(ddlWithout(["__delta_reachable_sign", "__frontier_reachable_phase"])), "F - _phase index too");
}

// G: the group index is what boundarySql GROUPs on, so this one should hurt.
function noGroupIndex() {
  fastPath(seededDdl(ddlWithout(["__delta_reachable_sign", "__frontier_reachable_phase", "__delta_reachable_group"])), "G - group index too");
}


// H: drop ONLY the indexes no emitted query reads (idxuse.mjs says which).
function noDeadIndexes() {
  fastPath(
    seededDdl(ddlWithout([
      "__delta_reachable_group", "__delta_edge_group",
      "__next_frontier_reachable_phase", "__next_frontier_edge_phase",
    ])),
    "H - never-read indexes gone",
  );
}

console.log("variant                       derived checksum             ms");
shipped(); materialized(); noWindow(); orIgnoreHead();
noSignIndex(); noPhaseIndex(); noGroupIndex();
noDeadIndexes();
