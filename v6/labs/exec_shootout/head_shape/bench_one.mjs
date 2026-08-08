import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const HERE = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
// libsql is a transitive dep of @libsql/client, so it lives in the pnpm store
// rather than at the top of tsv2/node_modules.
const LIBSQL_PATH = join(
  HERE, "..", "..", "..", "tsv2", "node_modules", ".pnpm",
  "libsql@0.5.29", "node_modules", "libsql", "index.js",
);
const libsqlModule = require(LIBSQL_PATH);
const Database = libsqlModule.default ?? libsqlModule;

function argumentValue(flag, fallback) {
  const at = process.argv.indexOf(flag);
  return at === -1 ? fallback : process.argv[at + 1];
}

const inputPath = argumentValue("--input", null);
const armName = argumentValue("--arm", null);
if (inputPath === null || armName === null) {
  process.stderr.write("head_shape: --input <path.tin> --arm <name> required\n");
  process.exit(2);
}

// The .tin first line is `p <nodes> <edges> text4`; every later line is four
// tab-separated strings: from_path, from_name, to_path, to_name.
function readTextEdges(path) {
  const lines = readFileSync(path, "utf8").split("\n");
  const edges = [];
  for (let index = 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (line.length === 0) continue;
    const parts = line.split("\t");
    if (parts.length !== 4) throw new Error(`bad .tin line ${index + 1}`);
    edges.push(parts);
  }
  return edges;
}

const REFCOUNT = `, "__refcount" INTEGER NOT NULL DEFAULT 1`;
const KEY4 = `"from_path", "from_name", "to_path", "to_name"`;

function columnsSql(sqlType) {
  return [
    `"from_path" ${sqlType} NOT NULL`,
    `"from_name" ${sqlType} NOT NULL`,
    `"to_path" ${sqlType} NOT NULL`,
    `"to_name" ${sqlType} NOT NULL`,
  ].join(", ");
}

const arms = {
  // The head the emitter writes today for a level-headed rel.
  wor: {
    keyType: "INTEGER",
    headDdl: `CREATE TABLE "flow_reach" (${columnsSql("INTEGER")}${REFCOUNT}, PRIMARY KEY (${KEY4})) WITHOUT ROWID`,
    delta: "pingpong",
  },
  // The candidate flip: rowid + UNIQUE, same wavefront, storage is the only move.
  rowid_unique: {
    keyType: "INTEGER",
    headDdl: `CREATE TABLE "flow_reach" ("__id" INTEGER PRIMARY KEY, ${columnsSql("INTEGER")}${REFCOUNT}, UNIQUE (${KEY4}))`,
    delta: "pingpong",
  },
  // rowid + UNIQUE with the rowid-range delta, which only a rowid head admits.
  rowid_range: {
    keyType: "INTEGER",
    headDdl: `CREATE TABLE "flow_reach" ("__id" INTEGER PRIMARY KEY, ${columnsSql("INTEGER")}${REFCOUNT}, UNIQUE (${KEY4}))`,
    delta: "range",
  },
  // The pre-flip direct baseline: raw TEXT in every key column.
  wor_text: {
    keyType: "TEXT",
    headDdl: `CREATE TABLE "flow_reach" (${columnsSql("TEXT")}${REFCOUNT}, PRIMARY KEY (${KEY4})) WITHOUT ROWID`,
    delta: "pingpong",
  },
  rowid_unique_text: {
    keyType: "TEXT",
    headDdl: `CREATE TABLE "flow_reach" ("__id" INTEGER PRIMARY KEY, ${columnsSql("TEXT")}${REFCOUNT}, UNIQUE (${KEY4}))`,
    delta: "pingpong",
  },
};

const arm = arms[armName];
if (arm === undefined) {
  process.stderr.write(`head_shape: unknown arm ${armName}\n`);
  process.exit(2);
}

const database = new Database(":memory:");
database.exec(`PRAGMA page_size=16384`);
database.exec(`PRAGMA temp_store=MEMORY`);

const readStartedAt = performance.now();
const textEdges = readTextEdges(inputPath);
const readMs = performance.now() - readStartedAt;

// Interning is the ingest door's set-based pass: one Map for the tick, one
// INSERT OR IGNORE over a json_each batch, one read-back of the ids.
let internMs = 0;
let internDistinctMs = 0;
let internPayloadMs = 0;
let internStatementMs = 0;
let dictionaryRows = 0;
let edgeRows;

if (arm.keyType === "INTEGER") {
  database.exec(
    `CREATE TABLE "__str" ("__id" INTEGER PRIMARY KEY, "content" TEXT NOT NULL UNIQUE)`,
  );
  const internStartedAt = performance.now();
  const distinct = new Set();
  for (const edge of textEdges) {
    distinct.add(edge[0]);
    distinct.add(edge[1]);
    distinct.add(edge[2]);
    distinct.add(edge[3]);
  }
  const distinctMs = performance.now() - internStartedAt;
  const payload = JSON.stringify([...distinct]);
  const payloadMs = performance.now() - internStartedAt - distinctMs;
  const identifierOf = new Map();
  // internSql and lookupSql are the two statements the emitter writes
  // (flagship module, textInternPlan); the read-back is the emitted join.
  database.transaction(() => {
    database
      .prepare(
        `INSERT OR IGNORE INTO "__str" ("content") SELECT i.value FROM json_each(?) i`,
      )
      .run(payload);
    const lookup = database.prepare(
      `SELECT s."content" AS "__lookup", s."__id" AS "__id" FROM json_each(?) i JOIN "__str" s ON s."content" = i.value`,
    );
    for (const row of lookup.iterate(payload)) identifierOf.set(row.__lookup, row.__id);
  })();
  internStatementMs = performance.now() - internStartedAt - distinctMs - payloadMs;
  internDistinctMs = distinctMs;
  internPayloadMs = payloadMs;
  edgeRows = textEdges.map((edge) => [
    identifierOf.get(edge[0]),
    identifierOf.get(edge[1]),
    identifierOf.get(edge[2]),
    identifierOf.get(edge[3]),
  ]);
  internMs = performance.now() - internStartedAt;
  dictionaryRows = identifierOf.size;
} else {
  edgeRows = textEdges;
}

const sourceType = arm.keyType;
database.exec(
  `CREATE TABLE "flow_edge" (${columnsSql(sourceType)}${REFCOUNT}, PRIMARY KEY (${KEY4})) WITHOUT ROWID`,
);
database.exec(arm.headDdl);

const loadStartedAt = performance.now();
const insertEdge = database.prepare(
  `INSERT OR IGNORE INTO "flow_edge" (${KEY4}) VALUES (?, ?, ?, ?)`,
);
database.transaction(() => {
  for (const row of edgeRows) insertEdge.run(row[0], row[1], row[2], row[3]);
})();
const loadMs = performance.now() - loadStartedAt;

// The head insert with no join feeding it: the G2 / dict-vs-direct number,
// isolated from the fixpoint.
database.exec(
  arm.headDdl.replace(`"flow_reach"`, `"__insert_probe"`).replace(
    /UNIQUE \(/,
    `UNIQUE (`,
  ),
);
const probeInsert = database.prepare(
  `INSERT OR IGNORE INTO "__insert_probe" (${KEY4}) VALUES (?, ?, ?, ?)`,
);
const probeStartedAt = performance.now();
database.transaction(() => {
  for (const row of edgeRows) probeInsert.run(row[0], row[1], row[2], row[3]);
})();
const headInsertMs = performance.now() - probeStartedAt;
database.exec(`DROP TABLE "__insert_probe"`);

const frontierDdl = (name) =>
  `CREATE TEMP TABLE "${name}" (${columnsSql(sourceType)}, PRIMARY KEY (${KEY4})) WITHOUT ROWID`;

let rounds = 0;
let statements = 0;

function pingPongFixpoint() {
  database.exec(frontierDdl("__ping_flow_reach"));
  database.exec(frontierDdl("__pong_flow_reach"));
  const stepSql = (frontier, next) => `
    INSERT OR IGNORE INTO "${next}" (${KEY4})
    SELECT frontier."from_path", frontier."from_name", edge."to_path", edge."to_name"
    FROM "${frontier}" frontier
    JOIN "flow_edge" edge
      ON edge."from_path" = frontier."to_path"
     AND edge."from_name" = frontier."to_name"
    WHERE NOT EXISTS (
      SELECT 1 FROM "flow_reach" known
      WHERE known."from_path" = frontier."from_path"
        AND known."from_name" = frontier."from_name"
        AND known."to_path" = edge."to_path"
        AND known."to_name" = edge."to_name")`;
  const pingToPong = database.prepare(stepSql("__ping_flow_reach", "__pong_flow_reach"));
  const pongToPing = database.prepare(stepSql("__pong_flow_reach", "__ping_flow_reach"));
  const promoteSql = (frontier) =>
    `INSERT OR IGNORE INTO "flow_reach" (${KEY4}) SELECT ${KEY4} FROM "${frontier}"`;
  const promotePing = database.prepare(promoteSql("__ping_flow_reach"));
  const promotePong = database.prepare(promoteSql("__pong_flow_reach"));
  const clearPing = database.prepare(`DELETE FROM "__ping_flow_reach"`);
  const clearPong = database.prepare(`DELETE FROM "__pong_flow_reach"`);
  database.transaction(() => {
    database
      .prepare(
        `INSERT OR IGNORE INTO "__ping_flow_reach" (${KEY4}) SELECT ${KEY4} FROM "flow_edge"`,
      )
      .run();
    promotePing.run();
    statements += 2;
    let usePing = true;
    for (;;) {
      const clear = usePing ? clearPong : clearPing;
      const step = usePing ? pingToPong : pongToPing;
      const promote = usePing ? promotePong : promotePing;
      clear.run();
      const derived = step.run().changes;
      statements += 2;
      if (derived === 0) break;
      promote.run();
      statements += 1;
      rounds += 1;
      usePing = !usePing;
    }
  })();
}

function rangeFixpoint() {
  const seed = database.prepare(
    `INSERT OR IGNORE INTO "flow_reach" (${KEY4}) SELECT ${KEY4} FROM "flow_edge"`,
  );
  const step = database.prepare(`
    INSERT OR IGNORE INTO "flow_reach" (${KEY4})
    SELECT known."from_path", known."from_name", edge."to_path", edge."to_name"
    FROM "flow_reach" known
    JOIN "flow_edge" edge
      ON edge."from_path" = known."to_path"
     AND edge."from_name" = known."to_name"
    WHERE known."__id" BETWEEN ? AND ?`);
  database.transaction(() => {
    let low = 1;
    let high = seed.run().changes;
    statements += 1;
    for (;;) {
      const derived = step.run(low, high).changes;
      statements += 1;
      if (derived === 0) break;
      low = high + 1;
      high += derived;
      rounds += 1;
    }
  })();
}

const fixpointStartedAt = performance.now();
if (arm.delta === "pingpong") pingPongFixpoint();
else rangeFixpoint();
const fixpointMs = performance.now() - fixpointStartedAt;

const derived = database.prepare(`SELECT count(*) AS rows FROM "flow_reach"`).get().rows;

// Order-independent fold so every INTEGER arm is comparable row-for-row; the
// TEXT arms carry a different id space and are compared on count alone.
let checksum = 0;
if (arm.keyType === "INTEGER") {
  const folded = database
    .prepare(
      `SELECT sum("from_path" * 1000003 + "from_name") AS head,
              sum("to_path" * 1000003 + "to_name") AS tail
       FROM "flow_reach"`,
    )
    .get();
  checksum = `${folded.head}:${folded.tail}`;
} else {
  checksum = null;
}

// Materialize back to TEXT: what a render boundary pays on an interned head.
let materializeMs = 0;
if (arm.keyType === "INTEGER") {
  const materializeStartedAt = performance.now();
  const materialized = database
    .prepare(
      `SELECT count(*) AS rows FROM (
         SELECT (SELECT s."content" FROM "__str" s WHERE s."__id" = t."from_path") AS a,
                (SELECT s."content" FROM "__str" s WHERE s."__id" = t."from_name") AS b,
                (SELECT s."content" FROM "__str" s WHERE s."__id" = t."to_path") AS c,
                (SELECT s."content" FROM "__str" s WHERE s."__id" = t."to_name") AS d
         FROM "flow_reach" t)`,
    )
    .get().rows;
  materializeMs = performance.now() - materializeStartedAt;
  if (materialized !== derived) throw new Error("materialize row count drifted");
}

const pageCount = database.prepare(`PRAGMA page_count`).get();
const pageSize = database.prepare(`PRAGMA page_size`).get();
const databaseBytes =
  (pageCount.page_count ?? Object.values(pageCount)[0]) *
  (pageSize.page_size ?? Object.values(pageSize)[0]);

process.stdout.write(
  `${JSON.stringify({
    arm: armName,
    input: inputPath.split("/").pop(),
    edges: edgeRows.length,
    derived,
    checksum,
    dictionaryRows,
    readMs: Math.round(readMs),
    internMs: Math.round(internMs * 100) / 100,
    internDistinctMs: Math.round(internDistinctMs * 100) / 100,
    internPayloadMs: Math.round(internPayloadMs * 100) / 100,
    internStatementMs: Math.round(internStatementMs * 100) / 100,
    loadMs: Math.round(loadMs),
    headInsertMs: Math.round(headInsertMs * 100) / 100,
    fixpointMs: Math.round(fixpointMs),
    materializeMs: Math.round(materializeMs * 100) / 100,
    rounds,
    statements,
    databaseBytes,
    peakRssKb: process.resourceUsage().maxRSS,
  })}\n`,
);
database.close();
