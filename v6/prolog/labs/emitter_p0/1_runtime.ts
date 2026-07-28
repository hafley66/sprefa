import { defer, firstValueFrom, from, type Observable } from "rxjs";

import { stmt_counter } from "../../../sprefa-store/js/src/engine/counter.ts";
import type {
  IArrivalBatch,
  IGenProgram,
  IRelDelta,
  IRow,
  ISqlSeam,
  ITickDeltas,
  SqlStatement,
} from "../../../tsv2/runtime/types.ts";

export type LabFamily =
  | "semi_naive_delta_join"
  | "count_ivm_support"
  | "distinct_placement"
  | "boundary_diff_delta_stream";

export type LabSpelling = "inline" | "helper";

export interface IExplainReceipt {
  readonly label: string;
  readonly sql: string;
  readonly args: readonly (string | number | bigint)[];
  readonly deltaTable: string;
  readonly deltaPlanName?: string;
}

export interface IReceiptProgram extends IGenProgram {
  readonly fixtureName: string;
  readonly statementCounts: number[];
  readonly explainReceipts: readonly IExplainReceipt[];
}

export interface ILabVariant {
  readonly family: LabFamily;
  readonly spelling: LabSpelling;
  readonly programs: readonly IReceiptProgram[];
}

interface IProgramConfig {
  readonly fixtureName: string;
  readonly ddl: readonly string[];
  readonly relColumns: Readonly<Record<string, readonly string[]>>;
  readonly arrivalTargets: readonly string[];
  readonly explainReceipts: readonly IExplainReceipt[];
  readonly executeTick: (
    seam: ISqlSeam,
    batchId: number,
    arrivals: IArrivalBatch,
  ) => Promise<boolean>;
  readonly boundarySql?: string;
}

interface ISemiNaiveInlineSql {
  readonly forkDeltaJoinSql: string;
  readonly recursiveDeltaSql: string;
  readonly forkExplainSql: string;
  readonly recursiveExplainSql: string;
}

interface ICountIvmInlineSql {
  readonly transitionSql: string;
  readonly reseedAuditSql: string;
  readonly explainSql: string;
}

interface IDistinctInlineSql {
  readonly demandAssertSql: string;
  readonly mirrorAssertSql: string;
  readonly mirrorRetractSql: string;
  readonly assertExplainSql: string;
  readonly retractExplainSql: string;
}

interface IBoundaryInlineSql {
  readonly selectSql: string;
  readonly explainSql: string;
}

const COMMON_BOUNDARY_DDL = [
  "CREATE TABLE boundary_delta (batch_id INTEGER NOT NULL, rel TEXT NOT NULL, sign TEXT NOT NULL, row_json TEXT NOT NULL)",
  "CREATE INDEX boundary_delta_batch ON boundary_delta(batch_id, rel, sign)",
] as const;

export const LAB_SCHEDULES: Readonly<Record<string, readonly IArrivalBatch[]>> = {
  fork_join_is_a_conjunctive_body: [
    [{ rel: "result_a", sign: "add", row: ["alpha"] }],
    [],
    [{ rel: "result_b", sign: "add", row: ["beta"] }],
  ],
  repeat_is_a_self_carry_chain: [[{ rel: "kick", sign: "add", row: ["go"] }]],
  departed_fires_next_tick_on_retraction: [
    [{ rel: "source_row", sign: "add", row: ["alpha"] }],
    [{ rel: "source_row", sign: "del", row: ["alpha"] }],
  ],
  demand_view_fires_its_consumer_once: [
    [{ rel: "stale", sign: "add", row: ["repos/cli/cli"] }],
    [{ rel: "stale", sign: "add", row: ["repos/cli/cli"] }],
  ],
};

function createProgram(config: IProgramConfig): IReceiptProgram {
  let batchId = 0;
  const statementCounts: number[] = [];
  const boundarySql =
    config.boundarySql ??
    "SELECT rel, sign, row_json FROM boundary_delta WHERE batch_id = ?1 ORDER BY rel, sign, row_json";

  return {
    name: config.fixtureName,
    fixtureName: config.fixtureName,
    ddl: config.ddl,
    relColumns: config.relColumns,
    arrivalTargets: config.arrivalTargets,
    statementCounts,
    explainReceipts: config.explainReceipts,
    tick(seam: ISqlSeam, arrivals: IArrivalBatch): Observable<ITickDeltas> {
      return defer(() => {
        batchId += 1;
        const statementsBefore = stmt_counter.get();
        return from(
          config.executeTick(seam, batchId, arrivals).then(async (carryPending) => {
            const rels = await readBoundaryDeltas(
              seam,
              boundarySql,
              batchId,
              Object.keys(config.relColumns),
            );
            statementCounts.push(stmt_counter.get() - statementsBefore);
            return { rels, carryPending };
          }),
        );
      });
    },
  };
}

async function executeStatements(
  seam: ISqlSeam,
  statements: readonly SqlStatement[],
): Promise<void> {
  await firstValueFrom(seam.runner.batch(seam.db, statements));
}

async function executeScalar(
  seam: ISqlSeam,
  statement: SqlStatement,
): Promise<number> {
  return firstValueFrom(seam.runner.scalar(seam.db, statement));
}

async function readBoundaryDeltas(
  seam: ISqlSeam,
  sql: string,
  batchId: number,
  relNames: readonly string[],
): Promise<IRelDelta[]> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, { sql, args: [BigInt(batchId)] }),
  );
  const byRel = new Map<string, { add: IRow[]; del: IRow[] }>();
  for (const relName of relNames) byRel.set(relName, { add: [], del: [] });
  for (const resultRow of result.rows) {
    const relName = String(resultRow.rel);
    const sign = String(resultRow.sign);
    const row = JSON.parse(String(resultRow.row_json)) as IRow;
    const entry = byRel.get(relName);
    if (entry === undefined) throw new Error(`unknown boundary rel ${relName}`);
    if (sign === "add") entry.add.push(row);
    else if (sign === "del") entry.del.push(row);
    else throw new Error(`unknown boundary sign ${sign}`);
  }
  return relNames.map((relName) => {
    const entry = byRel.get(relName)!;
    return { rel: relName, add: entry.add, del: entry.del };
  });
}

function arrivalArgs(batchId: number, arrivals: IArrivalBatch): (bigint | string)[] {
  return [BigInt(batchId), JSON.stringify(arrivals)];
}

function batchArg(batchId: number): bigint[] {
  return [BigInt(batchId)];
}

function batchAndPreviousArgs(batchId: number): bigint[] {
  return [BigInt(batchId), BigInt(batchId - 1)];
}

function quoteIdentifier(identifier: string): string {
  if (!/^[a-z][a-z0-9_]*$/.test(identifier)) {
    throw new Error(`unsafe SQL identifier ${identifier}`);
  }
  return `"${identifier}"`;
}

function semiNaiveHelperSql(parameters: {
  readonly forkDeltaTable: string;
  readonly recursiveDeltaTable: string;
}): ISemiNaiveInlineSql {
  const forkDeltaTable = quoteIdentifier(parameters.forkDeltaTable);
  const recursiveDeltaTable = quoteIdentifier(parameters.recursiveDeltaTable);
  return {
    forkDeltaJoinSql: `
      INSERT OR IGNORE INTO fork_combined_delta(batch_id, value_a, value_b)
      SELECT ?1, delta_result_a.value, current_result_b.value
      FROM ${forkDeltaTable} AS delta_result_a
      JOIN fork_result_b AS current_result_b
      WHERE delta_result_a.batch_id = ?1 AND delta_result_a.rel = 'result_a'
      UNION
      SELECT ?1, current_result_a.value, delta_result_b.value
      FROM fork_result_a AS current_result_a
      JOIN ${forkDeltaTable} AS delta_result_b
      WHERE delta_result_b.batch_id = ?1 AND delta_result_b.rel = 'result_b'
    `,
    recursiveDeltaSql: `
      INSERT OR IGNORE INTO repeat_pulse_delta(batch_id, value)
      SELECT ?1, previous_frontier.value + 1
      FROM ${recursiveDeltaTable} AS previous_frontier
      WHERE previous_frontier.batch_id = ?2 AND previous_frontier.value < 3
    `,
    forkExplainSql: `
      SELECT delta_result_a.value, current_result_b.value
      FROM ${forkDeltaTable} AS delta_result_a
      JOIN fork_result_b AS current_result_b
      WHERE delta_result_a.batch_id = ?1 AND delta_result_a.rel = 'result_a'
    `,
    recursiveExplainSql: `
      SELECT previous_frontier.value + 1
      FROM ${recursiveDeltaTable} AS previous_frontier
      WHERE previous_frontier.batch_id = ?1 AND previous_frontier.value < 3
    `,
  };
}

export function createSemiNaiveInlineVariant(
  sql: ISemiNaiveInlineSql,
): ILabVariant {
  return createSemiNaiveVariant("inline", sql);
}

export function createSemiNaiveHelperVariant(parameters: {
  readonly forkDeltaTable: string;
  readonly recursiveDeltaTable: string;
}): ILabVariant {
  return createSemiNaiveVariant("helper", semiNaiveHelperSql(parameters));
}

function createSemiNaiveVariant(
  spelling: LabSpelling,
  sql: ISemiNaiveInlineSql,
): ILabVariant {
  return {
    family: "semi_naive_delta_join",
    spelling,
    programs: [createForkJoinProgram(sql), createRepeatProgram(sql)],
  };
}

function createForkJoinProgram(sql: ISemiNaiveInlineSql): IReceiptProgram {
  const ddl = [
    "CREATE TABLE fork_result_a (value TEXT PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE fork_result_b (value TEXT PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE fork_combined (value_a TEXT NOT NULL, value_b TEXT NOT NULL, PRIMARY KEY(value_a, value_b)) WITHOUT ROWID",
    "CREATE TABLE fork_delta (batch_id INTEGER NOT NULL, rel TEXT NOT NULL, value TEXT NOT NULL)",
    "CREATE INDEX fork_delta_batch_rel ON fork_delta(batch_id, rel, value)",
    "CREATE TABLE fork_combined_delta (batch_id INTEGER NOT NULL, value_a TEXT NOT NULL, value_b TEXT NOT NULL, PRIMARY KEY(batch_id, value_a, value_b)) WITHOUT ROWID",
    ...COMMON_BOUNDARY_DDL,
  ];
  return createProgram({
    fixtureName: "fork_join_is_a_conjunctive_body",
    ddl,
    relColumns: { result_a: ["value_a"], result_b: ["value_b"], combined: ["value_a", "value_b"] },
    arrivalTargets: ["result_a", "result_b"],
    explainReceipts: [
      {
        label: "non_recursive_delta_join",
        sql: sql.forkExplainSql,
        args: [1n],
        deltaTable: "fork_delta",
        deltaPlanName: "delta_result_a",
      },
    ],
    async executeTick(seam, batchId, arrivals) {
      const currentBatchArgs = batchArg(batchId);
      await executeStatements(seam, [
        {
          sql: `
            INSERT INTO fork_delta(batch_id, rel, value)
            SELECT ?1, json_extract(arrival.value, '$.rel'), json_extract(arrival.value, '$.row[0]')
            FROM json_each(?2) AS arrival
            WHERE json_extract(arrival.value, '$.sign') = 'add'
              AND json_extract(arrival.value, '$.rel') IN ('result_a', 'result_b')
          `,
          args: arrivalArgs(batchId, arrivals),
        },
        {
          sql: "INSERT OR IGNORE INTO fork_result_a(value) SELECT value FROM fork_delta WHERE batch_id = ?1 AND rel = 'result_a'",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT OR IGNORE INTO fork_result_b(value) SELECT value FROM fork_delta WHERE batch_id = ?1 AND rel = 'result_b'",
          args: currentBatchArgs,
        },
        { sql: sql.forkDeltaJoinSql, args: currentBatchArgs },
        {
          sql: "INSERT OR IGNORE INTO fork_combined(value_a, value_b) SELECT value_a, value_b FROM fork_combined_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT INTO boundary_delta(batch_id, rel, sign, row_json) SELECT batch_id, rel, 'add', json_array(value) FROM fork_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT INTO boundary_delta(batch_id, rel, sign, row_json) SELECT batch_id, 'combined', 'add', json_array(value_a, value_b) FROM fork_combined_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
      ]);
      return false;
    },
  });
}

function createRepeatProgram(sql: ISemiNaiveInlineSql): IReceiptProgram {
  const ddl = [
    "CREATE TABLE repeat_kick (value TEXT NOT NULL)",
    "CREATE TABLE repeat_pulse (value INTEGER PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE repeat_pulse_delta (batch_id INTEGER NOT NULL, value INTEGER NOT NULL, PRIMARY KEY(batch_id, value)) WITHOUT ROWID",
    "CREATE INDEX repeat_pulse_delta_batch ON repeat_pulse_delta(batch_id, value)",
    ...COMMON_BOUNDARY_DDL,
  ];
  return createProgram({
    fixtureName: "repeat_is_a_self_carry_chain",
    ddl,
    relColumns: { kick: ["value"], pulse: ["value"] },
    arrivalTargets: ["kick"],
    explainReceipts: [
      {
        label: "recursive_frontier_join",
        sql: sql.recursiveExplainSql,
        args: [1n],
        deltaTable: "repeat_pulse_delta",
        deltaPlanName: "previous_frontier",
      },
    ],
    async executeTick(seam, batchId, arrivals) {
      const currentBatchArgs = batchArg(batchId);
      await executeStatements(seam, [
        {
          sql: `
            INSERT INTO repeat_kick(value)
            SELECT json_extract(arrival.value, '$.row[0]')
            FROM json_each(?2) AS arrival
            WHERE json_extract(arrival.value, '$.rel') = 'kick'
              AND json_extract(arrival.value, '$.sign') = 'add'
          `,
          args: arrivalArgs(batchId, arrivals),
        },
        {
          sql: `
            INSERT INTO boundary_delta(batch_id, rel, sign, row_json)
            SELECT ?1, 'kick', 'add', json_array(json_extract(arrival.value, '$.row[0]'))
            FROM json_each(?2) AS arrival
            WHERE json_extract(arrival.value, '$.rel') = 'kick'
              AND json_extract(arrival.value, '$.sign') = 'add'
          `,
          args: arrivalArgs(batchId, arrivals),
        },
        {
          sql: `
            INSERT OR IGNORE INTO repeat_pulse_delta(batch_id, value)
            SELECT DISTINCT ?1, 1
            FROM json_each(?2) AS arrival
            WHERE json_extract(arrival.value, '$.rel') = 'kick'
              AND json_extract(arrival.value, '$.sign') = 'add'
          `,
          args: arrivalArgs(batchId, arrivals),
        },
        { sql: sql.recursiveDeltaSql, args: batchAndPreviousArgs(batchId) },
        {
          sql: "INSERT OR IGNORE INTO repeat_pulse(value) SELECT value FROM repeat_pulse_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT INTO boundary_delta(batch_id, rel, sign, row_json) SELECT batch_id, 'pulse', 'add', json_array(value) FROM repeat_pulse_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
      ]);
      return (
        (await executeScalar(seam, {
          sql: "SELECT COUNT(*) FROM repeat_pulse_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        })) > 0
      );
    },
  });
}

function countIvmHelperSql(parameters: {
  readonly sourceDeltaTable: string;
  readonly supportTable: string;
}): ICountIvmInlineSql {
  const sourceDeltaTable = quoteIdentifier(parameters.sourceDeltaTable);
  const supportTable = quoteIdentifier(parameters.supportTable);
  return {
    transitionSql: `
      INSERT INTO count_support_transition(batch_id, row_value, old_support, new_support)
      SELECT ?1, source_delta.row_value, COALESCE(current_support.support_count, 0),
             COALESCE(current_support.support_count, 0) + SUM(source_delta.weight)
      FROM ${sourceDeltaTable} AS source_delta
      LEFT JOIN ${supportTable} AS current_support
        ON current_support.row_value = source_delta.row_value
      WHERE source_delta.batch_id = ?1
      GROUP BY source_delta.row_value
    `,
    reseedAuditSql: `
      WITH RECURSIVE live(row_value) AS (
        SELECT row_value FROM count_source
        UNION
        SELECT current_mirror.row_value
        FROM count_mirror AS current_mirror
        JOIN live ON live.row_value = current_mirror.row_value
      ),
      mismatch(row_value) AS (
        SELECT current_mirror.row_value
        FROM count_mirror AS current_mirror
        LEFT JOIN live ON live.row_value = current_mirror.row_value
        WHERE live.row_value IS NULL
        UNION ALL
        SELECT live.row_value
        FROM live
        LEFT JOIN count_mirror AS current_mirror ON current_mirror.row_value = live.row_value
        WHERE current_mirror.row_value IS NULL
      )
      SELECT COUNT(*) FROM mismatch
    `,
    explainSql: `
      SELECT row_value, SUM(weight)
      FROM ${sourceDeltaTable}
      WHERE batch_id = ?1
      GROUP BY row_value
    `,
  };
}

export function createCountIvmInlineVariant(sql: ICountIvmInlineSql): ILabVariant {
  return createCountIvmVariant("inline", sql);
}

export function createCountIvmHelperVariant(parameters: {
  readonly sourceDeltaTable: string;
  readonly supportTable: string;
}): ILabVariant {
  return createCountIvmVariant("helper", countIvmHelperSql(parameters));
}

function createCountIvmVariant(
  spelling: LabSpelling,
  sql: ICountIvmInlineSql,
): ILabVariant {
  return {
    family: "count_ivm_support",
    spelling,
    programs: [createCountIvmProgram(sql)],
  };
}

function createCountIvmProgram(sql: ICountIvmInlineSql): IReceiptProgram {
  const ddl = [
    "CREATE TABLE count_source (row_value TEXT PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE count_source_delta (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL, weight INTEGER NOT NULL)",
    "CREATE INDEX count_source_delta_batch ON count_source_delta(batch_id, row_value)",
    "CREATE TABLE count_support (row_value TEXT PRIMARY KEY, support_count INTEGER NOT NULL) WITHOUT ROWID",
    "CREATE TABLE count_support_transition (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL, old_support INTEGER NOT NULL, new_support INTEGER NOT NULL, PRIMARY KEY(batch_id, row_value)) WITHOUT ROWID",
    "CREATE TABLE count_mirror (row_value TEXT PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE count_closed_at (row_value TEXT NOT NULL, tick_number INTEGER NOT NULL)",
    ...COMMON_BOUNDARY_DDL,
  ];
  return createProgram({
    fixtureName: "departed_fires_next_tick_on_retraction",
    ddl,
    relColumns: { source_row: ["item"], mirror: ["item"], closed_at: ["item", "tick"] },
    arrivalTargets: ["source_row"],
    explainReceipts: [
      {
        label: "support_delta_group",
        sql: sql.explainSql,
        args: [1n],
        deltaTable: "count_source_delta",
      },
    ],
    async executeTick(seam, batchId, arrivals) {
      const currentBatchArgs = batchArg(batchId);
      const currentAndPreviousArgs = batchAndPreviousArgs(batchId);
      await executeStatements(seam, [
        {
          sql: `
            INSERT INTO count_source_delta(batch_id, row_value, weight)
            SELECT ?1, json_extract(arrival.value, '$.row[0]'),
                   CASE json_extract(arrival.value, '$.sign') WHEN 'add' THEN 1 ELSE -1 END
            FROM json_each(?2) AS arrival
            WHERE json_extract(arrival.value, '$.rel') = 'source_row'
          `,
          args: arrivalArgs(batchId, arrivals),
        },
        {
          sql: "INSERT OR IGNORE INTO count_source(row_value) SELECT row_value FROM count_source_delta WHERE batch_id = ?1 AND weight > 0",
          args: currentBatchArgs,
        },
        {
          sql: "DELETE FROM count_source WHERE row_value IN (SELECT row_value FROM count_source_delta WHERE batch_id = ?1 AND weight < 0)",
          args: currentBatchArgs,
        },
        { sql: sql.transitionSql, args: currentBatchArgs },
        {
          sql: `
            INSERT INTO count_support(row_value, support_count)
            SELECT row_value, new_support
            FROM count_support_transition
            WHERE batch_id = ?1
            ON CONFLICT(row_value) DO UPDATE SET support_count = excluded.support_count
          `,
          args: currentBatchArgs,
        },
        {
          sql: "DELETE FROM count_support WHERE support_count <= 0",
        },
        {
          sql: "INSERT OR IGNORE INTO count_mirror(row_value) SELECT row_value FROM count_support_transition WHERE batch_id = ?1 AND old_support <= 0 AND new_support > 0",
          args: currentBatchArgs,
        },
        {
          sql: "DELETE FROM count_mirror WHERE row_value IN (SELECT row_value FROM count_support_transition WHERE batch_id = ?1 AND old_support > 0 AND new_support <= 0)",
          args: currentBatchArgs,
        },
        {
          sql: `
            INSERT INTO boundary_delta(batch_id, rel, sign, row_json)
            SELECT batch_id, 'source_row', CASE WHEN weight > 0 THEN 'add' ELSE 'del' END, json_array(row_value)
            FROM count_source_delta WHERE batch_id = ?1
          `,
          args: currentBatchArgs,
        },
        {
          sql: `
            INSERT INTO boundary_delta(batch_id, rel, sign, row_json)
            SELECT batch_id, 'mirror',
                   CASE WHEN old_support <= 0 AND new_support > 0 THEN 'add' ELSE 'del' END,
                   json_array(row_value)
            FROM count_support_transition
            WHERE batch_id = ?1
              AND ((old_support <= 0 AND new_support > 0) OR (old_support > 0 AND new_support <= 0))
          `,
          args: currentBatchArgs,
        },
        {
          sql: `
            INSERT INTO count_closed_at(row_value, tick_number)
            SELECT json_extract(row_json, '$[0]'), ?1
            FROM boundary_delta
            WHERE batch_id = ?2 AND rel = 'mirror' AND sign = 'del'
          `,
          args: currentAndPreviousArgs,
        },
        {
          sql: `
            INSERT INTO boundary_delta(batch_id, rel, sign, row_json)
            SELECT ?1, 'closed_at', 'add', json_array(json_extract(row_json, '$[0]'), ?1)
            FROM boundary_delta
            WHERE batch_id = ?2 AND rel = 'mirror' AND sign = 'del'
          `,
          args: currentAndPreviousArgs,
        },
      ]);
      const reseedMismatchCount = await executeScalar(seam, sql.reseedAuditSql);
      if (reseedMismatchCount !== 0) {
        throw new Error(`recursive CTE reseed audit found ${reseedMismatchCount} mismatches`);
      }
      const boundaryKinds = await boundaryKindsForBatch(seam, batchId);
      return boundaryKinds.has("mirror:del") || boundaryKinds.has("closed_at:add");
    },
  });
}

async function boundaryKindsForBatch(
  seam: ISqlSeam,
  batchId: number,
): Promise<ReadonlySet<string>> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, {
      sql: "SELECT DISTINCT rel, sign FROM boundary_delta WHERE batch_id = ?1",
      args: batchArg(batchId),
    }),
  );
  return new Set(result.rows.map((resultRow) => `${String(resultRow.rel)}:${String(resultRow.sign)}`));
}

function distinctHelperSql(parameters: {
  readonly demandSourceDeltaTable: string;
  readonly mirrorSourceDeltaTable: string;
}): IDistinctInlineSql {
  const demandSourceDeltaTable = quoteIdentifier(parameters.demandSourceDeltaTable);
  const mirrorSourceDeltaTable = quoteIdentifier(parameters.mirrorSourceDeltaTable);
  return {
    demandAssertSql: `
      INSERT OR IGNORE INTO distinct_demand_delta(batch_id, row_value)
      SELECT DISTINCT ?1, source_delta.row_value
      FROM ${demandSourceDeltaTable} AS source_delta
      WHERE source_delta.batch_id = ?1
        AND NOT EXISTS (
          SELECT 1 FROM distinct_demand AS current_demand
          WHERE current_demand.row_value = source_delta.row_value
        )
    `,
    mirrorAssertSql: `
      INSERT OR IGNORE INTO distinct_mirror_assert(batch_id, row_value)
      SELECT DISTINCT ?1, source_delta.row_value
      FROM ${mirrorSourceDeltaTable} AS source_delta
      WHERE source_delta.batch_id = ?1 AND source_delta.weight > 0
    `,
    mirrorRetractSql: `
      INSERT OR IGNORE INTO distinct_mirror_retract(batch_id, row_value)
      SELECT DISTINCT ?1, source_delta.row_value
      FROM ${mirrorSourceDeltaTable} AS source_delta
      WHERE source_delta.batch_id = ?1 AND source_delta.weight < 0
    `,
    assertExplainSql: `
      SELECT DISTINCT row_value
      FROM ${demandSourceDeltaTable}
      WHERE batch_id = ?1
    `,
    retractExplainSql: `
      SELECT DISTINCT row_value
      FROM ${mirrorSourceDeltaTable}
      WHERE batch_id = ?1 AND weight < 0
    `,
  };
}

export function createDistinctInlineVariant(sql: IDistinctInlineSql): ILabVariant {
  return createDistinctVariant("inline", sql);
}

export function createDistinctHelperVariant(parameters: {
  readonly demandSourceDeltaTable: string;
  readonly mirrorSourceDeltaTable: string;
}): ILabVariant {
  return createDistinctVariant("helper", distinctHelperSql(parameters));
}

function createDistinctVariant(
  spelling: LabSpelling,
  sql: IDistinctInlineSql,
): ILabVariant {
  return {
    family: "distinct_placement",
    spelling,
    programs: [createDemandDistinctProgram(sql), createDepartedDistinctProgram(sql)],
  };
}

function createDemandDistinctProgram(sql: IDistinctInlineSql): IReceiptProgram {
  const ddl = [
    "CREATE TABLE distinct_stale (row_value TEXT NOT NULL)",
    "CREATE TABLE distinct_stale_delta (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL)",
    "CREATE INDEX distinct_stale_delta_batch ON distinct_stale_delta(batch_id, row_value)",
    "CREATE TABLE distinct_demand (row_value TEXT PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE distinct_demand_delta (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL, PRIMARY KEY(batch_id, row_value)) WITHOUT ROWID",
    "CREATE TABLE distinct_fetch_call (row_value TEXT NOT NULL)",
    ...COMMON_BOUNDARY_DDL,
  ];
  return createProgram({
    fixtureName: "demand_view_fires_its_consumer_once",
    ddl,
    relColumns: { stale: ["endpoint"], fetch_demand: ["endpoint"], fetch_call: ["endpoint"] },
    arrivalTargets: ["stale"],
    explainReceipts: [
      {
        label: "distinct_assert_body",
        sql: sql.assertExplainSql,
        args: [1n],
        deltaTable: "distinct_stale_delta",
      },
    ],
    async executeTick(seam, batchId, arrivals) {
      const currentBatchArgs = batchArg(batchId);
      await executeStatements(seam, [
        {
          sql: `
            INSERT INTO distinct_stale_delta(batch_id, row_value)
            SELECT ?1, json_extract(arrival.value, '$.row[0]')
            FROM json_each(?2) AS arrival
            WHERE json_extract(arrival.value, '$.rel') = 'stale'
              AND json_extract(arrival.value, '$.sign') = 'add'
          `,
          args: arrivalArgs(batchId, arrivals),
        },
        {
          sql: "INSERT INTO distinct_stale(row_value) SELECT row_value FROM distinct_stale_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        { sql: sql.demandAssertSql, args: currentBatchArgs },
        {
          sql: "INSERT OR IGNORE INTO distinct_demand(row_value) SELECT row_value FROM distinct_demand_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT INTO distinct_fetch_call(row_value) SELECT row_value FROM distinct_demand_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT INTO boundary_delta(batch_id, rel, sign, row_json) SELECT batch_id, 'stale', 'add', json_array(row_value) FROM distinct_stale_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT INTO boundary_delta(batch_id, rel, sign, row_json) SELECT batch_id, 'fetch_demand', 'add', json_array(row_value) FROM distinct_demand_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT INTO boundary_delta(batch_id, rel, sign, row_json) SELECT batch_id, 'fetch_call', 'add', json_array(row_value) FROM distinct_demand_delta WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
      ]);
      return false;
    },
  });
}

function createDepartedDistinctProgram(sql: IDistinctInlineSql): IReceiptProgram {
  const ddl = [
    "CREATE TABLE distinct_source (row_value TEXT PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE distinct_source_delta (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL, weight INTEGER NOT NULL)",
    "CREATE INDEX distinct_source_delta_batch ON distinct_source_delta(batch_id, row_value, weight)",
    "CREATE TABLE distinct_mirror (row_value TEXT PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE distinct_mirror_assert (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL, PRIMARY KEY(batch_id, row_value)) WITHOUT ROWID",
    "CREATE TABLE distinct_mirror_retract (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL, PRIMARY KEY(batch_id, row_value)) WITHOUT ROWID",
    "CREATE TABLE distinct_closed_at (row_value TEXT NOT NULL, tick_number INTEGER NOT NULL)",
    ...COMMON_BOUNDARY_DDL,
  ];
  return createProgram({
    fixtureName: "departed_fires_next_tick_on_retraction",
    ddl,
    relColumns: { source_row: ["item"], mirror: ["item"], closed_at: ["item", "tick"] },
    arrivalTargets: ["source_row"],
    explainReceipts: [
      {
        label: "distinct_retract_cascade",
        sql: sql.retractExplainSql,
        args: [1n],
        deltaTable: "distinct_source_delta",
      },
    ],
    async executeTick(seam, batchId, arrivals) {
      const currentBatchArgs = batchArg(batchId);
      const currentAndPreviousArgs = batchAndPreviousArgs(batchId);
      await executeStatements(seam, [
        {
          sql: `
            INSERT INTO distinct_source_delta(batch_id, row_value, weight)
            SELECT ?1, json_extract(arrival.value, '$.row[0]'),
                   CASE json_extract(arrival.value, '$.sign') WHEN 'add' THEN 1 ELSE -1 END
            FROM json_each(?2) AS arrival
            WHERE json_extract(arrival.value, '$.rel') = 'source_row'
          `,
          args: arrivalArgs(batchId, arrivals),
        },
        {
          sql: "INSERT OR IGNORE INTO distinct_source(row_value) SELECT row_value FROM distinct_source_delta WHERE batch_id = ?1 AND weight > 0",
          args: currentBatchArgs,
        },
        {
          sql: "DELETE FROM distinct_source WHERE row_value IN (SELECT row_value FROM distinct_source_delta WHERE batch_id = ?1 AND weight < 0)",
          args: currentBatchArgs,
        },
        { sql: sql.mirrorAssertSql, args: currentBatchArgs },
        { sql: sql.mirrorRetractSql, args: currentBatchArgs },
        {
          sql: "INSERT OR IGNORE INTO distinct_mirror(row_value) SELECT row_value FROM distinct_mirror_assert WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        {
          sql: "DELETE FROM distinct_mirror WHERE row_value IN (SELECT row_value FROM distinct_mirror_retract WHERE batch_id = ?1)",
          args: currentBatchArgs,
        },
        {
          sql: `
            INSERT INTO boundary_delta(batch_id, rel, sign, row_json)
            SELECT batch_id, 'source_row', CASE WHEN weight > 0 THEN 'add' ELSE 'del' END, json_array(row_value)
            FROM distinct_source_delta WHERE batch_id = ?1
          `,
          args: currentBatchArgs,
        },
        {
          sql: `
            INSERT INTO boundary_delta(batch_id, rel, sign, row_json)
            SELECT batch_id, 'mirror', 'add', json_array(row_value)
            FROM distinct_mirror_assert WHERE batch_id = ?1
            UNION ALL
            SELECT batch_id, 'mirror', 'del', json_array(row_value)
            FROM distinct_mirror_retract WHERE batch_id = ?1
          `,
          args: currentBatchArgs,
        },
        {
          sql: `
            INSERT INTO distinct_closed_at(row_value, tick_number)
            SELECT json_extract(row_json, '$[0]'), ?1
            FROM boundary_delta
            WHERE batch_id = ?2 AND rel = 'mirror' AND sign = 'del'
          `,
          args: currentAndPreviousArgs,
        },
        {
          sql: `
            INSERT INTO boundary_delta(batch_id, rel, sign, row_json)
            SELECT ?1, 'closed_at', 'add', json_array(json_extract(row_json, '$[0]'), ?1)
            FROM boundary_delta
            WHERE batch_id = ?2 AND rel = 'mirror' AND sign = 'del'
          `,
          args: currentAndPreviousArgs,
        },
      ]);
      const boundaryKinds = await boundaryKindsForBatch(seam, batchId);
      return boundaryKinds.has("mirror:del") || boundaryKinds.has("closed_at:add");
    },
  });
}

function boundaryHelperSql(parameters: {
  readonly streamTable: string;
}): IBoundaryInlineSql {
  const streamTable = quoteIdentifier(parameters.streamTable);
  return {
    selectSql: `
      SELECT rel, sign, row_json
      FROM ${streamTable}
      WHERE batch_id = ?1
      ORDER BY rel, sign, row_json
    `,
    explainSql: `
      SELECT rel, sign, row_json
      FROM ${streamTable}
      WHERE batch_id = ?1
    `,
  };
}

export function createBoundaryInlineVariant(sql: IBoundaryInlineSql): ILabVariant {
  return createBoundaryVariant("inline", sql);
}

export function createBoundaryHelperVariant(parameters: {
  readonly streamTable: string;
}): ILabVariant {
  return createBoundaryVariant("helper", boundaryHelperSql(parameters));
}

function createBoundaryVariant(
  spelling: LabSpelling,
  sql: IBoundaryInlineSql,
): ILabVariant {
  return {
    family: "boundary_diff_delta_stream",
    spelling,
    programs: [createBoundaryProgram(sql)],
  };
}

function createBoundaryProgram(sql: IBoundaryInlineSql): IReceiptProgram {
  const ddl = [
    "CREATE TABLE stream_source (row_value TEXT PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE stream_source_delta (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL, weight INTEGER NOT NULL)",
    "CREATE INDEX stream_source_delta_batch ON stream_source_delta(batch_id, row_value)",
    "CREATE TABLE stream_mirror (row_value TEXT PRIMARY KEY) WITHOUT ROWID",
    "CREATE TABLE stream_mirror_assert (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL, PRIMARY KEY(batch_id, row_value)) WITHOUT ROWID",
    "CREATE TABLE stream_mirror_retract (batch_id INTEGER NOT NULL, row_value TEXT NOT NULL, PRIMARY KEY(batch_id, row_value)) WITHOUT ROWID",
    "CREATE TABLE stream_closed_at (row_value TEXT NOT NULL, tick_number INTEGER NOT NULL)",
    "CREATE TABLE change_stream (batch_id INTEGER NOT NULL, rel TEXT NOT NULL, sign TEXT NOT NULL, row_json TEXT NOT NULL)",
    "CREATE INDEX change_stream_batch ON change_stream(batch_id, rel, sign)",
  ];
  return createProgram({
    fixtureName: "departed_fires_next_tick_on_retraction",
    ddl,
    relColumns: { source_row: ["item"], mirror: ["item"], closed_at: ["item", "tick"] },
    arrivalTargets: ["source_row"],
    boundarySql: sql.selectSql,
    explainReceipts: [
      {
        label: "boundary_delta_stream",
        sql: sql.explainSql,
        args: [1n],
        deltaTable: "change_stream",
      },
    ],
    async executeTick(seam, batchId, arrivals) {
      const currentBatchArgs = batchArg(batchId);
      const currentAndPreviousArgs = batchAndPreviousArgs(batchId);
      await executeStatements(seam, [
        {
          sql: `
            INSERT INTO stream_source_delta(batch_id, row_value, weight)
            SELECT ?1, json_extract(arrival.value, '$.row[0]'),
                   CASE json_extract(arrival.value, '$.sign') WHEN 'add' THEN 1 ELSE -1 END
            FROM json_each(?2) AS arrival
            WHERE json_extract(arrival.value, '$.rel') = 'source_row'
          `,
          args: arrivalArgs(batchId, arrivals),
        },
        {
          sql: "INSERT OR IGNORE INTO stream_source(row_value) SELECT row_value FROM stream_source_delta WHERE batch_id = ?1 AND weight > 0",
          args: currentBatchArgs,
        },
        {
          sql: "DELETE FROM stream_source WHERE row_value IN (SELECT row_value FROM stream_source_delta WHERE batch_id = ?1 AND weight < 0)",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT OR IGNORE INTO stream_mirror_assert(batch_id, row_value) SELECT DISTINCT ?1, row_value FROM stream_source_delta WHERE batch_id = ?1 AND weight > 0",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT OR IGNORE INTO stream_mirror_retract(batch_id, row_value) SELECT DISTINCT ?1, row_value FROM stream_source_delta WHERE batch_id = ?1 AND weight < 0",
          args: currentBatchArgs,
        },
        {
          sql: "INSERT OR IGNORE INTO stream_mirror(row_value) SELECT row_value FROM stream_mirror_assert WHERE batch_id = ?1",
          args: currentBatchArgs,
        },
        {
          sql: "DELETE FROM stream_mirror WHERE row_value IN (SELECT row_value FROM stream_mirror_retract WHERE batch_id = ?1)",
          args: currentBatchArgs,
        },
        {
          sql: `
            INSERT INTO change_stream(batch_id, rel, sign, row_json)
            SELECT batch_id, 'source_row', CASE WHEN weight > 0 THEN 'add' ELSE 'del' END, json_array(row_value)
            FROM stream_source_delta WHERE batch_id = ?1
            UNION ALL
            SELECT batch_id, 'mirror', 'add', json_array(row_value)
            FROM stream_mirror_assert WHERE batch_id = ?1
            UNION ALL
            SELECT batch_id, 'mirror', 'del', json_array(row_value)
            FROM stream_mirror_retract WHERE batch_id = ?1
          `,
          args: currentBatchArgs,
        },
        {
          sql: `
            INSERT INTO stream_closed_at(row_value, tick_number)
            SELECT json_extract(row_json, '$[0]'), ?1
            FROM change_stream
            WHERE batch_id = ?2 AND rel = 'mirror' AND sign = 'del'
          `,
          args: currentAndPreviousArgs,
        },
        {
          sql: `
            INSERT INTO change_stream(batch_id, rel, sign, row_json)
            SELECT ?1, 'closed_at', 'add', json_array(json_extract(row_json, '$[0]'), ?1)
            FROM change_stream
            WHERE batch_id = ?2 AND rel = 'mirror' AND sign = 'del'
          `,
          args: currentAndPreviousArgs,
        },
      ]);
      const result = await firstValueFrom(
        seam.runner.execute(seam.db, {
          sql: "SELECT DISTINCT rel, sign FROM change_stream WHERE batch_id = ?1",
          args: currentBatchArgs,
        }),
      );
      const boundaryKinds = new Set(
        result.rows.map((resultRow) => `${String(resultRow.rel)}:${String(resultRow.sign)}`),
      );
      return boundaryKinds.has("mirror:del") || boundaryKinds.has("closed_at:add");
    },
  });
}
