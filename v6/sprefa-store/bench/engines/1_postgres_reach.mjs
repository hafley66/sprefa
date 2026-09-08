import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { readdir, stat } from "node:fs/promises";
import { execFileSync } from "node:child_process";

const sourceDir = dirname(fileURLToPath(import.meta.url));
const dependencyDir = process.env.PG_DEPENDENCY_DIR
  ?? resolve(sourceDir, "../../../labs/exec_shootout/postgres_pglite_ivm");
const taskRequire = createRequire(resolve(dependencyDir, "package.json"));

function addEdge(head, to, next, cursor, parent, child) {
  to[cursor] = child;
  next[cursor] = head[parent];
  head[parent] = cursor;
  return cursor + 1;
}

export function graphOracle(layers, width) {
  const nodes = 2 + layers * width;
  const edges = width + Math.ceil(width / 3) + 2 * (layers - 1) * width;
  const head = new Int32Array(nodes);
  head.fill(-1);
  const to = new Int32Array(edges);
  const next = new Int32Array(edges);
  let cursor = 0;

  for (let column = 0; column < width; column += 1) {
    const child = 2 + column;
    cursor = addEdge(head, to, next, cursor, 0, child);
    if (column % 3 === 0) cursor = addEdge(head, to, next, cursor, 1, child);
  }
  for (let layer = 1; layer < layers; layer += 1) {
    const previous = 2 + (layer - 1) * width;
    for (let column = 0; column < width; column += 1) {
      const child = 2 + layer * width + column;
      cursor = addEdge(head, to, next, cursor, previous + column, child);
      cursor = addEdge(head, to, next, cursor, previous + ((column + 1) % width), child);
    }
  }
  if (cursor !== edges) throw new Error(`edge count mismatch: ${cursor} != ${edges}`);

  const reachable = (roots) => {
    const seen = new Uint8Array(nodes);
    const queue = new Int32Array(nodes);
    let read = 0;
    let write = 0;
    for (const root of roots) {
      if (seen[root]) continue;
      seen[root] = 1;
      queue[write] = root;
      write += 1;
    }
    while (read < write) {
      const parent = queue[read];
      read += 1;
      for (let edge = head[parent]; edge !== -1; edge = next[edge]) {
        const child = to[edge];
        if (seen[child]) continue;
        seen[child] = 1;
        queue[write] = child;
        write += 1;
      }
    }
    return { seen, count: write };
  };

  return { nodes, edges, before: reachable([0, 1]), after: reachable([1]) };
}

export function checksumSeen(seen) {
  const hash = createHash("sha256");
  for (let node = 0; node < seen.length; node += 1) {
    if (seen[node]) hash.update(`${node}\n`);
  }
  return hash.digest("hex");
}

export function validateRows(rows, expected, phase) {
  if (rows.length !== expected.count) {
    throw new Error(`${phase} count mismatch: ${rows.length} != ${expected.count}`);
  }
  const hash = createHash("sha256");
  let cursor = 0;
  for (let node = 0; node < expected.seen.length; node += 1) {
    if (!expected.seen[node]) continue;
    const actual = Number(rows[cursor]?.node);
    if (actual !== node) throw new Error(`${phase} node ${cursor}: ${actual} != ${node}`);
    hash.update(`${actual}\n`);
    cursor += 1;
  }
  const actualChecksum = hash.digest("hex");
  const expectedChecksum = checksumSeen(expected.seen);
  if (actualChecksum !== expectedChecksum) {
    throw new Error(`${phase} checksum mismatch: ${actualChecksum} != ${expectedChecksum}`);
  }
  return actualChecksum;
}

function elapsedMs(started) {
  return Number(process.hrtime.bigint() - started) / 1_000_000;
}

async function timed(operation) {
  const started = process.hrtime.bigint();
  const value = await operation();
  return { value, ms: elapsedMs(started) };
}

function postgresTreeRssKb(rootPid) {
  if (!rootPid) return 0;
  try {
    const lines = execFileSync("/bin/ps", ["-axo", "pid=,ppid=,rss="], { encoding: "utf8" })
      .trim().split("\n");
    const records = lines.map((line) => line.trim().split(/\s+/).map(Number));
    const included = new Set([Number(rootPid)]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const [pid, parent] of records) {
        if (included.has(parent) && !included.has(pid)) {
          included.add(pid);
          changed = true;
        }
      }
    }
    return records.reduce((sum, [pid, , rss]) => sum + (included.has(pid) ? rss : 0), 0);
  } catch {
    return 0;
  }
}

async function directoryBytes(path) {
  let total = 0;
  for (const entry of await readdir(path, { withFileTypes: true })) {
    const child = resolve(path, entry.name);
    total += entry.isDirectory() ? await directoryBytes(child) : (await stat(child)).size;
  }
  return total;
}

async function openDatabase(runtime, dataDir) {
  if (runtime === "pglite") {
    const { PGlite } = taskRequire("@electric-sql/pglite");
    const database = new PGlite(dataDir);
    await database.waitReady;
    return {
      exec: (sql) => database.exec(sql),
      query: (sql) => database.query(sql),
      close: () => database.close(),
      version: async () => (await database.query("SHOW server_version")).rows[0].server_version,
      diskBytes: () => directoryBytes(dataDir),
    };
  }

  const pg = taskRequire("pg");
  const database = new pg.Client({
    host: process.env.PGHOST,
    port: Number(process.env.PGPORT ?? "5432"),
    database: process.env.PGDATABASE,
    user: process.env.PGUSER,
  });
  await database.connect();
  return {
    exec: (sql) => database.query(sql),
    query: (sql) => database.query(sql),
    close: () => database.end(),
    version: async () => (await database.query("SHOW server_version")).rows[0].server_version,
    diskBytes: async () => Number((await database.query("SELECT pg_database_size(current_database()) AS bytes")).rows[0].bytes),
  };
}

function reachSnapshotSql() {
  return `
    CREATE TEMP TABLE alive_snapshot AS
    WITH RECURSIVE alive(node) AS (
      SELECT node FROM root
      UNION
      SELECT edge.child
        FROM alive
        JOIN edge ON edge.parent = alive.node
    )
    SELECT node FROM alive;
  `;
}

async function materializeAndCount(database) {
  await database.exec(reachSnapshotSql());
  const count = await database.query("SELECT count(*) AS count FROM alive_snapshot");
  return Number(count.rows[0].count);
}

async function transferAndValidate(database, expected, phase) {
  const transfer = await timed(() => database.query("SELECT node FROM alive_snapshot ORDER BY node"));
  const checksum = await timed(() => validateRows(transfer.value.rows, expected, phase));
  await database.exec("DROP TABLE alive_snapshot");
  return { checksum: checksum.value, transferMs: transfer.ms, checksumMs: checksum.ms };
}

function safeStatus(value) {
  return String(value).replaceAll("|", "/").replaceAll("\n", " ");
}

async function run() {
  const runtime = process.argv[2];
  const layers = Number(process.argv[3]);
  const width = Number(process.argv[4]);
  const dataDir = process.argv[5];
  if (!new Set(["native", "pglite"]).has(runtime)) throw new Error(`unknown runtime: ${runtime}`);
  if (!Number.isInteger(layers) || layers < 1 || !Number.isInteger(width) || width < 1) {
    throw new Error(`layers and width must be positive integers: ${layers}x${width}`);
  }
  if (runtime === "native" && process.env.PG_NATIVE_READY !== "1") {
    throw new Error(process.env.PG_NATIVE_REASON ?? "native PostgreSQL is unavailable");
  }
  if (runtime === "pglite" && !dataDir) throw new Error("PGlite data directory is required");

  const label = runtime === "native" ? "native-postgres-query" : "pglite-query";
  const oracle = graphOracle(layers, width);
  const database = await openDatabase(runtime, dataDir);
  try {
    const version = await database.version();
    const setupStarted = process.hrtime.bigint();
    await database.exec(`
      DROP SCHEMA public CASCADE;
      CREATE SCHEMA public;
      CREATE TABLE root(node integer PRIMARY KEY);
      CREATE TABLE edge(parent integer NOT NULL, child integer NOT NULL);
      INSERT INTO root VALUES (0), (1);
      INSERT INTO edge
      SELECT 0, 2 + column_id FROM generate_series(0, ${width - 1}) AS column_id;
      INSERT INTO edge
      SELECT 1, 2 + column_id FROM generate_series(0, ${width - 1}) AS column_id
       WHERE column_id % 3 = 0;
      INSERT INTO edge
      SELECT 2 + (layer_id - 1) * ${width} + column_id,
             2 + layer_id * ${width} + column_id
        FROM generate_series(1, ${layers - 1}) AS layer_id
       CROSS JOIN generate_series(0, ${width - 1}) AS column_id;
      INSERT INTO edge
      SELECT 2 + (layer_id - 1) * ${width} + ((column_id + 1) % ${width}),
             2 + layer_id * ${width} + column_id
        FROM generate_series(1, ${layers - 1}) AS layer_id
       CROSS JOIN generate_series(0, ${width - 1}) AS column_id;
      CREATE INDEX edge_parent_idx ON edge(parent);
    `);
    const beforeCount = await materializeAndCount(database);
    const setupMs = elapsedMs(setupStarted);
    if (beforeCount !== oracle.before.count) {
      throw new Error(`before count mismatch: ${beforeCount} != ${oracle.before.count}`);
    }
    const before = await transferAndValidate(database, oracle.before, "before");

    const retractStarted = process.hrtime.bigint();
    await database.exec("BEGIN; DELETE FROM root WHERE node = 0; COMMIT");
    const afterCount = await materializeAndCount(database);
    const retractMs = elapsedMs(retractStarted);
    if (afterCount !== oracle.after.count) {
      throw new Error(`after count mismatch: ${afterCount} != ${oracle.after.count}`);
    }
    const after = await transferAndValidate(database, oracle.after, "after");

    const killed = oracle.before.count - oracle.after.count;
    const databaseMb = (await database.diskBytes()) / 1_048_576;
    const nodeRssKb = Math.round(process.memoryUsage().rss / 1024);
    const sampledRssKb = runtime === "native"
      ? nodeRssKb + postgresTreeRssKb(process.env.PG_POSTMASTER_PID)
      : nodeRssKb;
    const memoryScope = runtime === "native"
      ? "sampled sum of Node client and disposable PostgreSQL process-tree RSS; shared mappings may be counted more than once"
      : "sampled RSS of the Node process containing PGlite WASM";
    const limitScope = runtime === "native"
      ? `DL_MEMCAP_MB=${process.env.DL_MEMCAP_MB ?? "unset"} is unenforced for total PostgreSQL memory`
      : `DL_MEMCAP_MB=${process.env.DL_MEMCAP_MB ?? "unset"} limits Node old-space only; total process RSS is unenforced`;
    console.log(`STATUS|${label}|ok|ordinary recursive SQL full recomputation; timed phases end after materialization and count; exact before=${before.checksum} after=${after.checksum}; untimed transfer_ms before=${before.transferMs.toFixed(3)} after=${after.transferMs.toFixed(3)}; untimed checksum_validation_ms before=${before.checksumMs.toFixed(3)} after=${after.checksumMs.toFixed(3)}; PostgreSQL ${safeStatus(version)}|${memoryScope}|${limitScope}`);
    console.log([
      "CSV", label, oracle.nodes, oracle.edges, killed,
      setupMs.toFixed(3), retractMs.toFixed(3), "N/A",
      (sampledRssKb / 1024).toFixed(1), "N/A", "N/A", databaseMb.toFixed(3),
    ].join(","));
  } finally {
    await database.close();
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  run().catch((error) => {
    const runtime = process.argv[2];
    const label = runtime === "native" ? "native-postgres-query" : "pglite-query";
    console.log(`STATUS|${label}|error|${safeStatus(error.stack ?? error)}|unavailable|unenforced`);
    process.exitCode = 1;
  });
}
