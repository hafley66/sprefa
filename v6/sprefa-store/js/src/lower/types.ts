/** Contract header for the lowering plane. Imports only ./ast.ts and ../engine/types.ts. */

import type { Observable } from "rxjs";

import type { Program, RelDecl, Rule } from "./ast.ts";
import type { QueryResult, SqliteDb, TraceStatement } from "../engine/types.ts";

/** A rel's physical table plus its column names, positional. */
export interface RelTable {
  readonly table: string;
  readonly columns: readonly string[];
}

export type RelTables = ReadonlyMap<string, RelTable>;

/** Rule dependency graph, dense-indexed. Edge `head -> body-read` means head depends on body. */
export interface Graph {
  readonly nodes: readonly string[];
  readonly adj: readonly (readonly number[])[];
  /** Subset of adj that came from `!rel(args)`. Filled by buildRuleGraph. */
  readonly negAdj?: readonly (readonly number[])[];
}

/** One SCC. `recursive` picks single-pass versus semi-naive delta loop. */
export interface Stratum {
  readonly rels: readonly string[];
  readonly recursive: boolean;
  /** Topo position, 0 = no dependencies. */
  readonly order: number;
}

/**
 * Support-graph emission for the Z-set fact plane. Keys are `tag * stride + row_id`,
 * so each rel table must carry a dense integer surrogate.
 *
 * Only rules that are pure positive joins get edges. A negated body predicate or an
 * aggregate head has non-monotone support, and counting retraction over such an edge
 * is unsound.
 */
export interface SupportEdges {
  readonly table: string;
  readonly tagOf: ReadonlyMap<string, number>;
  readonly stride: number;
  readonly surrogate: string;
}

export interface SupportReport {
  readonly rulesWithoutSupport: readonly { readonly head: string; readonly reason: string }[];
}

/**
 * Evaluate every rule-headed IDB rel into its table, stratum by stratum.
 *
 * Emits exactly once, when every such table holds its current rowset. No further
 * emission follows, which is why this is not an rx operator type.
 *
 * The caller owns the transaction. Single statements only: `executeMultiple` would
 * trip the adapter's rollback guard.
 */
export type EvalProgram = (
  db: SqliteDb,
  prog: Program,
  tables: RelTables,
  support?: SupportEdges,
  traceStatement?: TraceStatement,
) => Observable<SupportReport>;

export type RulesByHead = ReadonlyMap<string, readonly Rule[]>;
export type RelDeclsByName = ReadonlyMap<string, RelDecl>;

/** One positive body occurrence: its alias and the rel it reads. The support pass needs
 *  this to name each parent row; the model SELECT does not use it. */
export interface PositiveSource {
  readonly alias: string;
  readonly rel: string;
}

/** The FROM / WHERE / variable-binding core of a rule, shared by the model SELECT and the
 *  support-edge emission so the two can never drift apart on join semantics. */
export interface CompiledJoin {
  readonly bound: ReadonlyMap<string, string>;
  readonly fromParts: readonly string[];
  readonly where: readonly string[];
  readonly positiveSources: readonly PositiveSource[];
}

/**
 * One evaluation of one program against one connection. Holds what the lowering used to
 * thread through every helper as parameters: the connection, the program, the table map,
 * the trace hook, and the derived stratification.
 */
/** What the support pass produces before anything is executed. */
export interface SupportPlan {
  readonly statements: readonly string[];
  readonly rulesWithoutSupport: readonly { readonly head: string; readonly reason: string }[];
}

export interface IDatalogEvaluator {
  readonly db: SqliteDb;
  readonly program: Program;
  readonly tables: RelTables;
  readonly support?: SupportEdges;
  readonly trace?: TraceStatement;
  readonly rulesByHead: RulesByHead;
  readonly strata: readonly Stratum[];

  /** Every statement goes through here, so every one is counted and traced. */
  exec(sql: string): Observable<QueryResult>;
  tableOf(relName: string): RelTable;
  rulesFor(relName: string): readonly Rule[];

  /** Emits exactly once, when every rule-headed table holds its current rowset. */
  run(): Observable<SupportReport>;

  /** SQL building is synchronous: these return statements, not observables. Only `exec`
   *  and `runAll` touch the connection. */
  clearStatements(): string[];
  acyclicStatements(relName: string): string[];
  insertNewRowsStatement(
    rule: Rule,
    bodyPositionOverrides: ReadonlyMap<number, string>,
    intoDelta: string,
  ): string | null;
  /** The caller runs this through `exec`, not `runAll`, because `rowsAffected` is the
   *  fixpoint's growth signal. */
  mergeStatement(relName: string, delta: string): string;
  createLikeStatements(name: string, like: RelTable): string[];
  supportPlan(support: SupportEdges): SupportPlan;

  /** The one place a list of SQL becomes execution. */
  runAll(statements: readonly string[]): Observable<void>;

  compileRuleJoin(rule: Rule, bodyPositionOverrides: ReadonlyMap<number, string>): CompiledJoin | null;
  compileRuleSelect(rule: Rule, bodyPositionOverrides: ReadonlyMap<number, string>): string | null;
}

/** One recursive SCC's semi-naive fixpoint. Its delta/next table names live only for the
 *  duration of the loop. */
export interface IRecursiveStratum {
  readonly evaluator: IDatalogEvaluator;
  readonly memberRels: readonly string[];
  readonly members: ReadonlySet<string>;
  readonly rules: readonly Rule[];
  readonly deltaTableOf: ReadonlyMap<string, string>;
  readonly nextTableOf: ReadonlyMap<string, string>;

  delta(relName: string): string;
  next(relName: string): string;
  recursivePositions(rule: Rule): number[];

  seedStatements(): string[];
  deriveStatements(): string[];
  promoteStatements(relName: string): string[];
  dropDeltaStatements(): string[];

  /** The only genuinely async step: each merge's `rowsAffected` decides whether the
   *  fixpoint runs again. */
  round(): Observable<boolean>;
  run(): Observable<void>;
}
