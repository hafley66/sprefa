/** Datalog least-fixpoint lowered to SQL. Stratified program -> INSERT..SELECT per rel;
 *  a recursive stratum runs a semi-naive delta loop in SQLite, rows never hit the JS heap.
 *  Compiles against table + column NAMES, so it runs over interned-INTEGER tables and over
 *  plain text tables alike. Stratification is rulegraph.ts's job. */

import type { AggFn, Compare, Program, RelDecl, Rule } from "./ast.ts";
import { buildRuleGraph, scc, stratify } from "./rulegraph.ts";
import type { EvalProgram, RelTable, RelTables, SupportEdges, SupportReport } from "./types.ts";
import type { AssertTrue, SqliteDb } from "../engine/types.ts";
import { stmt_counter } from "../engine/engine.ts";


/** A rel with an aggregate head inside a recursive SCC: non-monotone, not stratifiable
 *  as plain datalog. Raised before any SQL runs (mirrors NonStratifiableError's timing). */
export class AggregateInRecursionError extends Error {
  constructor(rels: readonly string[]) {
    super(`aggregate head inside a recursive stratum (non-monotone): rels = [${rels.join(", ")}]`);
    this.name = "AggregateInRecursionError";
  }
}

/**
 * Evaluate every IDB rel of `prog` into its table, stratum by stratum in topological
 * order. EDB tables are read as-is; each IDB table is DELETEd then refilled (full
 * recompute from the current EDB — the incremental/demand path is a later arc). The
 * caller owns the surrounding transaction; this issues plain statements.
 */
export async function evalProgramSql(
  db: SqliteDb,
  prog: Program,
  tables: RelTables,
  support?: SupportEdges,
  traceStatement?: (sql: string) => void,
): Promise<SupportReport> {
  const exec = makeExec(db, traceStatement);
  const count = makeCount(db);
  const graph = buildRuleGraph(prog);
  const strata = stratify(graph, scc(graph));

  const relDecls = new Map<string, RelDecl>();
  for (const decl of prog.rels) relDecls.set(decl.name, decl);

  const rulesByHead = new Map<string, Rule[]>();
  for (const rule of prog.rules) {
    const list = rulesByHead.get(rule.head);
    if (list) list.push(rule);
    else rulesByHead.set(rule.head, [rule]);
  }

  // Clear every rule-headed IDB table once up front (a later stratum reads earlier IDB
  // rels, so all must start empty before the topo sweep fills them in order).
  for (const decl of prog.rels) {
    if (decl.origin !== "EDB" && rulesByHead.has(decl.name)) {
      await exec(`DELETE FROM ${tableOf(tables, decl.name).table}`);
    }
  }

  for (const stratum of strata) {
    if (!stratum.recursive) {
      for (const relName of stratum.rels) {
        const rules = rulesByHead.get(relName);
        if (!rules || rules.length === 0) continue;
        await evalAcyclicRel(exec, relName, rules, tables);
      }
      continue;
    }
    await evalRecursiveStratum(exec, count, stratum.rels, new Set(stratum.rels), rulesByHead, tables);
  }

  if (support === undefined) return { rulesWithoutSupport: [] };
  return emitSupportEdges(exec, prog, tables, support);
}

/** Support edges, one final pass over the settled model. Per rule per positive body
 *  position: INSERT the (parent_key, child_key) pairs, joining back to the head table to
 *  find the head row's surrogate. */

async function emitSupportEdges(
  exec: Exec,
  prog: Program,
  tables: RelTables,
  support: SupportEdges,
): Promise<SupportReport> {
  const rulesWithoutSupport: { head: string; reason: string }[] = [];
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
    if (rule.body.some((pred) => pred.kind === "notrel")) {
      rulesWithoutSupport.push({ head: rule.head, reason: "negated body predicate: a body row destroys rather than adds a derivation" });
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
      const ref = compiled.bound.get(term.name);
      const column = headTable.columns[index];
      if (ref === undefined || column === undefined) {
        headBindingUnresolved = true;
        return;
      }
      headMatch.push(`${headAlias}.${column} = ${ref}`);
    });
    if (headBindingUnresolved) {
      rulesWithoutSupport.push({ head: rule.head, reason: "head term is not bound by a positive body rel" });
      continue;
    }

    const childKey = `${tagOf(rule.head)} * ${support.stride} + ${headAlias}.${support.surrogate}`;
    const from = [...compiled.fromParts, `${headTable.table} ${headAlias}`].join(", ");
    const where = [...compiled.where, ...headMatch];

    for (const source of compiled.positiveSources) {
      const parentKey = `${tagOf(source.rel)} * ${support.stride} + ${source.alias}.${support.surrogate}`;
      let sql = `INSERT OR IGNORE INTO ${support.table}(parent_key, child_key) SELECT ${parentKey}, ${childKey} FROM ${from}`;
      if (where.length > 0) sql += ` WHERE ${where.join(" AND ")}`;
      await exec(sql);
    }
  }

  return { rulesWithoutSupport };
}


async function evalAcyclicRel(exec: Exec, relName: string, rules: readonly Rule[], tables: RelTables): Promise<void> {
  const head = tableOf(tables, relName);
  for (const rule of rules) {
    const select = compileRuleSelect(rule, tables, new Map());
    if (select === null) continue; // bodyless rule: produces nothing (matches lower.ts)
    await exec(`INSERT OR IGNORE INTO ${head.table}(${head.columns.join(", ")}) ${select}`);
  }
}


async function evalRecursiveStratum(
  exec: Exec,
  count: Count,
  memberRels: readonly string[],
  members: ReadonlySet<string>,
  rulesByHead: ReadonlyMap<string, Rule[]>,
  tables: RelTables,
): Promise<void> {
  const stratumRules: Rule[] = memberRels.flatMap((r) => rulesByHead.get(r) ?? []);
  for (const rule of stratumRules) {
    if (rule.headTerms.some((t) => t.kind === "hagg")) throw new AggregateInRecursionError(memberRels);
  }

  const deltaOf = new Map<string, string>();
  for (const relName of memberRels) {
    const delta = `_dl_delta_${relName}`;
    deltaOf.set(relName, delta);
    await createLike(exec, delta, tableOf(tables, relName));
  }

  // Seed: every rule over the full tables; recursive members are still empty, so only
  // the exit rules produce rows. Those rows become the seed delta.
  for (const rule of stratumRules) await insertNewRows(exec, rule, tables, new Map(), deltaOf.get(rule.head)!);
  for (const relName of memberRels) await mergeDeltaIntoFull(exec, count, relName, tables, deltaOf.get(relName)!);

  for (;;) {
    const nextDeltaOf = new Map<string, string>();
    for (const relName of memberRels) {
      const next = `_dl_next_${relName}`;
      nextDeltaOf.set(relName, next);
      await createLike(exec, next, tableOf(tables, relName));
    }

    for (const rule of stratumRules) {
      // Each recursive body position, one at a time, reads its delta; the rest read full.
      const recursivePositions: number[] = [];
      rule.body.forEach((pred, bodyIndex) => {
        if (pred.kind === "rel" && members.has(pred.rel)) recursivePositions.push(bodyIndex);
      });
      for (const bodyIndex of recursivePositions) {
        const pred = rule.body[bodyIndex]!;
        if (pred.kind !== "rel") continue;
        const override = new Map<number, string>([[bodyIndex, deltaOf.get(pred.rel)!]]);
        await insertNewRows(exec, rule, tables, override, nextDeltaOf.get(rule.head)!);
      }
    }

    let grew = false;
    for (const relName of memberRels) {
      const merged = await mergeDeltaIntoFull(exec, count, relName, tables, nextDeltaOf.get(relName)!);
      if (merged > 0) grew = true;
      await exec(`DROP TABLE IF EXISTS ${deltaOf.get(relName)!}`);
      await exec(`ALTER TABLE ${nextDeltaOf.get(relName)!} RENAME TO ${deltaOf.get(relName)!}`);
    }
    if (!grew) break;
  }

  for (const relName of memberRels) await exec(`DROP TABLE IF EXISTS ${deltaOf.get(relName)!}`);
}

/** Rows produced by `rule` (with the given body-position delta overrides) that are NOT
 *  already in the head's full table go into `intoDelta`. EXCEPT vs full = "genuinely new". */
async function insertNewRows(
  exec: Exec,
  rule: Rule,
  tables: RelTables,
  bodyPositionOverrides: ReadonlyMap<number, string>,
  intoDelta: string,
): Promise<void> {
  const head = tableOf(tables, rule.head);
  const select = compileRuleSelect(rule, tables, bodyPositionOverrides);
  if (select === null) return;
  await exec(`INSERT OR IGNORE INTO ${intoDelta}(${head.columns.join(", ")}) ${select} ` +
      `EXCEPT SELECT ${head.columns.join(", ")} FROM ${head.table}`,
  );
}

async function mergeDeltaIntoFull(exec: Exec, count: Count, relName: string, tables: RelTables, delta: string): Promise<number> {
  const full = tableOf(tables, relName);
  const before = await count(full.table);
  await exec(`INSERT OR IGNORE INTO ${full.table}(${full.columns.join(", ")}) SELECT ${full.columns.join(", ")} FROM ${delta}`);
  return (await count(full.table)) - before;
}

async function createLike(exec: Exec, name: string, like: RelTable): Promise<void> {
  await exec(`DROP TABLE IF EXISTS ${name}`);
  await exec(`CREATE TEMP TABLE ${name}(${like.columns.join(", ")}, PRIMARY KEY (${like.columns.join(", ")})) WITHOUT ROWID`);
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

  rule.body.forEach((pred, bodyIndex) => {
    if (pred.kind !== "rel") return;
    const relTable = tableOf(tables, pred.rel);
    const alias = `b${positiveIndex++}`;
    const sourceTable = bodyPositionOverrides.get(bodyIndex) ?? relTable.table;
    fromParts.push(`${sourceTable} ${alias}`);
    positiveSources.push({ alias, rel: pred.rel });
    for (let col = 0; col < pred.args.length; col++) {
      const arg = pred.args[col]!;
      if (arg.kind === "wild") continue;
      const colRef = `${alias}.${relTable.columns[col]}`;
      if (arg.kind === "lit") {
        where.push(`${colRef} = ${sqlLit(arg.value)}`);
      } else {
        const existing = bound.get(arg.name);
        if (existing !== undefined) where.push(`${colRef} = ${existing}`);
        else bound.set(arg.name, colRef);
      }
    }
  });

  if (fromParts.length === 0) return null;

  for (const pred of rule.body) {
    if (pred.kind === "cmp") {
      const lhs = bound.get(pred.lhs.name);
      if (lhs === undefined) continue; // range-restriction should bind it; skip defensively
      where.push(`${lhs} ${sqlCmpOp(pred.op)} ${sqlLit(pred.rhs.value)}`);
    } else if (pred.kind === "notrel") {
      const negTable = tableOf(tables, pred.rel);
      const alias = `n${negIndex++}`;
      const sub: string[] = [];
      for (let col = 0; col < pred.args.length; col++) {
        const arg = pred.args[col]!;
        if (arg.kind === "wild") continue;
        const colRef = `${alias}.${negTable.columns[col]}`;
        if (arg.kind === "lit") {
          sub.push(`${colRef} = ${sqlLit(arg.value)}`);
        } else {
          const existing = bound.get(arg.name);
          // A bound var equi-joins into the anti-check; an unbound var is existential
          // over the negated rel's rows (negation-as-failure), so it adds no condition.
          if (existing !== undefined) sub.push(`${colRef} = ${existing}`);
        }
      }
      where.push(`NOT EXISTS (SELECT 1 FROM ${negTable.table} ${alias}${sub.length > 0 ? ` WHERE ${sub.join(" AND ")}` : ""})`);
    }
  }

  return { bound, fromParts, where, positiveSources };
}

function compileRuleSelect(rule: Rule, tables: RelTables, bodyPositionOverrides: ReadonlyMap<number, string>): string | null {
  const compiled = compileRuleJoin(rule, tables, bodyPositionOverrides);
  if (compiled === null) return null;
  const { bound, fromParts, where } = compiled;

  const hasAgg = rule.headTerms.some((t) => t.kind === "hagg");
  const selectList = rule.headTerms.map((term) => {
    if (term.kind === "hvar") {
      const ref = bound.get(term.name);
      if (ref === undefined) throw new Error(`lowerSql: head var '${term.name}' of rel '${rule.head}' is unbound`);
      return ref;
    }
    const ref = bound.get(term.arg.name);
    if (ref === undefined) throw new Error(`lowerSql: aggregate arg '${term.arg.name}' of rel '${rule.head}' is unbound`);
    return `${sqlAgg(term.fn)}(${ref})`;
  });

  let sql = `SELECT ${selectList.join(", ")} FROM ${fromParts.join(", ")}`;
  if (where.length > 0) sql += ` WHERE ${where.join(" AND ")}`;
  if (hasAgg) {
    const groupRefs = rule.headTerms
      .filter((t): t is Extract<typeof t, { kind: "hvar" }> => t.kind === "hvar")
      .map((t) => bound.get(t.name)!);
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
  const t = tables.get(relName);
  if (t === undefined) throw new Error(`lowerSql: no table registered for rel '${relName}'`);
  return t;
}

/** Every statement this file builds is a SINGLE statement, so it goes through
 *  `db.execute`, not `executeMultiple`. That is not a style choice: the local-sqlite3
 *  adapter's `executeMultiple` carries a `finally { if (db.inTransaction) ROLLBACK }`
 *  guard (engine.ts header, sqlite3.js:161-172), so an `executeMultiple` inside an open
 *  `with_txn` bracket would silently kill the bracket. dl's tick runs the whole fixpoint
 *  inside one BEGIN IMMEDIATE, so this file obeys the same law cascade's `exec` does. */
type Exec = (sql: string) => Promise<void>;
type Count = (table: string) => Promise<number>;

/** Single statements only: `executeMultiple` trips the adapter's rollback guard, which
 *  would kill the caller's open transaction. `traceStatement` sees exactly what runs. */
function makeExec(db: SqliteDb, traceStatement?: (sql: string) => void): Exec {
  return async (sql) => {
    stmt_counter.incr();
    traceStatement?.(sql);
    await db.execute(sql);
  };
}

function makeCount(db: SqliteDb): Count {
  return async (table) => {
    stmt_counter.incr();
    const res = await db.execute(`SELECT count(*) FROM ${table}`);
    return Number(res.rows[0]?.[0] ?? 0);
  };
}



export type EvalProgramHolds = AssertTrue<typeof evalProgramSql extends EvalProgram ? true : false>;
