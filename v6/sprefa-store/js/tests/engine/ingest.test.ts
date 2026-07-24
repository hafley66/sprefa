/**
 * ingest.test.ts — the E2 golden: fixture jsonl -> tables populated -> dirty() is exactly
 * the cone of the changed rows -> one derived rel recomputes and matches a from-scratch
 * reference -> a second identical ingest is a no-op (changed=[], zero propagate recomputes).
 */

import { test } from "node:test";
import { strict as assert } from "node:assert";
import { createClient } from "@libsql/client";
import { createReadStream } from "node:fs";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";

import { Store, RelStore, key as cascade_key } from "../../src/engine/lib.ts";
import { reconcile, stmt_counter } from "../../src/engine/engine.ts";
import { ingestJsonl, content_digest, REL_NODE, rel_id_for_name } from "../../src/engine/ingest.ts";

const FIXTURE_PATH = fileURLToPath(new URL("./fixtures/ingest.jsonl", import.meta.url));

/** The fixture's own shape (see tests/engine/fixtures/ingest.jsonl's generator comment
 *  in this file): 20 strings, 5 files, 100 nodes, 50 edges, 10 demo_rel, 15 demo_text_rel. */
const EXPECT = {
  strs: 20,
  files: 5,
  nodes: 100,
  edges: 50,
  demo_rel: 10,
  demo_text_rel: 15,
};
const TOTAL_LINES = EXPECT.strs + EXPECT.files + EXPECT.nodes + EXPECT.edges + EXPECT.demo_rel + EXPECT.demo_text_rel;

async function fixture_lines(): Promise<AsyncIterable<string>> {
  return createInterface({ input: createReadStream(FIXTURE_PATH), crlfDelay: Infinity });
}

interface Engine {
  db: ReturnType<typeof createClient>;
  store: Store;
  rels: RelStore;
}

async function fresh_engine(): Promise<Engine> {
  const db = createClient({ url: ":memory:", intMode: "bigint" });
  const store = await Store.open(db);
  const rels = await RelStore.attach(db);
  return { db, store, rels };
}

async function table_count(db: Engine["db"], table: string): Promise<number> {
  const res = await db.execute(`SELECT count(*) FROM ${table}`);
  return Number(res.rows[0]?.[0] ?? 0);
}

test("ingest: fixture populates every table, stmt count is O(lines/CHUNK)", async () => {
  const engine = await fresh_engine();
  stmt_counter.reset();
  const report = await ingestJsonl(engine.store, engine.rels, await fixture_lines(), 1);

  assert.strictEqual(report.lines, TOTAL_LINES, "every non-blank line parsed");
  assert.strictEqual(await table_count(engine.db, "strings"), EXPECT.strs);
  assert.strictEqual(await table_count(engine.db, "files"), EXPECT.files);
  assert.strictEqual(await table_count(engine.db, "node"), EXPECT.nodes);
  assert.strictEqual(await table_count(engine.db, "edge"), EXPECT.edges);
  assert.strictEqual(await table_count(engine.db, "demo_rel"), EXPECT.demo_rel);
  assert.strictEqual(await table_count(engine.db, "demo_text_rel"), EXPECT.demo_text_rel);

  const total_cells = EXPECT.strs + EXPECT.files + EXPECT.nodes + EXPECT.edges + EXPECT.demo_rel + EXPECT.demo_text_rel;
  assert.strictEqual(report.changed.length, total_cells, "every fresh fact is a changed cell");

  // O(lines/CHUNK), not O(lines): 200 lines at CHUNK_ROWS=100 should cost a couple dozen
  // statements at most, nowhere near one-per-line.
  console.log(`ingest stmt count (fixture, ${TOTAL_LINES} lines): ${report.stmts}`);
  assert.ok(report.stmts < TOTAL_LINES / 4, `${report.stmts} stmts is not O(lines/CHUNK) for ${TOTAL_LINES} lines`);
});

test("ingest: dirty() is exactly the cone of the changed rows; derived rel matches from-scratch; idempotent re-ingest", async () => {
  const engine = await fresh_engine();
  const rev1 = 1;
  const report1 = await ingestJsonl(engine.store, engine.rels, await fixture_lines(), rev1);
  assert.ok(report1.changed.length > 0, "first ingest discovers new cells");

  // Wire ONE derived rel that reads every ingested node cell: its own reconcile row is
  // seeded "as of rev 0" (verified_at=0), deliberately stale relative to the rev-1 seeds
  // ingest just wrote, so dirty() must name exactly this one reader.
  const node_cells = report1.changed.filter(([rel]) => rel === REL_NODE);
  assert.strictEqual(node_cells.length, EXPECT.nodes, "every node is a fresh cell on a clean store");

  const DERIVED_REL = 5_000_000; // well clear of REL_STR..REL_EDGE and the named-rel hash bucket
  const derived_key = cascade_key(DERIVED_REL, 0);
  await reconcile.seed(
    engine.rels.conn(),
    engine.rels.ns(),
    derived_key,
    0n, // placeholder digest; about to be superseded by the first propagate
    node_cells.map(([rel, row]) => cascade_key(rel, row)),
    0, // verified_at=0: stale relative to the node cells' changed_at=rev1
  );

  const dirty_before = await engine.rels.dirty();
  assert.deepStrictEqual(
    dirty_before,
    [[DERIVED_REL, 0]],
    "dirty() names exactly the one reader of the changed cone",
  );

  // recompute = XOR-fold of the reader's dep digests (order-independent — content_digest
  // XORs mix() per part). The from-scratch reference recomputes the SAME formula directly
  // from the `node` table's own rows, entirely independent of the reconcile plane.
  const recomputes = await reconcile.propagate(
    engine.rels.conn(),
    engine.rels.ns(),
    [derived_key],
    rev1,
    (_id, dep_digests) => content_digest(dep_digests),
  );
  assert.strictEqual(recomputes, 1, "exactly one derived rel recomputed");

  const node_ids_res = await engine.db.execute("SELECT node_id FROM node");
  const node_ids = node_ids_res.rows.map((r) => Number(r[0]));
  assert.strictEqual(node_ids.length, EXPECT.nodes);
  const reference_digest = content_digest(node_ids.map((id) => content_digest([REL_NODE, id])));

  const memo_res = await engine.db.execute(`SELECT digest FROM ${engine.rels.ns().memo} WHERE id=${derived_key}`);
  const stored_digest = memo_res.rows[0]?.digest as bigint;
  assert.strictEqual(stored_digest, reference_digest, "propagated digest matches the from-scratch reference");

  const dirty_after = await engine.rels.dirty();
  assert.deepStrictEqual(dirty_after, [], "the reader is verified; nothing left dirty");

  // Second, identical ingest at a new rev: append-only identity means nothing is new.
  const report2 = await ingestJsonl(engine.store, engine.rels, await fixture_lines(), rev1 + 1);
  assert.deepStrictEqual(report2.changed, [], "re-ingesting the identical fixture yields changed=[]");
  assert.strictEqual(report2.lines, TOTAL_LINES);

  const dirty_final = await engine.rels.dirty();
  assert.deepStrictEqual(dirty_final, [], "no reader went dirty from a no-op ingest");
  const recomputes2 = await reconcile.propagate(
    engine.rels.conn(),
    engine.rels.ns(),
    dirty_final,
    rev1 + 1,
    (_id, dep_digests) => content_digest(dep_digests),
  );
  assert.strictEqual(recomputes2, 0, "zero propagate recomputes on the idempotent re-ingest (early cutoff)");
});

test("ingest: named rel tables get the right column shapes (int/int, text/int)", async () => {
  const engine = await fresh_engine();
  await ingestJsonl(engine.store, engine.rels, await fixture_lines(), 1);

  const demo = await engine.db.execute("SELECT c0, c1 FROM demo_rel ORDER BY c0");
  assert.strictEqual(demo.rows.length, EXPECT.demo_rel);
  assert.strictEqual(Number(demo.rows[0]!.c0), 0);
  assert.strictEqual(Number(demo.rows[0]!.c1), 0);
  assert.strictEqual(Number(demo.rows[9]!.c0), 9);
  assert.strictEqual(Number(demo.rows[9]!.c1), 18);

  const text_rel = await engine.db.execute("SELECT c0, c1 FROM demo_text_rel ORDER BY c1");
  assert.strictEqual(text_rel.rows.length, EXPECT.demo_text_rel);
  assert.strictEqual(text_rel.rows[0]!.c0, "label_0");
  assert.strictEqual(Number(text_rel.rows[0]!.c1), 0);

  // Every demo_rel/demo_text_rel row landed a distinct reconcile cell under its own
  // name-hashed rel id (not accidentally sharing REL_STR..REL_EDGE's fixed ids).
  const rel_id = rel_id_for_name("demo_rel");
  assert.ok(rel_id >= 4, "named rels stay clear of the fixed spine rel ids");
});

test("ingest: edges resolve to real node ids, not local stream indices", async () => {
  const engine = await fresh_engine();
  await ingestJsonl(engine.store, engine.rels, await fixture_lines(), 1);

  // Fixture edge 0 is local (src=0, dst=7); node local index i has byte_start = i*8, so its
  // real node_id is discoverable via that identity, independent of ingest's own bookkeeping.
  const src_row = await engine.db.execute("SELECT node_id FROM node WHERE byte_start = 0");
  const dst_row = await engine.db.execute("SELECT node_id FROM node WHERE byte_start = 56");
  const src_id = Number(src_row.rows[0]!.node_id);
  const dst_id = Number(dst_row.rows[0]!.node_id);

  const edge_row = await engine.db.execute(
    `SELECT count(*) FROM edge WHERE family=1 AND src_id=${src_id} AND dst_id=${dst_id} AND kind=0`,
  );
  assert.strictEqual(Number(edge_row.rows[0]?.[0]), 1, "the edge landed on the RESOLVED node ids, not 0/7 literally");
});
