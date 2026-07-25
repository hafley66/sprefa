/** Contract header for the lowering plane. Imports only ./ast.ts and ../engine/types.ts. */

import type { Observable } from "rxjs";

import type { Program, RelDecl, Rule } from "./ast.ts";
import type { SqliteDb } from "../engine/types.ts";

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
  traceStatement?: (sql: string) => void,
) => Observable<SupportReport>;

export interface IDatalog {
  readonly evalProgram: EvalProgram;
}

export type RulesByHead = ReadonlyMap<string, readonly Rule[]>;
export type RelDeclsByName = ReadonlyMap<string, RelDecl>;
