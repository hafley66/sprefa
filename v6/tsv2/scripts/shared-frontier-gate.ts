/** Per-arm parity gate for frontier(shared): identical ticks and finals plus
 *  pinned statement counts and a SEARCH plan; run via shared-frontier-gate.sh. */

import { concatMap, firstValueFrom, map, of, toArray } from "rxjs";
import type { Observable } from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import { BootRunner } from "../runtime/2_boot.ts";
import type {
  IArrivalBatch,
  IBootStatement,
  IGenProgram,
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

interface IFixtureCase {
  readonly name: string;
  readonly schedule: readonly IArrivalBatch[];
  /** Pinned statements per arm over the whole run; a drift is a defect. */
  readonly expected_statements: { readonly per_rel: number; readonly shared: number };
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
];

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

async function main(): Promise<void> {
  let failed = false;
  for (const fixture_case of CASES) {
    const per_rel = await run_arm(`${fixture_case.name}_per`, fixture_case.schedule);
    const shared = await run_arm(`${fixture_case.name}_shared`, fixture_case.schedule);
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
    const verdict = tick_equal && final_equal && searched && counts_pinned ? "PASS" : "FAIL";
    if (verdict === "FAIL") failed = true;
    console.log(
      `${verdict} ${fixture_case.name} ticks=${tick_equal} final=${final_equal} search=${searched} statements per_rel=${per_rel.statements} shared=${shared.statements} pinned=${counts_pinned}`,
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
