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
import { WatchBindRunner, bind_plans_for } from "../serve/2_binds.ts";
import { LiveEngine, boot_served_program } from "../serve/3_engine.ts";

const WATCH_RAIL_DL6 = fileURLToPath(new URL("../../dl/fixtures/served-watch-rail.dl6", import.meta.url));
const GLOB = "**/*.ts";
const COALESCE_MS = 50;

export interface FileWatchScaleConfig {
  readonly files: number;
  readonly events_per_file: number;
  readonly edit_ratio: number;
  readonly delete_ratio: number;
  readonly identical_resave_ratio: number;
}

interface FinalRelationReceipt {
  readonly expected: number;
  readonly actual: number;
  readonly sha256: string;
  readonly expected_sha256: string;
}

export interface FileWatchScaleResult {
  readonly benchmark: "v6.2_file_watch";
  readonly files: number;
  readonly edit_files: number;
  readonly delete_files: number;
  readonly identical_resave_files: number;
  readonly events_per_file: number;
  readonly events: number;
  readonly unique_notified_paths: number;
  readonly subscriptions: number;
  readonly watch_roots: readonly string[];
  readonly arrivals: number;
  readonly extraction_demands: null;
  readonly ticks: number;
  readonly watch_batches: number;
  readonly logical_mutations: number;
  readonly write_amplification: number;
  readonly sql_statements: number;
  readonly wall_ms: number;
  readonly peak_rss_bytes: number;
  readonly sqlite_bytes: number;
  readonly sqlite_bytes_per_live_byte: number;
  readonly exact_final_rows: {
    readonly watch: FinalRelationReceipt;
    readonly seen: FinalRelationReceipt;
  };
  readonly duplicate_count: number;
  readonly stale_count: number;
  readonly missing_count: number;
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

function row_text(row: IRow): string {
  return JSON.stringify(row);
}

function sorted_rows(rows: readonly IRow[]): readonly IRow[] {
  return [...rows].sort((left, right) => row_text(left).localeCompare(row_text(right)));
}

function row_hash(rows: readonly IRow[]): string {
  return bytesToHex(sha256(Buffer.from(JSON.stringify(sorted_rows(rows)))));
}

function duplicate_paths(rows: readonly IRow[], path_index: number): number {
  return rows.length - new Set(rows.map((row) => String(row[path_index] ?? ""))).size;
}

function stale_rows(
  rows: readonly IRow[],
  expected: ReadonlyMap<string, string>,
  path_index: number,
  digest_index: number,
): number {
  return rows.filter(
    (row) => expected.get(String(row[path_index] ?? "")) !== String(row[digest_index] ?? ""),
  ).length;
}

function missing_rows(
  rows: readonly IRow[],
  expected: ReadonlyMap<string, string>,
  path_index: number,
  digest_index: number,
): number {
  const actual = new Map(
    rows.map((row) => [String(row[path_index] ?? ""), String(row[digest_index] ?? "")]),
  );
  return [...expected].filter(([path, expected_digest]) => actual.get(path) !== expected_digest).length;
}

function validate_config(config: FileWatchScaleConfig): void {
  if (!Number.isInteger(config.files) || config.files < 1) {
    throw new Error("5_file-watch-scale: files must be a positive integer");
  }
  if (!Number.isInteger(config.events_per_file) || config.events_per_file < 1) {
    throw new Error("5_file-watch-scale: events-per-file must be a positive integer");
  }
  for (const [name, ratio] of [
    ["edit-ratio", config.edit_ratio],
    ["delete-ratio", config.delete_ratio],
    ["identical-resave-ratio", config.identical_resave_ratio],
  ] as const) {
    if (!Number.isFinite(ratio) || ratio < 0 || ratio > 1) {
      throw new Error(`5_file-watch-scale: ${name} must be between 0 and 1`);
    }
  }
}

export async function compile_file_watch_scale_program(): Promise<IServedProgram> {
  return firstValueFrom(ProgramCompiler.compile(readFileSync(WATCH_RAIL_DL6, "utf8")));
}

export async function run_file_watch_scale_cell(
  config: FileWatchScaleConfig,
  compiled_program?: IServedProgram,
): Promise<FileWatchScaleResult> {
  validate_config(config);
  const program = compiled_program ?? (await compile_file_watch_scale_program());
  const work_dir = mkdtempSync(join(tmpdir(), "tsv2-file-watch-scale-"));
  const source_root = join(work_dir, "repo");
  const source_dir = join(source_root, "src");
  const db_path = join(work_dir, "watch.sqlite");
  mkdirSync(source_dir, { recursive: true });

  const expected = new Map<string, string>();
  const absolute_paths = Array.from({ length: config.files }, (_, index) => {
    const relative_path = `src/file_${String(index).padStart(8, "0")}.ts`;
    const absolute_path = join(source_root, relative_path);
    const text = content(index, 0);
    writeFileSync(absolute_path, text);
    expected.set(relative_path, digest(text));
    return absolute_path;
  });

  const edit_files = Math.floor(config.files * config.edit_ratio);
  const delete_files = Math.floor(config.files * config.delete_ratio);
  const identical_resave_files = Math.floor(config.files * config.identical_resave_ratio);
  const seam = ScratchStore.open(`file:${db_path}`);
  const ticks: ITickOutcome[] = [];
  const source = new SyntheticWatchSource();
  const scheduler = new VirtualTimeScheduler();
  let peak_rss_bytes = process.memoryUsage().rss;
  let arrivals = 0;
  let watch_batches = 0;
  let boot_read_resolved = false;
  let resolve_boot_read!: () => void;
  const boot_read = new Promise<void>((resolve) => {
    resolve_boot_read = resolve;
  });
  let runner_failure: unknown;
  const firing_waiters: Array<{
    readonly count: number;
    readonly resolve: () => void;
    readonly reject: (failure: unknown) => void;
  }> = [];
  const firings: IWatchFired[] = [];

  const sample_rss = (): void => {
    peak_rss_bytes = Math.max(peak_rss_bytes, process.memoryUsage().rss);
  };

  try {
    await firstValueFrom(boot_served_program(seam, program));
    const engine = new LiveEngine(program, seam);
    const observed_engine: ILiveEngine = {
      program: engine.program,
      ticks$: engine.ticks$,
      rows(rel: string): Observable<readonly IRow[]> {
        return engine.rows(rel).pipe(
          finalize(() => {
            if (!boot_read_resolved) {
              boot_read_resolved = true;
              resolve_boot_read();
            }
          }),
        );
      },
      submit(batch: IArrivalBatch): Observable<ITickOutcome> {
        arrivals += batch.length;
        watch_batches += 1;
        sample_rss();
        return engine.submit(batch).pipe(tap(sample_rss));
      },
    };

    const tick_subscription = engine.ticks$.subscribe({
      next: (tick) => {
        ticks.push(tick);
        sample_rss();
      },
      error: (failure: unknown) => {
        runner_failure = failure;
        for (const waiter of firing_waiters.splice(0)) waiter.reject(failure);
      },
    });
    const watch_subscription = new WatchBindRunner(
      observed_engine,
      bind_plans_for(program.bind_plans, "live_watch"),
      {
        root: source_root,
        coalesce_ms: COALESCE_MS,
        scheduler,
        source,
      },
    ).firings$.subscribe({
      next: (firing) => {
        firings.push(firing);
        for (let index = firing_waiters.length - 1; index >= 0; index -= 1) {
          const waiter = firing_waiters[index];
          if (waiter !== undefined && firings.length >= waiter.count) {
            firing_waiters.splice(index, 1);
            waiter.resolve();
          }
        }
      },
      error: (failure: unknown) => {
        runner_failure = failure;
        for (const waiter of firing_waiters.splice(0)) waiter.reject(failure);
      },
    });

    try {
      await boot_read;
      stmt_counter.reset();
      let wall_ns = 0n;
      let unique_notified_paths = 0;

      const wait_for_firing = (count: number): Promise<void> => {
        if (runner_failure !== undefined) return Promise.reject(runner_failure);
        if (firings.length >= count) return Promise.resolve();
        return new Promise<void>((resolve, reject) => {
          firing_waiters.push({ count, resolve, reject });
        });
      };

      const drive = async (paths: readonly string[], expects_batch: boolean): Promise<void> => {
        if (paths.length === 0) return;
        unique_notified_paths += paths.length;
        const completion = expects_batch ? wait_for_firing(firings.length + 1) : Promise.resolve();
        const started = process.hrtime.bigint();
        source.notify(paths, config.events_per_file);
        scheduler.maxFrames = scheduler.frame + COALESCE_MS * 2;
        scheduler.flush();
        sample_rss();
        await completion;
        wall_ns += process.hrtime.bigint() - started;
      };

      await drive(absolute_paths, true);

      const edited_paths = absolute_paths.slice(0, edit_files);
      for (let index = 0; index < edited_paths.length; index += 1) {
        const absolute_path = edited_paths[index]!;
        const file_index = index;
        const text = content(file_index, 1);
        writeFileSync(absolute_path, text);
        expected.set(relative(source_root, absolute_path), digest(text));
      }
      await drive(edited_paths, edited_paths.length > 0);

      const identical_paths = absolute_paths.slice(0, identical_resave_files);
      for (const absolute_path of identical_paths) {
        const relative_path = relative(source_root, absolute_path);
        const file_index = Number(relative_path.match(/file_(\d+)\.ts$/)?.[1] ?? 0);
        const revision = file_index < edit_files ? 1 : 0;
        writeFileSync(absolute_path, content(file_index, revision));
      }
      await drive(identical_paths, false);

      const deleted_paths = absolute_paths.slice(config.files - delete_files);
      for (const absolute_path of deleted_paths) {
        rmSync(absolute_path);
        expected.delete(relative(source_root, absolute_path));
      }
      await drive(deleted_paths, deleted_paths.length > 0);

      if (runner_failure !== undefined) throw runner_failure;
      const sql_statements = stmt_counter.get();
      const watch_rows = await firstValueFrom(engine.rows("watch"));
      const seen_rows = await firstValueFrom(engine.rows("seen"));
      const expected_watch_rows: readonly IRow[] = [...expected]
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([path, path_digest]) => [GLOB, path, path_digest]);
      const expected_seen_rows: readonly IRow[] = [...expected]
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([path, path_digest]) => [path, path_digest]);
      const sqlite = await firstValueFrom(ServeStats.sqlite_snapshot(seam, ["watch", "seen"]));
      const duplicate_count = duplicate_paths(watch_rows, 1) + duplicate_paths(seen_rows, 0);
      const stale_count =
        stale_rows(watch_rows, expected, 1, 2) + stale_rows(seen_rows, expected, 0, 1);
      const missing_count =
        missing_rows(watch_rows, expected, 1, 2) + missing_rows(seen_rows, expected, 0, 1);
      const logical_mutations = config.files + edit_files + delete_files;
      const live_bytes = [...expected].reduce(
        (total, [path]) => {
          const file_index = Number(path.match(/file_(\d+)\.ts$/)?.[1] ?? 0);
          return total + Buffer.byteLength(content(file_index, file_index < edit_files ? 1 : 0));
        },
        0,
      );
      const exact_final_rows = {
        watch: {
          expected: expected_watch_rows.length,
          actual: watch_rows.length,
          sha256: row_hash(watch_rows),
          expected_sha256: row_hash(expected_watch_rows),
        },
        seen: {
          expected: expected_seen_rows.length,
          actual: seen_rows.length,
          sha256: row_hash(seen_rows),
          expected_sha256: row_hash(expected_seen_rows),
        },
      };
      const correct =
        duplicate_count === 0 &&
        stale_count === 0 &&
        missing_count === 0 &&
        exact_final_rows.watch.sha256 === exact_final_rows.watch.expected_sha256 &&
        exact_final_rows.seen.sha256 === exact_final_rows.seen.expected_sha256;

      return {
        benchmark: "v6.2_file_watch",
        files: config.files,
        edit_files,
        delete_files,
        identical_resave_files,
        events_per_file: config.events_per_file,
        events: source.events,
        unique_notified_paths,
        subscriptions: source.subscriptions,
        watch_roots: [...new Set(source.roots.map((root) => relative(source_root, root) || "."))],
        arrivals,
        extraction_demands: null,
        ticks: ticks.length,
        watch_batches,
        logical_mutations,
        write_amplification: arrivals / logical_mutations,
        sql_statements,
        wall_ms: Number(wall_ns) / 1_000_000,
        peak_rss_bytes,
        sqlite_bytes: sqlite.db_bytes,
        sqlite_bytes_per_live_byte: live_bytes === 0 ? 0 : sqlite.db_bytes / live_bytes,
        exact_final_rows,
        duplicate_count,
        stale_count,
        missing_count,
        correct,
      };
    } finally {
      watch_subscription.unsubscribe();
      tick_subscription.unsubscribe();
    }
  } finally {
    seam.db.close();
    rmSync(work_dir, { recursive: true, force: true });
  }
}

function option(args: readonly string[], name: string, fallback: string): string {
  const index = args.indexOf(name);
  if (index < 0) return fallback;
  const value = args[index + 1];
  if (value === undefined) throw new Error(`5_file-watch-scale: ${name} needs a value`);
  return value;
}

function ratio_option(args: readonly string[], name: string, fallback: string): number {
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
  const output_path = option(args, "--jsonl", "-");
  const common = {
    events_per_file: Number(option(args, "--events-per-file", "3")),
    edit_ratio: ratio_option(args, "--edit-ratio", "0.25"),
    delete_ratio: ratio_option(args, "--delete-ratio", "0.10"),
    identical_resave_ratio: ratio_option(args, "--identical-resave-ratio", "0.25"),
  };
  const program = await compile_file_watch_scale_program();
  for (const file_count of files) {
    const result = await run_file_watch_scale_cell({ files: file_count, ...common }, program);
    const line = `${JSON.stringify(result)}\n`;
    if (output_path === "-") process.stdout.write(line);
    else appendFileSync(output_path, line);
    if (!result.correct) process.exitCode = 1;
  }
}

const invoked_path = process.argv[1];
if (invoked_path !== undefined && import.meta.url === pathToFileURL(invoked_path).href) {
  void main().catch((failure: unknown) => {
    process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
    process.exitCode = 1;
  });
}
