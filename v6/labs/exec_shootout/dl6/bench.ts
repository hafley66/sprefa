// dl6 core benchmark, instrumented: the contract numbers plus the on-disk
// trail that says WHERE they went, per SQL shape and per emitted rule.

import diagnostics_channel from "node:diagnostics_channel";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { EMPTY, concatMap, expand, last, map, of, tap, toArray, type Observable } from "rxjs";

import { ScratchStore } from "./runtime/scratchStore.ts";
import { BootRunner } from "./runtime/2_boot.ts";
import { TickFold } from "./runtime/tickLoop.ts";
import type {
  IArrivalBatch,
  IGenProgram,
  ISqlRunner,
  ISqlSeam,
  QueryResult,
  SqlStatement,
} from "./runtime/types.ts";

interface IShapeCost {
  readonly shape: string;
  calls: number;
  ms: number;
  rows: number;
}

interface IRuleCost {
  readonly rule: string;
  calls: number;
  ms: number;
  rows: number;
}

interface ICaseFacts {
  readonly name: string;
  readonly edges: number;
  readonly derived: number;
  readonly checksum: string;
  readonly loadedMs: number;
  readonly fixpointMs: number;
  readonly peakRssKb: number;
  readonly statements: number;
  readonly shapes: readonly IShapeCost[];
  readonly rules: readonly IRuleCost[];
}

const OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV_PRIME = 0x00000100000001b3n;
const MASK_64 = 0xffffffffffffffffn;
const CHECKSUM_PAGE_ROWS = 250_000;
// Runs each batched statement on its own so cost lands per statement. This
// drops the batch's atomicity, so it is a diagnostic and never the default.
const UNBATCH = process.env.DL6_BENCH_UNBATCH === "1";

function fnv1a64(source: number, target: number): bigint {
  const bytes = new Uint8Array(8);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, source, true);
  view.setUint32(4, target, true);
  let hash = OFFSET_BASIS;
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = (hash * FNV_PRIME) & MASK_64;
  }
  return hash;
}

/** The emitted SQL with its literals and whitespace normalized away, so 89
 *  executions of one statement aggregate into one row of the report. */
function sqlShape(sql: string): string {
  return sql
    .replace(/\s+/g, " ")
    .replace(/'[^']*'/g, "'?'")
    .trim()
    .slice(0, 150);
}

/** Wraps the seam's runner and records wall time per SQL shape. The wrapper is
 *  the only instrument: no emitted module and no runtime file knows it exists. */
function recordingRunner(inner: ISqlRunner, shapes: Map<string, IShapeCost>): ISqlRunner {
  const note = (sql: string, startedAt: number, rows: number): void => {
    const shape = sqlShape(sql);
    const existing = shapes.get(shape) ?? { shape, calls: 0, ms: 0, rows: 0 };
    existing.calls += 1;
    existing.ms += performance.now() - startedAt;
    existing.rows += rows;
    shapes.set(shape, existing);
  };
  return {
    execute(db, statement, trace) {
      const startedAt = performance.now();
      return inner.execute(db, statement, trace).pipe(
        tap((result: QueryResult) => note(statementSql(statement), startedAt, result.rows.length)),
      );
    },
    scalar(db, statement, trace) {
      const startedAt = performance.now();
      return inner.scalar(db, statement, trace).pipe(
        tap(() => note(statementSql(statement), startedAt, 1)),
      );
    },
    executeMultiple(db, sql, trace) {
      const startedAt = performance.now();
      return inner.executeMultiple(db, sql, trace).pipe(tap(() => note(sql, startedAt, 0)));
    },
    batch(db, statements, trace) {
      if (UNBATCH) {
        return statements
          .reduce(
            (work, statement) =>
              work.pipe(
                concatMap((carried) => {
                  const startedAt = performance.now();
                  return inner.execute(db, statement, trace).pipe(
                    map((result) => {
                      note(statementSql(statement), startedAt, result.rows.length);
                      return [...carried, result];
                    }),
                  );
                }),
              ),
            of([] as QueryResult[]),
          );
      }
      const startedAt = performance.now();
      return inner.batch(db, statements, trace).pipe(
        tap((results: QueryResult[]) => {
          const rows = results.reduce((total, result) => total + result.rows.length, 0);
          note(batchShape(statements), startedAt, rows);
        }),
      );
    },
    inTransaction(db, body) {
      return inner.inTransaction(db, body);
    },
  };
}

function statementSql(statement: SqlStatement | string): string {
  return typeof statement === "string" ? statement : statement.sql;
}

/** One atomic batch is one timing. Splitting its wall time across its members
 *  would invent per-statement numbers the driver never measured. */
function batchShape(statements: readonly SqlStatement[]): string {
  const verbs = statements.map((statement) => {
    const match = /^(\w+)(?:\s+(?:OR\s+IGNORE\s+)?(?:INTO|FROM)?\s*)("[^"]+")?/.exec(statement.sql.trim());
    return `${match?.[1] ?? "?"} ${match?.[2] ?? ""}`.trim();
  });
  return `BATCH x${statements.length}: ${verbs.join(" | ")}`;
}

function parseInput(path: string): readonly (readonly [number, number])[] {
  const edges: [number, number][] = [];
  for (const raw of readFileSync(path, "utf8").split("\n")) {
    const line = raw.trim();
    if (line.length === 0 || line.startsWith("p ")) continue;
    const [source, target] = line.split(/\s+/).map(Number);
    edges.push([source!, target!]);
  }
  return edges;
}

function foldChecksum(seam: ISqlSeam): Observable<{ rows: number; checksum: bigint }> {
  type Page = { rows: number; checksum: bigint; source: number; target: number; done: boolean };
  const readPage = (carried: Page): Observable<Page> =>
    seam.runner
      .execute(seam.db, {
        sql: `SELECT "source", "target" FROM "reachable" WHERE ("source", "target") > (?, ?) ORDER BY "source", "target" LIMIT ?`,
        args: [carried.source, carried.target, CHECKSUM_PAGE_ROWS],
      })
      .pipe(
        map((result): Page => {
          let checksum = carried.checksum;
          let source = carried.source;
          let target = carried.target;
          for (const row of result.rows) {
            source = Number(row.source);
            target = Number(row.target);
            checksum ^= fnv1a64(source, target);
          }
          return {
            rows: carried.rows + result.rows.length,
            checksum,
            source,
            target,
            done: result.rows.length < CHECKSUM_PAGE_ROWS,
          };
        }),
      );
  const seed: Page = { rows: 0, checksum: 0n, source: -1, target: -1, done: false };
  return readPage(seed).pipe(
    expand((page) => (page.done ? EMPTY : readPage(page))),
    last(),
    map((page) => ({ rows: page.rows, checksum: page.checksum })),
  );
}

function runCase(name: string, inputPath: string, modulePath: string): Promise<ICaseFacts> {
  const shapes = new Map<string, IShapeCost>();
  const rules = new Map<string, IRuleCost>();
  const onRule = (event: unknown): void => {
    const { rule, rows, wall_ms: wallMs } = event as { rule: string; rows: number; wall_ms: number };
    const existing = rules.get(rule) ?? { rule, calls: 0, ms: 0, rows: 0 };
    existing.calls += 1;
    existing.ms += wallMs;
    existing.rows += rows;
    rules.set(rule, existing);
  };
  diagnostics_channel.channel("sprefa:rule").subscribe(onRule);

  const loadedStart = performance.now();
  const edges = parseInput(inputPath);
  const opened = ScratchStore.open(":memory:");
  const seam: ISqlSeam = {
    ...opened,
    runner: recordingRunner(opened.runner, shapes),
    unreadRels: new Set(["edge", "reachable"]),
  };

  return import(resolve(modulePath)).then(
    (loaded: { program: IGenProgram }) =>
      new Promise<ICaseFacts>((settle, fail) => {
        const program = loaded.program;
        let loadedMs = 0;
        let fixpointMs = 0;
        let tickShapes: readonly IShapeCost[] = [];
        let tickStatements = 0;
        ScratchStore.boot(seam, program.ddl)
          .pipe(
            concatMap(() => BootRunner.run(seam, program.boot)),
            concatMap(() => {
              loadedMs = Math.round(performance.now() - loadedStart);
              const fixpointStart = performance.now();
              shapes.clear();
              return TickFold.run(
                program,
                seam,
                [edges.map(([source, target]) => ({ rel: "edge", sign: "add" as const, row: [source, target] })) as IArrivalBatch],
                1_000_000,
              ).pipe(
                toArray(),
                map(() => {
                  fixpointMs = Math.round(performance.now() - fixpointStart);
                }),
              );
            }),
            concatMap(() => {
              tickShapes = [...shapes.values()].sort((left, right) => right.ms - left.ms);
              tickStatements = tickShapes.reduce((total, shape) => total + shape.calls, 0);
              return foldChecksum(seam);
            }),
          )
          .subscribe({
            next: ({ rows, checksum }) => {
              diagnostics_channel.channel("sprefa:rule").unsubscribe(onRule);
              settle({
                name,
                edges: edges.length,
                derived: rows,
                checksum: checksum.toString(16).padStart(16, "0"),
                loadedMs,
                fixpointMs,
                peakRssKb: process.resourceUsage().maxRSS,
                statements: tickStatements,
                shapes: tickShapes,
                rules: [...rules.values()].sort((left, right) => right.ms - left.ms),
              });
            },
            error: fail,
          });
      }),
  );
}

function report(facts: readonly ICaseFacts[]): string {
  const lines: string[] = [];
  lines.push("# dl6 core benchmark facts");
  lines.push("");
  lines.push("Regenerate: `just dl6-bench` from `v6/`. One run per case, no best-of, so");
  lines.push("treat single-digit percent moves as noise.");
  lines.push("");
  lines.push("| env | effect |");
  lines.push("|---|---|");
  lines.push("| `DL6_BENCH_FULL=1` | adds `layered_10000` and `chain_10000`, about 35s more |");
  lines.push("| `DL6_BENCH_UNBATCH=1` | runs batched statements one at a time so cost lands per statement; drops atomicity, so totals read high |");
  lines.push("| `DL6_BENCH_WORK=<dir>` | where generated inputs live, default `dl6/.bench` |");
  if (UNBATCH) {
    lines.push("");
    lines.push("**Ran with DL6_BENCH_UNBATCH=1**, so per-statement cost is attributed and the total reads high against a real tick.");
  }
  lines.push("");
  lines.push(`Node ${process.version} · ${process.platform}/${process.arch} · ${new Date().toISOString()}`);
  lines.push("");
  lines.push("## Contract numbers");
  lines.push("");
  lines.push("| case | edges | derived | checksum | load ms | fixpoint ms | rows/sec | peak RSS |");
  lines.push("|---|---|---|---|---|---|---|---|");
  for (const one of facts) {
    const perSecond = Math.round(one.derived / Math.max(one.fixpointMs / 1000, 0.001));
    lines.push(
      `| \`${one.name}\` | ${one.edges.toLocaleString()} | ${one.derived.toLocaleString()} | \`${one.checksum}\` | ${one.loadedMs} | ${one.fixpointMs} | ${perSecond.toLocaleString()} | ${Math.round(one.peakRssKb / 1024)} MB |`,
    );
  }
  for (const one of facts) {
    lines.push("");
    lines.push(`## \`${one.name}\`: where the fixpoint went`);
    lines.push("");
    lines.push(`${one.statements} statements ran during the tick.`);
    lines.push("");
    lines.push("| ms | share | calls | rows out | SQL shape |");
    lines.push("|---|---|---|---|---|");
    const total = one.shapes.reduce((sum, shape) => sum + shape.ms, 0) || 1;
    for (const shape of one.shapes.slice(0, 12)) {
      const share = ((shape.ms / total) * 100).toFixed(1);
      lines.push(
        `| ${shape.ms.toFixed(1)} | ${share}% | ${shape.calls} | ${shape.rows.toLocaleString()} | \`${shape.shape.replaceAll("|", "\\|")}\` |`,
      );
    }
    if (one.rules.length > 0) {
      lines.push("");
      lines.push("| ms | calls | rows | emitted rule |");
      lines.push("|---|---|---|---|");
      for (const rule of one.rules) {
        lines.push(`| ${rule.ms.toFixed(1)} | ${rule.calls} | ${rule.rows.toLocaleString()} | \`${rule.rule}\` |`);
      }
    }
  }
  lines.push("");
  return lines.join("\n");
}

async function main(): Promise<void> {
  const modulePath = process.argv[2];
  const cases = process.argv.slice(3);
  if (modulePath === undefined || cases.length === 0) {
    process.stderr.write("usage: bench.ts <compiled.ts> <name=path>...\n");
    process.exitCode = 2;
    return;
  }
  const facts: ICaseFacts[] = [];
  for (const spec of cases) {
    const [name, inputPath] = spec.split("=");
    process.stderr.write(`dl6-bench: ${name}\n`);
    facts.push(await runCase(name!, inputPath!, modulePath));
  }
  process.stdout.write(report(facts));
  const jsonPath = process.env.DL6_BENCH_JSON;
  if (jsonPath !== undefined) {
    writeFileSync(
      jsonPath,
      `${JSON.stringify({ node: process.version, platform: `${process.platform}/${process.arch}`, at: new Date().toISOString(), unbatched: UNBATCH, cases: facts }, null, 2)}\n`,
    );
    process.stderr.write(`dl6-bench: wrote ${jsonPath}\n`);
  }
}

await main();
