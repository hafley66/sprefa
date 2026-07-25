/** Datalog least-fixpoint lowered to SQL, as one observable. Stratified program ->
 *  INSERT..SELECT per rel; a recursive stratum is an `expand` fixpoint whose rounds are
 *  observable emissions. Compiles against table + column NAMES, so it runs over
 *  interned-INTEGER tables and plain text tables alike. Nothing here is async: the only
 *  promise is `db.execute`, and it arrives already wrapped by SqlRunner. */

import { EMPTY, Observable, concat, count, defer, expand, from, last, map, of, reduce, throwError, concatMap } from "rxjs";

import type { AggFn, Compare, Program, Rule } from "./ast.ts";
import { buildRuleGraph, scc, stratify } from "./rulegraph.ts";
import type {
  CompiledJoin,
  CompiledSelect,
  EvalProgram,
  IDatalogEvaluator,
  IRecursiveStratum,
  PositiveSource,
  RelTable,
  RelTables,
  RulesByHead,
  Stratum,
  SupportEdges,
  SupportReport,
} from "./types.ts";
import type { AssertTrue, QueryResult, SqlStatement, SqlValue, SqliteDb, TraceStatement } from "../engine/types.ts";
import { SqlRunner } from "../engine/sqlRunner.ts";

/** A rel with an aggregate head inside a recursive SCC: non-monotone, not stratifiable. */
export class AggregateInRecursionError extends Error {
  constructor(rels: readonly string[]) {
    super(`aggregate head inside a recursive stratum (non-monotone): rels = [${rels.join(", ")}]`);
    this.name = "AggregateInRecursionError";
  }
}

/**
 * One evaluation of one program against one connection. The connection, the program, its
 * table map, and the trace hook were being threaded through seven free functions as
 * parameters; they are constructor fields.
 */
export class DatalogEvaluator implements IDatalogEvaluator {
  readonly rulesByHead: RulesByHead;
  readonly strata: readonly Stratum[];

  constructor(
    readonly db: SqliteDb,
    readonly program: Program,
    readonly tables: RelTables,
    readonly support?: SupportEdges,
    readonly trace?: TraceStatement,
  ) {
    const graph = buildRuleGraph(program);
    this.strata = stratify(graph, scc(graph));

    const rulesByHead = new Map<string, Rule[]>();
    for (const rule of program.rules) {
      const list = rulesByHead.get(rule.head);
      if (list) list.push(rule);
      else rulesByHead.set(rule.head, [rule]);
    }
    this.rulesByHead = rulesByHead;
  }

  /** Every statement in this file goes through here, so every one is counted and traced. */
  exec(statement: SqlStatement): Observable<QueryResult> {
    return SqlRunner.execute(this.db, statement, this.trace);
  }

  tableOf(relName: string): RelTable {
    const relTable = this.tables.get(relName);
    if (relTable === undefined) throw new Error(`lowerSql: no table registered for rel '${relName}'`);
    return relTable;
  }

  rulesFor(relName: string): readonly Rule[] {
    return this.rulesByHead.get(relName) ?? [];
  }

  /** Every rule-headed IDB table starts empty: a later stratum reads earlier ones. */
  clearStatements(): SqlStatement[] {
    return this.program.rels
      .filter((decl) => decl.origin !== "EDB" && this.rulesByHead.has(decl.name))
      .map((decl) => `DELETE FROM ${quoteIdent(this.tableOf(decl.name).table)}`);
  }

  acyclicStatements(relName: string): SqlStatement[] {
    const head = this.tableOf(relName);
    return this.rulesFor(relName)
      .map((rule) => this.compileRuleSelect(rule, new Map()))
      .filter((select): select is CompiledSelect => select !== null)
      .map((select) => ({
        sql: `INSERT OR IGNORE INTO ${quoteIdent(head.table)}(${columnList(head.columns)}) ${select.sql}`,
        args: select.args,
      }));
  }

  /** The one place a list of SQL becomes execution. Emits once, when the last statement
   *  finishes. DDL result sets are not values anybody wants, so `void` is the honest
   *  payload; `count` guarantees the single emission even for an empty list. */
  runAll(statements: readonly SqlStatement[]): Observable<void> {
    return concat(...statements.map((statement) => this.exec(statement))).pipe(
      count(),
      map(() => undefined),
    );
  }

  run(): Observable<SupportReport> {
    const settleModel = this.strata.reduce<Observable<void>>(
      (chain, stratum) =>
        chain.pipe(
          concatMap(() =>
            stratum.recursive
              ? new RecursiveStratum(this, stratum.rels).run()
              : this.runAll(stratum.rels.flatMap((relName) => this.acyclicStatements(relName))),
          ),
        ),
      this.runAll(this.clearStatements()),
    );

    return settleModel.pipe(
      concatMap(() => {
        if (this.support === undefined) return of<SupportReport>({ rulesWithoutSupport: [] });
        const plan = this.supportPlan(this.support);
        return this.runAll(plan.statements).pipe(map(() => plan));
      }),
    );
  }

  /** Rows `rule` produces that are not already in the head's full table go into `intoDelta`. */
  insertNewRowsStatement(
    rule: Rule,
    bodyPositionOverrides: ReadonlyMap<number, string>,
    intoDelta: string,
  ): SqlStatement | null {
    const head = this.tableOf(rule.head);
    const select = this.compileRuleSelect(rule, bodyPositionOverrides);
    if (select === null) return null;
    return {
      sql:
        `INSERT OR IGNORE INTO ${quoteIdent(intoDelta)}(${columnList(head.columns)}) ${select.sql} ` +
        `EXCEPT SELECT ${columnList(head.columns)} FROM ${quoteIdent(head.table)}`,
      args: select.args,
    };
  }

  /** `rowsAffected` off this INSERT is the growth count, which is why the caller runs it
   *  through `exec` rather than `runAll`: a pair of `SELECT count(*)` scans used to
   *  recompute a number the driver already returns. */
  mergeStatement(relName: string, delta: string): SqlStatement {
    const full = this.tableOf(relName);
    const columns = columnList(full.columns);
    return `INSERT OR IGNORE INTO ${quoteIdent(full.table)}(${columns}) SELECT ${columns} FROM ${quoteIdent(delta)}`;
  }

  createLikeStatements(name: string, like: RelTable): SqlStatement[] {
    const columns = columnList(like.columns);
    return [
      `DROP TABLE IF EXISTS ${quoteIdent(name)}`,
      `CREATE TEMP TABLE ${quoteIdent(name)}(${columns}, PRIMARY KEY (${columns})) WITHOUT ROWID`,
    ];
  }

  /** Support edges, one pass over the settled model. Per rule per positive body position:
   *  the (parent_key, child_key) pairs, joining back to the head table for its surrogate. */
  supportPlan(support: SupportEdges): { statements: SqlStatement[]; rulesWithoutSupport: { head: string; reason: string }[] } {
    {
      const rulesWithoutSupport: { head: string; reason: string }[] = [];
      const statements: SqlStatement[] = [];
      const tagOf = (relName: string): number => {
        const tag = support.tagOf.get(relName);
        if (tag === undefined) throw new Error(`lowerSql: no dense tag registered for rel '${relName}'`);
        return tag;
      };

      for (const rule of this.program.rules) {
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
        const compiled = this.compileRuleJoin(rule, new Map());
        if (compiled === null) {
          rulesWithoutSupport.push({ head: rule.head, reason: "no positive body rel" });
          continue;
        }

        const headTable = this.tableOf(rule.head);
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

        const childKey = `${tagOf(rule.head)} * ${support.stride} + ${headAlias}.${quoteIdent(support.surrogate)}`;
        const fromClause = [...compiled.fromParts, `${quoteIdent(headTable.table)} ${headAlias}`].join(", ");
        const whereClause = [...compiled.where, ...headMatch];

        for (const source of compiled.positiveSources) {
          const parentKey = `${tagOf(source.rel)} * ${support.stride} + ${source.alias}.${quoteIdent(support.surrogate)}`;
          let sql = `INSERT OR IGNORE INTO ${quoteIdent(support.table)}(parent_key, child_key) SELECT ${parentKey}, ${childKey} FROM ${fromClause}`;
          if (whereClause.length > 0) sql += ` WHERE ${whereClause.join(" AND ")}`;
          // compiled.where carries `?` placeholders; headMatch binds nothing, so the arg
          // order is exactly compiled.args. Pushing sql without them silently emptied the
          // support graph while leaving the model query correct.
          statements.push({ sql, args: [...compiled.args] });
        }
      }

      return { statements, rulesWithoutSupport };
    }
  }

  /** The FROM / WHERE / variable-binding core of a rule, shared by the model SELECT and
   *  the support-edge emission so the two can never drift apart on join semantics. */
  compileRuleJoin(rule: Rule, bodyPositionOverrides: ReadonlyMap<number, string>): CompiledJoin | null {
    const bound = new Map<string, string>(); // var name -> "alias.column"
    const fromParts: string[] = [];
    const where: string[] = [];
    const positiveSources: PositiveSource[] = [];
    // Values are bound, never spliced. Positional `?` means arg order must match the
    // order the fragments are concatenated in, which is the order they are pushed here.
    const args: SqlValue[] = [];
    const bind = (value: SqlValue): string => {
      args.push(value);
      return "?";
    };

    let positiveIndex = 0;
    let negIndex = 0;

    rule.body.forEach((predicate, bodyIndex) => {
      if (predicate.kind !== "rel") return;
      const relTable = this.tableOf(predicate.rel);
      const alias = `b${positiveIndex++}`;
      const sourceTable = bodyPositionOverrides.get(bodyIndex) ?? relTable.table;
      fromParts.push(`${sourceTable} ${alias}`);
      positiveSources.push({ alias, rel: predicate.rel });
      for (let columnIndex = 0; columnIndex < predicate.args.length; columnIndex++) {
        const argument = predicate.args[columnIndex]!;
        if (argument.kind === "wild") continue;
        const columnRef = `${alias}.${relTable.columns[columnIndex]}`;
        if (argument.kind === "lit") {
          where.push(`${columnRef} = ${bind(argument.value)}`);
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
        where.push(`${lhs} ${sqlCmpOp(predicate.op)} ${bind(predicate.rhs.value)}`);
      } else if (predicate.kind === "notrel") {
        const negTable = this.tableOf(predicate.rel);
        const negatedTableAlias = `n${negIndex++}`;
        const conditions: string[] = [];
        for (let columnIndex = 0; columnIndex < predicate.args.length; columnIndex++) {
          const argument = predicate.args[columnIndex]!;
          if (argument.kind === "wild") continue;
          const columnRef = `${negatedTableAlias}.${negTable.columns[columnIndex]}`;
          if (argument.kind === "lit") {
            conditions.push(`${columnRef} = ${bind(argument.value)}`);
          } else {
            const boundValue = bound.get(argument.name);
            // A bound var equi-joins into the anti-check; an unbound var is existential
            // over the negated rel's rows (negation-as-failure), so it adds no condition.
            if (boundValue !== undefined) conditions.push(`${columnRef} = ${boundValue}`);
          }
        }
        where.push(
          `NOT EXISTS (SELECT 1 FROM ${negTable.table} ${negatedTableAlias}${conditions.length > 0 ? ` WHERE ${conditions.join(" AND ")}` : ""})`,
        );
      }
    }

    return { bound, fromParts, where, positiveSources, args };
  }

  compileRuleSelect(rule: Rule, bodyPositionOverrides: ReadonlyMap<number, string>): CompiledSelect | null {
    const compiled = this.compileRuleJoin(rule, bodyPositionOverrides);
    if (compiled === null) return null;
    const { bound, fromParts, where } = compiled;

    const hasAgg = rule.headTerms.some((term) => term.kind === "hagg");
    const selectList = rule.headTerms.map((term) => {
      if (term.kind === "hvar") {
        const boundColumnRef = bound.get(term.name);
        if (boundColumnRef === undefined)
          throw new Error(`lowerSql: head var '${term.name}' of rel '${rule.head}' is unbound`);
        return boundColumnRef;
      }
      const boundColumnRef = bound.get(term.arg.name);
      if (boundColumnRef === undefined)
        throw new Error(`lowerSql: aggregate arg '${term.arg.name}' of rel '${rule.head}' is unbound`);
      return `${sqlAgg(term.fn)}(${boundColumnRef})`;
    });

    let sql = `SELECT ${selectList.join(", ")} FROM ${fromParts.join(", ")}`;
    if (where.length > 0) sql += ` WHERE ${where.join(" AND ")}`;
    const args = compiled.args;
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
    return { sql, args };
  }
}

/**
 * One recursive SCC's semi-naive fixpoint. The delta and next table names live for the
 * duration of the loop and nothing outside it needs them, which is why this is its own
 * instance rather than five more parameters on the evaluator.
 */
export class RecursiveStratum implements IRecursiveStratum {
  readonly members: ReadonlySet<string>;
  readonly rules: readonly Rule[];
  readonly deltaTableOf: ReadonlyMap<string, string>;
  readonly nextTableOf: ReadonlyMap<string, string>;

  constructor(
    readonly evaluator: IDatalogEvaluator,
    readonly memberRels: readonly string[],
  ) {
    this.members = new Set(memberRels);
    this.rules = memberRels.flatMap((relName) => evaluator.rulesFor(relName));
    this.deltaTableOf = new Map(memberRels.map((relName) => [relName, `_dl_delta_${relName}`]));
    this.nextTableOf = new Map(memberRels.map((relName) => [relName, `_dl_next_${relName}`]));
  }

  delta(relName: string): string {
    return this.deltaTableOf.get(relName)!;
  }

  next(relName: string): string {
    return this.nextTableOf.get(relName)!;
  }

  /** Body positions that read a rel inside this SCC; each one gets its own delta pass. */
  recursivePositions(rule: Rule): number[] {
    return rule.body.flatMap((pred, bodyIndex) => (pred.kind === "rel" && this.members.has(pred.rel) ? [bodyIndex] : []));
  }

  /** Every rule over the full tables. Recursive members are still empty, so only the exit
   *  rules produce rows, and those become the first delta. Nothing here needs a result
   *  back, so it is a statement list. */
  seedStatements(): SqlStatement[] {
    return [
      ...this.memberRels.flatMap((relName) =>
        this.evaluator.createLikeStatements(this.delta(relName), this.evaluator.tableOf(relName)),
      ),
      ...this.rules.flatMap((rule) => {
        const statement = this.evaluator.insertNewRowsStatement(rule, new Map(), this.delta(rule.head));
        return statement === null ? [] : [statement];
      }),
      ...this.memberRels.map((relName) => this.evaluator.mergeStatement(relName, this.delta(relName))),
    ];
  }

  /** Fresh `next` tables, then one delta pass per recursive body position. */
  deriveStatements(): SqlStatement[] {
    return [
      ...this.memberRels.flatMap((relName) =>
        this.evaluator.createLikeStatements(this.next(relName), this.evaluator.tableOf(relName)),
      ),
      ...this.rules.flatMap((rule) =>
        this.recursivePositions(rule).flatMap((bodyIndex) => {
          const pred = rule.body[bodyIndex];
          if (pred?.kind !== "rel") return [];
          const override = new Map<number, string>([[bodyIndex, this.delta(pred.rel)]]);
          const statement = this.evaluator.insertNewRowsStatement(rule, override, this.next(rule.head));
          return statement === null ? [] : [statement];
        }),
      ),
    ];
  }

  /** `next` becomes the new delta. */
  promoteStatements(relName: string): SqlStatement[] {
    return [
      `DROP TABLE IF EXISTS ${quoteIdent(this.delta(relName))}`,
      `ALTER TABLE ${quoteIdent(this.next(relName))} RENAME TO ${quoteIdent(this.delta(relName))}`,
    ];
  }

  dropDeltaStatements(): SqlStatement[] {
    return this.memberRels.map((relName) => `DROP TABLE IF EXISTS ${quoteIdent(this.delta(relName))}`);
  }

  /** One semi-naive round. The only genuinely async step: each merge's `rowsAffected`
   *  decides whether the fixpoint runs again. */
  round(): Observable<boolean> {
    return this.evaluator.runAll(this.deriveStatements()).pipe(
      concatMap(() =>
        from(this.memberRels).pipe(
          concatMap((relName) =>
            this.evaluator.exec(this.evaluator.mergeStatement(relName, this.next(relName))).pipe(
              map((queryResult) => Number(queryResult.rowsAffected) > 0),
              concatMap((grew) => this.evaluator.runAll(this.promoteStatements(relName)).pipe(map(() => grew))),
            ),
          ),
          reduce((grew: boolean, relGrew: boolean) => grew || relGrew, false),
        ),
      ),
    );
  }

  /** `expand` IS the loop: each round emits whether anything grew and feeds another round
   *  back in until nothing does. `last` waits for the fixpoint to settle before the drops. */
  run(): Observable<void> {
    return defer(() => {
      if (this.rules.some((rule) => rule.headTerms.some((term) => term.kind === "hagg"))) {
        return throwError(() => new AggregateInRecursionError(this.memberRels));
      }
      return this.evaluator.runAll(this.seedStatements()).pipe(
        concatMap(() => this.round()),
        expand((grew) => (grew ? this.round() : EMPTY)),
        last(),
        concatMap(() => this.evaluator.runAll(this.dropDeltaStatements())),
      );
    });
  }
}

/** Table and column names are not values, so they cannot be bound. They come from `.dl`
 *  rel declarations through bridge(), which is grammar-constrained, but quoting also
 *  makes reserved words safe. Anything a double quote could escape out of is rejected. */
export function quoteIdent(name: string): string {
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
    throw new Error(`lowerSql: refusing to build SQL with the identifier ${JSON.stringify(name)}`);
  }
  return `"${name}"`;
}

/** A quoted, comma-separated column list. */
function columnList(columns: readonly string[]): string {
  return columns.map(quoteIdent).join(", ");
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

/** Function-shaped entry point for callers that just want the evaluation run. */
export const evalProgramSql: EvalProgram = (db, prog, tables, support, traceStatement) =>
  defer(() => new DatalogEvaluator(db, prog, tables, support, traceStatement).run());

export type EvalProgramHolds = AssertTrue<typeof evalProgramSql extends EvalProgram ? true : false>;
