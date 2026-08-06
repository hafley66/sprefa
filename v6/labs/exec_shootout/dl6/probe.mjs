import Database from "/Users/chrishafley/projects/sprefa-lanes/emitrace/v6/tsv2/node_modules/.pnpm/libsql@0.5.29/node_modules/libsql/index.js";
import { program, incrementalPlan } from "./.compiled/reachability.ts";
import { readFileSync } from "node:fs";

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

function readEdges(path) {
  const edges = [];
  for (const raw of readFileSync(path, "utf8").split("\n")) {
    const line = raw.trim();
    if (line.length === 0 || line.startsWith("p ")) continue;
    const [source, target] = line.split(/\s+/).map(Number);
    edges.push([source, target]);
  }
  return edges;
}

function openSeeded(edges) {
  const db = new Database(":memory:");
  db.exec("PRAGMA journal_mode=WAL;PRAGMA synchronous=NORMAL;PRAGMA temp_store=MEMORY;");
  for (const ddl of program.ddl) db.exec(ddl);
  const insertEdge = db.prepare(`INSERT OR IGNORE INTO "edge" ("source","target") VALUES (?,?)`);
  db.transaction(() => { for (const edge of edges) insertEdge.run(edge); })();
  return db;
}

function report(name, db, ms, rounds) {
  const rows = db.prepare(`SELECT "source","target" FROM "reachable"`).all();
  let checksum = 0n;
  for (const row of rows) checksum ^= fnv1a64(row.source, row.target);
  console.log(
    `${name}\t${rows.length}\t${checksum.toString(16).padStart(16, "0")}\t${Math.round(ms)}\t${rounds}`,
  );
}

const level = incrementalPlan.levels.find((statement) => statement.insertSql !== null);

// V1: the emitted loop as the runtime drives it today, frontier never retired.
function accumulating(edges) {
  const db = openSeeded(edges);
  const stageEdge = db.prepare(`INSERT INTO "__frontier_edge" ("_phase","_sequence","source","target") VALUES (2,?,?,?)`);
  db.transaction(() => { edges.forEach((edge, i) => stageEdge.run(i, edge[0], edge[1])); })();
  const insert = db.prepare(level.insertSql);
  const stage = db.prepare(`INSERT INTO "__frontier_reachable" ("_phase","_sequence","source","target") VALUES (2,?,?,?)`);
  const startedAt = performance.now();
  let rounds = 0;
  for (;;) {
    rounds += 1;
    const produced = insert.all();
    if (produced.length === 0) break;
    db.transaction(() => { produced.forEach((row, i) => stage.run(i, row.source, row.target)); })();
  }
  return report("accumulating", db, performance.now() - startedAt, rounds);
}

// V2: same emitted SQL, but each round reads only the previous round's rows.
function retired(edges) {
  const db = openSeeded(edges);
  const stageEdge = db.prepare(`INSERT INTO "__frontier_edge" ("_phase","_sequence","source","target") VALUES (2,?,?,?)`);
  db.transaction(() => { edges.forEach((edge, i) => stageEdge.run(i, edge[0], edge[1])); })();
  const insert = db.prepare(level.insertSql);
  const clearReach = db.prepare(`DELETE FROM "__frontier_reachable"`);
  const clearEdge = db.prepare(`DELETE FROM "__frontier_edge"`);
  const stage = db.prepare(`INSERT INTO "__frontier_reachable" ("_phase","_sequence","source","target") VALUES (2,?,?,?)`);
  const startedAt = performance.now();
  let rounds = 0;
  for (;;) {
    rounds += 1;
    const produced = insert.all();
    if (produced.length === 0) break;
    db.transaction(() => {
      clearReach.run();
      clearEdge.run();
      produced.forEach((row, i) => stage.run(i, row.source, row.target));
    })();
  }
  return report("retired", db, performance.now() - startedAt, rounds);
}

// V3: the SQLite floor, one recursive CTE, no round trip at all.
function recursiveCte(edges) {
  const db = openSeeded(edges);
  const startedAt = performance.now();
  db.exec(`INSERT OR IGNORE INTO "reachable" ("source","target")
    WITH RECURSIVE "closure" ("source","target") AS (
      SELECT b0."source", b0."target" FROM "edge" b0
      UNION
      SELECT b0."source", b1."target" FROM "closure" b0, "edge" b1 WHERE b1."source" = b0."target"
    ) SELECT "source","target" FROM "closure"`);
  return report("recursive_cte", db, performance.now() - startedAt, 1);
}

const edges = readEdges(process.argv[2]);
console.log("variant\tderived\tchecksum\tms\trounds");
accumulating(edges);
retired(edges);
recursiveCte(edges);
