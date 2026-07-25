/** Datalog least-fixpoint lowered to SQL, as one observable. Stratified program ->
 *  INSERT..SELECT per rel; a recursive stratum is an `expand` fixpoint whose rounds are
 *  observable emissions. Compiles against table + column NAMES, so it runs over
 *  interned-INTEGER tables and plain text tables alike. Nothing here is async: the only
 *  promise is `db.execute`, wrapped once at the bottom in `makeExec`. */

import { EMPTY, Observable, concat, count, defer, from, last, map, of, reduce, throwError, concatMap, expand } from "rxjs";

import type { AggFn, Compare, Program, RelDecl, Rule } from "./ast.ts";
import { buildRuleGraph, scc, stratify } from "./rulegraph.ts";
import type { EvalProgram, RelTable, RelTables, SupportEdges, SupportReport } from "./types.ts";
import type { AssertTrue, SqliteDb } from "../engine/types.ts";
import { stmt_counter } from "../engine/engine.ts";

type Exec = (sql: string) => Observable<void>;
type CountRows = (table: string) => Observable<number>;

/** Run every step in order, emit once when the last one completes. */
function inOrder(steps: readonly Observable<unknown>[]): Observable<void> {
  return steps.length === 0 ? of(undefined) : concat(...steps).pipe(count(), map(() => undefined));
}

/** Map each item to a step, run them in order, emit once at the end. */
function forEachInOrder<Item>(items: readonly Item[], step: (item: Item) => Observable<unknown>): Observable<void> {
  return from(items).pipe(concatMap(step), count(), map(() => undefined));
}

/** A rel with an aggregate head inside a recursive SCC: non-monotone, not stratifiable. */
export class AggregateInRecursionError extends Error {
  constructor(rels: readonly string[]) {
    super(`aggregate head inside a recursive stratum (non-monotone): rels = [${rels.join(", ")}]`);
    this.name = "AggregateInRecursionError";
  }
}

/** Evaluate every rule-headed IDB rel into its table, stratum by stratum. Emits once,
 *  when every table is settled. The caller owns the transaction. */
export function evalProgramSql(
  db: SqliteDb,
  prog: Program,
  tables: RelTables,
  support?: SupportEdges,
  traceStatement?: (sql: string) => void,
): Observable<SupportReport> {
  return defer(() => {
    const sqlExecutor = makeExec(db, traceStatement);
    const countRows = makeCountRows(db);
    const graph = buildRuleGraph(prog);
    const strata = stratify(graph, scc(graph));

    const rulesByHead = new Map<string, Rule[]>();
    for (const rule of prog.rules) {
      const list = rulesByHead.get(rule.head);
      if (list) list.push(rule);
      else rulesByHead.set(rule.head, [rule]);
    }

    // Every rule-headed IDB table starts empty: a later stratum reads earlier ones.
    const clearStatements = prog.rels
      .filter((decl) => decl.origin !== "EDB" && rulesByHead.has(decl.name))
      .map((decl) => `DELETE FROM ${tableOf(tables, decl.name).table}`);

    return forEachInOrder(clearStatements, sqlExecutor).pipe(
      concatMap(() =>
        forEachInOrder(strata, (stratum) =>
          stratum.recursive
            ? evalRecursiveStratum(sqlExecutor, countRows, stratum.rels, new Set(stratum.rels), rulesByHead, tables)
            : forEachInOrder(stratum.rels, (relName) =>
                evalAcyclicRel(sqlExecutor, relName, rulesByHead.get(relName) ?? [], tables),
              ),
        ),
      ),
      concatMap(() =>
        support === undefined ? of({ rulesWithoutSupport: [] }) : emitSupportEdges(sqlExecutor, prog, tables, support),
      ),
    );
  });
}

function evalAcyclicRel(sqlExecutor: Exec, relName: string, rules: readonly Rule[], tables: RelTables): Observable<void> {
  const head = tableOf(tables, relName);
  const statements = rules
    .map((rule) => compileRuleSelect(rule, tables, new Map()))
    .filter((select): select is string => select !== null)
    .map((select) => `INSERT OR IGNORE INTO ${head.table}(${head.columns.join(", ")}) ${select}`);
  return forEachInOrder(statements, sqlExecutor);
}

/** Semi-naive fixpoint. `expand` IS the loop: each round emits whether anything grew,
 *  and feeds another round back in until nothing does. */
function evalRecursiveStratum(
  sqlExecutor: Exec,
  countRows: CountRows,
  memberRels: readonly string[],
  members: ReadonlySet<string>,
  rulesByHead: ReadonlyMap<string, Rule[]>,
  tables: RelTables,
): Observable<void> {
  return defer(() => {
    const stratumRules = memberRels.flatMap((relName) => rulesByHead.get(relName) ?? []);
    if (stratumRules.some((rule) => rule.headTerms.some((term) => term.kind === "hagg"))) {
      return throwError(() => new AggregateInRecursionError(memberRels));
    }

    const deltaTableOf = new Map(memberRels.map((relName) => [relName, `_dl_delta_${relName}`] as const));
    const nextTableOf = new Map(memberRels.map((relName) => [relName, `_dl_next_${relName}`] as const));

    const recursivePositions = (rule: Rule): number[] =>
      rule.body.flatMap((pred, bodyIndex) => (pred.kind === "rel" && members.has(pred.rel) ? [bodyIndex] : []));

    // Seed: every rule over the full tables. Recursive members are still empty, so only
    // the exit rules produce rows, and those become the first delta.
    const seed = forEachInOrder(memberRels, (relName) =>
      createLike(sqlExecutor, deltaTableOf.get(relName)!, tableOf(tables, relName)),
    ).pipe(
      concatMap(() =>
        forEachInOrder(stratumRules, (rule) =>
          insertNewRows(sqlExecutor, rule, tables, new Map(), deltaTableOf.get(rule.head)!),
        ),
      ),
      concatMap(() =>
        forEachInOrder(memberRels, (relName) =>
          mergeDeltaIntoFull(sqlExecutor, countRows, relName, tables, deltaTableOf.get(relName)!),
        ),
      ),
    );

    const round = (): Observable<boolean> =>
      forEachInOrder(memberRels, (relName) => createLike(sqlExecutor, nextTableOf.get(relName)!, tableOf(tables, relName))).pipe(
        concatMap(() =>
          forEachInOrder(stratumRules, (rule) =>
            forEachInOrder(recursivePositions(rule), (bodyIndex) => {
              const pred = rule.body[bodyIndex];
              if (pred?.kind !== "rel") return EMPTY;
              const override = new Map<number, string>([[bodyIndex, deltaTableOf.get(pred.rel)!]]);
              return insertNewRows(sqlExecutor, rule, tables, override, nextTableOf.get(rule.head)!);
            }),
          ),
        ),
        concatMap(() =>
          from(memberRels).pipe(
            concatMap((relName) =>
              mergeDeltaIntoFull(sqlExecutor, countRows, relName, tables, nextTableOf.get(relName)!).pipe(
                concatMap((rowsAdded) =>
                  inOrder([
                    sqlExecutor(`DROP TABLE IF EXISTS ${deltaTableOf.get(relName)!}`),
                    sqlExecutor(`ALTER TABLE ${nextTableOf.get(relName)!} RENAME TO ${deltaTableOf.get(relName)!}`),
                  ]).pipe(map(() => rowsAdded > 0)),
                ),
              ),
            ),
            reduce((grew: boolean, relGrew: boolean) => grew || relGrew, false),
          ),
        ),
      );

    return seed.pipe(
      concatMap(() => round()),
      expand((grew) => (grew ? round() : EMPTY)),
      last(),
      concatMap(() => forEachInOrder(memberRels, (relName) => sqlExecutor(`DROP TABLE IF EXISTS ${deltaTableOf.get(relName)!}`))),
    );
  });
}

/** Rows `rule` produces that are not already in the head's full table go into `intoDelta`. */
function insertNewRows(
  sqlExecutor: Exec,
  rule: Rule,
  tables: RelTables,
  bodyPositionOverrides: ReadonlyMap<number, string>,
  intoDelta: string,
): Observable<void> {
  const head = tableOf(tables, rule.head);
  const select = compileRuleSelect(rule, tables, bodyPositionOverrides);
  if (select === null) return of(undefined);
  return sqlExecutor(
    `INSERT OR IGNORE INTO ${intoDelta}(${head.columns.join(", ")}) ${select} ` +
      `EXCEPT SELECT ${head.columns.join(", ")} FROM ${head.table}`,
  );
}

function mergeDeltaIntoFull(
  sqlExecutor: Exec,
  countRows: CountRows,
  relName: string,
  tables: RelTables,
  delta: string,
): Observable<number> {
  const full = tableOf(tables, relName);
  const columns = full.columns.join(", ");
  return countRows(full.table).pipe(
    concatMap((before) =>
      sqlExecutor(`INSERT OR IGNORE INTO ${full.table}(${columns}) SELECT ${columns} FROM ${delta}`).pipe(
        concatMap(() => countRows(full.table)),
        map((after) => after - before),
      ),
    ),
  );
}

function createLike(sqlExecutor: Exec, name: string, like: RelTable): Observable<void> {
  const columns = like.columns.join(", ");
  return inOrder([
    sqlExecutor(`DROP TABLE IF EXISTS ${name}`),
    sqlExecutor(`CREATE TEMP TABLE ${name}(${columns}, PRIMARY KEY (${columns})) WITHOUT ROWID`),
  ]);
}

/** Support edges, one pass over the settled model. Per rule per positive body position:
 *  the (parent_key, child_key) pairs, joining back to the head table for its surrogate. */
function emitSupportEdges(
  sqlExecutor: Exec,
  prog: Program,
  tables: RelTables,
  support: SupportEdges,
): Observable<SupportReport> {
  return defer(() => {
    const rulesWithoutSupport: { head: string; reason: string }[] = [];
    const statements: string[] = [];
    const tagOf = (relName: string): number => {
      const tag = support.tagOf.get(relName);
      if (tag === undefined) throw new Error(`lowerSql: no dense tag registered for rel '${relName}'`);
      return tag;
    };

    for (const rule of prog.rules) {
      if (rule.headTerms.some((term) => term.kind === "hagg")) {
        rulesWithoutSupport.push({ head: rule.head, reason: "aggregate head: support is not monotone in the body" });
        continue;
      }
      if (rule.body.some((predicate) => predicate.kind === "notrel")) {
        rulesWithoutSupport.push({
          head: rule.head,
          reason: "negated body predicate: a body row destroys rather than adds a derivation",
        });
        continue;
      }
      const compiled = compileRuleJoin(rule, tables, new Map());
      if (compiled === null) {
        rulesWithoutSupport.push({ head: rule.head, reason: "no positive body rel" });
        continue;
      }

      const headTable = tableOf(tables, rule.head);
      const headAlias = "hh";
      const headMatch: string[] = [];
      let headBindingUnresolved = false;
      rule.headTerms.forEach((term, index) => {
        if (term.kind !== "hvar") return;
        const boundColumnRef = compiled.bound.get(term.name);
        const column = headTable.columns[index];
        if (boundColumnRef === undefined || column === undefined) {
          headBindingUnresolved = true;
          return;
        }
        headMatch.push(`${headAlias}.${column} = ${boundColumnRef}`);
      });
      if (headBindingUnresolved) {
        rulesWithoutSupport.push({ head: rule.head, reason: "head term is not bound by a positive body rel" });
        continue;
      }

      const childKey = `${tagOf(rule.head)} * ${support.stride} + ${headAlias}.${support.surrogate}`;
      const fromClause = [...compiled.fromParts, `${headTable.table} ${headAlias}`].join(", ");
      const whereClause = [...compiled.where, ...headMatch];

      for (const source of compiled.positiveSources) {
        const parentKey = `${tagOf(source.rel)} * ${support.stride} + ${source.alias}.${support.surrogate}`;
        let sql = `INSERT OR IGNORE INTO ${support.table}(parent_key, child_key) SELECT ${parentKey}, ${childKey} FROM ${fromClause}`;
        if (whereClause.length > 0) sql += ` WHERE ${whereClause.join(" AND ")}`;
        statements.push(sql);
      }
    }

    return forEachInOrder(statements, sqlExecutor).pipe(map(() => ({ rulesWithoutSupport })));
  });
}

/** One positive body occurrence: its alias and the rel it reads. The support pass needs
 *  this to name each parent row; the model SELECT does not use it. */
interface PositiveSource {
  readonly alias: string;
  readonly rel: string;
}

/** The FROM / WHERE / variable-binding core of a rule, shared by the model SELECT and the
 *  support-edge emission so the two can never drift apart on join semantics. */
interface CompiledJoin {
  readonly bound: ReadonlyMap<string, string>;
  readonly fromParts: readonly string[];
  readonly where: readonly string[];
  readonly positiveSources: readonly PositiveSource[];
}

function compileRuleJoin(
  rule: Rule,
  tables: RelTables,
  bodyPositionOverrides: ReadonlyMap<number, string>,
): CompiledJoin | null {
  const bound = new Map<string, string>(); // var name -> "alias.column"
  const fromParts: string[] = [];
  const where: string[] = [];
  const positiveSources: PositiveSource[] = [];

  let positiveIndex = 0;
  let negIndex = 0;

  rule.body.forEach((predicate, bodyIndex) => {
    if (predicate.kind !== "rel") return;
    const relTable = tableOf(tables, predicate.rel);
    const alias = `b${positiveIndex++}`;
    const sourceTable = bodyPositionOverrides.get(bodyIndex) ?? relTable.table;
    fromParts.push(`${sourceTable} ${alias}`);
    positiveSources.push({ alias, rel: predicate.rel });
    for (let columnIndex = 0; columnIndex < predicate.args.length; columnIndex++) {
      const argument = predicate.args[columnIndex]!;
      if (argument.kind === "wild") continue;
      const columnRef = `${alias}.${relTable.columns[columnIndex]}`;
      if (argument.kind === "lit") {
        where.push(`${columnRef} = ${sqlLit(argument.value)}`);
      } else {
        const boundValue = bound.get(argument.name);
        if (boundValue !== undefined) where.push(`${columnRef} = ${boundValue}`);
        else bound.set(argument.name, columnRef);
      }
    }
  });

  if (fromParts.length === 0) return null;

  for (const predicate of rule.body) {
    if (predicate.kind === "cmp") {
      const lhs = bound.get(predicate.lhs.name);
      if (lhs === undefined) continue; // range-restriction should bind it; skip defensively
      where.push(`${lhs} ${sqlCmpOp(predicate.op)} ${sqlLit(predicate.rhs.value)}`);
    } else if (predicate.kind === "notrel") {
      const negTable = tableOf(tables, predicate.rel);
      const negatedTableAlias = `n${negIndex++}`;
      const conditions: string[] = [];
      for (let columnIndex = 0; columnIndex < predicate.args.length; columnIndex++) {
        const argument = predicate.args[columnIndex]!;
        if (argument.kind === "wild") continue;
        const columnRef = `${negatedTableAlias}.${negTable.columns[columnIndex]}`;
        if (argument.kind === "lit") {
          conditions.push(`${columnRef} = ${sqlLit(argument.value)}`);
        } else {
          const boundValue = bound.get(argument.name);
          // A bound var equi-joins into the anti-check; an unbound var is existential
          // over the negated rel's rows (negation-as-failure), so it adds no condition.
          if (boundValue !== undefined) conditions.push(`${columnRef} = ${boundValue}`);
        }
      }
      where.push(`NOT EXISTS (SELECT 1 FROM ${negTable.table} ${negatedTableAlias}${conditions.length > 0 ? ` WHERE ${conditions.join(" AND ")}` : ""})`);
    }
  }

  return { bound, fromParts, where, positiveSources };
}

function compileRuleSelect(rule: Rule, tables: RelTables, bodyPositionOverrides: ReadonlyMap<number, string>): string | null {
  const compiled = compileRuleJoin(rule, tables, bodyPositionOverrides);
  if (compiled === null) return null;
  const { bound, fromParts, where } = compiled;

  const hasAgg = rule.headTerms.some((term) => term.kind === "hagg");
  const selectList = rule.headTerms.map((term) => {
    if (term.kind === "hvar") {
      const boundColumnRef = bound.get(term.name);
      if (boundColumnRef === undefined) throw new Error(`lowerSql: head var '${term.name}' of rel '${rule.head}' is unbound`);
      return boundColumnRef;
    }
    const boundColumnRef = bound.get(term.arg.name);
    if (boundColumnRef === undefined) throw new Error(`lowerSql: aggregate arg '${term.arg.name}' of rel '${rule.head}' is unbound`);
    return `${sqlAgg(term.fn)}(${boundColumnRef})`;
  });

  let sql = `SELECT ${selectList.join(", ")} FROM ${fromParts.join(", ")}`;
  if (where.length > 0) sql += ` WHERE ${where.join(" AND ")}`;
  if (hasAgg) {
    const groupRefs = rule.headTerms
      .filter((term): term is Extract<typeof term, { kind: "hvar" }> => term.kind === "hvar")
      .map((term) => bound.get(term.name)!);
    if (groupRefs.length > 0) sql += ` GROUP BY ${groupRefs.join(", ")}`;
    // Ungrouped aggregate over an EMPTY body: bare `SELECT count(x) FROM t` still
    // returns one row (0), while the in-memory evaluator groups bindings and so emits
    // NO row for zero bindings (lower.ts projectAndAggregate: no bindings = no groups).
    // `HAVING count(*) > 0` restores the datalog reading — an aggregate rule with no
    // satisfying body derives no fact — and is a no-op once any binding exists.
    else sql += " HAVING count(*) > 0";
  }
  return sql;
}


function sqlLit(value: string | number | boolean | null): string {
  if (value === null) return "NULL";
  if (typeof value === "number") return String(value);
  if (typeof value === "boolean") return value ? "1" : "0";
  return `'${value.replace(/'/g, "''")}'`;
}

function sqlCmpOp(op: Compare["op"]): string {
  switch (op) {
    case "eq":
      return "=";
    case "ne":
      return "<>";
    case "lt":
      return "<";
    case "le":
      return "<=";
    case "gt":
      return ">";
    case "ge":
      return ">=";
  }
}

function sqlAgg(fn: AggFn): string {
  switch (fn) {
    case "max":
      return "MAX";
    case "min":
      return "MIN";
    case "sum":
      return "SUM";
    case "count":
      return "COUNT";
  }
}

function tableOf(tables: RelTables, relName: string): RelTable {
  const relTable = tables.get(relName);
  if (relTable === undefined) throw new Error(`lowerSql: no table registered for rel '${relName}'`);
  return relTable;
}

/** The one async seam in this file. `defer` keeps it cold; `traceStatement` sees exactly
 *  what runs. Single statements only: `executeMultiple` trips the adapter's rollback
 *  guard, which would kill the caller's open transaction. */
function makeExec(db: SqliteDb, traceStatement?: (sql: string) => void): Exec {
  return (sql) =>
    defer(() => {
      stmt_counter.incr();
      traceStatement?.(sql);
      return from(db.execute(sql));
    }).pipe(map(() => undefined));
}

function makeCountRows(db: SqliteDb): CountRows {
  return (table) =>
    defer(() => {
      stmt_counter.incr();
      return from(db.execute(`SELECT count(*) FROM ${table}`));
    }).pipe(map((res) => Number(res.rows[0]?.[0] ?? 0)));
}

export type EvalProgramHolds = AssertTrue<typeof evalProgramSql extends EvalProgram ? true : false>;
