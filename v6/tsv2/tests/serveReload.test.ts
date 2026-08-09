/**
 * serveReload.test.ts — the SWAP contract of the served engine: what a second
 * `POST /program` does to a database the first program already filled.
 *
 * RED-BEFORE, all three live tests, measured 2026-08-09 against 87d19562 with
 * the planner unwired (3 fail / 1 skip, and the runner never exits):
 *
 *   1  8002ms, `reload of a widened rel never answered within 5000ms`
 *   2  verdict `drop`: + undefined - 'drop'
 *   3  verdict `keep`: + undefined - 'keep'
 *
 * Test 1's shape is the one that cost a whole server. `boot_served_program`
 * swallowed the CREATE for the already-existing `edge_row`, the table kept its
 * two-column shape, the boot recompute hit `no such column: weight_units`, and
 * that error left `run_program$` -> `switchMap` -> the app's ONE subscription:
 * process dead, POST unanswered, client hung. Hence `within`.
 *
 * SABOTAGE RECEIPT (run 2026-08-09, reverted): forcing `allow_drop` to false at
 * the `ReloadPlanner.plan` call site turns test 2 red (the drop becomes an
 * `unsupported` entry and the table survives) and leaves the other three green.
 */

import assert from "node:assert/strict";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { select_rows } from "../runtime/rows.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import type { IRelCatalogRow, IReloadOutcome, RelVerdict } from "../runtime/types.ts";
import { ProgramCompiler } from "../serve/0_compile.ts";
import { post_arrivals, post_program, request, start_served } from "./serveHelpers.ts";

const NARROW = `rel edge_row(src_node: text, dst_node: text).
rel reach_row(src_node: text, dst_node: text).

reach_row(SrcNode, DstNode) <- edge_row(SrcNode, DstNode).
`;

/** Same rels, same rule text, head-to-body wiring reversed. */
const REWIRED = `rel edge_row(src_node: text, dst_node: text).
rel reach_row(src_node: text, dst_node: text).

reach_row(DstNode, SrcNode) <- edge_row(SrcNode, DstNode).
`;

const WIDENED = `rel edge_row(src_node: text, dst_node: text, weight_units: int).
rel reach_row(src_node: text, dst_node: text, weight_units: int).

reach_row(SrcNode, DstNode, WeightUnits) <- edge_row(SrcNode, DstNode, WeightUnits).
`;

const WITHOUT_REACH = `rel edge_row(src_node: text, dst_node: text).

rel loop_row(src_node: text).
loop_row(SrcNode) <- edge_row(SrcNode, SrcNode).
`;

type RowSet = { readonly rows: readonly (readonly (string | number)[])[] };
type LoadReply = { readonly loaded: boolean; readonly reload?: IReloadOutcome };

/** A swap that never answers is the pre-wire failure mode, and a hung promise
 *  is not a test result. */
function within<T>(work: Promise<T>, budget_ms: number, what: string): Promise<T> {
  let alarm: NodeJS.Timeout;
  const refusal = new Promise<never>((_resolve, reject) => {
    alarm = setTimeout(() => reject(new Error(`${what} never answered within ${budget_ms}ms`)), budget_ms);
  });
  return Promise.race([work, refusal]).finally(() => clearTimeout(alarm));
}

function scratch_db_url(): string {
  return `file:${join(mkdtempSync(join(tmpdir(), "tsv2-serve-reload-")), "served.sqlite")}`;
}

function rows_of(body: string): readonly (readonly (string | number)[])[] {
  return (JSON.parse(body) as RowSet).rows;
}

function verdict_of(outcome: IReloadOutcome | undefined, rel: string): RelVerdict | undefined {
  return outcome?.verdicts.find(([key]) => key.endsWith(`:${rel}`))?.[1];
}

function table_names(db_url: string): Promise<readonly string[]> {
  const inspection = ScratchStore.open(db_url);
  return firstValueFrom(
    select_rows(
      inspection,
      `SELECT "name" FROM "sqlite_master" WHERE "type" = 'table' ORDER BY "name"`,
      ["name"],
      ["text"],
    ),
  ).then((rows) => {
    inspection.db.close();
    return rows.map((row) => String(row[0]));
  });
}

test("a swap that widens a rel drops and recreates its table instead of swallowing the CREATE", { timeout: 8000 }, async () => {
  const db_url = scratch_db_url();
  const served = await start_served(0, undefined, db_url);
  try {
    assert.equal((await post_program(served.port, NARROW)).statusCode, 200);
    await post_arrivals(served.port, [{ rel: "edge_row", sign: "add", row: ["one", "two"] }]);
    assert.deepEqual(rows_of((await request(served.port, "/idb/reach_row", "GET")).body), [["one", "two"]]);

    const swap = await within(post_program(served.port, WIDENED), 5000, "reload of a widened rel");
    assert.equal(swap.statusCode, 200, swap.body);
    const reply = JSON.parse(swap.body) as LoadReply;
    assert.equal(verdict_of(reply.reload, "edge_row"), "recreate");
    assert.equal(verdict_of(reply.reload, "reach_row"), "recreate");

    await post_arrivals(served.port, [{ rel: "edge_row", sign: "add", row: ["three", "four", 7] }]);
    assert.deepEqual(rows_of((await request(served.port, "/idb/reach_row", "GET")).body), [["three", "four", 7]]);
  } finally {
    await served.stop();
  }
});

test("a rel the incoming program does not declare loses its table", { timeout: 8000 }, async () => {
  const db_url = scratch_db_url();
  const served = await start_served(0, undefined, db_url);
  try {
    assert.equal((await post_program(served.port, NARROW)).statusCode, 200);
    await post_arrivals(served.port, [{ rel: "edge_row", sign: "add", row: ["one", "two"] }]);
    assert.ok((await table_names(db_url)).includes("reach_row"));

    const swap = await within(post_program(served.port, WITHOUT_REACH), 5000, "reload dropping a rel");
    assert.equal(swap.statusCode, 200, swap.body);
    assert.equal(verdict_of((JSON.parse(swap.body) as LoadReply).reload, "reach_row"), "drop");

    const remaining = await table_names(db_url);
    assert.ok(!remaining.includes("reach_row"), `reach_row survived its drop: ${remaining.join(", ")}`);
    assert.ok(remaining.includes("loop_row"), `loop_row was never created: ${remaining.join(", ")}`);
  } finally {
    await served.stop();
  }
});

/**
 * THE h_rule HOLE, GUARDED FROM THE OTHER SIDE. `reach_row`'s fingerprint does
 * not move when only its head-to-body wiring moves (test 4), so the planner
 * verdicts `keep` and contributes no statement at all. The rows still flip,
 * because every level rel's boot statements are `DELETE FROM` + a full
 * re-derive and the boot leg runs UNCONDITIONALLY. That is the whole reason the
 * wire above must never let a `keep` verdict skip the boot recompute; this test
 * is what goes red the day someone tries.
 */
test("a rule whose head wiring moved re-derives even though the planner says keep", { timeout: 8000 }, async () => {
  const db_url = scratch_db_url();
  const served = await start_served(0, undefined, db_url);
  try {
    assert.equal((await post_program(served.port, NARROW)).statusCode, 200);
    await post_arrivals(served.port, [{ rel: "edge_row", sign: "add", row: ["one", "two"] }]);
    assert.deepEqual(rows_of((await request(served.port, "/idb/reach_row", "GET")).body), [["one", "two"]]);

    const swap = await within(post_program(served.port, REWIRED), 5000, "reload of a rewired head");
    assert.equal(swap.statusCode, 200, swap.body);
    assert.equal(verdict_of((JSON.parse(swap.body) as LoadReply).reload, "reach_row"), "keep");

    assert.deepEqual(rows_of((await request(served.port, "/idb/edge_row", "GET")).body), [["one", "two"]]);
    assert.deepEqual(rows_of((await request(served.port, "/idb/reach_row", "GET")).body), [["two", "one"]]);
  } finally {
    await served.stop();
  }
});

/**
 * FAILING-FIRST, SKIPPED: the fix is `rule_bodies_map` in v6/prolog/lower.pl,
 * which a concurrent lane owns. `findall` collects bare bodies, severing
 * head-body variable sharing, so `numbervars` canonicalizes both wirings to
 * `edge_row('$VAR'(0),'$VAR'(1))`. Measured on these two programs: h_rule is
 * `64609212297cf158` for BOTH.
 */
test("h_rule moves when a rule's head-to-body wiring moves", { skip: "fix lives in v6/prolog/lower.pl rule_bodies_map/2, owned by the type-IR lane" }, async () => {
  const narrow = await firstValueFrom(ProgramCompiler.compile(NARROW));
  const rewired = await firstValueFrom(ProgramCompiler.compile(REWIRED));
  const rule_hash = (catalog: readonly IRelCatalogRow[]): string =>
    catalog.find((row) => row.kind === "rel" && row.local_name === "reach_row")?.h_rule ?? "";
  assert.notEqual(rule_hash(narrow.rel_catalog), rule_hash(rewired.rel_catalog));
});
