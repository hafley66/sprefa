import {
  openDatabase,
  loadEdges,
  readEdges,
  foldChecksumConcat,
  hashSourcePrefix,
  hashPair,
  formatChecksum,
} from "./common.mjs";

const BIG = 4294967296;
const TIMEOUT_MS = 130_000;

const EXPECTED = {
  grid_10000: { derived: 1069200, checksum: "9d7239568960d6a8" },
  chain_10000: { derived: 9996213, checksum: "df09b2f409f8b9a8" },
  layered_10000: { derived: 9951396, checksum: "addcf85b5162b9da" },
};

function argumentValue(flag, fallback) {
  const at = process.argv.indexOf(flag);
  return at === -1 ? fallback : process.argv[at + 1];
}

const inputPath = argumentValue("--input", null);
const expNames = argumentValue("--exp", "all").split(",");
if (inputPath === null) {
  process.stderr.write("exp_batch: --input <path> is required\n");
  process.exit(2);
}
const caseName = inputPath.split("/").pop().replace(/\.in$/, "");
const expected = EXPECTED[caseName];

const SCHEMA_ROWID = `
  CREATE TABLE reachable (source INTEGER NOT NULL, target INTEGER NOT NULL);
  CREATE UNIQUE INDEX reachable_pair ON reachable (source, target);
`;
const SCHEMA_PACKED = `
  CREATE TABLE reachable (pair INTEGER PRIMARY KEY);
  CREATE TEMP TABLE frontier_ping (pair INTEGER PRIMARY KEY) WITHOUT ROWID;
  CREATE TEMP TABLE frontier_pong (pair INTEGER PRIMARY KEY) WITHOUT ROWID;
`;

function rowidWavefix(db, hops, ordered) {
  const order = ordered ? ` ORDER BY known.source, edge.target` : "";
  const seed = db.prepare(
    `INSERT OR IGNORE INTO reachable (source, target) SELECT source, target FROM edge`,
  );
  const step = db.prepare(`
    INSERT OR IGNORE INTO reachable (source, target)
    SELECT known.source, edge.target
    FROM reachable known JOIN edge ON edge.source = known.target
    WHERE known.rowid BETWEEN ? AND ?${order}`);
  const dbl = db.prepare(`
    INSERT OR IGNORE INTO reachable (source, target)
    SELECT known.source, e2.target
    FROM reachable known
    JOIN edge e1 ON e1.source = known.target
    JOIN edge e2 ON e2.source = e1.target
    WHERE known.rowid BETWEEN ? AND ?`);
  let rounds = 0;
  let statements = 0;
  db.transaction(() => {
    let low = 1;
    let high = seed.run().changes;
    statements += 1;
    for (;;) {
      let derived;
      if (hops === 2) {
        const c1 = step.run(low, high).changes;
        const c2 = dbl.run(low, high).changes;
        statements += 2;
        derived = c1 + c2;
      } else {
        derived = step.run(low, high).changes;
        statements += 1;
      }
      if (derived === 0) break;
      low = high + 1;
      high += derived;
      rounds += 1;
    }
  })();
  const tally = db.prepare(`SELECT max(rowid) AS top, count(*) AS rows FROM reachable`).get();
  if (tally.top !== tally.rows) {
    throw new Error(`rowid range broke: max(rowid)=${tally.top} count=${tally.rows}`);
  }
  return { rounds, statements };
}

function packedWavefix(db, ordered) {
  const order = ordered ? ` ORDER BY 1` : "";
  const pingToPong = db.prepare(
    `INSERT OR IGNORE INTO frontier_pong (pair)
     SELECT (cur.pair / ${BIG}) * ${BIG} + edge.target
     FROM frontier_ping cur JOIN edge ON edge.source = cur.pair % ${BIG}
     WHERE NOT EXISTS (SELECT 1 FROM reachable r WHERE r.pair = (cur.pair / ${BIG}) * ${BIG} + edge.target)${order}`,
  );
  const pongToPing = db.prepare(
    `INSERT OR IGNORE INTO frontier_ping (pair)
     SELECT (cur.pair / ${BIG}) * ${BIG} + edge.target
     FROM frontier_pong cur JOIN edge ON edge.source = cur.pair % ${BIG}
     WHERE NOT EXISTS (SELECT 1 FROM reachable r WHERE r.pair = (cur.pair / ${BIG}) * ${BIG} + edge.target)${order}`,
  );
  const clearPing = db.prepare(`DELETE FROM frontier_ping`);
  const clearPong = db.prepare(`DELETE FROM frontier_pong`);
  const promotePong = db.prepare(
    `INSERT OR IGNORE INTO reachable (pair) SELECT pair FROM frontier_pong ORDER BY 1`,
  );
  const promotePing = db.prepare(
    `INSERT OR IGNORE INTO reachable (pair) SELECT pair FROM frontier_ping ORDER BY 1`,
  );
  let rounds = 0;
  let statements = 0;
  db.transaction(() => {
    db.prepare(
      `INSERT OR IGNORE INTO reachable (pair) SELECT source * ${BIG} + target FROM edge`,
    ).run();
    db.prepare(
      `INSERT OR IGNORE INTO frontier_ping (pair) SELECT source * ${BIG} + target FROM edge`,
    ).run();
    statements += 2;
    let usePing = true;
    for (;;) {
      if (usePing) {
        clearPong.run();
        const derived = pingToPong.run().changes;
        statements += 2;
        if (derived === 0) break;
        promotePong.run();
        statements += 1;
      } else {
        clearPing.run();
        const derived = pongToPing.run().changes;
        statements += 2;
        if (derived === 0) break;
        promotePing.run();
        statements += 1;
      }
      rounds += 1;
      usePing = !usePing;
    }
  })();
  return { rounds, statements };
}

function dispatchCost(db, count) {
  const noop = db.prepare(`SELECT 1`);
  const start = performance.now();
  db.transaction(() => {
    for (let i = 0; i < count; i++) noop.run();
  })();
  const dispatchMs = performance.now() - start;
  const fusedText = Array.from({ length: count }, () => `SELECT 1`).join("; ");
  const fusedStart = performance.now();
  db.exec(fusedText);
  const fusedMs = performance.now() - fusedStart;
  return { dispatchMs: Math.round(dispatchMs), fusedMs: Math.round(fusedMs) };
}

function foldPacked(db) {
  const statement = db.prepare(`SELECT pair FROM reachable ORDER BY pair`).raw(true);
  let accumulatorHi = 0;
  let accumulatorLo = 0;
  let rowCount = 0;
  let cachedSource = -1;
  let prefixHi = 0;
  let prefixLo = 0;
  for (const row of statement.iterate()) {
    const pair = row[0];
    const target = pair % BIG;
    const source = (pair - target) / BIG;
    if (source !== cachedSource) {
      const prefix = hashSourcePrefix(source);
      prefixHi = prefix[0];
      prefixLo = prefix[1];
      cachedSource = source;
    }
    const pairHash = hashPair(prefixHi, prefixLo, target);
    accumulatorHi ^= pairHash[0];
    accumulatorLo ^= pairHash[1];
    rowCount += 1;
  }
  return { rowCount, checksum: formatChecksum(accumulatorHi, accumulatorLo) };
}

function run(exp, db, derive, foldFn) {
  const fixpointStartedAt = performance.now();
  const shape = derive(db);
  const fixpointMs = performance.now() - fixpointStartedAt;
  const foldStartedAt = performance.now();
  const foldRes = foldFn(db);
  foldRes.foldMs = performance.now() - foldStartedAt;
  const derived = foldRes.rowCount === -1 ? -1 : foldRes.rowCount;
  process.stdout.write(
    `${JSON.stringify({
      exp,
      case: caseName,
      fixpoint_ms: Math.round(fixpointMs),
      fold_ms: Math.round(foldRes.foldMs),
      derived,
      checksum: foldRes.checksum,
      match: foldRes.checksum === expected.checksum,
      rounds: shape.rounds,
      statements: shape.statements,
    })}\n`,
  );
}

for (const exp of expNames) {
  const started = Date.now();
  if (exp === "e1") {
    const db = openDatabase("chosen");
    db.exec(SCHEMA_ROWID);
    const bound = dispatchCost(db, 2582);
    process.stdout.write(
      `${JSON.stringify({ exp: "e1-direct", case: caseName, dispatch_ms: bound.dispatchMs, fused_ms: bound.fusedMs, note: "2582 no-op .run() vs one db.exec of 2582 statements" })}\n`,
    );
    db.close();
  } else if (exp === "e2") {
    const db = openDatabase("chosen");
    db.exec(SCHEMA_ROWID);
    loadEdges(db, readEdges(inputPath).edges);
    run("e2", db, (d) => rowidWavefix(d, 2, false), (d) =>
      Object.assign(foldChecksumConcat(d, "reachable"), { foldMs: 0 }));
    db.close();
  } else if (exp === "e3") {
    const db = openDatabase("chosen");
    db.exec(SCHEMA_PACKED);
    const { edges } = readEdges(inputPath);
    for (const node of edges) {
      if (node[0] >= BIG || node[1] >= BIG) {
        process.stderr.write(`exp e3: node id exceeds 32 bits: ${node}\n`);
        process.exit(1);
      }
    }
    loadEdges(db, edges);
    run("e3", db, (d) => packedWavefix(d, false), (d) =>
      Object.assign(foldPacked(d), { foldMs: 0 }));
    db.close();
  } else if (exp === "e4") {
    const db = openDatabase("chosen");
    db.exec(SCHEMA_ROWID);
    loadEdges(db, readEdges(inputPath).edges);
    run("e4a", db, (d) => rowidWavefix(d, 1, true), (d) =>
      Object.assign(foldChecksumConcat(d, "reachable"), { foldMs: 0 }));
    db.close();
    const dbPacked = openDatabase("chosen");
    dbPacked.exec(SCHEMA_PACKED);
    loadEdges(dbPacked, readEdges(inputPath).edges);
    run("e4b", dbPacked, (d) => packedWavefix(d, true), (d) =>
      Object.assign(foldPacked(d), { foldMs: 0 }));
    dbPacked.close();
  } else if (exp === "e5") {
    const db = openDatabase("chosen");
    db.exec(SCHEMA_ROWID);
    loadEdges(db, readEdges(inputPath).edges);
    run("e5", db, (d) => rowidWavefix(d, 2, true), (d) =>
      Object.assign(foldChecksumConcat(d, "reachable"), { foldMs: 0 }));
    db.close();
  }
  process.stderr.write(`exp_batch: ${exp} on ${caseName} done in ${Date.now() - started}ms\n`);
}
process.stderr.write(`exp_batch: __DONE__ ${caseName}\n`);
