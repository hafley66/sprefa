import { concatMap, forkJoin, map, of, type Observable } from "rxjs";

import type {
  IAggregateLevelPlan,
  IArrivalBatch,
  IIncrementalEdgeStatement,
  IIncrementalLevelStatement,
  IIncrementalRelationPlan,
  IIncrementalRetentionStatement,
  IIncrementalRuntime,
  IRelDelta,
  IRow,
  IRowValue,
  ISqlSeam,
  QueryResult,
  SqlStatement,
} from "./types.ts";

type DeltaEvent = {
  readonly rel: string;
  readonly sign: 1 | -1;
  readonly sequence: number;
  readonly row: IRow;
};

function quoteIdentifier(identifier: string): string {
  return `"${identifier.replaceAll('"', '""')}"`;
}

function bindArgs(values: readonly IRowValue[]): (string | number | bigint)[] {
  return values.map((value) =>
    typeof value === "number" && Number.isInteger(value) ? BigInt(value) : value,
  );
}

function resultRows(result: QueryResult, columns: readonly string[]): readonly IRow[] {
  return result.rows.map((row) => columns.map((column) => row[column] as IRowValue));
}

function valuesSql(rowCount: number, columnCount: number): string {
  const row = `(${Array.from({ length: columnCount }, () => "?").join(", ")})`;
  return Array.from({ length: rowCount }, () => row).join(", ");
}

function keyedArrivalRowsStatement(
  relation: IIncrementalRelationPlan,
  entries: readonly { readonly row: IRow }[],
  keyIndices: readonly number[],
): SqlStatement {
  const columns = relation.columns.map(quoteIdentifier);
  const keyColumns = keyIndices.map((index) => columns[index]!);
  const distinctKeys = new Map<string, IRow>();
  for (const entry of entries) {
    const keyValues = keyIndices.map((index) => entry.row[index]!) as IRow;
    distinctKeys.set(JSON.stringify(keyValues), keyValues);
  }
  const keys = [...distinctKeys.values()];
  return {
    sql: `SELECT ${columns.join(", ")} FROM ${quoteIdentifier(relation.tableName)} WHERE (${keyColumns.join(", ")}) IN (${valuesSql(keys.length, keyColumns.length)})`,
    args: keys.flatMap(bindArgs),
  };
}

function boundaryStageStatement(
  relation: IIncrementalRelationPlan,
  events: readonly DeltaEvent[],
): SqlStatement {
  const columns = ["_sign", "_sequence", ...relation.columns].map(quoteIdentifier);
  const valueExpressions = columns.map(
    (_column, index) => `json_extract(value, '$[${index}]')`,
  );
  const encodedEvents = events.map((event) => [
    event.sign,
    event.sequence,
    ...event.row,
  ]);
  return {
    sql: `INSERT INTO ${quoteIdentifier(relation.deltaTableName)} (${columns.join(", ")}) SELECT ${valueExpressions.join(", ")} FROM json_each(?)`,
    args: [JSON.stringify(encodedEvents)],
  };
}

function frontierStageStatement(
  relation: IIncrementalRelationPlan,
  tableName: string,
  phase: number,
  events: readonly DeltaEvent[],
): SqlStatement {
  const columns = ["_phase", "_sequence", ...relation.columns].map(quoteIdentifier);
  const valueExpressions = columns.map(
    (_column, index) => `json_extract(value, '$[${index}]')`,
  );
  const encodedEvents = events.map((event) => [phase, event.sequence, ...event.row]);
  return {
    sql: `INSERT INTO ${quoteIdentifier(tableName)} (${columns.join(", ")}) SELECT ${valueExpressions.join(", ")} FROM json_each(?)`,
    args: [JSON.stringify(encodedEvents)],
  };
}

function stageEvents(
  seam: ISqlSeam,
  relations: readonly IIncrementalRelationPlan[],
  events: readonly DeltaEvent[],
  frontierCopies: readonly {
    readonly tableName: (relation: IIncrementalRelationPlan) => string;
    readonly phase: number;
  }[],
): Observable<void> {
  const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
  const eventsByRel = new Map<string, DeltaEvent[]>();
  for (const event of events) {
    const grouped = eventsByRel.get(event.rel);
    if (grouped === undefined) eventsByRel.set(event.rel, [event]);
    else grouped.push(event);
  }
  const statements = [...eventsByRel].flatMap(([rel, grouped]) => {
    const relation = relationByName.get(rel);
    if (relation === undefined) throw new Error(`incremental delta relation missing: ${rel}`);
    const boundary = boundaryStageStatement(relation, grouped);
    const additions = grouped.filter((event) => event.sign === 1);
    if (additions.length === 0) return [boundary];
    return [
      boundary,
      ...frontierCopies.map((copy) =>
        frontierStageStatement(
          relation,
          copy.tableName(relation),
          copy.phase,
          additions,
        )
      ),
    ];
  });
  if (statements.length === 0) return of(undefined);
  return seam.runner.batch(seam.db, statements).pipe(map(() => undefined));
}

/**
 * Replace the carry-in frontiers used by emitted ordered-occurrence programs
 * with this tick's boundary-visible additions. Intermediate keyed fold rows
 * never reach this function: the emitter supplies only its start/end diff.
 */
export function stageOrderedFrontiers(
  seam: ISqlSeam,
  relations: readonly IIncrementalRelationPlan[],
  additions: readonly IRelDelta[],
): Observable<boolean> {
  const eventsByRel = new Map<string, DeltaEvent[]>();
  let sequence = 0;
  for (const delta of additions) {
    for (const row of delta.add) {
      const event: DeltaEvent = { rel: delta.rel, sign: 1, sequence, row };
      sequence += 1;
      const grouped = eventsByRel.get(delta.rel);
      if (grouped === undefined) eventsByRel.set(delta.rel, [event]);
      else grouped.push(event);
    }
  }
  const statements: SqlStatement[] = [];
  let carryPending = false;
  for (const relation of relations) {
    statements.push(
      { sql: `DELETE FROM ${quoteIdentifier(relation.frontierTableName)}`, args: [] },
      { sql: `DELETE FROM ${quoteIdentifier(relation.nextFrontierTableName)}`, args: [] },
    );
    const events = eventsByRel.get(relation.rel) ?? [];
    if (events.length === 0) continue;
    carryPending = true;
    statements.push(
      frontierStageStatement(relation, relation.frontierTableName, 0, events),
    );
  }
  if (statements.length === 0) return of(carryPending);
  return seam.runner.batch(seam.db, statements).pipe(map(() => carryPending));
}

function keyedRowsSql(
  statement: IIncrementalEdgeStatement,
  rowCount: number,
): string {
  const columns = statement.headColumns.map(quoteIdentifier);
  const keyColumns = statement.keyIndices.map((index) => columns[index]!);
  return `SELECT ${columns.join(", ")} FROM ${quoteIdentifier(statement.headTableName)} WHERE (${keyColumns.join(", ")}) IN (${valuesSql(rowCount, keyColumns.length)})`;
}

function rowKey(row: IRow, indices: readonly number[]): string {
  return JSON.stringify(indices.map((index) => row[index]));
}

function rowsEqual(left: IRow, right: IRow): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function keyedWriteStatement(
  statement: IIncrementalEdgeStatement,
  rows: readonly IRow[],
): SqlStatement {
  const columns = statement.headColumns.map(quoteIdentifier);
  const keyColumns = statement.keyIndices.map((index) => columns[index]!);
  const keyIndexSet = new Set(statement.keyIndices);
  const nonKeyColumns = columns.filter((_column, index) => !keyIndexSet.has(index));
  const conflict = nonKeyColumns.length === 0
    ? `ON CONFLICT(${keyColumns.join(", ")}) DO NOTHING`
    : `ON CONFLICT(${keyColumns.join(", ")}) DO UPDATE SET ${nonKeyColumns
        .map((column) => `${column} = excluded.${column}`)
        .join(", ")}`;
  return {
    sql: `INSERT INTO ${quoteIdentifier(statement.headTableName)} (${columns.join(", ")}) VALUES ${valuesSql(rows.length, columns.length)} ${conflict}`,
    args: rows.flatMap(bindArgs),
  };
}

function logWriteStatement(
  statement: IIncrementalEdgeStatement,
  rows: readonly IRow[],
): SqlStatement {
  const columns = statement.headColumns.map(quoteIdentifier);
  return {
    sql: `INSERT INTO ${quoteIdentifier(statement.headTableName)} (${columns.join(", ")}) VALUES ${valuesSql(rows.length, columns.length)}`,
    args: rows.flatMap(bindArgs),
  };
}

function applyLogEdge(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
  relation: IIncrementalRelationPlan,
  rows: readonly IRow[],
): Observable<void> {
  if (rows.length === 0) return of(undefined);
  const events = rows.map(
    (row, sequence): DeltaEvent => ({ rel: statement.headRel, sign: 1, sequence, row }),
  );
  return seam.runner.execute(seam.db, logWriteStatement(statement, rows)).pipe(
    concatMap(() =>
      stageEvents(seam, [relation], events, [
        { tableName: (plan) => plan.nextFrontierTableName, phase: 0 },
      ])
    ),
  );
}

function applyKeyedEdge(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
  relation: IIncrementalRelationPlan,
  projectedRows: readonly IRow[],
): Observable<void> {
  const resolved = new Map<string, IRow>();
  for (const row of projectedRows) resolved.set(rowKey(row, statement.keyIndices), row);
  const rows = [...resolved.values()];
  if (rows.length === 0) return of(undefined);
  const keyArgs = rows.flatMap((row) =>
    bindArgs(statement.keyIndices.map((index) => row[index]!) as IRow),
  );
  return seam.runner
    .execute(seam.db, { sql: keyedRowsSql(statement, rows.length), args: keyArgs })
    .pipe(
      concatMap((beforeResult) => {
        const beforeRows = resultRows(beforeResult, statement.headColumns);
        const beforeByKey = new Map(
          beforeRows.map((row) => [rowKey(row, statement.keyIndices), row]),
        );
        const changedRows = rows.filter((row) => {
          const before = beforeByKey.get(rowKey(row, statement.keyIndices));
          return before === undefined || !rowsEqual(before, row);
        });
        if (changedRows.length === 0) return of(undefined);
        const events: DeltaEvent[] = [];
        for (const [sequence, row] of changedRows.entries()) {
          const before = beforeByKey.get(rowKey(row, statement.keyIndices));
          if (before !== undefined) {
            events.push({ rel: statement.headRel, sign: -1, sequence: sequence * 2, row: before });
          }
          events.push({ rel: statement.headRel, sign: 1, sequence: sequence * 2 + 1, row });
        }
        return seam.runner.execute(seam.db, keyedWriteStatement(statement, changedRows)).pipe(
          concatMap(() =>
            stageEvents(seam, [relation], events, [
              { tableName: (plan) => plan.nextFrontierTableName, phase: 0 },
            ])
          ),
        );
      }),
    );
}

function applyEdgeStatement(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
  relationByName: ReadonlyMap<string, IIncrementalRelationPlan>,
): Observable<void> {
  const relation = relationByName.get(statement.headRel);
  if (relation === undefined) {
    throw new Error(`incremental edge head relation missing: ${statement.headRel}`);
  }
  return seam.runner.execute(seam.db, statement.projectSql).pipe(
    concatMap((result) => {
      const rows = resultRows(result, statement.headColumns);
      return statement.headKind === "log"
        ? applyLogEdge(seam, statement, relation, rows)
        : applyKeyedEdge(seam, statement, relation, rows);
    }),
  );
}

function levelFrontierCopies(
  afterEdges: boolean,
): readonly { tableName: (plan: IIncrementalRelationPlan) => string; phase: number }[] {
  return afterEdges
    ? [
        { tableName: (plan: IIncrementalRelationPlan) => plan.frontierTableName, phase: 2 },
        { tableName: (plan: IIncrementalRelationPlan) => plan.nextFrontierTableName, phase: 1 },
      ]
    : [{ tableName: (plan: IIncrementalRelationPlan) => plan.frontierTableName, phase: 2 }];
}

/**
 * Group-scoped aggregate maintenance (IAggregateLevelPlan). The DELETE and
 * every INSERT return only the rows of the AFFECTED GROUPS, so the aggregate
 * head is re-derived without a full-table read on either side of the seam --
 * the host_residency criterion holds here exactly as it does on the delta-join
 * path. min/max ride this same shape because incremental min/max over a
 * retractable set is not decomposable (match-frontier lab), and the scope is
 * what keeps the recompute off the whole table.
 */
function applyAggregateLevelStatement(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
  aggregate: IAggregateLevelPlan,
  relation: IIncrementalRelationPlan,
  afterEdges: boolean,
  nextSequence: () => number,
): Observable<void> {
  const scopeStatements: SqlStatement[] = [
    { sql: aggregate.scopeClearSql, args: [] },
    ...aggregate.scopeSeedSql.map((sql): SqlStatement => ({ sql, args: [] })),
  ];
  return seam.runner.batch(seam.db, scopeStatements).pipe(
    concatMap(() => seam.runner.execute(seam.db, aggregate.deleteScopedSql)),
    concatMap((deleteResult) => {
      const removedRows = resultRows(deleteResult, statement.headColumns);
      return seam.runner
        .batch(
          seam.db,
          aggregate.insertScopedSql.map((sql): SqlStatement => ({ sql, args: [] })),
        )
        .pipe(
          concatMap((insertResults) => {
            const events: DeltaEvent[] = removedRows.map((row) => ({
              rel: statement.headRel,
              sign: -1 as const,
              sequence: nextSequence(),
              row,
            }));
            for (const insertResult of insertResults) {
              for (const row of resultRows(insertResult, statement.headColumns)) {
                events.push({
                  rel: statement.headRel,
                  sign: 1,
                  sequence: nextSequence(),
                  row,
                });
              }
            }
            if (events.length === 0) return of(undefined);
            return stageEvents(seam, [relation], events, levelFrontierCopies(afterEdges));
          }),
        );
    }),
  );
}

function applyLevelStatement(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
  relationByName: ReadonlyMap<string, IIncrementalRelationPlan>,
  afterEdges: boolean,
  nextSequence: () => number,
): Observable<void> {
  const relation = relationByName.get(statement.headRel);
  if (relation === undefined) {
    throw new Error(`incremental level head relation missing: ${statement.headRel}`);
  }
  if (statement.aggregateSql !== null) {
    return applyAggregateLevelStatement(
      seam,
      statement,
      statement.aggregateSql,
      relation,
      afterEdges,
      nextSequence,
    );
  }
  if (statement.insertSql === null) {
    throw new Error(`incremental level statement has neither insertSql nor aggregateSql: ${statement.headRel}`);
  }
  return seam.runner.execute(seam.db, statement.insertSql).pipe(
    concatMap((result) => {
      const rows = resultRows(result, statement.headColumns);
      if (rows.length === 0) return of(undefined);
      const events = rows.map(
        (row, sequence): DeltaEvent => ({ rel: statement.headRel, sign: 1, sequence, row }),
      );
      return stageEvents(seam, [relation], events, levelFrontierCopies(afterEdges));
    }),
  );
}

function recomputeLevelStatement(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
  relationByName: ReadonlyMap<string, IIncrementalRelationPlan>,
): Observable<void> {
  const relation = relationByName.get(statement.headRel);
  if (relation === undefined) {
    throw new Error(`incremental level head relation missing: ${statement.headRel}`);
  }
  return seam.runner.execute(seam.db, statement.selectSql).pipe(
    concatMap((beforeResult) =>
      seam.runner.executeMultiple(seam.db, statement.recomputeSql).pipe(
        concatMap(() => seam.runner.execute(seam.db, statement.selectSql)),
        concatMap((afterResult) => {
          const beforeRows = resultRows(beforeResult, statement.headColumns);
          const afterRows = resultRows(afterResult, statement.headColumns);
          const beforeByKey = new Map(beforeRows.map((row) => [JSON.stringify(row), row]));
          const afterByKey = new Map(afterRows.map((row) => [JSON.stringify(row), row]));
          const events: DeltaEvent[] = [];
          let sequence = 0;
          for (const [key, row] of beforeByKey) {
            if (!afterByKey.has(key)) {
              events.push({ rel: statement.headRel, sign: -1, sequence, row });
              sequence += 1;
            }
          }
          for (const [key, row] of afterByKey) {
            if (!beforeByKey.has(key)) {
              events.push({ rel: statement.headRel, sign: 1, sequence, row });
              sequence += 1;
            }
          }
          return stageEvents(
            seam,
            [relation],
            events,
            [{ tableName: (plan) => plan.nextFrontierTableName, phase: 1 }],
          );
        }),
      )
    ),
  );
}

function sequenceWork<Item>(
  items: readonly Item[],
  run: (item: Item) => Observable<void>,
): Observable<void> {
  return items.reduce(
    (work, item) => work.pipe(concatMap(() => run(item))),
    of(undefined) as Observable<void>,
  );
}

/**
 * The refCount reconcile of ONE plain (non-aggregate) level statement:
 * reseed `__support_next_<rel>` from the base tables, subtract the difference
 * into the head's own count, delete what fell to zero, insert what is newly
 * derivable. Extracted from `recomputeLevelsAfterEdges` so the pre-edge pass
 * (TICK PHASE ALIGNMENT) runs the SAME five statements against a different
 * frontier target -- a mid-tick correction's new rows are THIS tick's
 * occurrences (frontier phase 2), the post-edge pass's are next tick's
 * (nextFrontier phase 1). The statement TEXT is emitter-owned and identical
 * on both passes; only where the +1 events are copied differs.
 */
function reconcileSupportStatement(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
  relations: readonly IIncrementalRelationPlan[],
  frontierCopies: readonly {
    readonly tableName: (relation: IIncrementalRelationPlan) => string;
    readonly phase: number;
  }[],
  nextSequence: () => number,
): Observable<void> {
  if (statement.supportSql === null) {
    throw new Error(
      `incremental level statement has neither supportSql nor aggregateSql: ${statement.headRel}`,
    );
  }
  const sql = statement.supportSql.map((text): SqlStatement => ({ sql: text, args: [] }));
  return seam.runner.batch(seam.db, sql).pipe(
    concatMap((results) => {
      const events: DeltaEvent[] = [];
      const deletedRows = resultRows(results[3]!, statement.headColumns);
      const insertedRows = resultRows(results[4]!, statement.headColumns);
      for (const row of deletedRows) {
        events.push({ rel: statement.headRel, sign: -1, sequence: nextSequence(), row });
      }
      for (const row of insertedRows) {
        events.push({ rel: statement.headRel, sign: 1, sequence: nextSequence(), row });
      }
      if (events.length === 0) return of(undefined);
      return stageEvents(seam, relations, events, frontierCopies);
    }),
  );
}

/** `1` when some delta table already holds a retraction this tick. The gate
 *  both reconcile passes use when the program has no negative level body
 *  (`reconcileEveryTick === false`): with only monotone level bodies, a level
 *  row can stop being derivable only if one of its inputs left. */
function retractionGuardSql(relations: readonly IIncrementalRelationPlan[]): string {
  const terms = relations.map(
    (relation) =>
      `EXISTS (SELECT 1 FROM ${quoteIdentifier(relation.deltaTableName)} WHERE "_sign" = -1 LIMIT 1)`,
  );
  return `SELECT CASE WHEN ${terms.join(" OR ")} THEN 1 ELSE 0 END AS has_retraction`;
}

function applyRetentionStatement(
  seam: ISqlSeam,
  statement: IIncrementalRetentionStatement,
  relationByName: ReadonlyMap<string, IIncrementalRelationPlan>,
  nextSequence: () => number,
): Observable<void> {
  const relation = relationByName.get(statement.rel);
  if (relation === undefined) {
    throw new Error(`incremental retention relation missing: ${statement.rel}`);
  }
  return seam.runner.execute(seam.db, statement.deleteSql).pipe(
    concatMap((result) => {
      const rows = resultRows(result, relation.columns);
      if (rows.length === 0) return of(undefined);
      const events = rows.map(
        (row): DeltaEvent => ({
          rel: statement.rel,
          sign: -1,
          sequence: nextSequence(),
          row,
        }),
      );
      return stageEvents(seam, [relation], events, []);
    }),
  );
}

function boundaryDelta(
  relation: IIncrementalRelationPlan,
  result: QueryResult,
): IRelDelta {
  const weights = new Map<string, { row: IRow; weight: number }>();
  for (const resultRow of result.rows) {
    const row = relation.columns.map((column) => resultRow[column] as IRowValue);
    const key = JSON.stringify(row);
    const weight = Number(resultRow.__sign) * Number(resultRow.__count);
    const previous = weights.get(key);
    weights.set(key, { row, weight: (previous?.weight ?? 0) + weight });
  }
  const add: IRow[] = [];
  const del: IRow[] = [];
  for (const { row, weight } of weights.values()) {
    for (let count = 0; count < weight; count += 1) add.push(row);
    if (relation.kind === "set") {
      for (let count = 0; count > weight; count -= 1) del.push(row);
    }
  }
  return { rel: relation.rel, add, del };
}

export const IncrementalRuntime: IIncrementalRuntime = {
  prepareTick(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    if (relations.length === 0) return of(undefined);
    const sql = relations
      .flatMap((relation) => [
        `DELETE FROM ${quoteIdentifier(relation.deltaTableName)}`,
        `DELETE FROM ${quoteIdentifier(relation.nextFrontierTableName)}`,
      ])
      .join(";\n");
    return seam.runner.executeMultiple(seam.db, sql);
  },

  applyArrivals(
    seam: ISqlSeam,
    arrivals: IArrivalBatch,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    if (arrivals.length === 0) return of(undefined);
    const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
    type ArrivalEntry = { readonly sequence: number; readonly row: IRow };
    type ArrivalGroup = {
      readonly relation: IIncrementalRelationPlan;
      readonly sign: 1 | -1;
      readonly entries: ArrivalEntry[];
    };
    const groupedArrivals: ArrivalGroup[] = [];
    for (const [sequence, arrival] of arrivals.entries()) {
      const relation = relationByName.get(arrival.rel);
      if (relation === undefined) {
        throw new Error(`incremental arrival relation missing: ${arrival.rel}`);
      }
      const sign = arrival.sign === "add" ? 1 : -1;
      const sql = sign === 1 ? relation.arrivalAddSql : relation.arrivalDelSql;
      if (sql === null) {
        throw new Error(
          sign === -1 && relation.kind === "log"
            ? `retract from log rel '${arrival.rel}'`
            : `incremental ${sign === 1 ? "add" : "delete"} statement missing: ${arrival.rel}`,
        );
      }
      const previous = groupedArrivals.at(-1);
      const entry = { sequence, row: arrival.row };
      if (
        previous !== undefined &&
        previous.relation.rel === relation.rel &&
        previous.sign === sign
      ) {
        previous.entries.push(entry);
      } else {
        groupedArrivals.push({ relation, sign, entries: [entry] });
      }
    }
    return sequenceWork(groupedArrivals, ({ relation, sign, entries }) => {
      const sql = (sign === 1 ? relation.arrivalAddSql : relation.arrivalDelSql)!;
      const writeStatement: SqlStatement = {
        sql,
        args: [JSON.stringify(entries.map((entry) => entry.row))],
      };
      const keyIndices = relation.keyIndices ?? [];
      if (relation.kind === "set" && sign === 1 && keyIndices.length > 0) {
        return seam.runner
          .execute(seam.db, keyedArrivalRowsStatement(relation, entries, keyIndices))
          .pipe(
            concatMap((beforeResult) => {
              const currentByKey = new Map(
                resultRows(beforeResult, relation.columns).map((row) => [
                  rowKey(row, keyIndices),
                  row,
                ]),
              );
              const events: DeltaEvent[] = [];
              for (const entry of entries) {
                const key = rowKey(entry.row, keyIndices);
                const before = currentByKey.get(key);
                if (before !== undefined && rowsEqual(before, entry.row)) continue;
                if (before !== undefined) {
                  events.push({
                    rel: relation.rel,
                    sign: -1,
                    sequence: entry.sequence * 2,
                    row: before,
                  });
                }
                events.push({
                  rel: relation.rel,
                  sign: 1,
                  sequence: entry.sequence * 2 + 1,
                  row: entry.row,
                });
                currentByKey.set(key, entry.row);
              }
              return seam.runner.execute(seam.db, writeStatement).pipe(
                concatMap(() =>
                  stageEvents(
                    seam,
                    relations,
                    events,
                    [{ tableName: (plan) => plan.frontierTableName, phase: 1 }],
                  )
                ),
              );
            }),
          );
      }
      return seam.runner.execute(seam.db, writeStatement).pipe(
        concatMap((result) => {
          const events: DeltaEvent[] = [];
          if (relation.kind === "log" && sign === 1) {
            for (const entry of entries) {
              events.push({
                rel: relation.rel,
                sign: 1,
                sequence: entry.sequence,
                row: entry.row,
              });
            }
            return stageEvents(
              seam,
              relations,
              events,
              [{ tableName: (plan) => plan.frontierTableName, phase: 1 }],
            );
          }
          const changedRows = new Set(
            resultRows(result, relation.columns).map((row) => JSON.stringify(row)),
          );
          const stagedRows = new Set<string>();
          for (const entry of entries) {
            const row = JSON.stringify(entry.row);
            if (!changedRows.has(row) || stagedRows.has(row)) continue;
            stagedRows.add(row);
            events.push({
              rel: relation.rel,
              sign,
              sequence: entry.sequence,
              row: entry.row,
            });
          }
          return stageEvents(
            seam,
            relations,
            events,
            [{ tableName: (plan) => plan.frontierTableName, phase: 1 }],
          );
        }),
      );
    });
  },

  applyEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalEdgeStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
    return sequenceWork(
      statements,
      (statement) => applyEdgeStatement(seam, statement, relationByName),
    );
  },

  applyLevelsBeforeEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
    let sequence = 0;
    const nextSequence = (): number => {
      const current = sequence;
      sequence += 1;
      return current;
    };
    return sequenceWork(
      statements,
      (statement) => applyLevelStatement(seam, statement, relationByName, false, nextSequence),
    );
  },

  /**
   * TICK PHASE ALIGNMENT: the emitted mid-tick level plane, frozen the way
   * engine.pl freezes it.
   *
   * engine.pl:tick/7 computes `MidLevel = level_closure(store AFTER arrivals)`
   * and hands it to `process_occurrences/7` as `frozen(MidLevel, PrevLevel)`,
   * so a level row an arrival RETRACTED this tick is already gone from the
   * `Visible` an edge body reads. `applyLevelsBeforeEdges` is insert-only
   * (`INSERT OR IGNORE ... RETURNING` plus, for aggregate heads, a
   * group-scoped delete+reinsert), so before this pass existed the retracted
   * half of that closure only ran in `recomputeLevelsAfterEdges` -- AFTER the
   * edges had already joined the stale rows. Receipt, `clock_rel_join_storms`
   * tick 3: three `diag_seen` rows where the oracle derives one.
   *
   * Three narrowings, each with its reason:
   *   - `arrivals.length === 0` returns immediately. The level tables at tick
   *     start are the closure the previous tick's post-edge pass left; with no
   *     arrivals nothing has moved them, so the frozen closure is already on
   *     disk and a drain tick pays zero statements.
   *   - aggregate statements are skipped: `applyLevelsBeforeEdges` already
   *     dispatches them to `applyAggregateLevelStatement`, whose scoped
   *     DELETE+INSERT is a full correction of the affected groups, retraction
   *     included. Running them twice would restage net-zero delta pairs.
   *   - the `reconcileEveryTick` / retraction-guard policy is the SAME one
   *     `recomputeLevelsAfterEdges` uses, not a new one: `reconcileEveryTick`
   *     is emitted true exactly when some level rule has a NEGATED body ref
   *     (emit_ts.pl:reconcile_every_tick/2), which is the one way an ADDED row
   *     can retract a level row without staging any -1.
   */
  recomputeLevelsBeforeEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
    reconcileEveryTick: boolean,
    arrivals: IArrivalBatch,
  ): Observable<void> {
    if (arrivals.length === 0) return of(undefined);
    const supportStatements = statements.filter((statement) => statement.aggregateSql !== null ? false : true);
    if (supportStatements.length === 0 || relations.length === 0) return of(undefined);
    const reconcile = (): Observable<void> => {
      let sequence = 0;
      const nextSequence = (): number => {
        const current = sequence;
        sequence += 1;
        return current;
      };
      return sequenceWork(supportStatements, (statement) =>
        reconcileSupportStatement(
          seam,
          statement,
          relations,
          [{ tableName: (plan) => plan.frontierTableName, phase: 2 }],
          nextSequence,
        ),
      );
    };
    if (reconcileEveryTick) return reconcile();
    return seam.runner.execute(seam.db, retractionGuardSql(relations)).pipe(
      concatMap((result) =>
        Number(result.rows[0]?.has_retraction ?? 0) === 0 ? of(undefined) : reconcile()
      ),
    );
  },

  mergeNextIntoCurrent(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    if (relations.length === 0) return of(undefined);
    const sql = relations
      .map((relation) => {
        const columns = ["_phase", "_sequence", ...relation.columns]
          .map(quoteIdentifier)
          .join(", ");
        return `INSERT INTO ${quoteIdentifier(relation.frontierTableName)} (${columns}) SELECT ${columns} FROM ${quoteIdentifier(relation.nextFrontierTableName)}`;
      })
      .join(";\n");
    return seam.runner.executeMultiple(seam.db, sql);
  },

  applyLevelsAfterEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
    let sequence = 0;
    const nextSequence = (): number => {
      const current = sequence;
      sequence += 1;
      return current;
    };
    return sequenceWork(
      statements,
      (statement) => applyLevelStatement(seam, statement, relationByName, true, nextSequence),
    );
  },

  applyRetention(
    seam: ISqlSeam,
    statements: readonly IIncrementalRetentionStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
    let sequence = 0;
    const nextSequence = (): number => {
      const current = sequence;
      sequence += 1;
      return current;
    };
    return sequenceWork(
      statements,
      (statement) =>
        applyRetentionStatement(seam, statement, relationByName, nextSequence),
    );
  },

  recomputeLevelsAfterEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
    reconcileEveryTick: boolean,
  ): Observable<void> {
    if (statements.length === 0) return of(undefined);
    const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
    // Per-statement rather than one batch over every statement's supportSql:
    // the list is in strat.pl's own stratum order, and an AGGREGATE statement
    // dispatches to the group-scoped plan instead of a refCount reconcile.
    // Running them in order is what lets an aggregate observe the refCount
    // deletions of the rel it reads (an aggregate always sits in a strictly
    // higher stratum than its inputs -- level_eval.pl forces Gap=1 for every
    // body ref of an aggregate head, and strat.pl mirrors that).
    // `sequence` stays a single running counter across the whole reconcile so
    // staged event ordering is unchanged from the batched shape.
    const reconcile = (): Observable<void> => {
      let sequence = 0;
      const nextSequence = (): number => {
        const current = sequence;
        sequence += 1;
        return current;
      };
      return sequenceWork(statements, (statement) => {
        const relation = relationByName.get(statement.headRel);
        if (relation === undefined) {
          throw new Error(`incremental level head relation missing: ${statement.headRel}`);
        }
        if (statement.aggregateSql !== null) {
          // afterEdges=false HERE, unlike applyLevelsAfterEdges. engine.pl's
          // carry set is edge-written rows plus POST-WRITE level growth
          // (tick/7: `ord_subtract(Level, MidLevel, PostWriteLevelRows)` --
          // rows that became true because EDGE WRITES moved the store between
          // the two level closures). A reconcile pass is a correction inside
          // the same closure, not post-write growth, so its rows must not
          // reach the next-tick frontier.
          //
          // MEASURED: with afterEdges=true here, both across-ticks aggregate
          // fixtures grew one spurious empty drain tick past the oracle's last
          // line (actual {"tick":4,"deltas":{}} vs oracle <missing tick>) --
          // promoteFrontiers reports carryPending from any row in a
          // nextFrontier table, and the reconcile's +1 events had landed
          // there.
          return applyAggregateLevelStatement(
            seam,
            statement,
            statement.aggregateSql,
            relation,
            false,
            nextSequence,
          );
        }
        // No frontier copies HERE either, for the same reason as the
        // aggregate branch above: a reconcile row is a correction inside the
        // same closure, never post-write growth, so it must not reach the
        // next-tick frontier. The refCount branch shipped from P3 staging
        // into nextFrontier phase 1; the flagship callgraph fixture is the
        // corpus member that finally distinguished the two — a schedule
        // ending on its retraction tick minted one {"deltas":{}} drain the
        // oracle lacks (extraDrainTick.test.ts holds the fail-first receipt).
        return reconcileSupportStatement(
          seam,
          statement,
          relations,
          [],
          nextSequence,
        );
      });
    };
    if (reconcileEveryTick) return reconcile();
    if (relations.length === 0) return of(undefined);
    return seam.runner.execute(seam.db, retractionGuardSql(relations)).pipe(
      concatMap((result) =>
        Number(result.rows[0]?.has_retraction ?? 0) === 0 ? of(undefined) : reconcile()
      ),
    );
  },

  readBoundary(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<readonly IRelDelta[]> {
    if (relations.length === 0) return of([]);
    return forkJoin(
      relations.map((relation) =>
        seam.runner.execute(seam.db, relation.boundarySql).pipe(
          map((result) => boundaryDelta(relation, result)),
        ),
      ),
    );
  },

  /**
   * The DEPARTURE half of engine.pl's carry-out. `tick/7` turns each -delta of
   * a LISTENED rel into a `dep(Row)` occurrence at T+1:
   *
   *   findall(dep(Row), ( member(-Row, Deltas), memberchk(DepRef, DepartureRefs) ), ...)
   *
   * so the source is the tick's BOUNDARY delta, never the raw staged events: a
   * row removed and re-added inside one tick nets to zero and is not a
   * departure. That is why this runs on `readBoundary`'s own result rather
   * than reading the delta tables again -- the net is already computed, and no
   * statement is spent recomputing it.
   *
   * A SEPARATE table per listened rel, not a `_sign` column on the shared
   * frontier: a sign column changes the DDL, the promote column list and the
   * merge column list of EVERY relation, including the ones no rule listens
   * to. This way a program with no `finalize` in it emits exactly the text it
   * emitted before this existed.
   *
   * Cleared and refilled here, at the END of the tick, NOT in `prepareTick`:
   * during a tick the table still holds the PREVIOUS tick's departures, which
   * is what the arms are reading.
   *
   * Log rels never reach this: `boundaryDelta` fills `del` for `kind === "set"`
   * only, mirroring engine.pl's `delta_ref_is_set/2`. `finalize` over a Log rel
   * is silently dead in both implementations (update-arm verdict U5,
   * SLOT-LOG-FINALIZE-REFUSAL, unruled).
   */
  stageDepartures(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
    deltas: readonly IRelDelta[],
  ): Observable<void> {
    const listening = relations.filter(
      (relation) => relation.departureFrontierTableName !== undefined,
    );
    if (listening.length === 0) return of(undefined);
    const deltaByRel = new Map(deltas.map((delta) => [delta.rel, delta]));
    const statements = listening.flatMap((relation): SqlStatement[] => {
      const table = quoteIdentifier(relation.departureFrontierTableName!);
      const clear: SqlStatement = { sql: `DELETE FROM ${table}`, args: [] };
      const departed = deltaByRel.get(relation.rel)?.del ?? [];
      if (departed.length === 0) return [clear];
      const columns = ["_phase", "_sequence", ...relation.columns].map(quoteIdentifier);
      const valueExpressions = columns.map(
        (_column, index) => `json_extract(value, '$[${index}]')`,
      );
      const encoded = departed.map((row, sequence) => [0, sequence, ...row]);
      return [
        clear,
        {
          sql: `INSERT INTO ${table} (${columns.join(", ")}) SELECT ${valueExpressions.join(", ")} FROM json_each(?)`,
          args: [JSON.stringify(encoded)],
        },
      ];
    });
    return seam.runner.batch(seam.db, statements).pipe(map(() => undefined));
  },

  promoteFrontiers(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<boolean> {
    if (relations.length === 0) return of(false);
    const carryTerms = relations.flatMap((relation) => [
      `EXISTS (SELECT 1 FROM ${quoteIdentifier(relation.nextFrontierTableName)} LIMIT 1)`,
      // A staged departure is carry exactly the way a staged addition is:
      // engine.pl appends DepartureCarry to ArrivalCarry in one CarryOut list,
      // and a non-empty CarryOut is what mints the drain tick. Leaving it out
      // makes the drain boundary lie -- the arm would be waiting on a tick
      // that never runs.
      ...(relation.departureFrontierTableName === undefined
        ? []
        : [`EXISTS (SELECT 1 FROM ${quoteIdentifier(relation.departureFrontierTableName)} LIMIT 1)`]),
    ]);
    const carrySql = `SELECT CASE WHEN ${carryTerms.join(" OR ")} THEN 1 ELSE 0 END AS carry_pending`;
    const promoteSql = relations
      .flatMap((relation) => {
        const columns = ["_phase", "_sequence", ...relation.columns]
          .map(quoteIdentifier)
          .join(", ");
        return [
          `DELETE FROM ${quoteIdentifier(relation.frontierTableName)}`,
          `INSERT INTO ${quoteIdentifier(relation.frontierTableName)} (${columns}) SELECT ${columns} FROM ${quoteIdentifier(relation.nextFrontierTableName)}`,
          `DELETE FROM ${quoteIdentifier(relation.nextFrontierTableName)}`,
        ];
      })
      .join(";\n");
    return seam.runner.execute(seam.db, carrySql).pipe(
      concatMap((result) => {
        const carryPending = Number(result.rows[0]?.carry_pending ?? 0) === 1;
        return seam.runner.executeMultiple(seam.db, promoteSql).pipe(map(() => carryPending));
      }),
    );
  },
};
