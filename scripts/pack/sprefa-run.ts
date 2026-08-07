// A compiled .dl6 module imports "rxjs" and five "../runtime/*.ts" siblings by
// NAME; outside the repo the hook below serves both from the inlined bundle.

import { readFileSync, writeFileSync } from "node:fs";
import { registerHooks } from "node:module";
import { resolve as resolvePath } from "node:path";
import { pathToFileURL } from "node:url";

import * as rxjsNamespace from "rxjs";
import { EMPTY, concatMap, expand, last, map, of, tap, toArray, type Observable } from "rxjs";

import * as runtimeIncremental from "../../v6/tsv2/runtime/1_incremental.ts";
import * as runtimeSubscribe from "../../v6/tsv2/runtime/3_subscribe.ts";
import * as runtimeDiff from "../../v6/tsv2/runtime/diff.ts";
import * as runtimeRows from "../../v6/tsv2/runtime/rows.ts";
import * as runtimeStructPlane from "../../v6/tsv2/runtime/structPlane.ts";

import { BootRunner } from "../../v6/tsv2/runtime/2_boot.ts";
import { ScratchStore } from "../../v6/tsv2/runtime/scratchStore.ts";
import { TickFold } from "../../v6/tsv2/runtime/tickLoop.ts";
import type {
  IArrivalBatch,
  IArrivalRow,
  IGenProgram,
  ISqlSeam,
  ITickLogLine,
} from "../../v6/tsv2/runtime/types.ts";

// ── the resolution hook ──────────────────────────────────────────────────────

const RUNTIME_NAMESPACES: Readonly<Record<string, Record<string, unknown>>> = {
  "1_incremental": runtimeIncremental as Record<string, unknown>,
  "3_subscribe": runtimeSubscribe as Record<string, unknown>,
  diff: runtimeDiff as Record<string, unknown>,
  rows: runtimeRows as Record<string, unknown>,
  structPlane: runtimeStructPlane as Record<string, unknown>,
  rxjs: rxjsNamespace as unknown as Record<string, unknown>,
};

const SCHEME = "sprefa-bundled:";
const RUNTIME_SPECIFIER = /(?:^|\/)runtime\/([A-Za-z0-9_]+)\.ts$/;

function bundledKey(specifier: string): string | null {
  if (specifier === "rxjs") return "rxjs";
  const match = RUNTIME_SPECIFIER.exec(specifier);
  if (match === null) return null;
  const key = match[1]!;
  return key in RUNTIME_NAMESPACES ? key : null;
}

// A live namespace cannot be handed to node's loader; only source text can. The
// keys are read off the namespace at load time, so the shim is never hand-kept.
function shimSource(key: string): string {
  const namespace = RUNTIME_NAMESPACES[key]!;
  const lines = [`const bundled = globalThis[${JSON.stringify(GLOBAL_KEY)}][${JSON.stringify(key)}];`];
  for (const name of Object.keys(namespace)) {
    if (name === "default") lines.push(`export default bundled.default;`);
    else lines.push(`export const ${name} = bundled[${JSON.stringify(name)}];`);
  }
  return lines.join("\n");
}

const GLOBAL_KEY = "__sprefaBundledRuntime";

function installResolutionHook(): void {
  (globalThis as Record<string, unknown>)[GLOBAL_KEY] = RUNTIME_NAMESPACES;
  registerHooks({
    resolve(specifier, context, nextResolve) {
      const key = bundledKey(specifier);
      if (key === null) return nextResolve(specifier, context);
      return { url: `${SCHEME}${key}`, shortCircuit: true };
    },
    load(url, context, nextLoad) {
      if (!url.startsWith(SCHEME)) return nextLoad(url, context);
      return { format: "module", source: shimSource(url.slice(SCHEME.length)), shortCircuit: true };
    },
  });
}

// ── the exec_shootout CONTRACT.md checksum ───────────────────────────────────

const OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV_PRIME = 0x00000100000001b3n;
const MASK_64 = 0xffffffffffffffffn;
const CHECKSUM_PAGE_ROWS = 250_000;

function fnv1a64(first: number, second: number): bigint {
  const bytes = new Uint8Array(8);
  const view = new DataView(bytes.buffer);
  view.setUint32(0, first, true);
  view.setUint32(4, second, true);
  let hash = OFFSET_BASIS;
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = (hash * FNV_PRIME) & MASK_64;
  }
  return hash;
}

interface IChecksumPage {
  readonly rows: number;
  readonly checksum: bigint;
  readonly lastFirst: number;
  readonly lastSecond: number;
  readonly done: boolean;
}

// Paging keeps peak heap flat: a 10M-row answer materialized as JS objects is
// about 1.4 GB, so every page is folded into the running xor and dropped.
function foldChecksum(
  seam: ISqlSeam,
  rel: string,
  columns: readonly string[],
): Observable<{ rows: number; checksum: bigint }> {
  const [first, second] = columns;
  if (first === undefined || second === undefined) {
    throw new Error(`--checksum ${rel}: the CONTRACT checksum needs two columns, rel has ${columns.length}`);
  }
  const readPage = (afterFirst: number, afterSecond: number, carried: IChecksumPage): Observable<IChecksumPage> =>
    seam.runner
      .execute(seam.db, {
        sql:
          `SELECT "${first}", "${second}" FROM "${rel}" WHERE ("${first}", "${second}") > (?, ?) ` +
          `ORDER BY "${first}", "${second}" LIMIT ?`,
        args: [afterFirst, afterSecond, CHECKSUM_PAGE_ROWS],
      })
      .pipe(
        map((result): IChecksumPage => {
          let checksum = carried.checksum;
          let lastFirst = afterFirst;
          let lastSecond = afterSecond;
          for (const row of result.rows) {
            lastFirst = Number(row[first as keyof typeof row]);
            lastSecond = Number(row[second as keyof typeof row]);
            checksum ^= fnv1a64(lastFirst, lastSecond);
          }
          return {
            rows: carried.rows + result.rows.length,
            checksum,
            lastFirst,
            lastSecond,
            done: result.rows.length < CHECKSUM_PAGE_ROWS,
          };
        }),
      );
  const seed: IChecksumPage = { rows: 0, checksum: 0n, lastFirst: -1, lastSecond: -1, done: false };
  return readPage(-1, -1, seed).pipe(
    expand((page) => (page.done ? EMPTY : readPage(page.lastFirst, page.lastSecond, page))),
    last(),
    map((page) => ({ rows: page.rows, checksum: page.checksum })),
  );
}

// ── arrivals ─────────────────────────────────────────────────────────────────

// Per line: a `{`-led IArrivalRow JSON object, or a whitespace row for
// --arrival-rel. Blank / `p `-led / `#`-led lines are edge-list header lines.
function readArrivals(path: string, defaultRel: string | undefined): IArrivalBatch {
  const arrivals: IArrivalRow[] = [];
  for (const raw of readFileSync(path, "utf8").split("\n")) {
    const line = raw.trim();
    if (line.length === 0 || line.startsWith("p ") || line.startsWith("#")) continue;
    if (line.startsWith("{")) {
      arrivals.push(JSON.parse(line) as IArrivalRow);
      continue;
    }
    if (defaultRel === undefined) {
      throw new Error(`--arrivals ${path}: a bare row line needs --arrival-rel <rel>`);
    }
    const row = line.split(/\s+/).map((field) => (/^-?\d+$/.test(field) ? Number(field) : field));
    arrivals.push({ rel: defaultRel, sign: "add", row });
  }
  return arrivals;
}

// ── CLI ──────────────────────────────────────────────────────────────────────

interface IOptions {
  readonly module: string;
  readonly arrivals: string | undefined;
  readonly arrivalRel: string | undefined;
  readonly schedule: string | undefined;
  readonly db: string;
  readonly checksum: string | undefined;
  readonly counts: readonly string[];
  readonly drainCap: number;
  readonly tickLog: string | undefined;
}

const USAGE = `usage: sprefa-run --module <compiled.ts> [options]

  --module <path>        a .ts module emitted by dl6c (required)
  --arrivals <path>      arrival rows for ONE tick: JSONL {"rel","sign","row"}
                         objects, or bare whitespace rows for --arrival-rel
  --arrival-rel <rel>    the rel bare arrival rows belong to
  --schedule <path>      a JSON array of arrival batches, one per tick
  --db <url>             ":memory:" (default) or file:/abs/path.sqlite
  --checksum <rel>       exec_shootout CONTRACT fnv1a64 over the rel's first
                         two columns; also reports its row count as "derived"
  --count <rel>          print {"event":"count","rel":..,"rows":..}; repeatable
  --drain-cap <n>        max drain ticks before the fold gives up (default 1e6)
  --tick-log <path>      write the tick log there, "-" for stderr

stdout is JSONL events only. Every other word goes to stderr.`;

function parseOptions(argv: readonly string[]): IOptions | null {
  const flags = new Map<string, string>();
  const counts: string[] = [];
  for (let index = 0; index < argv.length; index += 1) {
    const key = argv[index];
    if (key === undefined || !key.startsWith("--")) return null;
    const value = argv[index + 1];
    if (value === undefined) return null;
    index += 1;
    if (key === "--count") counts.push(value);
    else flags.set(key.slice(2), value);
  }
  const modulePath = flags.get("module");
  if (modulePath === undefined) return null;
  const drainCap = Number(flags.get("drain-cap") ?? 1_000_000);
  return {
    module: modulePath,
    arrivals: flags.get("arrivals"),
    arrivalRel: flags.get("arrival-rel"),
    schedule: flags.get("schedule"),
    db: flags.get("db") ?? ":memory:",
    checksum: flags.get("checksum"),
    counts,
    drainCap: Number.isFinite(drainCap) && drainCap > 0 ? drainCap : 1_000_000,
    tickLog: flags.get("tick-log"),
  };
}

interface ILoadedModule {
  readonly program: IGenProgram & {
    readonly boot: readonly { readonly sql: string; readonly params: readonly (string | number | bigint)[] }[];
  };
}

function readSchedule(options: IOptions): readonly IArrivalBatch[] {
  if (options.schedule !== undefined) {
    return JSON.parse(readFileSync(options.schedule, "utf8")) as readonly IArrivalBatch[];
  }
  if (options.arrivals === undefined) return [];
  return [readArrivals(options.arrivals, options.arrivalRel)];
}

function run(options: IOptions): void {
  installResolutionHook();
  const loadStart = performance.now();
  const schedule = readSchedule(options);
  const arrivalCount = schedule.reduce((total, batch) => total + batch.length, 0);
  const observed = new Set<string>(options.counts);
  if (options.checksum !== undefined) observed.add(options.checksum);
  const logStream = options.tickLog === undefined || options.tickLog === "-" ? null : options.tickLog;
  const opened = ScratchStore.open(options.db);

  import(pathToFileURL(resolvePath(options.module)).href)
    .then((loaded: ILoadedModule) => {
      const program = loaded.program;
      // A rel nobody reads at the boundary skips its per-tick event copies, so
      // asking for the tick log has to widen the observed set to everything.
      const unobservedRels =
        options.tickLog !== undefined
          ? new Set<string>()
          : new Set(Object.keys(program.relColumns).filter((rel) => !observed.has(rel)));
      const seam: ISqlSeam = { ...opened, unobservedRels };
      ScratchStore.boot(seam, program.ddl)
        .pipe(
          concatMap(() => BootRunner.run(seam, program.boot)),
          map(() => {
            const ms = Math.round(performance.now() - loadStart);
            process.stdout.write(`{"event":"loaded","arrivals":${arrivalCount},"ms":${ms}}\n`);
          }),
          concatMap(() => {
            const tickStart = performance.now();
            const collected: string[] = [];
            return TickFold.run(program, seam, schedule, options.drainCap).pipe(
              tap((line: ITickLogLine) => {
                if (options.tickLog === "-") process.stderr.write(`${line}\n`);
                else if (logStream !== null) collected.push(line);
              }),
              toArray(),
              map((lines) => {
                if (logStream !== null) writeFileSync(logStream, collected.map((line) => `${line}\n`).join(""));
                return { ticks: lines.length, ms: Math.round(performance.now() - tickStart) };
              }),
            );
          }),
          concatMap((fixpoint) => {
            if (options.checksum === undefined) {
              process.stdout.write(`{"event":"fixpoint","ticks":${fixpoint.ticks},"ms":${fixpoint.ms}}\n`);
              return of(undefined);
            }
            const columns = program.relColumns[options.checksum];
            if (columns === undefined) throw new Error(`--checksum ${options.checksum}: no such rel`);
            return foldChecksum(seam, options.checksum, columns).pipe(
              map(({ rows, checksum }) => {
                process.stdout.write(
                  `{"event":"fixpoint","derived":${rows},"ticks":${fixpoint.ticks},"ms":${fixpoint.ms}}\n`,
                );
                process.stdout.write(
                  `{"event":"done","checksum":"${checksum.toString(16).padStart(16, "0")}",` +
                    `"peak_rss_kb":${process.resourceUsage().maxRSS}}\n`,
                );
              }),
            );
          }),
          concatMap(() =>
            options.counts.length === 0
              ? of(undefined)
              : of(...options.counts).pipe(
                  concatMap((rel) =>
                    seam.runner.scalar(seam.db, `SELECT count(*) FROM "${rel}"`).pipe(
                      map((rows) => {
                        process.stdout.write(`{"event":"count","rel":${JSON.stringify(rel)},"rows":${rows}}\n`);
                      }),
                    ),
                  ),
                ),
          ),
        )
        .subscribe({
          error: (failure: unknown) => {
            process.stderr.write(`${failure instanceof Error ? (failure.stack ?? failure.message) : String(failure)}\n`);
            process.exitCode = 1;
          },
        });
    })
    .catch((failure: unknown) => {
      process.stderr.write(`${failure instanceof Error ? (failure.stack ?? failure.message) : String(failure)}\n`);
      process.exitCode = 1;
    });
}

const options = parseOptions(process.argv.slice(2));
if (options === null) {
  process.stderr.write(`${USAGE}\n`);
  process.exitCode = 2;
} else {
  run(options);
}
