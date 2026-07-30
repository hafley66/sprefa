/**
 * Deterministic scale harness for the V6.2 file-watch path.
 *
 * The compiler, database boot, fixture writes, and final receipts sit outside
 * the measured region. Each measured phase starts with real bytes already on
 * disk, pushes duplicate path notifications through the real WatchBindRunner,
 * closes one virtual coalesce window, and waits for the real LiveEngine tick.
 *
 * Example:
 *   node --experimental-transform-types scripts/5_file-watch-scale.ts \
 *     --files 100,1000 --events-per-file 3 \
 *     --edit-ratio 0.25 --delete-ratio 0.10 \
 *     --identical-resave-ratio 0.25 --jsonl /tmp/watch.jsonl
 */

import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, relative } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";

import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex } from "@noble/hashes/utils.js";
import {
  Observable,
  Subject,
  VirtualTimeScheduler,
  finalize,
  firstValueFrom,
  tap,
} from "rxjs";
import { stmt_counter } from "sprefa-store-engine/src/engine/counter.ts";

import { ScratchStore } from "../runtime/scratchStore.ts";
import { ServeStats } from "../runtime/serveStats.ts";
import type {
  IArrivalBatch,
  ILiveEngine,
  IRow,
  IServedProgram,
  ITickOutcome,
  IWatchFired,
  IWatchSource,
} from "../runtime/types.ts";
import { ProgramCompiler } from "../serve/0_compile.ts";
import { WatchBindRunner, bindPlansFor } from "../serve/2_binds.ts";
import { LiveEngine, bootServedProgram } from "../serve/3_engine.ts";

const WATCH_RAIL_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-watch-rail.dl6", import.meta.url));
const GLOB = "**/*.ts";
const COALESCE_MS = 50;

export interface FileWatchScaleConfig {
  readonly files: number;
  readonly eventsPerFile: number;
  readonly editRatio: number;
  readonly deleteRatio: number;
  readonly identicalResaveRatio: number;
}

interface FinalRelationReceipt {
  readonly expected: number;
  readonly actual: number;
  readonly sha256: string;
  readonly expectedSha256: string;
}

export interface FileWatchScaleResult {
  readonly benchmark: "v6.2_file_watch";
  readonly files: number;
  readonly editFiles: number;
  readonly deleteFiles: number;
  readonly identicalResaveFiles: number;
  readonly eventsPerFile: number;
  readonly events: number;
  readonly uniqueNotifiedPaths: number;
  readonly subscriptions: number;
  readonly watchRoots: readonly string[];
  readonly arrivals: number;
  readonly extractionDemands: null;
  readonly ticks: number;
  readonly watchBatches: number;
  readonly logicalMutations: number;
  readonly writeAmplification: number;
  readonly sqlStatements: number;
  readonly wallMs: number;
  readonly peakRssBytes: number;
  readonly sqliteBytes: number;
  readonly sqliteBytesPerLiveByte: number;
  readonly exactFinalRows: {
    readonly watch: FinalRelationReceipt;
    readonly seen: FinalRelationReceipt;
  };
  readonly duplicateCount: number;
  readonly staleCount: number;
  readonly missingCount: number;
  readonly correct: boolean;
}

class SyntheticWatchSource implements IWatchSource {
  private readonly paths = new Subject<string>();
  readonly roots: string[] = [];
  subscriptions = 0;
  events = 0;

  watch(root: string): Observable<string> {
    return new Observable<string>((subscriber) => {
      this.subscriptions += 1;
      this.roots.push(root);
      const running = this.paths.subscribe(subscriber);
      return () => running.unsubscribe();
    });
  }

  notify(paths: readonly string[], repeats: number): void {
    for (const path of paths) {
      for (let repeat = 0; repeat < repeats; repeat += 1) {
        this.events += 1;
        this.paths.next(path);
      }
    }
  }
}

function digest(text: string): string {
  return bytesToHex(sha256(Buffer.from(text)));
}

function content(index: number, revision: number): string {
  return `export const file_${index} = ${revision};\n`;
}

function rowText(row: IRow): string {
  return JSON.stringify(row);
}

function sortedRows(rows: readonly IRow[]): readonly IRow[] {
  return [...rows].sort((left, right) => rowText(left).localeCompare(rowText(right)));
}

function rowHash(rows: readonly IRow[]): string {
  return bytesToHex(sha256(Buffer.from(JSON.stringify(sortedRows(rows)))));
}

function duplicatePaths(rows: readonly IRow[], pathIndex: number): number {
  return rows.length - new Set(rows.map((row) => String(row[pathIndex] ?? ""))).size;
}

function staleRows(
  rows: readonly IRow[],
  expected: ReadonlyMap<string, string>,
  pathIndex: number,
  digestIndex: number,
): number {
  return rows.filter(
    (row) => expected.get(String(row[pathIndex] ?? "")) !== String(row[digestIndex] ?? ""),
  ).length;
}

function missingRows(
  rows: readonly IRow[],
  expected: ReadonlyMap<string, string>,
  pathIndex: number,
  digestIndex: number,
): number {
  const actual = new Map(
    rows.map((row) => [String(row[pathIndex] ?? ""), String(row[digestIndex] ?? "")]),
  );
  return [...expected].filter(([path, expectedDigest]) => actual.get(path) !== expectedDigest).length;
}

function validateConfig(config: FileWatchScaleConfig): void {
  if (!Number.isInteger(config.files) || config.files < 1) {
    throw new Error("5_file-watch-scale: files must be a positive integer");
  }
  if (!Number.isInteger(config.eventsPerFile) || config.eventsPerFile < 1) {
    throw new Error("5_file-watch-scale: events-per-file must be a positive integer");
  }
  for (const [name, ratio] of [
    ["edit-ratio", config.editRatio],
    ["delete-ratio", config.deleteRatio],
    ["identical-resave-ratio", config.identicalResaveRatio],
  ] as const) {
    if (!Number.isFinite(ratio) || ratio < 0 || ratio > 1) {
      throw new Error(`5_file-watch-scale: ${name} must be between 0 and 1`);
    }
  }
}

export async function compileFileWatchScaleProgram(): Promise<IServedProgram> {
  return firstValueFrom(ProgramCompiler.compile(readFileSync(WATCH_RAIL_DL6, "utf8")));
}

export async function runFileWatchScaleCell(
  config: FileWatchScaleConfig,
  compiledProgram?: IServedProgram,
): Promise<FileWatchScaleResult> {
  validateConfig(config);
  const program = compiledProgram ?? (await compileFileWatchScaleProgram());
  const workDir = mkdtempSync(join(tmpdir(), "tsv2-file-watch-scale-"));
  const sourceRoot = join(workDir, "repo");
  const sourceDir = join(sourceRoot, "src");
  const dbPath = join(workDir, "watch.sqlite");
  mkdirSync(sourceDir, { recursive: true });

  const expected = new Map<string, string>();
  const absolutePaths = Array.from({ length: config.files }, (_, index) => {
    const relativePath = `src/file_${String(index).padStart(8, "0")}.ts`;
    const absolutePath = join(sourceRoot, relativePath);
    const text = content(index, 0);
    writeFileSync(absolutePath, text);
    expected.set(relativePath, digest(text));
    return absolutePath;
  });

  const editFiles = Math.floor(config.files * config.editRatio);
  const deleteFiles = Math.floor(config.files * config.deleteRatio);
  const identicalResaveFiles = Math.floor(config.files * config.identicalResaveRatio);
  const seam = ScratchStore.open(`file:${dbPath}`);
  const ticks: ITickOutcome[] = [];
  const source = new SyntheticWatchSource();
  const scheduler = new VirtualTimeScheduler();
  let peakRssBytes = process.memoryUsage().rss;
  let arrivals = 0;
  let watchBatches = 0;
  let bootReadResolved = false;
  let resolveBootRead!: () => void;
  const bootRead = new Promise<void>((resolve) => {
    resolveBootRead = resolve;
  });
  let runnerFailure: unknown;
  const firingWaiters: Array<{
    readonly count: number;
    readonly resolve: () => void;
    readonly reject: (failure: unknown) => void;
  }> = [];
  const firings: IWatchFired[] = [];

  const sampleRss = (): void => {
    peakRssBytes = Math.max(peakRssBytes, process.memoryUsage().rss);
  };

  try {
    await firstValueFrom(bootServedProgram(seam, program));
    const engine = new LiveEngine(program, seam);
    const observedEngine: ILiveEngine = {
      program: engine.program,
      ticks$: engine.ticks$,
      rows(rel: string): Observable<readonly IRow[]> {
        return engine.rows(rel).pipe(
          finalize(() => {
            if (!bootReadResolved) {
              bootReadResolved = true;
              resolveBootRead();
            }
          }),
        );
      },
      submit(batch: IArrivalBatch): Observable<ITickOutcome> {
        arrivals += batch.length;
        watchBatches += 1;
        sampleRss();
        return engine.submit(batch).pipe(tap(sampleRss));
      },
    };

    const tickSubscription = engine.ticks$.subscribe({
      next: (tick) => {
        ticks.push(tick);
        sampleRss();
      },
      error: (failure: unknown) => {
        runnerFailure = failure;
        for (const waiter of firingWaiters.splice(0)) waiter.reject(failure);
      },
    });
    const watchSubscription = new WatchBindRunner(
      observedEngine,
      bindPlansFor(program.bindPlans, "live_watch"),
      {
        root: sourceRoot,
        coalesceMs: COALESCE_MS,
        scheduler,
        source,
      },
    ).firings$.subscribe({
      next: (firing) => {
        firings.push(firing);
        for (let index = firingWaiters.length - 1; index >= 0; index -= 1) {
          const waiter = firingWaiters[index];
          if (waiter !== undefined && firings.length >= waiter.count) {
            firingWaiters.splice(index, 1);
            waiter.resolve();
          }
        }
      },
      error: (failure: unknown) => {
        runnerFailure = failure;
        for (const waiter of firingWaiters.splice(0)) waiter.reject(failure);
      },
    });

    try {
      await bootRead;
      stmt_counter.reset();
      let wallNs = 0n;
      let uniqueNotifiedPaths = 0;

      const waitForFiring = (count: number): Promise<void> => {
        if (runnerFailure !== undefined) return Promise.reject(runnerFailure);
        if (firings.length >= count) return Promise.resolve();
        return new Promise<void>((resolve, reject) => {
          firingWaiters.push({ count, resolve, reject });
        });
      };

      const drive = async (paths: readonly string[], expectsBatch: boolean): Promise<void> => {
        if (paths.length === 0) return;
        uniqueNotifiedPaths += paths.length;
        const completion = expectsBatch ? waitForFiring(firings.length + 1) : Promise.resolve();
        const started = process.hrtime.bigint();
        source.notify(paths, config.eventsPerFile);
        scheduler.maxFrames = scheduler.frame + COALESCE_MS * 2;
        scheduler.flush();
        sampleRss();
        await completion;
        wallNs += process.hrtime.bigint() - started;
      };

      await drive(absolutePaths, true);

      const editedPaths = absolutePaths.slice(0, editFiles);
      for (let index = 0; index < editedPaths.length; index += 1) {
        const absolutePath = editedPaths[index]!;
        const fileIndex = index;
        const text = content(fileIndex, 1);
        writeFileSync(absolutePath, text);
        expected.set(relative(sourceRoot, absolutePath), digest(text));
      }
      await drive(editedPaths, editedPaths.length > 0);

      const identicalPaths = absolutePaths.slice(0, identicalResaveFiles);
      for (const absolutePath of identicalPaths) {
        const relativePath = relative(sourceRoot, absolutePath);
        const fileIndex = Number(relativePath.match(/file_(\d+)\.ts$/)?.[1] ?? 0);
        const revision = fileIndex < editFiles ? 1 : 0;
        writeFileSync(absolutePath, content(fileIndex, revision));
      }
      await drive(identicalPaths, false);

      const deletedPaths = absolutePaths.slice(config.files - deleteFiles);
      for (const absolutePath of deletedPaths) {
        rmSync(absolutePath);
        expected.delete(relative(sourceRoot, absolutePath));
      }
      await drive(deletedPaths, deletedPaths.length > 0);

      if (runnerFailure !== undefined) throw runnerFailure;
      const sqlStatements = stmt_counter.get();
      const watchRows = await firstValueFrom(engine.rows("watch"));
      const seenRows = await firstValueFrom(engine.rows("seen"));
      const expectedWatchRows: readonly IRow[] = [...expected]
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([path, pathDigest]) => [GLOB, path, pathDigest]);
      const expectedSeenRows: readonly IRow[] = [...expected]
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([path, pathDigest]) => [path, pathDigest]);
      const sqlite = await firstValueFrom(ServeStats.sqliteSnapshot(seam, ["watch", "seen"]));
      const duplicateCount = duplicatePaths(watchRows, 1) + duplicatePaths(seenRows, 0);
      const staleCount =
        staleRows(watchRows, expected, 1, 2) + staleRows(seenRows, expected, 0, 1);
      const missingCount =
        missingRows(watchRows, expected, 1, 2) + missingRows(seenRows, expected, 0, 1);
      const logicalMutations = config.files + editFiles + deleteFiles;
      const liveBytes = [...expected].reduce(
        (total, [path]) => {
          const fileIndex = Number(path.match(/file_(\d+)\.ts$/)?.[1] ?? 0);
          return total + Buffer.byteLength(content(fileIndex, fileIndex < editFiles ? 1 : 0));
        },
        0,
      );
      const exactFinalRows = {
        watch: {
          expected: expectedWatchRows.length,
          actual: watchRows.length,
          sha256: rowHash(watchRows),
          expectedSha256: rowHash(expectedWatchRows),
        },
        seen: {
          expected: expectedSeenRows.length,
          actual: seenRows.length,
          sha256: rowHash(seenRows),
          expectedSha256: rowHash(expectedSeenRows),
        },
      };
      const correct =
        duplicateCount === 0 &&
        staleCount === 0 &&
        missingCount === 0 &&
        exactFinalRows.watch.sha256 === exactFinalRows.watch.expectedSha256 &&
        exactFinalRows.seen.sha256 === exactFinalRows.seen.expectedSha256;

      return {
        benchmark: "v6.2_file_watch",
        files: config.files,
        editFiles,
        deleteFiles,
        identicalResaveFiles,
        eventsPerFile: config.eventsPerFile,
        events: source.events,
        uniqueNotifiedPaths,
        subscriptions: source.subscriptions,
        watchRoots: [...new Set(source.roots.map((root) => relative(sourceRoot, root) || "."))],
        arrivals,
        extractionDemands: null,
        ticks: ticks.length,
        watchBatches,
        logicalMutations,
        writeAmplification: arrivals / logicalMutations,
        sqlStatements,
        wallMs: Number(wallNs) / 1_000_000,
        peakRssBytes,
        sqliteBytes: sqlite.dbBytes,
        sqliteBytesPerLiveByte: liveBytes === 0 ? 0 : sqlite.dbBytes / liveBytes,
        exactFinalRows,
        duplicateCount,
        staleCount,
        missingCount,
        correct,
      };
    } finally {
      watchSubscription.unsubscribe();
      tickSubscription.unsubscribe();
    }
  } finally {
    seam.db.close();
    rmSync(workDir, { recursive: true, force: true });
  }
}

function option(args: readonly string[], name: string, fallback: string): string {
  const index = args.indexOf(name);
  if (index < 0) return fallback;
  const value = args[index + 1];
  if (value === undefined) throw new Error(`5_file-watch-scale: ${name} needs a value`);
  return value;
}

function ratioOption(args: readonly string[], name: string, fallback: string): number {
  return Number(option(args, name, fallback));
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const files = option(args, "--files", "100,1000")
    .split(",")
    .map(Number);
  if (files.some((count) => !Number.isInteger(count) || count < 1)) {
    throw new Error("5_file-watch-scale: --files must be comma-separated positive integers");
  }
  const outputPath = option(args, "--jsonl", "-");
  const common = {
    eventsPerFile: Number(option(args, "--events-per-file", "3")),
    editRatio: ratioOption(args, "--edit-ratio", "0.25"),
    deleteRatio: ratioOption(args, "--delete-ratio", "0.10"),
    identicalResaveRatio: ratioOption(args, "--identical-resave-ratio", "0.25"),
  };
  const program = await compileFileWatchScaleProgram();
  for (const fileCount of files) {
    const result = await runFileWatchScaleCell({ files: fileCount, ...common }, program);
    const line = `${JSON.stringify(result)}\n`;
    if (outputPath === "-") process.stdout.write(line);
    else appendFileSync(outputPath, line);
    if (!result.correct) process.exitCode = 1;
  }
}

const invokedPath = process.argv[1];
if (invokedPath !== undefined && import.meta.url === pathToFileURL(invokedPath).href) {
  void main().catch((failure: unknown) => {
    process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
    process.exitCode = 1;
  });
}
