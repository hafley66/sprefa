import { createHash } from "node:crypto";
import { createRequire } from "node:module";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function closureOracle(graphCase, n) {
  const adjacency = Array.from({ length: n }, () => []);
  if (graphCase === "chain") {
    for (let node = 0; node + 1 < n; node += 1) adjacency[node].push(node + 1);
  } else if (graphCase === "ring") {
    for (let node = 0; node < n; node += 1) adjacency[node].push((node + 1) % n);
  } else {
    throw new Error(`unknown graph case: ${graphCase}`);
  }

  const pairs = [];
  for (let source = 0; source < n; source += 1) {
    const seen = new Uint8Array(n);
    const queue = new Int32Array(n);
    let read = 0;
    let write = 0;
    for (const target of adjacency[source]) {
      if (seen[target]) continue;
      seen[target] = 1;
      queue[write] = target;
      write += 1;
    }
    while (read < write) {
      const node = queue[read];
      read += 1;
      for (const target of adjacency[node]) {
        if (seen[target]) continue;
        seen[target] = 1;
        queue[write] = target;
        write += 1;
      }
    }
    for (let target = 0; target < n; target += 1) {
      if (seen[target]) pairs.push([source, target]);
    }
  }
  return { edges: adjacency.reduce((sum, targets) => sum + targets.length, 0), pairs };
}

export function validateClosure(rows, expected) {
  if (rows.length !== expected.length) {
    throw new Error(`closure count mismatch: ${rows.length} != ${expected.length}`);
  }
  const hash = createHash("sha256");
  for (let index = 0; index < expected.length; index += 1) {
    const actual = [Number(rows[index].source), Number(rows[index].target)];
    if (actual[0] !== expected[index][0] || actual[1] !== expected[index][1]) {
      throw new Error(`closure pair ${index}: ${actual.join(",")} != ${expected[index].join(",")}`);
    }
    hash.update(`${actual[0]}\t${actual[1]}\n`);
  }
  return hash.digest("hex");
}

function elapsedMs(started) {
  return Number(process.hrtime.bigint() - started) / 1_000_000;
}

async function timed(operation) {
  const started = process.hrtime.bigint();
  const value = await operation();
  return { value, ms: elapsedMs(started) };
}

async function openDatabase(runtime) {
  const dependencyDir = process.env.PG_DEPENDENCY_DIR;
  if (!dependencyDir) throw new Error("PG_DEPENDENCY_DIR is required");
  const taskRequire = createRequire(resolve(dependencyDir, "package.json"));
  if (runtime === "pglite-query") {
    const { PGlite } = taskRequire("@electric-sql/pglite");
    const dataDir = await mkdtemp(join(tmpdir(), "runtime-shootout-pglite."));
    const database = new PGlite(dataDir);
    await database.waitReady;
    return {
      exec: (sql) => database.exec(sql),
      query: (sql) => database.query(sql),
      version: async () => (await database.query("SHOW server_version")).rows[0].server_version,
      async close() {
        await database.close();
        await rm(dataDir, { recursive: true, force: true });
      },
    };
  }

  if (process.env.PG_NATIVE_READY !== "1") throw new Error("native PostgreSQL is unavailable");
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
    version: async () => (await database.query("SHOW server_version")).rows[0].server_version,
    close: () => database.end(),
  };
}

async function run() {
  const runtime = process.argv[2];
  const graphCase = process.argv[3];
  const n = Number(process.argv[4]);
  if (!new Set(["pglite-query", "native-postgres-query"]).has(runtime)) {
    throw new Error(`unknown runtime: ${runtime}`);
  }
  if (!Number.isInteger(n) || n < 1) throw new Error(`N must be positive: ${n}`);
  const oracle = closureOracle(graphCase, n);
  const database = await openDatabase(runtime);
  try {
    const version = await database.version();
    const setupStarted = process.hrtime.bigint();
    await database.exec(`
      DROP SCHEMA public CASCADE;
      CREATE SCHEMA public;
      CREATE TABLE edge(source integer NOT NULL, target integer NOT NULL);
    `);
    if (oracle.edges > 0) {
      const values = graphCase === "chain"
        ? Array.from({ length: n - 1 }, (_, source) => `(${source},${source + 1})`)
        : Array.from({ length: n }, (_, source) => `(${source},${(source + 1) % n})`);
      await database.exec(`INSERT INTO edge VALUES ${values.join(",")}; CREATE INDEX edge_source_idx ON edge(source)`);
    }
    const setupMs = elapsedMs(setupStarted);

    const closureStarted = process.hrtime.bigint();
    await database.exec(`
      CREATE TEMP TABLE closure_snapshot AS
      WITH RECURSIVE reachable(source, target) AS (
        SELECT source, target FROM edge
        UNION
        SELECT reachable.source, edge.target
          FROM reachable
          JOIN edge ON edge.source = reachable.target
      )
      SELECT source, target FROM reachable
    `);
    const countResult = await database.query("SELECT count(*) AS count FROM closure_snapshot");
    const closureMs = elapsedMs(closureStarted);
    const closureCount = Number(countResult.rows[0].count);
    if (closureCount !== oracle.pairs.length) {
      throw new Error(`closure count mismatch: ${closureCount} != ${oracle.pairs.length}`);
    }
    const transfer = await timed(() => database.query(
      "SELECT source, target FROM closure_snapshot ORDER BY source, target",
    ));
    const checksum = await timed(() => validateClosure(transfer.value.rows, oracle.pairs));
    console.log(JSON.stringify({
      runtime,
      version,
      case: graphCase,
      n,
      edge_count: oracle.edges,
      closure_count: closureCount,
      setup_ms: setupMs,
      closure_ms: closureMs,
      transfer_ms: transfer.ms,
      checksum_ms: checksum.ms,
      checksum: checksum.value,
      evaluation: "ordinary recursive SQL full query",
      timing_boundary: "recursive query materialization and count; full transfer and exact validation excluded",
    }));
  } finally {
    await database.close();
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await run();
}
