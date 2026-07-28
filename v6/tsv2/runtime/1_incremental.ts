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

function stageStatement(
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

function stageEvents(
  seam: ISqlSeam,
  relations: readonly IIncrementalRelationPlan[],
  events: readonly DeltaEvent[],
): Observable<void> {
  const relationByName = new Map(relations.map((relation) => [relation.rel, relation]));
  const eventsByRel = new Map<string, DeltaEvent[]>();
  for (const event of events) {
    const grouped = eventsByRel.get(event.rel);
    if (grouped === undefined) eventsByRel.set(event.rel, [event]);
    else grouped.push(event);
  }
  const statements = [...eventsByRel].map(([rel, grouped]) => {
    const relation = relationByName.get(rel);
    if (relation === undefined) throw new Error(`incremental delta relation missing: ${rel}`);
    return stageStatement(relation, grouped);
  });
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
  rows: readonly IRow[],
): Observable<void> {
  if (rows.length === 0) return of(undefined);
  const relation: IIncrementalRelationPlan = {
    rel: statement.headRel,
    kind: "log",
    tableName: statement.headTableName,
    deltaTableName: statement.headDeltaTableName,
    columns: statement.headColumns,
    arrivalAddSql: null,
    boundarySql: "",
  };
  const events = rows.map(
    (row, sequence): DeltaEvent => ({ rel: statement.headRel, sign: 1, sequence, row }),
  );
  return seam.runner.execute(seam.db, logWriteStatement(statement, rows)).pipe(
    concatMap(() => stageEvents(seam, [relation], events)),
  );
}

function applyKeyedEdge(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
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
        const relation: IIncrementalRelationPlan = {
          rel: statement.headRel,
          kind: "set",
          tableName: statement.headTableName,
          deltaTableName: statement.headDeltaTableName,
          columns: statement.headColumns,
          arrivalAddSql: null,
          boundarySql: "",
        };
        return seam.runner.execute(seam.db, keyedWriteStatement(statement, changedRows)).pipe(
          concatMap(() => stageEvents(seam, [relation], events)),
        );
      }),
    );
}

function applyEdgeStatement(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
): Observable<void> {
  return seam.runner.execute(seam.db, statement.projectSql).pipe(
    concatMap((result) => {
      const rows = resultRows(result, statement.headColumns);
      return statement.headKind === "log"
        ? applyLogEdge(seam, statement, rows)
        : applyKeyedEdge(seam, statement, rows);
    }),
  );
}

function applyLevelStatement(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
): Observable<void> {
  return seam.runner.execute(seam.db, statement.insertSql).pipe(
    concatMap((result) => {
      const rows = resultRows(result, statement.headColumns);
      if (rows.length === 0) return of(undefined);
      const relation: IIncrementalRelationPlan = {
        rel: statement.headRel,
        kind: "set",
        tableName: "",
        deltaTableName: statement.headDeltaTableName,
        columns: statement.headColumns,
        arrivalAddSql: null,
        boundarySql: "",
      };
      const events = rows.map(
        (row, sequence): DeltaEvent => ({ rel: statement.headRel, sign: 1, sequence, row }),
      );
      return stageEvents(seam, [relation], events);
    }),
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
  clearDeltas(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    if (relations.length === 0) return of(undefined);
    const sql = relations
      .map((relation) => `DELETE FROM ${quoteIdentifier(relation.deltaTableName)}`)
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
        return stageEvents(seam, relations, events);
      }),
    );
  },

  applyEdges(
    seam: ISqlSeam,
    statements: readonly IIncrementalEdgeStatement[],
  ): Observable<void> {
    return sequenceWork(statements, (statement) => applyEdgeStatement(seam, statement));
  },

  applyLevels(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
  ): Observable<void> {
    return sequenceWork(statements, (statement) => applyLevelStatement(seam, statement));
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
};
