// dl6 shootout driver: the REAL v6 pipeline (prolog->TS+SQLite) on the
// reachability workload, speaking the exec_shootout CLI contract.

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { EMPTY, concatMap, expand, last, map, of, tap, toArray, type Observable } from "rxjs";

import { BootRunner } from "./runtime/2_boot.ts";
import { ScratchStore } from "./runtime/scratchStore.ts";
import { TickFold } from "./runtime/tickLoop.ts";
import type { IArrivalBatch, IGenProgram, ISqlSeam, ITickLogLine } from "./runtime/types.ts";

interface IEdge {
  readonly source: number;
  readonly target: number;
}

interface IProgram {
  readonly ddl: readonly string[];
  readonly boot: readonly { readonly sql: string; readonly params: readonly (string | number | bigint)[] }[];
  readonly finalSelect: Readonly<Record<string, string>>;
  readonly relColumns: Readonly<Record<string, readonly string[]>>;
  readonly tick: (seam: ISqlSeam, arrivals: IArrivalBatch) => Observable<ITickDeltasLike>;
}

// Deltas shape used only to name the tick return; carried but not inspected.
interface ITickDeltasLike {
  readonly carryPending: boolean;
}

const OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV_PRIME = 0x00000100000001b3n;
const MASK_64 = 0xffffffffffffffffn;

function fnv1a64(source: number, target: number): bigint {
  const bytes = new Uint8Array(8);
  new DataView(bytes.buffer).setUint32(0, source, true);
  new DataView(bytes.buffer).setUint32(4, target, true);
  let hash = OFFSET_BASIS;
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = (hash * FNV_PRIME) & MASK_64;
  }
  return hash;
}

function parseInput(path: string): readonly IEdge[] {
  const edges: IEdge[] = [];
  for (const raw of readFileSync(path, "utf8").split("\n")) {
    const line = raw.trim();
    if (line.length === 0) continue;
    const parts = line.split(/\s+/);
    if (parts[0] === "p") continue;
    const source = Number.parseInt(parts[0]!, 10);
    const target = Number.parseInt(parts[1]!, 10);
    if (Number.isNaN(source) || Number.isNaN(target)) throw new Error(`bad edge line: ${line}`);
    edges.push({ source, target });
  }
  return edges;
}

function arrivalBatch(edges: readonly IEdge[]): IArrivalBatch {
  return edges.map((edge) => ({ rel: "edge", sign: "add" as const, row: [edge.source, edge.target] }));
}

const CHECKSUM_PAGE_ROWS = 250_000;

// Materializing the whole answer costs about 140 bytes per row in V8 and
// exceeds the heap at 10M rows, so each page is folded and dropped.
function foldChecksum(seam: ISqlSeam, table: string): Observable<{ rows: number; checksum: bigint }> {
  type Page = { rows: number; checksum: bigint; lastSource: number; lastTarget: number; done: boolean };
  const readPage = (source: number, target: number, carried: Page): Observable<Page> =>
    seam.runner
      .execute(seam.db, {
        sql: `SELECT "source", "target" FROM ${table} WHERE ("source", "target") > (?, ?) ORDER BY "source", "target" LIMIT ?`,
        args: [source, target, CHECKSUM_PAGE_ROWS],
      })
      .pipe(
        map((result): Page => {
          let checksum = carried.checksum;
          let lastSource = source;
          let lastTarget = target;
          for (const row of result.rows) {
            lastSource = Number(row.source);
            lastTarget = Number(row.target);
            checksum ^= fnv1a64(lastSource, lastTarget);
          }
          return {
            rows: carried.rows + result.rows.length,
            checksum,
            lastSource,
            lastTarget,
            done: result.rows.length < CHECKSUM_PAGE_ROWS,
          };
        }),
      );
  const seed: Page = { rows: 0, checksum: 0n, lastSource: -1, lastTarget: -1, done: false };
  return readPage(-1, -1, seed).pipe(
    expand((page) => (page.done ? EMPTY : readPage(page.lastSource, page.lastTarget, page))),
    last(),
    map((page) => ({ rows: page.rows, checksum: page.checksum })),
  );
}

// Probe reachable count mid-run so a timeout kill still leaves a last count.
function scheduleProbe(seam: ISqlSeam, everyLines: number): (line: ITickLogLine) => void {
  let seen = 0;
  return (line: ITickLogLine): void => {
    seen += 1;
    if (seen % everyLines !== 0) return;
    seam.runner.scalar(seam.db, `SELECT count(*) FROM "reachable"`).subscribe((count) => {
      process.stderr.write(`dl6: tick ${line.tick}/n=${seen} reachable=${count}\n`);
    });
  };
}

function parseArgs(
  argv: readonly string[],
): { input: string; module: string; extraDdl: string | undefined } | null {
  const flags = new Map<string, string>();
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (key === undefined || value === undefined || !key.startsWith("--")) return null;
    flags.set(key.slice(2), value);
  }
  const input = flags.get("input");
  const module = flags.get("module");
  if (input === undefined || module === undefined) return null;
  return { input, module, extraDdl: flags.get("extra-ddl") };
}

// A/B lever for schema experiments: statements appended after the emitted DDL,
// one per line, so an index can be priced without regenerating the module.
function extraDdlStatements(path: string | undefined): readonly string[] {
  if (path === undefined) return [];
  return readFileSync(path, "utf8")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

function main(): void {
  const args = parseArgs(process.argv.slice(2));
  if (args === null) {
    process.stderr.write("usage: run.ts --input <path> --module <compiled.ts>\n");
    process.exitCode = 2;
    return;
  }
  // libuv normalizes getrusage ru_maxrss to KiB on every platform, macOS included.
  const rssKb = (): number => process.resourceUsage().maxRSS;

  const loadedStart = performance.now();
  const edges = parseInput(args.input);
  // This driver reads the final table, never the per-tick deltas, so the
  // boundary rows would become JS objects only to be discarded.
  const opened = ScratchStore.open(":memory:");
  const seam: ISqlSeam = { ...opened, unreadRels: new Set(["edge", "reachable"]) };

  import(resolve(args.module))
    .then((loaded: { program: IProgram }) => {
      const program = loaded.program;
      const bootDdl = [...program.ddl, ...extraDdlStatements(args.extraDdl)];
      ScratchStore.boot(seam, bootDdl)
        .pipe(
          concatMap(() => BootRunner.run(seam, program.boot)),
          map(() => {
            const loadedMs = Math.round(performance.now() - loadedStart);
            process.stdout.write(`{"event":"loaded","edges":${edges.length},"ms":${loadedMs}}\n`);
          }),
          concatMap(() => {
            const fixpointStart = performance.now();
            const probe = scheduleProbe(seam, 100);
            return TickFold.run(
              program as IGenProgram,
              seam,
              [arrivalBatch(edges)],
              1_000_000,
            ).pipe(
              tap(probe),
              toArray(),
              map(() => {
                const fixpointMs = Math.round(performance.now() - fixpointStart);
                process.stderr.write(`dl6: tick settled in ${fixpointMs}ms, reading final table\n`);
                const finalColumns = program.relColumns["reachable"];
                return { fixpointMs, finalColumns };
              }),
            );
          }),
          concatMap(({ fixpointMs }) =>
            foldChecksum(seam, `"reachable"`).pipe(
              map(({ rows, checksum }) => ({
                fixpointMs,
                rows,
                checksum: checksum.toString(16).padStart(16, "0"),
              })),
            ),
          ),
          map(({ fixpointMs, rows, checksum }) => {
            process.stdout.write(`{"event":"fixpoint","derived":${rows},"ms":${fixpointMs}}\n`);
            process.stderr.write(`dl6: fixpoint derived=${rows}\n`);
            process.stdout.write(
              `{"event":"done","checksum":"${checksum}","peak_rss_kb":${rssKb()}}\n`,
            );
          }),
        )
        .subscribe({
          error: (error: unknown) => {
            process.stderr.write(`${error instanceof Error ? (error.stack ?? error.message) : String(error)}\n`);
            process.exitCode = 1;
          },
        });
    })
    .catch((error: unknown) => {
      process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
      process.exitCode = 1;
    });
}

main();
