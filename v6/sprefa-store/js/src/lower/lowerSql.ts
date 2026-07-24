/**
 * lowerSql.ts — the datalog least-fixpoint lowered to SQL, evaluated in SQLite.
 *
 * The v5 model (the repo-root Rust engine) brought to the v6 store: a stratified
 * program compiles to INSERT ... SELECT statements over one table per rel, and a
 * recursive stratum runs a SEMI-NAIVE delta loop entirely in the C engine (row sets
 * never enter the JS heap). This is the SQL twin of lower.ts's in-memory rx fixpoint;
 * both are proven equal by tests/lower/lowerSql.test.ts (differential against the
 * in-memory evaluator, which is itself golden against the hand oracle).
 *
 * What it covers, the "basic datalog" bar:
 *   - joins (shared variables), literal selection, wildcards, trailing elision,
 *   - comparison selection (=, !=, <, <=, >, >=),
 *   - STRATIFIED NEGATION (!rel(...)) as NOT EXISTS, safe because strata are
 *     topologically ordered so a negated rel is fully materialized before its reader,
 *   - RECURSION as a semi-naive fixpoint (transitive closure, ancestor, same-generation),
 *   - AGGREGATION (max/min/sum/count) as GROUP BY, in its own stratum.
 *
 * Interning-agnostic: it compiles against table + column NAMES and joins on equality,
 * so it runs identically over the store's interned-INTEGER relbase_* tables (dl) and
 * over plain text tables (the test). sum/min/max are only meaningful on INT-typed
 * columns, where the stored id IS the number; the type system is what keeps them off
 * text columns, so the compiler needs no special case.
 *
 * Stratification and cycle detection are rulegraph.ts's job (buildRuleGraph -> scc ->
 * stratify); this file consumes the strata. A negated rel inside its own SCC is a
 * NonStratifiableError raised there, before any SQL is built; an aggregate head inside
 * a recursive SCC is an AggregateInRecursionError raised here, same timing.
 */

import type { AggFn, Compare, Program, RelDecl, Rule } from "./ast.ts";
import { buildRuleGraph, scc, stratify } from "./rulegraph.ts";
import type { SqliteDb } from "../engine/types.ts";
import { stmt_counter } from "../engine/engine.ts";

// ─────────────────────────────────────────────────────────────────────────────
// Public surface.
// ─────────────────────────────────────────────────────────────────────────────

/** Where a rel's rows live: a SQL table and its column names (positional, matching
 *  the rel's declared columns). EDB tables are pre-populated by the caller; IDB tables
 *  are created (WITHOUT ROWID, PK = all columns) and filled by `evalProgramSql`. */
export interface RelTable {
  readonly table: string;
  readonly columns: readonly string[];
}

export type RelTables = ReadonlyMap<string, RelTable>;

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
export async function evalProgramSql(db: SqliteDb, prog: Program, tables: RelTables): Promise<void> {
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
      await exec(db, `DELETE FROM ${tableOf(tables, decl.name).table}`);
    }
  }

  for (const stratum of strata) {
    if (!stratum.recursive) {
      for (const relName of stratum.rels) {
        const rules = rulesByHead.get(relName);
        if (!rules || rules.length === 0) continue;
        await evalAcyclicRel(db, relName, rules, tables);
      }
      continue;
    }
    await evalRecursiveStratum(db, stratum.rels, new Set(stratum.rels), rulesByHead, tables);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Acyclic rel: one INSERT OR IGNORE per rule (multiple rules = UNION by dedup).
// ─────────────────────────────────────────────────────────────────────────────

async function evalAcyclicRel(db: SqliteDb, relName: string, rules: readonly Rule[], tables: RelTables): Promise<void> {
  const head = tableOf(tables, relName);
  for (const rule of rules) {
    const select = compileRuleSelect(rule, tables, new Map());
    if (select === null) continue; // bodyless rule: produces nothing (matches lower.ts)
    await exec(db, `INSERT OR IGNORE INTO ${head.table}(${head.columns.join(", ")}) ${select}`);
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// Recursive stratum: semi-naive delta loop. Seed with each rule over the full tables,
// keeping the rows new to each head as its delta; then each round re-runs every rule
// with ONE recursive body atom bound to its delta (others to full), accumulating the
// next delta, until every delta is empty.
// ─────────────────────────────────────────────────────────────────────────────

async function evalRecursiveStratum(
  db: SqliteDb,
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
    await createLike(db, delta, tableOf(tables, relName));
  }

  // Seed: every rule over the full tables; recursive members are still empty, so only
  // the exit rules produce rows. Those rows become the seed delta.
  for (const rule of stratumRules) await insertNewRows(db, rule, tables, new Map(), deltaOf.get(rule.head)!);
  for (const relName of memberRels) await mergeDeltaIntoFull(db, relName, tables, deltaOf.get(relName)!);

  for (;;) {
    const nextDeltaOf = new Map<string, string>();
    for (const relName of memberRels) {
      const next = `_dl_next_${relName}`;
      nextDeltaOf.set(relName, next);
      await createLike(db, next, tableOf(tables, relName));
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
        await insertNewRows(db, rule, tables, override, nextDeltaOf.get(rule.head)!);
      }
    }

    let grew = false;
    for (const relName of memberRels) {
      const merged = await mergeDeltaIntoFull(db, relName, tables, nextDeltaOf.get(relName)!);
      if (merged > 0) grew = true;
      await exec(db, `DROP TABLE IF EXISTS ${deltaOf.get(relName)!}`);
      await exec(db, `ALTER TABLE ${nextDeltaOf.get(relName)!} RENAME TO ${deltaOf.get(relName)!}`);
    }
    if (!grew) break;
  }

  for (const relName of memberRels) await exec(db, `DROP TABLE IF EXISTS ${deltaOf.get(relName)!}`);
}

/** Rows produced by `rule` (with the given body-position delta overrides) that are NOT
 *  already in the head's full table go into `intoDelta`. EXCEPT vs full = "genuinely new". */
async function insertNewRows(
  db: SqliteDb,
  rule: Rule,
  tables: RelTables,
  bodyPositionOverrides: ReadonlyMap<number, string>,
  intoDelta: string,
): Promise<void> {
  const head = tableOf(tables, rule.head);
  const select = compileRuleSelect(rule, tables, bodyPositionOverrides);
  if (select === null) return;
  await exec(
    db,
    `INSERT OR IGNORE INTO ${intoDelta}(${head.columns.join(", ")}) ${select} ` +
      `EXCEPT SELECT ${head.columns.join(", ")} FROM ${head.table}`,
  );
}

async function mergeDeltaIntoFull(db: SqliteDb, relName: string, tables: RelTables, delta: string): Promise<number> {
  const full = tableOf(tables, relName);
  const before = await count(db, full.table);
  await exec(db, `INSERT OR IGNORE INTO ${full.table}(${full.columns.join(", ")}) SELECT ${full.columns.join(", ")} FROM ${delta}`);
  return (await count(db, full.table)) - before;
}

async function createLike(db: SqliteDb, name: string, like: RelTable): Promise<void> {
  await exec(db, `DROP TABLE IF EXISTS ${name}`);
  await exec(db, `CREATE TEMP TABLE ${name}(${like.columns.join(", ")}, PRIMARY KEY (${like.columns.join(", ")})) WITHOUT ROWID`);
}

// ─────────────────────────────────────────────────────────────────────────────
// Rule -> SELECT. FROM has one alias per positive rel occurrence (b0, b1, ...); the
// first occurrence of a var binds it, later occurrences and Compare/NegRelRef read the
// binding. `bodyPositionOverrides` maps a rule.body index (a positive rel pred) to a
// replacement source table (its delta) for semi-naive rounds. Returns null for a rule
// with no positive body rel (degenerate).
// ─────────────────────────────────────────────────────────────────────────────

function compileRuleSelect(rule: Rule, tables: RelTables, bodyPositionOverrides: ReadonlyMap<number, string>): string | null {
  const bound = new Map<string, string>(); // var name -> "alias.column"
  const fromParts: string[] = [];
  const where: string[] = [];

  let positiveIndex = 0;
  let negIndex = 0;

  rule.body.forEach((pred, bodyIndex) => {
    if (pred.kind !== "rel") return;
    const relTable = tableOf(tables, pred.rel);
    const alias = `b${positiveIndex++}`;
    const sourceTable = bodyPositionOverrides.get(bodyIndex) ?? relTable.table;
    fromParts.push(`${sourceTable} ${alias}`);
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
  }
  return sql;
}

// ─────────────────────────────────────────────────────────────────────────────
// SQL literal / operator helpers.
// ─────────────────────────────────────────────────────────────────────────────

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

async function exec(db: SqliteDb, sql: string): Promise<void> {
  stmt_counter.incr();
  await db.executeMultiple(sql);
}

async function count(db: SqliteDb, table: string): Promise<number> {
  stmt_counter.incr();
  const res = await db.execute(`SELECT count(*) FROM ${table}`);
  return Number(res.rows[0]?.[0] ?? 0);
}
