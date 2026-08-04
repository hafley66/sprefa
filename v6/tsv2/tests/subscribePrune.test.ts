/**
 * subscribePrune.test.ts — the four-combination receipt for ladder step 2,
 * {flag off, flag on} x {query-bearing module, zero-query module}.
 *
 * The two modules are real emitter output, read out of gen_emitted/ and never
 * hand-written here:
 *
 *   native_ts_query_term                          (2_hosts_wiring.pl:200)
 *     `? captured(Capture)`, so its cone is captured/query_value/file_digest/
 *     query_source/__host_response_tree_sitter. __host_demand_tree_sitter is
 *     DERIVED and outside the cone; `interval` is INGESTED and outside it.
 *     Those two are what the flag has to treat differently.
 *   level_view_reads_set_projection_not_occurrences (occurrence_identity.pl:199)
 *     no query decl at all, so under ruling zero_query_semantics its cone is
 *     empty and the flag must leave it evaluating NOTHING while `line` still
 *     ingests both arrivals.
 *
 * Each run imports the module with a cache-busting query so the module-scope
 * SubscribeCone.mode() call sees this run's environment; node caches by
 * specifier, and both modes have to exist inside one process.
 *
 * SABOTAGE RECEIPTS (each edit made, this file run, then reverted; messages
 * quoted are what the run printed):
 *   a. SubscribeCone.levels returning `statements` unfiltered under "on" ->
 *      both flag-on tests RED, and the runtime is what catches it: "Error:
 *      incremental level head relation missing: seen". A statement list wider
 *      than its relation list cannot run at all.
 *   b. SubscribeCone.relations keeping the cone only (dropping the
 *      arrivalTargets union) -> "Error: incremental arrival relation missing:
 *      line", the ingestion half.
 *   c. SubscribeCone.boot keeping the cone only -> "flag on + query-bearing:
 *      an ingested rel outside the cone still seeds", actual [] against
 *      expected [[300, 1]]: interval loses its Initial row.
 */

import assert from "node:assert/strict";
import { test } from "node:test";

import { concat, firstValueFrom, forkJoin, ignoreElements, map, of, toArray } from "rxjs";
import type { ISqlRunner } from "sprefa-store-engine/src/engine/types.ts";

import { BootRunner } from "../runtime/2_boot.ts";
import { SubscribeCone } from "../runtime/3_subscribe.ts";
import { rowValueFromSql } from "../runtime/rows.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type {
  IArrivalBatch,
  IRowValue,
  IServedProgram,
  ISqlSeam,
  ISubscribePruneMode,
  SqlStatement,
} from "../runtime/types.ts";

const QUERY_BEARING = "native_ts_query_term";
const ZERO_QUERY = "level_view_reads_set_projection_not_occurrences";

/** The fixture's own schedule (occurrence_identity.pl:202-203): the same log
 *  row twice, which is what makes `seen` a one-row set view over two
 *  occurrences. */
const ZERO_QUERY_SCHEDULE: readonly IArrivalBatch[] = [
  [{ rel: "line", sign: "add", row: [1, "a.ts", "alpha"] }],
  [{ rel: "line", sign: "add", row: [1, "a.ts", "alpha"] }],
];

/** native_ts_query_term's whole schedule is empty (its work is Initial rows +
 *  boot), so one drain tick is what the tick path gets to do. */
const QUERY_BEARING_SCHEDULE: readonly IArrivalBatch[] = [[]];

type RowsByRel = Readonly<Record<string, readonly (readonly IRowValue[])[]>>;

async function loadProgram(moduleName: string, mode: ISubscribePruneMode): Promise<IServedProgram> {
  const previous = process.env["SPREFA_TSV2_SUBSCRIBE_PRUNE"];
  process.env["SPREFA_TSV2_SUBSCRIBE_PRUNE"] = mode;
  try {
    const loaded = await import(`../gen_emitted/${moduleName}.ts?subscribePrune=${mode}`) as { program: IServedProgram };
    return loaded.program;
  } finally {
    if (previous === undefined) delete process.env["SPREFA_TSV2_SUBSCRIBE_PRUNE"];
    else process.env["SPREFA_TSV2_SUBSCRIBE_PRUNE"] = previous;
  }
}

/** Wraps the store's own runner, recording every statement text that crosses
 *  the seam. Nothing is intercepted or rewritten. */
function countingSeam(seam: ISqlSeam): { seam: ISqlSeam; statements: string[] } {
  const statements: string[] = [];
  const record = (statement: string | SqlStatement): void => {
    statements.push(typeof statement === "string" ? statement : statement.sql);
  };
  const runner: ISqlRunner = {
    ...seam.runner,
    execute(db, statement) {
      record(statement);
      return seam.runner.execute(db, statement);
    },
    batch(db, batched) {
      for (const statement of batched) record(statement);
      return seam.runner.batch(db, batched);
    },
    executeMultiple(db, sql) {
      for (const statement of sql.split(";\n")) record(statement);
      return seam.runner.executeMultiple(db, sql);
    },
  };
  return { seam: { db: seam.db, runner }, statements };
}

function finalRows(program: IServedProgram, seam: ISqlSeam) {
  const rels = Object.keys(program.finalSelect);
  if (rels.length === 0) return of({} as RowsByRel);
  return forkJoin(
    rels.map((rel) =>
      seam.runner.execute(seam.db, program.finalSelect[rel]!).pipe(
        map((result) => ({
          rel,
          rows: result.rows.map((row) =>
            (program.relColumns[rel] ?? []).map((column, index) =>
              rowValueFromSql(program.relColumnTypes?.[rel]?.[index], row[column]) as IRowValue,
            ),
          ),
        })),
      ),
    ),
  ).pipe(map((entries) => Object.fromEntries(entries.map((entry) => [entry.rel, entry.rows])) as RowsByRel));
}

/** Boot, run the schedule, read every rel back through the module's own decode
 *  SELECTs. The statement list covers boot AND the ticks. */
async function replay(
  moduleName: string,
  mode: ISubscribePruneMode,
  schedule: readonly IArrivalBatch[],
): Promise<{ program: IServedProgram; rows: RowsByRel; statements: readonly string[] }> {
  const program = await loadProgram(moduleName, mode);
  const base = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(base, program.ddl));
  const { seam, statements } = countingSeam(base);
  await firstValueFrom(
    concat(
      BootRunner.run(seam, program.boot).pipe(ignoreElements()),
      TickFold.run(program, seam, schedule),
    ).pipe(toArray()),
  );
  // Read back through the UNWRAPPED seam: the decode SELECTs are this test's
  // own measurement and must not land in the statement list it measures.
  const rows = await firstValueFrom(finalRows(program, base));
  return { program, rows, statements };
}

function statementsNaming(statements: readonly string[], table: string): number {
  return statements.filter((statement) => statement.includes(`"${table}"`)).length;
}

// ── the filter itself: "off" is identity, by reference ───────────────────────

test("flag off is identity on every list, by reference and not by value", () => {
  const relations = [{ rel: "kept" }] as never;
  const statements = [{ headRel: "kept" }] as never;
  const boot = [{ rel: "kept", sql: "", params: [] }] as never;
  assert.equal(SubscribeCone.relations("off", relations, [], []), relations);
  assert.equal(SubscribeCone.levels("off", statements, []), statements);
  assert.equal(SubscribeCone.edges("off", statements, []), statements);
  assert.equal(SubscribeCone.retention("off", boot, [], []), boot);
  assert.equal(SubscribeCone.boot("off", boot, [], []), boot);
});

test("the default environment is flag off", () => {
  const previous = process.env["SPREFA_TSV2_SUBSCRIBE_PRUNE"];
  delete process.env["SPREFA_TSV2_SUBSCRIBE_PRUNE"];
  try {
    assert.equal(SubscribeCone.mode(), "off");
  } finally {
    if (previous !== undefined) process.env["SPREFA_TSV2_SUBSCRIBE_PRUNE"] = previous;
  }
});

// ── combination 1 + 2: the query-bearing module ──────────────────────────────

test("flag off + query-bearing: the fixture's own expectations, unpruned", async () => {
  const { program, rows, statements } = await replay(QUERY_BEARING, "off", QUERY_BEARING_SCHEDULE);
  assert.deepEqual(
    program.subscribedRels,
    ["__host_response_tree_sitter/5", "captured/1", "file_digest/1", "query_source/1", "query_value/1"],
    "the cone this test is written against",
  );
  assert.equal(rows["captured"]?.length, 0, "2_hosts_wiring.pl:239 final(captured/1, [])");
  assert.equal(rows["query_value"]?.length, 1, "2_hosts_wiring.pl:240 final(query_value/1, [one row])");
  assert.equal(
    rows["__host_demand_tree_sitter"]?.length,
    1,
    "unpruned, the host demand rel derives its row from file_digest x query_value",
  );
  assert.ok(
    statementsNaming(statements, "__host_demand_tree_sitter") > 0,
    "unpruned, the module runs the demand rel's derivation",
  );
});

test("flag on + query-bearing: subscribed rels identical, unsubscribed derivation gone", async () => {
  const unpruned = await replay(QUERY_BEARING, "off", QUERY_BEARING_SCHEDULE);
  const pruned = await replay(QUERY_BEARING, "on", QUERY_BEARING_SCHEDULE);

  for (const ref of pruned.program.subscribedRels) {
    const rel = ref.slice(0, ref.lastIndexOf("/"));
    assert.deepEqual(
      pruned.rows[rel],
      unpruned.rows[rel],
      `subscribed rel ${rel} must be identical to the graded unpruned run`,
    );
  }

  assert.deepEqual(
    pruned.rows["__host_demand_tree_sitter"],
    [],
    "a derived rel outside the cone ends empty",
  );
  assert.equal(
    statementsNaming(pruned.statements, "__host_demand_tree_sitter"),
    0,
    "and no statement of its derivation runs at all, boot included",
  );
  assert.deepEqual(
    pruned.rows["interval"],
    unpruned.rows["interval"],
    "flag on + query-bearing: an ingested rel outside the cone still seeds",
  );
  assert.equal(
    pruned.program.boot.length < unpruned.program.boot.length,
    true,
    `pruned boot must be shorter: ${pruned.program.boot.length} vs ${unpruned.program.boot.length}`,
  );
});

// ── combination 3 + 4: the zero-query module ─────────────────────────────────

test("flag off + zero-query: the level view derives, cone or no cone", async () => {
  const { program, rows } = await replay(ZERO_QUERY, "off", ZERO_QUERY_SCHEDULE);
  assert.deepEqual(program.subscribedRels, [], "ruling zero_query_semantics: no query, empty cone");
  assert.deepEqual(rows["seen"], [["a.ts"]], "occurrence_identity.pl:204 deltas(seen/1, [[+seen(a.ts)], []])");
  assert.equal(rows["line"]?.length, 2, "occurrence_identity.pl:206 final(line/3, [two occurrences])");
});

test("flag on + zero-query: nothing derives, ingestion still lands", async () => {
  const { rows, statements } = await replay(ZERO_QUERY, "on", ZERO_QUERY_SCHEDULE);
  assert.deepEqual(rows["seen"], [], "a zero-query module subscribes to nothing, so it derives nothing");
  assert.equal(
    statementsNaming(statements, "seen"),
    0,
    `flag on + zero-query: a pruned rel's derivation must not run: ${statementsNaming(statements, "seen")} statements name seen`,
  );
  assert.equal(rows["line"]?.length, 2, "flag on + zero-query: ingestion is never pruned");
});
