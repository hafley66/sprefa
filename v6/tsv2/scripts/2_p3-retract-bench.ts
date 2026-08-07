import { statSync, unlinkSync, writeFileSync } from "node:fs";
import process from "node:process";

import { blake3 } from "@noble/hashes/blake3.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import { firstValueFrom } from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { ScratchStore } from "../runtime/scratchStore.ts";
import type { ISqlSeam, QueryResult, SqlStatement } from "../runtime/types.ts";

const TAG_STRIDE = 1_000_000_000;
const INSERT_CHUNK = 4_000;

type Shape = "DAG" | "CYC";
type HashState = ReturnType<typeof blake3.create>;

type Args = {
  readonly shape: Shape;
  readonly layers: number;
  readonly width: number;
  readonly stride: number;
  readonly db_path: string;
};

type NodeAddress = {
  readonly tier: number;
  readonly offset: number;
  readonly key: number;
};

const EXPECTED = new Map([
  ["DAG:6:10000:0", { input_hash: "e711731b974e1703", survivors: 50_002 }],
  ["DAG:6:40000:0", { input_hash: "db02ab7cd976622b", survivors: 200_002 }],
  ["DAG:6:160000:0", { input_hash: "ef153ee39296ef0f", survivors: 800_002 }],
  ["CYC:6:10000:7", { input_hash: "4ce3fce20608f808", survivors: 51_430 }],
  ["CYC:6:160000:7", { input_hash: "a10c4ce974755186", survivors: 815_240 }],
]);

function parse_args(): Args {
  const [, , shape_text, layers_text, width_text, stride_text, db_path] = process.argv;
  if (shape_text !== "DAG" && shape_text !== "CYC") {
    throw new Error("2_p3-retract-bench: shape must be DAG or CYC");
  }
  const layers = Number(layers_text);
  const width = Number(width_text);
  const stride = Number(stride_text);
  if (
    !Number.isInteger(layers) ||
    !Number.isInteger(width) ||
    !Number.isInteger(stride) ||
    layers < 1 ||
    width < 1 ||
    stride < 0
  ) {
    throw new Error("2_p3-retract-bench: invalid layers, width, or stride");
  }
  if (shape_text === "DAG" && stride !== 0) {
    throw new Error("2_p3-retract-bench: DAG requires stride 0");
  }
  if (shape_text === "CYC" && stride === 0) {
    throw new Error("2_p3-retract-bench: CYC requires a positive stride");
  }
  if (db_path === undefined || db_path.length === 0) {
    throw new Error("2_p3-retract-bench: missing scratch database path");
  }
  return { shape: shape_text, layers, width, stride, db_path };
}

function tag_of_tier(tier: number): number {
  return tier % 3;
}

function local_id(layers: number, width: number, tier: number, offset: number): number {
  if (tier === 0) return offset;
  const tag = tag_of_tier(tier);
  let earlier_tier_count = 0;
  for (let candidate = 1; candidate < tier; candidate += 1) {
    if (tag_of_tier(candidate) === tag) earlier_tier_count += 1;
  }
  return (tag === 0 ? 2 : 0) + earlier_tier_count * width + offset;
}

function key_of(layers: number, width: number, tier: number, offset: number): number {
  return tag_of_tier(tier) * TAG_STRIDE + local_id(layers, width, tier, offset);
}

function global_id(width: number, tier: number, offset: number): number {
  return tier === 0 ? offset : 2 + (tier - 1) * width + offset;
}

function* nodes_in_key_order(layers: number, width: number): Generator<NodeAddress> {
  for (let tag = 0; tag < 3; tag += 1) {
    if (tag === 0) {
      yield { tier: 0, offset: 0, key: 0 };
      yield { tier: 0, offset: 1, key: 1 };
    }
    for (let tier = 1; tier <= layers; tier += 1) {
      if (tag_of_tier(tier) !== tag) continue;
      for (let offset = 0; offset < width; offset += 1) {
        yield { tier, offset, key: key_of(layers, width, tier, offset) };
      }
    }
  }
}

function update_i64_pair(hash: HashState, parent: number, child: number): void {
  const bytes = new Uint8Array(16);
  const view = new DataView(bytes.buffer);
  view.setBigInt64(0, BigInt(parent), true);
  view.setBigInt64(8, BigInt(child), true);
  hash.update(bytes);
}

function update_i64(hash: HashState, key: number): void {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setBigInt64(0, BigInt(key), true);
  hash.update(bytes);
}

async function execute(seam: ISqlSeam, statement: SqlStatement): Promise<QueryResult> {
  return firstValueFrom(seam.runner.execute(seam.db, statement));
}

async function flush_rows(
  seam: ISqlSeam,
  rows: Array<readonly [number, number]>,
): Promise<void> {
  if (rows.length === 0) return;
  await execute(seam, {
    sql: `INSERT INTO cx_row(key,weight)
          SELECT json_extract(value,'$[0]'), json_extract(value,'$[1]')
          FROM json_each(?)`,
    args: [JSON.stringify(rows)],
  });
  rows.length = 0;
}

async function flush_edges(
  seam: ISqlSeam,
  edges: Array<readonly [number, number]>,
): Promise<void> {
  if (edges.length === 0) return;
  await execute(seam, {
    sql: `INSERT INTO cx_dep(parent_key,child_key)
          SELECT json_extract(value,'$[0]'), json_extract(value,'$[1]')
          FROM json_each(?)`,
    args: [JSON.stringify(edges)],
  });
  edges.length = 0;
}

function row_weight(args: Args, tier: number, offset: number): number {
  if (tier === 0) return 1;
  let weight = tier === 1 ? 1 + (offset % 3 === 0 ? 1 : 0) : 2;
  if (
    args.stride > 0 &&
    tier < args.layers &&
    global_id(args.width, tier + 1, offset) % args.stride === 0
  ) {
    weight += 1;
  }
  return weight;
}

async function load_rows(seam: ISqlSeam, args: Args): Promise<void> {
  const rows: Array<readonly [number, number]> = [];
  for (const node of nodes_in_key_order(args.layers, args.width)) {
    rows.push([node.key, row_weight(args, node.tier, node.offset)]);
    if (rows.length === INSERT_CHUNK) await flush_rows(seam, rows);
  }
  await flush_rows(seam, rows);
}

async function add_edge(
  seam: ISqlSeam,
  edges: Array<readonly [number, number]>,
  input_hash: HashState,
  parent: number,
  child: number,
): Promise<number> {
  edges.push([parent, child]);
  update_i64_pair(input_hash, parent, child);
  if (edges.length === INSERT_CHUNK) await flush_edges(seam, edges);
  return 1;
}

async function load_edges(
  seam: ISqlSeam,
  args: Args,
): Promise<{ readonly edge_count: number; readonly input_hash: string }> {
  const edges: Array<readonly [number, number]> = [];
  const input_hash = blake3.create();
  let edge_count = 0;
  for (const node of nodes_in_key_order(args.layers, args.width)) {
    if (node.tier === 0) {
      const start = node.offset === 0 ? 0 : 0;
      const step = node.offset === 0 ? 1 : 3;
      for (let child_offset = start; child_offset < args.width; child_offset += step) {
        edge_count += await add_edge(
          seam,
          edges,
          input_hash,
          node.key,
          key_of(args.layers, args.width, 1, child_offset),
        );
      }
      continue;
    }
    const children: number[] = [];
    if (node.tier < args.layers) {
      children.push(
        key_of(args.layers, args.width, node.tier + 1, node.offset),
        key_of(
          args.layers,
          args.width,
          node.tier + 1,
          (node.offset + args.width - 1) % args.width,
        ),
      );
    }
    if (
      args.stride > 0 &&
      node.tier >= 2 &&
      global_id(args.width, node.tier, node.offset) % args.stride === 0
    ) {
      children.push(key_of(args.layers, args.width, node.tier - 1, node.offset));
    }
    children.sort((left, right) => left - right);
    for (const child of children) {
      edge_count += await add_edge(seam, edges, input_hash, node.key, child);
    }
  }
  await flush_edges(seam, edges);
  return { edge_count, input_hash: bytesToHex(input_hash.digest()).slice(0, 16) };
}

async function boot(seam: ISqlSeam): Promise<void> {
  await firstValueFrom(
    seam.runner.executeMultiple(
      seam.db,
      `PRAGMA journal_mode=WAL;
       PRAGMA synchronous=NORMAL;
       PRAGMA cache_size=-262144;
       PRAGMA mmap_size=1073741824;
       CREATE TABLE cx_row (
         key INTEGER PRIMARY KEY,
         weight INTEGER NOT NULL
       );
       CREATE TABLE cx_dep (
         parent_key INTEGER NOT NULL,
         child_key INTEGER NOT NULL,
         PRIMARY KEY(parent_key,child_key)
       ) WITHOUT ROWID;
       CREATE INDEX ix_cx_dep_child ON cx_dep(child_key);
       CREATE TEMP TABLE cx_frontier(key INTEGER PRIMARY KEY);
       CREATE TEMP TABLE cx_next(key INTEGER PRIMARY KEY);
       CREATE TEMP TABLE cx_hits(key INTEGER PRIMARY KEY, dec INTEGER NOT NULL);
       CREATE TEMP TABLE cx_cone(key INTEGER PRIMARY KEY);`,
    ),
  );
}

function heap_sampler(): { readonly sample: () => void; readonly peak_mb: () => number } {
  let peak = process.memoryUsage().heapUsed;
  return {
    sample(): void {
      peak = Math.max(peak, process.memoryUsage().heapUsed);
    },
    peak_mb(): number {
      return peak / 1_048_576;
    },
  };
}

async function retract_count(
  seam: ISqlSeam,
  sample: () => void,
): Promise<void> {
  let frontier = "cx_frontier";
  let next = "cx_next";
  await seam.db.execute("BEGIN IMMEDIATE");
  try {
    await execute(seam, "DELETE FROM cx_frontier");
    await execute(seam, "DELETE FROM cx_next");
    await execute(seam, "UPDATE cx_row SET weight=weight-1 WHERE key=0");
    await execute(
      seam,
      "INSERT INTO cx_frontier SELECT key FROM cx_row WHERE key=0 AND weight<=0",
    );
    sample();
    while (true) {
      const frontier_count = Number(
        (await execute(seam, `SELECT count(*) AS n FROM ${frontier}`)).rows[0]?.n ?? 0,
      );
      sample();
      if (frontier_count === 0) break;
      await execute(seam, "DELETE FROM cx_hits");
      await execute(
        seam,
        `INSERT INTO cx_hits(key,dec)
         SELECT d.child_key,count(*)
         FROM ${frontier} f CROSS JOIN cx_dep d ON d.parent_key=f.key
         GROUP BY d.child_key`,
      );
      await execute(
        seam,
        `UPDATE cx_row SET weight=weight-
           (SELECT dec FROM cx_hits h WHERE h.key=cx_row.key)
         WHERE key IN (SELECT key FROM cx_hits)`,
      );
      await execute(seam, `DELETE FROM ${next}`);
      await execute(
        seam,
        `INSERT INTO ${next}(key)
         SELECT h.key
         FROM cx_hits h CROSS JOIN cx_row r ON r.key=h.key
         WHERE r.weight<=0 AND r.weight+h.dec>0`,
      );
      sample();
      [frontier, next] = [next, frontier];
    }
    await seam.db.execute("COMMIT");
  } catch (failure) {
    await seam.db.execute("ROLLBACK");
    throw failure;
  }
}

async function retract_recursive_cte(
  seam: ISqlSeam,
  sample: () => void,
): Promise<void> {
  await firstValueFrom(
    seam.runner.batch(seam.db, [
      "DELETE FROM cx_cone",
      `INSERT INTO cx_cone(key)
       WITH RECURSIVE cone(key) AS (
         SELECT key FROM cx_row WHERE key=0 AND weight>0
         UNION
         SELECT d.child_key
         FROM cone
         JOIN cx_dep d ON d.parent_key=cone.key
         JOIN cx_row r ON r.key=d.child_key
         WHERE r.weight>0
       )
       SELECT key FROM cone`,
      "UPDATE cx_row SET weight=0 WHERE key IN (SELECT key FROM cx_cone)",
      "DELETE FROM cx_frontier",
      `INSERT INTO cx_frontier(key)
       WITH RECURSIVE alive(key) AS (
         SELECT c.key
         FROM cx_cone c
         JOIN cx_dep d ON d.child_key=c.key
         JOIN cx_row p ON p.key=d.parent_key
         WHERE p.weight>0
         UNION
         SELECT d.child_key
         FROM alive
         JOIN cx_dep d ON d.parent_key=alive.key
         JOIN cx_cone c ON c.key=d.child_key
       )
       SELECT key FROM alive`,
      "UPDATE cx_row SET weight=1 WHERE key IN (SELECT key FROM cx_frontier)",
    ]),
  );
  sample();
}

async function survivor_receipt(
  seam: ISqlSeam,
): Promise<{ readonly count: number; readonly hash: string }> {
  const result = await execute(
    seam,
    "SELECT key FROM cx_row WHERE weight>0 ORDER BY key",
  );
  const hash = blake3.create();
  for (const row of result.rows) update_i64(hash, Number(row.key));
  return {
    count: result.rows.length,
    hash: bytesToHex(hash.digest()).slice(0, 16),
  };
}

async function main(): Promise<void> {
  const args = parse_args();
  const expected_key = `${args.shape}:${args.layers}:${args.width}:${args.stride}`;
  const expected = EXPECTED.get(expected_key);
  if (expected === undefined) {
    throw new Error(`2_p3-retract-bench: no perf input-hash gate for ${expected_key}`);
  }
  const seam = ScratchStore.open(`file:${args.db_path}`);
  await boot(seam);
  await load_rows(seam, args);
  const { edge_count, input_hash } = await load_edges(seam, args);
  if (input_hash !== expected.input_hash) {
    throw new Error(
      `2_p3-retract-bench: INPUT-MISMATCH expected=${expected.input_hash} actual=${input_hash}`,
    );
  }

  stmt_counter.reset();
  const heap = heap_sampler();
  const marker_path = process.env.P3_MEASURE_MARKER;
  if (marker_path !== undefined) writeFileSync(marker_path, "measured\n");
  const started = process.hrtime.bigint();
  if (args.shape === "DAG") await retract_count(seam, heap.sample);
  else await retract_recursive_cte(seam, heap.sample);
  const retract_ms = Number(process.hrtime.bigint() - started) / 1_000_000;
  const statements = stmt_counter.get();
  const survivors = await survivor_receipt(seam);
  const correct = survivors.count === expected.survivors;
  await execute(seam, "PRAGMA wal_checkpoint(TRUNCATE)");
  const db_mb = statSync(args.db_path).size / 1_048_576;
  const nodes = 2 + args.layers * args.width;
  const result = {
    engine: "tsv2",
    shape: args.shape,
    nodes,
    edges: edge_count,
    survivors: survivors.count,
    correct,
    input_hash: input_hash,
    output_hash: survivors.hash,
    retraction_guard:
      args.shape === "DAG" ? "plain-count-acyclic" : "recursive-cte-reseed",
    retract_ms: retract_ms,
    statements,
    host_peak_mb: heap.peak_mb(),
    process_rss_mb: process.memoryUsage().rss / 1_048_576,
    sqlite_hw_mb: "N/A",
    sqlite_hw_reason:
      "@libsql/client Client and ResultSet expose no sqlite3_memory_highwater binding or allocator-status API",
    db_mb: db_mb,
  };
  process.stdout.write(`${JSON.stringify(result)}\n`);
  process.stderr.write(
    `CSV,tsv2,${nodes},${edge_count},${nodes - survivors.count},0,${retract_ms.toFixed(3)},${statements},RSS_FROM_TIME,${heap.peak_mb().toFixed(2)},N/A,${db_mb.toFixed(2)}\n`,
  );
  seam.db.close();
  if (process.env.P3_KEEP_DB !== "1") unlinkSync(args.db_path);
  if (!correct) process.exitCode = 1;
}

void main().catch((failure: unknown) => {
  process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
  process.exitCode = 1;
});
