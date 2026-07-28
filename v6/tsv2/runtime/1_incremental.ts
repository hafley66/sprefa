import { concatMap, forkJoin, map, of, type Observable } from "rxjs";

import type {
  IAggregateLevelPlan,
  IArrivalBatch,
  IIncrementalEdgeStatement,
  IIncrementalLevelStatement,
  IIncrementalRelationPlan,
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
    for (let count = 0; count > weight; count -= 1) del.push(row);
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
    const statements = groupedArrivals.map(({ relation, sign, entries }): SqlStatement => ({
      sql: (sign === 1 ? relation.arrivalAddSql : relation.arrivalDelSql)!,
      args: [JSON.stringify(entries.map((entry) => entry.row))],
    }));
    return seam.runner.batch(seam.db, statements).pipe(
      concatMap((results) => {
        const events: DeltaEvent[] = [];
        for (const [groupIndex, result] of results.entries()) {
          const { relation, sign, entries } = groupedArrivals[groupIndex]!;
          if (relation.kind === "log" && sign === 1) {
            for (const entry of entries) {
              events.push({
                rel: relation.rel,
                sign: 1,
                sequence: entry.sequence,
                row: entry.row,
              });
            }
            continue;
          }
          const insertedKeys = new Set(
            resultRows(result, relation.columns).map((row) => JSON.stringify(row)),
          );
          const stagedKeys = new Set<string>();
          for (const entry of entries) {
            const key = JSON.stringify(entry.row);
            if (!insertedKeys.has(key) || stagedKeys.has(key)) continue;
            stagedKeys.add(key);
            events.push({
              rel: relation.rel,
              sign,
              sequence: entry.sequence,
              row: entry.row,
            });
          }
        }
        return stageEvents(
          seam,
          relations,
          events,
          [{ tableName: (relation) => relation.frontierTableName, phase: 1 }],
        );
      }),
    );
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

  recomputeLevelsAfterEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
    reconcileEveryTick: boolean,
  ): Observable<void> {
    if (statements.length === 0) return of(undefined);
    const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
    const retractionTerms = relations.map(
      (relation) =>
        `EXISTS (SELECT 1 FROM ${quoteIdentifier(relation.deltaTableName)} WHERE "_sign" = -1 LIMIT 1)`,
    );
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
          //
          // ASYMMETRY, named not hidden: the refCount branch just below still
          // stages its reconcile rows into nextFrontier phase 1, the shape P3
          // shipped. No corpus fixture distinguishes the two (every program
          // whose reconcile INSERTS rows also has edge rules), so this arc did
          // not change it; if one ever does, this comment is the pointer.
          return applyAggregateLevelStatement(
            seam,
            statement,
            statement.aggregateSql,
            relation,
            false,
            nextSequence,
          );
        }
        if (statement.supportSql === null) {
          throw new Error(`incremental level statement has neither supportSql nor aggregateSql: ${statement.headRel}`);
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
            return stageEvents(
              seam,
              relations,
              events,
              [{ tableName: (plan) => plan.nextFrontierTableName, phase: 1 }],
            );
          }),
        );
      });
    };
    if (reconcileEveryTick) return reconcile();
    if (retractionTerms.length === 0) return of(undefined);
    const retractionSql =
      `SELECT CASE WHEN ${retractionTerms.join(" OR ")} THEN 1 ELSE 0 END AS has_retraction`;
    return seam.runner.execute(seam.db, retractionSql).pipe(
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

  promoteFrontiers(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<boolean> {
    if (relations.length === 0) return of(false);
    const carryTerms = relations.map(
      (relation) =>
        `EXISTS (SELECT 1 FROM ${quoteIdentifier(relation.nextFrontierTableName)} LIMIT 1)`,
    );
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
