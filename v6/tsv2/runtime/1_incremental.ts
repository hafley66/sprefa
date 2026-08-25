import { concatMap, EMPTY, expand, forkJoin, last, map, of, type Observable } from "rxjs";

import type {
  IAggregateLevelPlan,
  IArrivalBatch,
  IDeltaEvent,
  IDredPlan,
  IFrontierCopy,
  IIncrementalEdgeStatement,
  IIncrementalLevelStatement,
  IIncrementalRelationPlan,
  IIncrementalRetentionStatement,
  IIncrementalRuntime,
  IRelDelta,
  IRow,
  IRowColumnType,
  IRowValue,
  ISqlSeam,
  IStageOrderedFrontiers,
  QueryResult,
  SqlStatement,
} from "./types.ts";
import { RuntimeTrace } from "./trace.ts";
import {
  bind_args,
  has_bytes,
  quote_identifier,
  values_sql,
  write_verbs_for,
} from "./writeVerbs.ts";

type DeltaEvent = IDeltaEvent;

/** Carry a skipped rel can no longer report from `__next_frontier_`: the row
 *  count of the fill its copies read stands in for the EXISTS. */
const skipped_carry = new WeakSet<ISqlSeam>();

/** Intern-on-write (§5.7.1): both statements read the same rows, so the intern
 *  runs with the bind args the row statement was given. Absent at direct. */
export function intern_then_execute(
  seam: ISqlSeam,
  intern_sql: readonly string[] | undefined,
  statement: SqlStatement,
): Observable<QueryResult> {
  if (intern_sql === undefined || intern_sql.length === 0) {
    return seam.runner.execute(seam.db, statement);
  }
  const args = typeof statement === "string" ? [] : (statement.args ?? []);
  const interns = intern_sql.map((sql): SqlStatement => ({ sql, args }));
  return seam.runner.batch(seam.db, interns).pipe(
    concatMap(() => seam.runner.execute(seam.db, statement)),
  );
}

/** Nobody reads this rel's event tables: no rule body (compile time) and no
 *  caller at the boundary (boot). An absent `ruleObservers` never skips. */
function is_unobserved(relation: IIncrementalRelationPlan, seam: ISqlSeam): boolean {
  return (
    (relation.rule_observers ?? ["*"]).length === 0 &&
    seam.unobserved_rels?.has(relation.rel) === true
  );
}

/** The same array by reference when no boot set narrows it, so a caller that
 *  never names an unobserved rel allocates and renders exactly what it did. */
function observed_rels(
  relations: readonly IIncrementalRelationPlan[],
  seam: ISqlSeam,
): readonly IIncrementalRelationPlan[] {
  if (seam.unobserved_rels === undefined || seam.unobserved_rels.size === 0) return relations;
  return relations.filter((relation) => !is_unobserved(relation, seam));
}

function result_rows(result: QueryResult, columns: readonly string[], types: readonly IRowColumnType[] = []): readonly IRow[] {
  return result.rows.map((row) =>
    columns.map((column, index) => {
      const value = row[column];
      if (types[index] === "bytes") {
        if (value instanceof Uint8Array) return value;
        if (value instanceof ArrayBuffer) return new Uint8Array(value);
        throw new Error(`bytes column '${columns[index]}' crossed SQLite with ${JSON.stringify(value)}`);
      }
      return normalize_integer_value(value);
    }),
  );
}

function normalize_integer_value(value: unknown): IRowValue {
  if (typeof value === "bigint") {
    if (value < -9007199254740991n || value > 9007199254740991n) {
      throw new Error("int_out_of_range");
    }
    return Number(value);
  }
  return value as IRowValue;
}

function keyed_arrival_rows_statement(
  relation: IIncrementalRelationPlan,
  entries: readonly { readonly row: IRow }[],
  key_indices: readonly number[],
): SqlStatement {
  const columns = relation.columns.map(quote_identifier);
  const key_columns = key_indices.map((index) => columns[index]!);
  const distinct_keys = new Map<string, IRow>();
  for (const entry of entries) {
    const key_values = key_indices.map((index) => entry.row[index]!) as IRow;
    distinct_keys.set(JSON.stringify(key_values), key_values);
  }
  const keys = [...distinct_keys.values()];
  return {
    sql: `SELECT ${columns.join(", ")} FROM ${quote_identifier(relation.table_name)} WHERE (${key_columns.join(", ")}) IN (${values_sql(keys.length, key_columns.length)})`,
    args: keys.flatMap(bind_args),
  };
}

function stages_next_frontier(
  relation: IIncrementalRelationPlan,
  frontier_copies: readonly IFrontierCopy[],
): boolean {
  return frontier_copies.some(
    (copy) => copy.table_name(relation) === relation.next_frontier_table_name,
  );
}

function stage_events(
  seam: ISqlSeam,
  relations: readonly IIncrementalRelationPlan[],
  events: readonly DeltaEvent[],
  frontier_copies: readonly IFrontierCopy[],
): Observable<void> {
  const relation_by_name = new Map(relations.map((relation) => [relation.rel, relation]));
  const events_by_rel = new Map<string, DeltaEvent[]>();
  for (const event of events) {
    const grouped = events_by_rel.get(event.rel);
    if (grouped === undefined) events_by_rel.set(event.rel, [event]);
    else grouped.push(event);
  }
  const verbs = write_verbs_for(relations);
  const statements = [...events_by_rel].flatMap(([rel, grouped]) => {
    const relation = relation_by_name.get(rel);
    if (relation === undefined) throw new Error(`incremental delta relation missing: ${rel}`);
    if (is_unobserved(relation, seam)) {
      const additions = grouped.filter((event) => event.sign === 1);
      if (additions.length > 0 && stages_next_frontier(relation, frontier_copies)) {
        skipped_carry.add(seam);
      }
      return [];
    }
    return verbs.stage(relation, grouped, frontier_copies);
  });
  if (statements.length === 0) return of(undefined);
  return seam.runner.batch(seam.db, statements).pipe(map(() => undefined));
}

/**
 * Replace the carry-in frontiers used by emitted ordered-occurrence programs
 * with this tick's boundary-visible additions. Intermediate keyed fold rows
 * never reach this function: the emitter supplies only its start/end diff.
 */
export const stage_ordered_frontiers: IStageOrderedFrontiers = (
  seam: ISqlSeam,
  relations: readonly IIncrementalRelationPlan[],
  additions: readonly IRelDelta[],
): Observable<boolean> => {
  const events_by_rel = new Map<string, DeltaEvent[]>();
  let sequence = 0;
  for (const delta of additions) {
    for (const row of delta.add) {
      const event: DeltaEvent = { rel: delta.rel, sign: 1, sequence, row };
      sequence += 1;
      const grouped = events_by_rel.get(delta.rel);
      if (grouped === undefined) events_by_rel.set(delta.rel, [event]);
      else grouped.push(event);
    }
  }
  const statements: SqlStatement[] = [];
  const verbs = write_verbs_for(relations);
  let carry_pending = false;
  for (const relation of relations) {
    statements.push(
      { sql: `DELETE FROM ${quote_identifier(relation.frontier_table_name)}`, args: [] },
      { sql: `DELETE FROM ${quote_identifier(relation.next_frontier_table_name)}`, args: [] },
    );
    const events = events_by_rel.get(relation.rel) ?? [];
    if (events.length === 0) continue;
    carry_pending = true;
    // `stage` returns the boundary delta write first and the frontier copies
    // after it; this path writes no delta rows, so it takes the copy alone.
    // The two DELETEs above name the rel's own tables, which an
    // ordered-occurrence program always has: frontier(shared) refuses one
    // (lower.pl:shared_frontier_todo, tick).
    statements.push(
      ...verbs.stage(relation, events, [
        { table_name: (plan) => plan.frontier_table_name, phase: 0 },
      ]).slice(1),
    );
  }
  if (statements.length === 0) return of(carry_pending);
  return seam.runner.batch(seam.db, statements).pipe(map(() => carry_pending));
};

function keyed_rows_sql(
  statement: IIncrementalEdgeStatement,
  row_count: number,
): string {
  if (statement.head_columns.length === 0) {
    return `SELECT "__unit" FROM ${quote_identifier(statement.head_table_name)} WHERE "__unit" = 1`;
  }
  const columns = statement.head_columns.map(quote_identifier);
  const key_columns = statement.key_indices.map((index) => columns[index]!);
  return `SELECT ${columns.join(", ")} FROM ${quote_identifier(statement.head_table_name)} WHERE (${key_columns.join(", ")}) IN (${values_sql(row_count, key_columns.length)})`;
}

function row_key(row: IRow, indices: readonly number[]): string {
  return JSON.stringify(indices.map((index) => row[index]));
}

function rows_equal(left: IRow, right: IRow): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function storage_row(relation: IIncrementalRelationPlan, row: IRow): IRow {
  return row.map((value, index) =>
    relation.column_types?.[index] === "bool" && typeof value === "boolean"
      ? (value ? 1 : 0)
      : value,
  );
}

function keyed_write_statement(
  statement: IIncrementalEdgeStatement,
  rows: readonly IRow[],
): SqlStatement {
  if (statement.head_columns.length === 0) {
    return {
      sql: `INSERT OR IGNORE INTO ${quote_identifier(statement.head_table_name)} ("__unit") VALUES (1)`,
      args: [],
    };
  }
  const columns = statement.head_columns.map(quote_identifier);
  const key_columns = statement.key_indices.map((index) => columns[index]!);
  const key_index_set = new Set(statement.key_indices);
  const non_key_columns = columns.filter((_column, index) => !key_index_set.has(index));
  const conflict = non_key_columns.length === 0
    ? `ON CONFLICT(${key_columns.join(", ")}) DO NOTHING`
    : `ON CONFLICT(${key_columns.join(", ")}) DO UPDATE SET ${non_key_columns
        .map((column) => `${column} = excluded.${column}`)
        .join(", ")}`;
  return {
    sql: `INSERT INTO ${quote_identifier(statement.head_table_name)} (${columns.join(", ")}) VALUES ${values_sql(rows.length, columns.length)} ${conflict}`,
    args: rows.flatMap(bind_args),
  };
}

function log_write_statement(
  statement: IIncrementalEdgeStatement,
  rows: readonly IRow[],
): SqlStatement {
  if (statement.head_columns.length === 0) {
    return {
      sql: `INSERT INTO ${quote_identifier(statement.head_table_name)} ("__unit") VALUES ${rows.map(() => "(1)").join(", ")}`,
      args: [],
    };
  }
  const columns = statement.head_columns.map(quote_identifier);
  return {
    sql: `INSERT INTO ${quote_identifier(statement.head_table_name)} (${columns.join(", ")}) VALUES ${values_sql(rows.length, columns.length)}`,
    args: rows.flatMap(bind_args),
  };
}

function apply_log_edge(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
  relation: IIncrementalRelationPlan,
  rows: readonly IRow[],
): Observable<void> {
  if (rows.length === 0) return of(undefined);
  const events = rows.map(
    (row, sequence): DeltaEvent => ({ rel: statement.head_rel, sign: 1, sequence, row }),
  );
  return seam.runner.execute(seam.db, log_write_statement(statement, rows)).pipe(
    concatMap(() =>
      stage_events(seam, [relation], events, [
        { table_name: (plan) => plan.next_frontier_table_name, phase: 0 },
      ])
    ),
  );
}

function apply_keyed_edge(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
  relation: IIncrementalRelationPlan,
  projected_rows: readonly IRow[],
): Observable<void> {
  const resolved = new Map<string, IRow>();
  for (const row of projected_rows) resolved.set(row_key(row, statement.key_indices), row);
  const rows = [...resolved.values()];
  if (rows.length === 0) return of(undefined);
  const key_args = rows.flatMap((row) =>
    bind_args(statement.key_indices.map((index) => row[index]!) as IRow),
  );
  return seam.runner
    .execute(seam.db, { sql: keyed_rows_sql(statement, rows.length), args: key_args })
    .pipe(
      concatMap((before_result) => {
        const before_rows = result_rows(before_result, statement.head_columns, relation.column_types);
        const before_by_key = new Map(
          before_rows.map((row) => [row_key(row, statement.key_indices), row]),
        );
        const changed_rows = rows.filter((row) => {
          const before = before_by_key.get(row_key(row, statement.key_indices));
          return before === undefined || !rows_equal(before, row);
        });
        if (changed_rows.length === 0) return of(undefined);
        const events: DeltaEvent[] = [];
        for (const [sequence, row] of changed_rows.entries()) {
          const before = before_by_key.get(row_key(row, statement.key_indices));
          if (before !== undefined) {
            events.push({ rel: statement.head_rel, sign: -1, sequence: sequence * 2, row: before });
          }
          events.push({ rel: statement.head_rel, sign: 1, sequence: sequence * 2 + 1, row });
        }
        return seam.runner.execute(seam.db, keyed_write_statement(statement, changed_rows)).pipe(
          concatMap(() =>
            stage_events(seam, [relation], events, [
              { table_name: (plan) => plan.next_frontier_table_name, phase: 0 },
            ])
          ),
        );
      }),
    );
}

function apply_edge_statement(
  seam: ISqlSeam,
  statement: IIncrementalEdgeStatement,
  relation_by_name: ReadonlyMap<string, IIncrementalRelationPlan>,
): Observable<void> {
  const relation = relation_by_name.get(statement.head_rel);
  if (relation === undefined) {
    throw new Error(`incremental edge head relation missing: ${statement.head_rel}`);
  }
  const started_at = RuntimeTrace.enabled ? performance.now() : 0;
  return intern_then_execute(seam, statement.intern_sql, statement.project_sql).pipe(
    concatMap((result) => {
      const rows = result_rows(result, statement.head_columns, relation.column_types);
      RuntimeTrace.rule(statement.rule_id, rows.length, performance.now() - started_at);
      return statement.head_kind === "log"
        ? apply_log_edge(seam, statement, relation, rows)
        : apply_keyed_edge(seam, statement, relation, rows);
    }),
  );
}

function level_frontier_copies(
  after_edges: boolean,
): readonly { table_name: (plan: IIncrementalRelationPlan) => string; phase: number }[] {
  return after_edges
    ? [
        { table_name: (plan: IIncrementalRelationPlan) => plan.frontier_table_name, phase: 2 },
        { table_name: (plan: IIncrementalRelationPlan) => plan.next_frontier_table_name, phase: 1 },
      ]
    : [{ table_name: (plan: IIncrementalRelationPlan) => plan.frontier_table_name, phase: 2 }];
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
function apply_aggregate_level_statement(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
  aggregate: IAggregateLevelPlan,
  relation: IIncrementalRelationPlan,
  after_edges: boolean,
  next_sequence: () => number,
): Observable<void> {
  // The intern arm reads the scope table, so it follows the seed inside the
  // same ordered batch and precedes the insert that looks its ids back up.
  const scope_statements: SqlStatement[] = [
    { sql: aggregate.scope_clear_sql, args: [] },
    ...aggregate.scope_seed_sql.map((sql): SqlStatement => ({ sql, args: [] })),
    ...(aggregate.intern_sql ?? []).map((sql): SqlStatement => ({ sql, args: [] })),
  ];
  return seam.runner.batch(seam.db, scope_statements).pipe(
    concatMap(() => seam.runner.execute(seam.db, aggregate.delete_scoped_sql)),
    concatMap((delete_result) => {
      const removed_rows = result_rows(delete_result, statement.head_columns, relation.column_types);
      return seam.runner
        .batch(
          seam.db,
          aggregate.insert_scoped_sql.map((sql): SqlStatement => ({ sql, args: [] })),
        )
        .pipe(
          concatMap((insert_results) => {
            const events: DeltaEvent[] = removed_rows.map((row) => ({
              rel: statement.head_rel,
              sign: -1 as const,
              sequence: next_sequence(),
              row,
            }));
            for (const insert_result of insert_results) {
              for (const row of result_rows(insert_result, statement.head_columns, relation.column_types)) {
                events.push({
                  rel: statement.head_rel,
                  sign: 1,
                  sequence: next_sequence(),
                  row,
                });
              }
            }
            if (events.length === 0) return of(undefined);
            return stage_events(seam, [relation], events, level_frontier_copies(after_edges));
          }),
        );
    }),
  );
}

function apply_level_statement(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
  relation_by_name: ReadonlyMap<string, IIncrementalRelationPlan>,
  after_edges: boolean,
  next_sequence: () => number,
): Observable<number> {
  const relation = relation_by_name.get(statement.head_rel);
  if (relation === undefined) {
    throw new Error(`incremental level head relation missing: ${statement.head_rel}`);
  }
  if (statement.aggregate_sql !== null) {
    return apply_aggregate_level_statement(
      seam,
      statement,
      statement.aggregate_sql,
      relation,
      after_edges,
      next_sequence,
    ).pipe(map(() => 0));
  }
  if (statement.insert_sql === null) {
    throw new Error(`incremental level statement has neither insert_sql nor aggregate_sql: ${statement.head_rel}`);
  }
  const started_at = RuntimeTrace.enabled ? performance.now() : 0;
  return intern_then_execute(seam, statement.intern_sql, statement.insert_sql).pipe(
    concatMap((result) => {
      const rows = result_rows(result, statement.head_columns, relation.column_types);
      RuntimeTrace.rule(statement.rule_id, rows.length, performance.now() - started_at);
      if (rows.length === 0) return of(0);
      const events = rows.map(
        (row, sequence): DeltaEvent => ({ rel: statement.head_rel, sign: 1, sequence, row }),
      );
      return stage_events(seam, [relation], events, level_frontier_copies(after_edges)).pipe(
        map(() => rows.length),
      );
    }),
  );
}

/** Heads that sit on a cycle of the level graph: deriving one can feed a rule
 *  that derives it again. A DAG of levels closes in one dependency-ordered pass. */
function recursive_heads(
  statements: readonly IIncrementalLevelStatement[],
  relations: readonly IIncrementalRelationPlan[],
): ReadonlySet<string> {
  const reads_frontier_of = new Map<string, readonly string[]>();
  for (const statement of statements) {
    const sources = relations
      .filter((relation) => statement.insert_sql?.includes(quote_identifier(relation.frontier_table_name)) === true)
      .map((relation) => relation.rel);
    reads_frontier_of.set(statement.head_rel, [...(reads_frontier_of.get(statement.head_rel) ?? []), ...sources]);
  }
  const reaches = (from: string, target: string, seen: Set<string>): boolean => {
    if (seen.has(from)) return false;
    seen.add(from);
    for (const source of reads_frontier_of.get(from) ?? []) {
      if (source === target) return true;
      if (reaches(source, target, seen)) return true;
    }
    return false;
  };
  const heads = new Set<string>();
  for (const head_rel of reads_frontier_of.keys()) {
    if (reaches(head_rel, head_rel, new Set())) heads.add(head_rel);
  }
  return heads;
}

function sequence_work<Item>(
  items: readonly Item[],
  run: (item: Item) => Observable<void>,
): Observable<void> {
  return items.reduce(
    (work, item) => work.pipe(concatMap(() => run(item))),
    of(undefined) as Observable<void>,
  );
}

/** Maximal runs of consecutive statements sharing one `recursion_group.group`.
 *  A statement off any cycle is its own run and pays exactly one pass. */
function level_statement_runs(
  statements: readonly IIncrementalLevelStatement[],
): readonly (readonly IIncrementalLevelStatement[])[] {
  const runs: IIncrementalLevelStatement[][] = [];
  let current_group: number | null = null;
  for (const statement of statements) {
    const group = statement.recursion_group?.group ?? null;
    if (group !== null && group === current_group) {
      runs[runs.length - 1]!.push(statement);
      continue;
    }
    runs.push([statement]);
    current_group = group;
  }
  return runs;
}

/** OUTER ROUNDS: a mutual cycle has no statement order that reaches the least
 *  fixpoint in one pass, so the group's pass repeats until no row moves. */
function sequence_level_rounds(
  statements: readonly IIncrementalLevelStatement[],
  run: (statement: IIncrementalLevelStatement) => Observable<number>,
): Observable<void> {
  return sequence_work(level_statement_runs(statements), (group_statements) => {
    const one_pass = (): Observable<number> =>
      group_statements.reduce(
        (work, statement) =>
          work.pipe(concatMap((moved) => run(statement).pipe(map((rows) => moved + rows)))),
        of(0) as Observable<number>,
      );
    const plan = group_statements[0]!.recursion_group ?? null;
    if (plan === null) return one_pass().pipe(map(() => undefined));
    return one_pass().pipe(
      expand((moved, index) => {
        if (moved === 0) return EMPTY;
        if (index + 1 >= plan.round_cap) {
          throw new Error(`diverging_measure_recursion(${plan.heads}, ${plan.round_cap})`);
        }
        return one_pass();
      }),
      last(),
      map(() => undefined),
    );
  });
}

/** The wavefront driver both recursive walks share. A growing measure never
 *  derives nothing, so the round index is the only thing that can stop it. */
function bounded_wave(
  head_rel: string,
  round_cap: number,
  round: (fills_b: boolean) => Observable<number>,
): Observable<number> {
  return round(true).pipe(
    expand((derived, index) => {
      if (derived === 0) return EMPTY;
      if (index + 1 >= round_cap) {
        throw new Error(`diverging_measure_recursion(${head_rel}, ${round_cap})`);
      }
      return round(index % 2 === 1);
    }),
    last(),
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
function reconcile_ref_count_statement(
  seam: ISqlSeam,
  statement: IIncrementalLevelStatement,
  relations: readonly IIncrementalRelationPlan[],
  frontier_copies: readonly {
    readonly table_name: (relation: IIncrementalRelationPlan) => string;
    readonly phase: number;
  }[],
): Observable<number> {
  if (statement.support_sql === null) {
    throw new Error(
      `incremental level statement has neither support_sql nor aggregate_sql: ${statement.head_rel}`,
    );
  }
  const relation = relations.find((candidate) => candidate.rel === statement.head_rel);
  if (relation === undefined) {
    throw new Error(`incremental level head relation missing: ${statement.head_rel}`);
  }
  const [
    clear, seed, update, stage_retract, collect_zero,
    clear_new, fill_new, stage_add, stage_frontier, stage_next_frontier, insert_new,
  ] = statement.support_sql;
  const skipped = is_unobserved(relation, seam);
  // Each copy names the table it wants and the emitted statement carries its
  // phase as the one bind, so the two copies share one prepared shape.
  const frontier_stages = skipped
    ? []
    : frontier_copies.map((copy): SqlStatement => ({
        sql: copy.table_name(relation) === relation.next_frontier_table_name ? stage_next_frontier! : stage_frontier!,
        args: [copy.phase],
      }));
  // `__new_` is the table the dropped copies read, so its row count is the
  // carry the dropped `__next_frontier_` rows would have signalled.
  const carries_next = skipped && stages_next_frontier(relation, frontier_copies);
  // The resolved value: what `sequenceLevelRounds` reads to know the group
  // stopped growing.
  let moved = 0;
  const note_fill = (rows: number): void => {
    moved += rows;
    if (carries_next && rows > 0) skipped_carry.add(seam);
  };
  const to_statements = (texts: readonly string[]): SqlStatement[] =>
    texts.map((text): SqlStatement => ({ sql: text, args: [] }));
  const tail_texts = skipped
    ? [update!, collect_zero!, clear_new!, fill_new!]
    : [update!, stage_retract!, collect_zero!, clear_new!, fill_new!, stage_add!];
  const fill_new_index = tail_texts.length - (skipped ? 1 : 2);
  // A round that only RETRACTS still has to run again, or the peers of a
  // mutual cycle keep the rows this one just took the support out from under.
  const collect_zero_index = skipped ? 1 : 2;
  const tail: SqlStatement[] = [
    ...to_statements(tail_texts),
    ...frontier_stages,
    { sql: insert_new!, args: [] },
    ...to_statements(write_verbs_for(relations).recount(statement)),
  ];
  const expand_plan = statement.expand_sql ?? null;
  const support_interns = statement.support_intern_sql ?? [];
  const recompute = (): Observable<void> => {
    if (expand_plan === null) {
      const offset = 2 + support_interns.length;
      return seam.runner
        .batch(seam.db, [...to_statements([clear!, ...support_interns, seed!]), ...tail])
        .pipe(map((results) => {
          note_fill(results[fill_new_index + offset]!.rowsAffected);
          moved += results[collect_zero_index + offset]!.rowsAffected;
        }));
    }
    // rx expand over the wavefront pair: hop fills the idle wave from the busy
    // one, absorb folds it into the refCount table, roles swap until a hop is 0.
    const round = (fills_b: boolean): Observable<number> =>
      seam.runner
        .batch(
          seam.db,
          to_statements(
            fills_b
              ? [expand_plan.clear_b_sql, expand_plan.hop_ab_sql, expand_plan.absorb_b_sql]
              : [expand_plan.clear_a_sql, expand_plan.hop_ba_sql, expand_plan.absorb_a_sql],
          ),
        )
        .pipe(map((results) => results[1]!.rowsAffected));
    const seed_wave = to_statements([
      clear!,
      expand_plan.clear_a_sql,
      expand_plan.clear_b_sql,
      ...expand_plan.seed_sqls,
      expand_plan.absorb_a_sql,
    ]);
    return seam.runner.batch(seam.db, seed_wave).pipe(
      concatMap(() => bounded_wave(statement.head_rel, expand_plan.round_cap, round)),
      concatMap(() => seam.runner.batch(seam.db, tail)),
      map((results) => {
        note_fill(results[fill_new_index]!.rowsAffected);
        moved += results[collect_zero_index]!.rowsAffected;
      }),
    );
  };
  const dred_plan = statement.dred_sql ?? null;
  if (dred_plan === null) return recompute().pipe(map(() => moved));
  // lower.pl:4450 mints both plans on the one recursive branch, so a dred plan
  // without the expand plan's cap is an emitter defect, never a runtime shape.
  if (expand_plan === null) {
    throw new Error(`incremental level dred plan without expand plan: ${statement.head_rel}`);
  }
  const arrival_tail: SqlStatement[] = skipped
    ? []
    : [...to_statements([stage_add!]), ...frontier_stages];
  return maintain_head_in_place(
    seam,
    dred_plan,
    relations,
    to_statements,
    {
      skipped,
      clear_new: clear_new!,
      arrival_tail,
      stage_retract: skipped ? [] : to_statements([dred_plan.stage_retract_sql]),
      note_fill,
      head_rel: statement.head_rel,
      round_cap: expand_plan.round_cap,
    },
    recompute,
  ).pipe(map(() => moved));
}

/**
 * IDredPlan's driver. The tick kind picks the half: `retractionGuardSql` is
 * the SAME gate the two recompute passes already read, so a purely additive
 * tick never touches the DRed statements at all.
 */
function maintain_head_in_place(
  seam: ISqlSeam,
  plan: IDredPlan,
  relations: readonly IIncrementalRelationPlan[],
  to_statements: (texts: readonly string[]) => SqlStatement[],
  staging: {
    readonly skipped: boolean;
    readonly clear_new: string;
    readonly arrival_tail: readonly SqlStatement[];
    readonly stage_retract: readonly SqlStatement[];
    readonly note_fill: (rows: number) => void;
    readonly head_rel: string;
    readonly round_cap: number;
  },
  recompute: () => Observable<void>,
): Observable<void> {
  const walk = (
    round: (fills_b: boolean) => Observable<number>,
  ): Observable<number> => bounded_wave(staging.head_rel, staging.round_cap, round);
  // A skipped rel's `arrivalA/B` fills `__new_<rel>` only to feed the carry via
  // noteFill; with nothing reading it, commit's rowsAffected carries the same.
  const assert_round = (fills_b: boolean): Observable<number> =>
    seam.runner
      .batch(
        seam.db,
        to_statements(
          fills_b
            ? staging.skipped
              ? [plan.clear_pong_sql, plan.assert_hop_ab_sql, plan.commit_b_sql]
              : [plan.clear_pong_sql, plan.assert_hop_ab_sql, plan.commit_b_sql, plan.arrival_b_sql]
            : staging.skipped
              ? [plan.clear_ping_sql, plan.assert_hop_ba_sql, plan.commit_a_sql]
              : [plan.clear_ping_sql, plan.assert_hop_ba_sql, plan.commit_a_sql, plan.arrival_a_sql],
        ),
      )
      .pipe(
        map((results) => {
          staging.note_fill(results[staging.skipped ? 2 : 3]!.rowsAffected);
          return results[1]!.rowsAffected;
        }),
      );
  const assert_half = (): Observable<void> =>
    seam.runner
      .batch(
        seam.db,
        to_statements(
          staging.skipped
            ? [
                staging.clear_new,
                plan.clear_ping_sql,
                plan.clear_pong_sql,
                ...plan.assert_seed_sqls,
                plan.commit_a_sql,
              ]
            : [
                staging.clear_new,
                plan.clear_ping_sql,
                plan.clear_pong_sql,
                ...plan.assert_seed_sqls,
                plan.commit_a_sql,
                plan.arrival_a_sql,
              ],
        ),
      )
      .pipe(
        concatMap((results) => {
          staging.note_fill(results[results.length - 1]!.rowsAffected);
          return walk(assert_round);
        }),
        concatMap(() =>
          staging.arrival_tail.length === 0
            ? of(undefined)
            : seam.runner.batch(seam.db, staging.arrival_tail)
        ),
        map(() => undefined),
      );
  // Resolves true when the cone outgrew a quarter of the head. The walk writes
  // only TEMP tables until `headDeleteSql`, so bailing costs nothing but them.
  const dred_half = (): Observable<boolean> =>
    seam.runner.scalar(seam.db, plan.head_count_sql).pipe(
      concatMap((head_count) => {
        const cone_cap = Math.floor(head_count / 4);
        let cone_count = 0;
        let bailed = false;
        const over_delete_round = (fills_b: boolean): Observable<number> => {
          if (cone_count > cone_cap) {
            bailed = true;
            return of(0);
          }
          return seam.runner
            .batch(
              seam.db,
              to_statements(
                fills_b
                  ? [plan.clear_pong_sql, plan.dred_hop_ab_sql, plan.cone_absorb_b_sql]
                  : [plan.clear_ping_sql, plan.dred_hop_ba_sql, plan.cone_absorb_a_sql],
              ),
            )
            .pipe(
              map((results) => {
                cone_count += results[2]!.rowsAffected;
                return results[1]!.rowsAffected;
              }),
            );
        };
        const revive_round = (fills_b: boolean): Observable<number> =>
          seam.runner
            .batch(
              seam.db,
              to_statements(
                fills_b
                  ? [plan.clear_pong_sql, plan.revive_hop_ab_sql, plan.commit_b_sql, plan.cone_drop_b_sql]
                  : [plan.clear_ping_sql, plan.revive_hop_ba_sql, plan.commit_a_sql, plan.cone_drop_a_sql],
              ),
            )
            .pipe(map((results) => results[1]!.rowsAffected));
        return seam.runner
          .batch(
            seam.db,
            to_statements([
              plan.clear_ping_sql,
              plan.clear_pong_sql,
              plan.clear_cone_sql,
              ...plan.dred_seed_sqls,
              plan.cone_absorb_a_sql,
            ]),
          )
          .pipe(
            map((results) => {
              cone_count = results[results.length - 1]!.rowsAffected;
            }),
            concatMap(() => walk(over_delete_round)),
            concatMap(() => {
              if (bailed) return of(true);
              return seam.runner
                .batch(
                  seam.db,
                  to_statements([
                    plan.cone_trim_sql,
                    plan.head_delete_sql,
                    plan.clear_ping_sql,
                    plan.clear_pong_sql,
                    ...plan.rederive_seed_sqls,
                    plan.commit_a_sql,
                    plan.cone_drop_a_sql,
                  ]),
                )
                .pipe(
                  concatMap(() => walk(revive_round)),
                  concatMap(() =>
                    staging.stage_retract.length === 0
                      ? of(undefined)
                      : seam.runner.batch(seam.db, staging.stage_retract)
                  ),
                  map(() => false),
                );
            }),
          );
      }),
    );
  return seam.runner.execute(seam.db, retraction_guard_sql(relations, seam)).pipe(
    concatMap((result) =>
      Number(result.rows[0]?.has_retraction ?? 0) === 0
        ? assert_half()
        : dred_half().pipe(
            concatMap((bailed) => (bailed ? recompute() : assert_half())),
          ),
    ),
  );
}

/** `1` when some delta table already holds a retraction this tick. The gate
 *  both reconcile passes use when the program has no negative level body
 *  (`reconcileEveryTick === false`): with only monotone level bodies, a level
 *  row can stop being derivable only if one of its inputs left. */
function retraction_guard_sql(
  relations: readonly IIncrementalRelationPlan[],
  seam: ISqlSeam,
): string {
  // A rel no rule body reads is nobody's input, so its departures cannot make
  // another rel's row stop being derivable.
  const terms = observed_rels(relations, seam).map(
    (relation) =>
      `EXISTS (SELECT 1 FROM ${quote_identifier(relation.delta_table_name)} WHERE "_sign" = -1 LIMIT 1)`,
  );
  if (terms.length === 0) return `SELECT 0 AS has_retraction`;
  return `SELECT CASE WHEN ${terms.join(" OR ")} THEN 1 ELSE 0 END AS has_retraction`;
}

function apply_retention_statement(
  seam: ISqlSeam,
  statement: IIncrementalRetentionStatement,
  relation_by_name: ReadonlyMap<string, IIncrementalRelationPlan>,
  next_sequence: () => number,
): Observable<void> {
  const relation = relation_by_name.get(statement.rel);
  if (relation === undefined) {
    throw new Error(`incremental retention relation missing: ${statement.rel}`);
  }
  return seam.runner.execute(seam.db, statement.delete_sql).pipe(
    concatMap((result) => {
      const rows = result_rows(result, relation.columns, relation.column_types);
      if (rows.length === 0) return of(undefined);
      const events = rows.map(
        (row): DeltaEvent => ({
          rel: statement.rel,
          sign: -1,
          sequence: next_sequence(),
          row,
        }),
      );
      return stage_events(seam, [relation], events, []);
    }),
  );
}

function boundary_delta(
  relation: IIncrementalRelationPlan,
  result: QueryResult,
): IRelDelta {
  const weights = new Map<string, { row: IRow; weight: number }>();
  for (const result_row of result.rows) {
    const row = relation.columns.map((column, index) => {
      const value = result_row[column];
      const type = relation.column_types?.[index];
      // F3: the delta boundary hydrates a list column the same way the SELECT
      // boundary does (rows.ts), so both reads hand the consumer Array<T>.
      if (type === "list") {
        if (typeof value !== "string") {
          throw new Error(`list column '${relation.rel}.${column}' crossed SQLite with ${JSON.stringify(value)}`);
        }
        const parsed: unknown = JSON.parse(value);
        if (!Array.isArray(parsed)) {
          throw new Error(`list column '${relation.rel}.${column}' crossed SQLite with non-array text ${value}`);
        }
        return parsed as IRowValue;
      }
      if (type === "bool") {
        if (value === 0 || value === 0n) return false;
        if (value === 1 || value === 1n) return true;
        throw new Error(`bool column '${relation.rel}.${column}' crossed SQLite with ${JSON.stringify(value)}`);
      }
      if (type === "float") {
        if (typeof value !== "number" || !Number.isFinite(value)) {
          throw new Error(`float column '${relation.rel}.${column}' crossed SQLite with ${JSON.stringify(value)}`);
        }
        return Object.is(value, -0) ? 0 : value;
      }
      if (type === "bytes") {
        if (value instanceof Uint8Array) return value;
        if (value instanceof ArrayBuffer) return new Uint8Array(value);
        throw new Error(`bytes column '${relation.rel}.${column}' crossed SQLite with ${JSON.stringify(value)}`);
      }
      return normalize_integer_value(value);
    });
    const key = JSON.stringify(row);
    const weight = Number(result_row.__sign) * Number(result_row.__count);
    const previous = weights.get(key);
    weights.set(key, { row, weight: (previous?.weight ?? 0) + weight });
  }
  const add: IRow[] = [];
  const del: IRow[] = [];
  for (const { row, weight } of weights.values()) {
    for (let count = 0; count < weight; count += 1) add.push(row);
    // Negative weight reports on BOTH planes. A `relation.kind === "set"`
    // guard used to sit here and suppressed a log rel's negative weight, which
    // is what made retention invisible on this door. A log rel's delta table
    // only ever carries -1 from applyRetentionStatement (appends are +1), so
    // reporting it reports exactly the prune: the emitter twin of engine.pl's
    // LogRemovals in boundary_deltas/6.
    for (let count = 0; count > weight; count -= 1) del.push(row);
  }
  return { rel: relation.rel, add, del };
}

export const IncrementalRuntime: IIncrementalRuntime = {
  prepare_tick(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    skipped_carry.delete(seam);
    if (relations.length === 0) return of(undefined);
    const sql = write_verbs_for(relations)
      .clear(observed_rels(relations, seam), "prepare")
      .join(";\n");
    if (sql === "") return of(undefined);
    return seam.runner.executeMultiple(seam.db, sql);
  },

  apply_arrivals(
    seam: ISqlSeam,
    arrivals: IArrivalBatch,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    if (arrivals.length === 0) return of(undefined);
    const relation_by_name = new Map(relations.map((relation) => [relation.rel, relation]));
    type ArrivalEntry = { readonly sequence: number; readonly row: IRow };
    type ArrivalGroup = {
      readonly relation: IIncrementalRelationPlan;
      readonly sign: 1 | -1;
      readonly entries: ArrivalEntry[];
    };
    const grouped_arrivals: ArrivalGroup[] = [];
    for (const [sequence, arrival] of arrivals.entries()) {
      const relation = relation_by_name.get(arrival.rel);
      if (relation === undefined) {
        throw new Error(`incremental arrival relation missing: ${arrival.rel}`);
      }
      const sign = arrival.sign === "add" ? 1 : -1;
      const sql = sign === 1 ? relation.arrival_add_sql : relation.arrival_del_sql;
      if (sql === null) {
        throw new Error(
          sign === -1 && relation.kind === "log"
            ? `retract from log rel '${arrival.rel}'`
            : `incremental ${sign === 1 ? "add" : "delete"} statement missing: ${arrival.rel}`,
        );
      }
      const previous = grouped_arrivals.at(-1);
      const entry = { sequence, row: storage_row(relation, arrival.row) };
      if (
        previous !== undefined &&
        previous.relation.rel === relation.rel &&
        previous.sign === sign
      ) {
        previous.entries.push(entry);
      } else {
        grouped_arrivals.push({ relation, sign, entries: [entry] });
      }
    }
    const verbs = write_verbs_for(relations);
    return sequence_work(grouped_arrivals, ({ relation, sign, entries }) => {
      const write_statement = verbs.arrive(
        relation,
        sign,
        entries.map((entry) => entry.row),
      );
      const key_indices = relation.key_indices ?? [];
      if (relation.kind === "set" && sign === 1 && key_indices.length > 0) {
        return seam.runner
          .execute(seam.db, keyed_arrival_rows_statement(relation, entries, key_indices))
          .pipe(
            concatMap((before_result) => {
              const current_by_key = new Map(
                result_rows(before_result, relation.columns, relation.column_types).map((row) => [
                  row_key(row, key_indices),
                  row,
                ]),
              );
              const events: DeltaEvent[] = [];
              for (const entry of entries) {
                const key = row_key(entry.row, key_indices);
                const before = current_by_key.get(key);
                if (before !== undefined && rows_equal(before, entry.row)) continue;
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
                current_by_key.set(key, entry.row);
              }
              return seam.runner.execute(seam.db, write_statement).pipe(
                concatMap(() =>
                  stage_events(
                    seam,
                    relations,
                    events,
                    [{ table_name: (plan) => plan.frontier_table_name, phase: 1 }],
                  )
                ),
              );
            }),
          );
      }
      return seam.runner.execute(seam.db, write_statement).pipe(
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
            return stage_events(
              seam,
              relations,
              events,
              [{ table_name: (plan) => plan.frontier_table_name, phase: 1 }],
            );
          }
          const changed_rows = result_rows(result, relation.columns, relation.column_types);
          const staged_rows = new Set<string>();
          for (const [index, entry] of entries.entries()) {
            const stored_row = changed_rows[index];
            if (stored_row === undefined) continue;
            const row = JSON.stringify(stored_row);
            if (staged_rows.has(row)) continue;
            staged_rows.add(row);
            events.push({
              rel: relation.rel,
              sign,
              sequence: entry.sequence,
              row: stored_row,
            });
          }
          return stage_events(
            seam,
            relations,
            events,
            [{ table_name: (plan) => plan.frontier_table_name, phase: 1 }],
          );
        }),
      );
    });
  },

  apply_edges(
    seam: ISqlSeam,
    statements: readonly IIncrementalEdgeStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    const relation_by_name = new Map(relations.map((relation) => [relation.rel, relation]));
    return sequence_work(
      statements,
      (statement) => apply_edge_statement(seam, statement, relation_by_name),
    );
  },

  apply_levels_before_edges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    const relation_by_name = new Map(relations.map((relation) => [relation.rel, relation]));
    let sequence = 0;
    const next_sequence = (): number => {
      const current = sequence;
      sequence += 1;
      return current;
    };
    // The frontier admits every round it ever staged, so looping the delta
    // insert costs rounds x |head|; support_sql closes the cycle in one pass.
    const feeds_another_round = recursive_heads(statements, relations);
    const closes_in_one_pass = (statement: IIncrementalLevelStatement): boolean =>
      feeds_another_round.has(statement.head_rel) && statement.support_sql !== null;
    return sequence_level_rounds(statements, (statement) =>
      closes_in_one_pass(statement)
        ? reconcile_ref_count_statement(
            seam,
            statement,
            relations,
            level_frontier_copies(false),
          )
        : apply_level_statement(seam, statement, relation_by_name, false, next_sequence),
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
  recompute_levels_before_edges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
    reconcile_every_tick: boolean,
    arrivals: IArrivalBatch,
  ): Observable<void> {
    if (arrivals.length === 0) return of(undefined);
    const ref_count_statements = statements.filter((statement) => statement.aggregate_sql !== null ? false : true);
    if (ref_count_statements.length === 0 || relations.length === 0) return of(undefined);
    const reconcile = (): Observable<void> =>
      sequence_level_rounds(ref_count_statements, (statement) =>
        reconcile_ref_count_statement(
          seam,
          statement,
          relations,
          [{ table_name: (plan) => plan.frontier_table_name, phase: 2 }],
        ),
      );
    if (reconcile_every_tick) return reconcile();
    return seam.runner.execute(seam.db, retraction_guard_sql(relations, seam)).pipe(
      concatMap((result) =>
        Number(result.rows[0]?.has_retraction ?? 0) === 0 ? of(undefined) : reconcile()
      ),
    );
  },

  merge_next_into_current(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    if (relations.length === 0) return of(undefined);
    const sql = write_verbs_for(relations)
      .clear(observed_rels(relations, seam), "merge")
      .join(";\n");
    if (sql === "") return of(undefined);
    return seam.runner.executeMultiple(seam.db, sql);
  },

  apply_levels_after_edges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    const relation_by_name = new Map(relations.map((relation) => [relation.rel, relation]));
    let sequence = 0;
    const next_sequence = (): number => {
      const current = sequence;
      sequence += 1;
      return current;
    };
    return sequence_level_rounds(statements, (statement) =>
      apply_level_statement(seam, statement, relation_by_name, true, next_sequence),
    );
  },

  apply_retention(
    seam: ISqlSeam,
    statements: readonly IIncrementalRetentionStatement[],
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<void> {
    const relation_by_name = new Map(relations.map((relation) => [relation.rel, relation]));
    let sequence = 0;
    const next_sequence = (): number => {
      const current = sequence;
      sequence += 1;
      return current;
    };
    return sequence_work(
      statements,
      (statement) =>
        apply_retention_statement(seam, statement, relation_by_name, next_sequence),
    );
  },

  recompute_levels_after_edges(
    seam: ISqlSeam,
    statements: readonly IIncrementalLevelStatement[],
    relations: readonly IIncrementalRelationPlan[],
    reconcile_every_tick: boolean,
  ): Observable<void> {
    if (statements.length === 0) return of(undefined);
    const relation_by_name = new Map(relations.map((relation) => [relation.rel, relation]));
    // Per-statement rather than one batch over every statement's support_sql:
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
      const next_sequence = (): number => {
        const current = sequence;
        sequence += 1;
        return current;
      };
      return sequence_level_rounds(statements, (statement) => {
        const relation = relation_by_name.get(statement.head_rel);
        if (relation === undefined) {
          throw new Error(`incremental level head relation missing: ${statement.head_rel}`);
        }
        if (statement.aggregate_sql !== null) {
          if (statement.aggregate_sql.delta_maintained === true) return of(0);
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
          return apply_aggregate_level_statement(
            seam,
            statement,
            statement.aggregate_sql,
            relation,
            false,
            next_sequence,
          ).pipe(map(() => 0));
        }
        // No frontier copies HERE either, for the same reason as the
        // aggregate branch above: a reconcile row is a correction inside the
        // same closure, never post-write growth, so it must not reach the
        // next-tick frontier. The refCount branch shipped from P3 staging
        // into nextFrontier phase 1; the flagship callgraph fixture is the
        // corpus member that finally distinguished the two — a schedule
        // ending on its retraction tick minted one {"deltas":{}} drain the
        // oracle lacks (extraDrainTick.test.ts holds the fail-first receipt).
        return reconcile_ref_count_statement(
          seam,
          statement,
          relations,
          [],
        );
      });
    };
    if (reconcile_every_tick) return reconcile();
    if (relations.length === 0) return of(undefined);
    return seam.runner.execute(seam.db, retraction_guard_sql(relations, seam)).pipe(
      concatMap((result) =>
        Number(result.rows[0]?.has_retraction ?? 0) === 0 ? of(undefined) : reconcile()
      ),
    );
  },

  read_boundary(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<readonly IRelDelta[]> {
    if (relations.length === 0) return of([]);
    return forkJoin(
      relations.map((relation) =>
        seam.unobserved_rels?.has(relation.rel) === true
          ? of({ rel: relation.rel, add: [], del: [] } satisfies IRelDelta)
          : seam.runner.execute(seam.db, write_verbs_for(relations).publish(relation)).pipe(
              map((result) => boundary_delta(relation, result)),
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
  stage_departures(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
    deltas: readonly IRelDelta[],
  ): Observable<void> {
    const listening = relations.filter(
      (relation) => relation.departure_frontier_table_name !== undefined,
    );
    if (listening.length === 0) return of(undefined);
    const delta_by_rel = new Map(deltas.map((delta) => [delta.rel, delta]));
    const statements = listening.flatMap((relation): SqlStatement[] => {
      const table = quote_identifier(relation.departure_frontier_table_name!);
      const clear: SqlStatement = { sql: `DELETE FROM ${table}`, args: [] };
      const departed = delta_by_rel.get(relation.rel)?.del ?? [];
      if (departed.length === 0) return [clear];
      const columns = ["_phase", "_sequence", ...relation.columns].map(quote_identifier);
      const value_expressions = columns.map(
        (_column, index) => `json_extract(value, '$[${index}]')`,
      );
      const encoded = departed.map((row, sequence) => [0, sequence, ...row]);
      return [
        clear,
        {
          sql: `INSERT INTO ${table} (${columns.join(", ")}) SELECT ${value_expressions.join(", ")} FROM json_each(?)`,
          args: [JSON.stringify(encoded)],
        },
      ];
    });
    return seam.runner.batch(seam.db, statements).pipe(map(() => undefined));
  },

  promote_frontiers(
    seam: ISqlSeam,
    relations: readonly IIncrementalRelationPlan[],
  ): Observable<boolean> {
    if (relations.length === 0) return of(false);
    // Read-and-clear: a skipped rel's `__next_frontier_` stayed empty, so the
    // fill counts collected during the tick are its only carry evidence.
    const skipped_carried = skipped_carry.delete(seam);
    const verbs = write_verbs_for(relations);
    const observed = observed_rels(relations, seam);
    const promote_sql = verbs.clear(observed, "promote").join(";\n");
    const promote = (): Observable<void> =>
      promote_sql === "" ? of(undefined) : seam.runner.executeMultiple(seam.db, promote_sql);
    const carry_sql = verbs.read_staged(observed);
    if (carry_sql === "") return promote().pipe(map(() => skipped_carried));
    return seam.runner.execute(seam.db, carry_sql).pipe(
      concatMap((result: QueryResult) => {
        const carry_pending = Number(result.rows[0]?.carry_pending ?? 0) === 1;
        return promote().pipe(map(() => carry_pending || skipped_carried));
      }),
    );
  },
};
