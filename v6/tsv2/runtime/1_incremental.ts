import { concatMap, forkJoin, map, of, type Observable } from "rxjs";

import type {
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

function applyLevelStatement(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
  relationByName: ReadonlyMap<string, IIncrementalRelationPlan>,
  afterEdges: boolean,
): Observable<void> {
  const relation = relationByName.get(statement.headRel);
  if (relation === undefined) {
    throw new Error(`incremental level head relation missing: ${statement.headRel}`);
  }
  return seam.runner.execute(seam.db, statement.insertSql).pipe(
    concatMap((result) => {
      const rows = resultRows(result, statement.headColumns);
      if (rows.length === 0) return of(undefined);
      const events = rows.map(
        (row, sequence): DeltaEvent => ({ rel: statement.headRel, sign: 1, sequence, row }),
      );
      const frontierCopies = afterEdges
        ? [
            { tableName: (plan: IIncrementalRelationPlan) => plan.frontierTableName, phase: 2 },
            {
              tableName: (plan: IIncrementalRelationPlan) => plan.nextFrontierTableName,
              phase: 1,
            },
          ]
        : [
            { tableName: (plan: IIncrementalRelationPlan) => plan.frontierTableName, phase: 2 },
          ];
      return stageEvents(seam, [relation], events, frontierCopies);
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
    const arrivalsByRel = new Map<string, { readonly sequence: number; readonly row: IRow }[]>();
    for (const [sequence, arrival] of arrivals.entries()) {
      const grouped = arrivalsByRel.get(arrival.rel);
      const entry = { sequence, row: arrival.row };
      if (grouped === undefined) arrivalsByRel.set(arrival.rel, [entry]);
      else grouped.push(entry);
    }
    const groupedArrivals = [...arrivalsByRel].map(([rel, entries]) => {
      const relation = relationByName.get(rel);
      if (relation === undefined || relation.arrivalAddSql === null) {
        throw new Error(`incremental arrival relation missing: ${rel}`);
      }
      return { relation, entries };
    });
    const statements = groupedArrivals.map(({ relation, entries }): SqlStatement => ({
      sql: relation.arrivalAddSql!,
      args: [JSON.stringify(entries.map((entry) => entry.row))],
    }));
    return seam.runner.batch(seam.db, statements).pipe(
      concatMap((results) => {
        const events: DeltaEvent[] = [];
        for (const [groupIndex, result] of results.entries()) {
          const { relation, entries } = groupedArrivals[groupIndex]!;
          if (relation.kind === "log") {
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
              sign: 1,
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
    return sequenceWork(
      statements,
      (statement) => applyLevelStatement(seam, statement, relationByName, false),
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
    return sequenceWork(
      statements,
      (statement) => applyLevelStatement(seam, statement, relationByName, true),
    );
  },

  recomputeLevelsAfterEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
    const retractionTerms = relations.map(
      (relation) =>
        `EXISTS (SELECT 1 FROM ${quoteIdentifier(relation.deltaTableName)} WHERE "_sign" = -1 LIMIT 1)`,
    );
    if (retractionTerms.length === 0) return of(undefined);
    const retractionSql =
      `SELECT CASE WHEN ${retractionTerms.join(" OR ")} THEN 1 ELSE 0 END AS has_retraction`;
    return seam.runner.execute(seam.db, retractionSql).pipe(
      concatMap((result) =>
        Number(result.rows[0]?.has_retraction ?? 0) === 0
          ? of(undefined)
          : sequenceWork(
              statements,
              (statement) => recomputeLevelStatement(seam, statement, relationByName),
            )
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
