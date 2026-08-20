/** Per-arm parity gate for frontier(shared): identical ticks and finals plus
 *  pinned statement counts and a SEARCH plan; run via shared-frontier-gate.sh.
 *
 *  The retraction cases add three legs the arrival cases cannot reach: the
 *  ORACLE tick log (conformance/ticklog.pl over the same fixture term), the
 *  shared support ledger's row-by-row agreement with the head refcounts it
 *  feeds, and a RESTART (the same schedule replayed on a fresh database). */

import { readFileSync } from "node:fs";

import { concatMap, firstValueFrom, map, of, toArray } from "rxjs";
import type { Observable } from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import { BootRunner } from "../runtime/2_boot.ts";
import type {
  IArrivalBatch,
  IBootStatement,
  IGenProgram,
  IIncrementalRelationPlan,
  ISqlSeam,
  QueryResult,
  SqliteDb,
  SqlStatement,
  TraceStatement,
} from "../runtime/types.ts";
import type { ISqlRunner } from "sprefa-store-engine/src/engine/types.ts";

type EmittedProgram = IGenProgram & {
  readonly boot: readonly IBootStatement[];
  readonly final_select: Record<string, string>;
};

/** The emitted module's plan export, the one place the relation ids the
 *  shared ledger keys on are readable from a test. */
type EmittedPlan = { readonly relations: readonly IIncrementalRelationPlan[] };

interface IFixtureCase {
  readonly name: string;
  /** Absent for a fixture-term case: the schedule then comes from the
   *  `<name>.schedule.json` the oracle arm reads too. */
  readonly schedule?: readonly IArrivalBatch[];
  /** Pinned statements per arm over the whole run; a drift is a defect. */
  readonly expected_statements: { readonly per_rel: number; readonly shared: number };
  /** Diff both arms against `gen_emitted/<name>.oracle.jsonl`. */
  readonly oracle?: boolean;
  /** Every head row's ledger sum equals its `__refcount`, and the ledger
   *  lookup is a SEARCH. */
  readonly ledger?: boolean;
  /** Replay the whole schedule on a fresh database; finals must match. */
  readonly restart?: boolean;
}

const CASES: readonly IFixtureCase[] = [
  {
    name: "sf_arrivals",
    schedule: [
      [
        { rel: "color", sign: "add", row: ["red", "#f00"] },
        { rel: "tag", sign: "add", row: ["hot", 1] },
      ],
      [
        { rel: "color", sign: "add", row: ["red", "#e00"] },
        { rel: "tag", sign: "del", row: ["hot", 1] },
      ],
      [[{ rel: "color", sign: "del", row: ["red", "#e00"] }][0]!],
    ],
    expected_statements: { per_rel: 60, shared: 48 },
  },
  {
    name: "sf_keyed_replace",
    schedule: [
      [
        { rel: "setting", sign: "add", row: ["depth", 1] },
        { rel: "setting", sign: "add", row: ["width", 2] },
      ],
      [{ rel: "setting", sign: "add", row: ["depth", 9] }],
      [{ rel: "setting", sign: "del", row: ["width", 2] }],
    ],
    expected_statements: { per_rel: 37, shared: 37 },
  },
  {
    name: "sf_join",
    schedule: [
      [
        { rel: "city", sign: "add", row: ["nyc"] },
        { rel: "person", sign: "add", row: ["ann", "nyc"] },
      ],
      [{ rel: "person", sign: "add", row: ["bob", "nyc"] }],
    ],
    expected_statements: { per_rel: 61, shared: 45 },
  },
  {
    name: "sf_guard",
    schedule: [
      [
        { rel: "person", sign: "add", row: ["ann", 30] },
        { rel: "person", sign: "add", row: ["kid", 7] },
      ],
      [{ rel: "person", sign: "add", row: ["bob", 44] }],
    ],
    expected_statements: { per_rel: 46, shared: 38 },
  },
  {
    name: "sf_retract_current",
    expected_statements: { per_rel: 96, shared: 74 },
    oracle: true,
    ledger: true,
    restart: true,
  },
  {
    name: "sf_retract_stale",
    expected_statements: { per_rel: 73, shared: 63 },
    oracle: true,
    ledger: true,
    restart: true,
  },
  {
    name: "sf_negation_support",
    expected_statements: { per_rel: 142, shared: 118 },
    oracle: true,
    ledger: true,
    restart: true,
  },
  {
    name: "sf_two_rule_support",
    expected_statements: { per_rel: 105, shared: 87 },
    oracle: true,
    ledger: true,
    restart: true,
  },
];

function case_schedule(fixture_case: IFixtureCase): readonly IArrivalBatch[] {
  if (fixture_case.schedule !== undefined) return fixture_case.schedule;
  const path = new URL(
    `../tests/shared_frontier/${fixture_case.name}.schedule.json`,
    import.meta.url,
  );
  return JSON.parse(readFileSync(path, "utf8")) as readonly IArrivalBatch[];
}

function oracle_lines(name: string): readonly string[] {
  const path = new URL(`../gen_emitted/${name}.oracle.jsonl`, import.meta.url);
  return readFileSync(path, "utf8").split("\n").filter((line) => line !== "");
}

/** Counts single statements; executeMultiple counts one per `;`-joined leg. */
function counting_runner(inner: ISqlRunner, counter: { count: number }): ISqlRunner {
  return {
    execute(db: SqliteDb, statement: SqlStatement, trace?: TraceStatement): Observable<QueryResult> {
      counter.count += 1;
      return inner.execute(db, statement, trace);
    },
    scalar(db: SqliteDb, statement: SqlStatement, trace?: TraceStatement): Observable<number> {
      counter.count += 1;
      return inner.scalar(db, statement, trace);
    },
    executeMultiple(db: SqliteDb, sql: string, trace?: TraceStatement): Observable<void> {
      counter.count += sql.split(";\n").length;
      return inner.executeMultiple(db, sql, trace);
    },
    batch(db: SqliteDb, statements: readonly SqlStatement[], trace?: TraceStatement): Observable<QueryResult[]> {
      counter.count += statements.length;
      return inner.batch(db, statements, trace);
    },
    inTransaction: inner.inTransaction.bind(inner),
  } as ISqlRunner;
}

function final_lines(seam: ISqlSeam, program: EmittedProgram): Observable<string> {
  const rels = Object.keys(program.final_select).sort();
  return of(...rels).pipe(
    concatMap((rel) =>
      seam.runner
        .execute(seam.db, { sql: program.final_select[rel]!, args: [] })
        .pipe(map((result: QueryResult) => `${rel}=${JSON.stringify(result.rows)}`)),
    ),
    toArray(),
    map((parts: string[]) => parts.join(" ")),
  );
}

interface IArmRun {
  readonly lines: readonly string[];
  readonly final: string;
  readonly statements: number;
  readonly seam: ISqlSeam;
}

async function run_arm(module_name: string, schedule: readonly IArrivalBatch[]): Promise<IArmRun> {
  const specifier = ["..", "gen_emitted", `${module_name}.ts`].join("/");
  const program = (await import(specifier)).program as EmittedProgram;
  const base = ScratchStore.open(":memory:");
  const counter = { count: 0 };
  const seam: ISqlSeam = { db: base.db, runner: counting_runner(base.runner, counter) };
  const run = ScratchStore.boot(base, program.ddl).pipe(
    concatMap(() => BootRunner.run(base, program.boot)),
    concatMap(() => TickFold.run(program, seam, schedule).pipe(toArray())),
    concatMap((lines: string[]) =>
      final_lines(base, program).pipe(map((final) => ({ lines, final }))),
    ),
  );
  const { lines, final } = await firstValueFrom(run);
  return { lines, final, statements: counter.count, seam: base };
}

async function explain_search(seam: ISqlSeam, view_name: string): Promise<string> {
  const result = await firstValueFrom(
    seam.runner.execute(seam.db, {
      sql: `EXPLAIN QUERY PLAN SELECT * FROM "${view_name}" WHERE "_phase" >= 0`,
      args: [],
    }),
  );
  return result.rows.map((row) => Object.values(row).join(" ")).join("\n");
}

interface ILedgerVerdict {
  readonly agrees: boolean;
  readonly searched: boolean;
  readonly rows: number;
  readonly detail: string;
}

/** Every row the shared ledger counts for is a head row whose `__refcount`
 *  is that count summed over its rules, and the lookup reads the ledger's
 *  primary key rather than scanning it. */
async function ledger_verdict(
  seam: ISqlSeam,
  plan: EmittedPlan,
): Promise<ILedgerVerdict> {
  const relations = plan.relations;
  const read = async (sql: string): Promise<QueryResult> =>
    await firstValueFrom(seam.runner.execute(seam.db, { sql, args: [] }));
  const ledger_plan = await read(
    'EXPLAIN QUERY PLAN SELECT "count" FROM "__support_count" WHERE "relation_id" = 0 AND "row_id" = 1',
  );
  const plan_text = ledger_plan.rows.map((row) => Object.values(row).join(" ")).join("\n");
  const searched = plan_text.includes("SEARCH") && !plan_text.includes("SCAN");
  let rows = 0;
  const disagreements: string[] = [];
  for (const relation of relations) {
    const shared = relation.shared_frontier;
    if (shared === undefined) continue;
    const columns = await read(`PRAGMA table_info("${relation.table_name}")`);
    const has_refcount = columns.rows.some((row) => row["name"] === "__refcount");
    if (!has_refcount) continue;
    const counted = await read(
      `SELECT count(*) AS bad FROM "${relation.table_name}" h LEFT JOIN (SELECT "row_id", sum("count") AS ledger FROM "__support_count" WHERE "relation_id" = ${shared.relation_id} GROUP BY "row_id") s ON s."row_id" = h."__id" WHERE COALESCE(s.ledger, 0) <> h."__refcount"`,
    );
    const present = await read(
      `SELECT count(*) AS held FROM "__support_count" WHERE "relation_id" = ${shared.relation_id}`,
    );
    rows += Number(present.rows[0]?.held ?? 0);
    const bad = Number(counted.rows[0]?.bad ?? 0);
    if (bad !== 0) disagreements.push(`${relation.table_name} rows=${bad}`);
  }
  return {
    agrees: disagreements.length === 0,
    searched,
    rows,
    detail: disagreements.join(" "),
  };
}

async function main(): Promise<void> {
  let failed = false;
  for (const fixture_case of CASES) {
    const schedule = case_schedule(fixture_case);
    const per_rel = await run_arm(`${fixture_case.name}_per`, schedule);
    const shared = await run_arm(`${fixture_case.name}_shared`, schedule);
    const tick_equal =
      per_rel.lines.length === shared.lines.length &&
      per_rel.lines.every((line, index) => line === shared.lines[index]);
    const final_equal = per_rel.final === shared.final;
    const shared_module = (await import(`../gen_emitted/${fixture_case.name}_shared.ts`)).program as EmittedProgram & {
      readonly rel_physical_names?: Record<string, string>;
    };
    const first_rel = Object.keys(shared_module.final_select).sort()[0]!;
    const physical = shared_module.rel_physical_names?.[first_rel] ?? first_rel;
    const plan_text = await explain_search(shared.seam, `__frontier_${physical}`);
    const searched = plan_text.includes("SEARCH") && !plan_text.includes('SCAN __frontier"');
    const counts_pinned =
      fixture_case.expected_statements.per_rel === 0 ||
      (per_rel.statements === fixture_case.expected_statements.per_rel &&
        shared.statements === fixture_case.expected_statements.shared);
    const legs = [`ticks=${tick_equal}`, `final=${final_equal}`, `search=${searched}`];
    let ok = tick_equal && final_equal && searched && counts_pinned;

    if (fixture_case.oracle === true) {
      const oracle = oracle_lines(fixture_case.name);
      const oracle_equal =
        oracle.length === shared.lines.length &&
        oracle.every((line, index) => line === shared.lines[index]);
      legs.push(`oracle=${oracle_equal}`);
      ok = ok && oracle_equal;
      if (!oracle_equal) {
        console.log(`  oracle lines:\n${oracle.join("\n")}\n  shared lines:\n${shared.lines.join("\n")}`);
      }
    }

    if (fixture_case.ledger === true) {
      const shared_plan = (await import(`../gen_emitted/${fixture_case.name}_shared.ts`))
        .incremental_plan as EmittedPlan;
      const ledger = await ledger_verdict(shared.seam, shared_plan);
      legs.push(`ledger=${ledger.agrees}`, `ledger_rows=${ledger.rows}`, `ledger_search=${ledger.searched}`);
      ok = ok && ledger.agrees && ledger.searched && ledger.rows > 0;
      if (!ledger.agrees) console.log(`  ledger disagrees: ${ledger.detail}`);
    }

    if (fixture_case.restart === true) {
      const replay = await run_arm(`${fixture_case.name}_shared`, schedule);
      const restart_equal = replay.final === shared.final &&
        replay.lines.every((line, index) => line === shared.lines[index]);
      legs.push(`restart=${restart_equal}`);
      ok = ok && restart_equal;
      if (!restart_equal) console.log(`  restart final=${replay.final} first=${replay.final}`);
    }

    const verdict = ok ? "PASS" : "FAIL";
    if (!ok) failed = true;
    console.log(
      `${verdict} ${fixture_case.name} ${legs.join(" ")} statements per_rel=${per_rel.statements} shared=${shared.statements} pinned=${counts_pinned}`,
    );
    if (!tick_equal) {
      const first = per_rel.lines.findIndex((line, index) => line !== shared.lines[index]);
      console.log(`  first tick diff at ${first}: per_rel=${per_rel.lines[first]} shared=${shared.lines[first]}`);
    }
    if (!final_equal) console.log(`  final per_rel=${per_rel.final}\n  final shared=${shared.final}`);
    if (!searched) console.log(`  plan:\n${plan_text}`);
  }
  process.exit(failed ? 1 : 0);
}

main().catch((error: unknown) => {
  console.log(`GATE CRASH: ${String(error)}`);
  process.exit(1);
});
