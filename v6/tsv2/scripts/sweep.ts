/**
 * sweep.ts — Phase C driver (plans/2026-07-27-tsv2-compile-target-header.md,
 * PHASE C CONTRACT): runs every prolog-compiled fixture named in
 * v6/prolog/compile/out/manifest.json (written by v6/prolog/compile/sweep.pl)
 * against the phase-A runtime, replaying each fixture's OWN schedule
 * (out/<name>.schedule.json, a JSON rendering of the fixture's Schedule term)
 * and diffing the resulting tick log byte-for-byte against
 * v6/prolog/compile/out/<name>.oracle.jsonl (written by
 * v6/prolog/compile/oracle_dump.pl, run over the SAME schedule via
 * conformance/ticklog.pl -- never edited by this arc).
 *
 * Differs from scripts/run-emitted.ts in scope, not seam: run-emitted.ts is
 * the phase A/B two-fixture reconciliation runner with a hand-registered
 * lookup table; this walks the WHOLE compiled set the manifest names, one
 * fixture at a time (concatMap, one seam per fixture, never shared), so
 * neither script needed to change for the other to exist.
 *
 * Copies every compiled fixture's emitted module into gen_emitted/<name>.ts
 * itself, because dynamic import resolves relative to THIS package and a
 * module still sitting only in compile/out/ cannot be imported directly (its
 * "../runtime/..." relative imports would resolve against the wrong
 * directory). The copy was 335 `rm`+`cp` pairs in sweep.sh, 670 process
 * spawns for 1.8s of the cold wall; done here it reuses the manifest read
 * this script already does and skips a file whose bytes already match.
 *
 * Writes v6/prolog/compile/out/run-results.json (one record per compiled
 * fixture: bucket + a short diff/error detail) and prints a summary line.
 *
 * EXIT CODE: 0, except when any fixture lands in `emitted_crash` -- an
 * emitted module that died on a schedule the ORACLE completed. See the
 * RunBucket comment for why that one bucket gates and `wrong` does not.
 *
 * SABOTAGE RECEIPT for that gate (fork_join_malformed_json arc): splicing
 * `AND json_extract(d0."value_a", '$.fn') = 'x'` into
 * gen_emitted/fork_join_is_a_conjunctive_body.ts's insert_sql -- the exact
 * shape of the defect this split was written for -- moved that fixture
 * identical -> emitted_crash and turned the script red:
 *   RUN total=189 identical=187 wrong=0 emitted_crash=1 rejection=1 ...
 *   SWEEP GATE: 1 emitted module(s) crashed on a schedule the oracle
 *   completed: fork_join_is_a_conjunctive_body
 *   EXIT=1
 * Reverted; clean run is emitted_crash=0 rejection=1, EXIT=0.
 *
 * Usage: node --experimental-transform-types scripts/sweep.ts
 */

import { spawn } from "node:child_process";
import { availableParallelism, tmpdir } from "node:os";
import { createHash } from "node:crypto";
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { Observable, catchError, concatMap, forkJoin, from, map, of, toArray } from "rxjs";

import { BootRunner } from "../runtime/2_boot.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import { row_value_from_sql } from "../runtime/rows.ts";
import type { IArrivalBatch, IBootStatement, IRowColumnType, IRowValue, ISqlSeam, IGenProgram } from "../runtime/types.ts";

const HERE = dirname(fileURLToPath(import.meta.url));
const COMPILE_OUT = join(HERE, "..", "..", "prolog", "compile", "out");
const RUNTIME_DIR = join(HERE, "..", "runtime");
const GEN_EMITTED = join(HERE, "..", "gen_emitted");
const REPLAY_DIGESTS = join(COMPILE_OUT, "sweep.replay.digests.json");
const TIMINGS = join(COMPILE_OUT, "sweep.timings.tsv");

interface IManifestEntry {
  readonly name: string;
  readonly file: string;
  readonly bucket: "compiled" | "unsupported" | "crash";
  readonly reason: string;
}

// The two extra fields emit_ts.pl adds beyond IGenProgram's five pinned
// names. `IBootStatement` and the loop that runs it are no longer per-script
// copies: both this file and run-emitted.ts had their own, and both bound
// `params` RAW, which is fail-first check (b) -- see runtime/2_boot.ts.
type EmittedProgram = IGenProgram & {
  readonly boot: readonly IBootStatement[];
  readonly final_select: Record<string, string>;
};

/** A compiled fixture whose replay THREW used to land in one `run_error`
 *  bucket, and that bucket held two unrelated things. `log_retraction_rejected`
 *  is a fixture whose whole point is that the schedule is illegal
 *  (`throws(retract_from_log(event/1))`): the ORACLE throws too, so the
 *  emitted module throwing is the two doors agreeing. `fork_join_error_arm_
 *  is_a_value` had a complete two-tick oracle log and the emitted module died
 *  on `SQLITE_ERROR: malformed JSON` -- a real emitter defect, and it sat next
 *  to the rejection fixture reading as one more expected line for three arcs
 *  (ARCH fork_join_malformed_json).
 *
 *  The discriminator is already on disk and needs no new bookkeeping: an
 *  oracle tick log EXISTS exactly when the oracle completed the schedule.
 *    rejection     -> no oracle log; both doors refuse the same schedule.
 *    emitted_crash -> oracle log present; only the emitted module died. This
 *                     bucket must read as a DEFECT, never as expected. */
type RunBucket = "identical" | "wrong" | "rejection" | "emitted_crash" | "no_oracle_log";

/** Final-state leg (EXPRESSION + AGGREGATE LIFT arc). Reported ALONGSIDE the
 *  tick-log bucket, never folded into it: the tick-log diff stays the gate
 *  every earlier arc was graded on, and this adds the grade an EMPTY-schedule
 *  fixture has no other way to earn (both sides print zero tick lines, which
 *  the tick-log diff calls IDENTICAL on no evidence -- SCOREBOARD.md Finding
 *  2's vacuous-pass class). `no_oracle_final` means the oracle run threw. */
type FinalBucket = "final_identical" | "final_wrong" | "no_oracle_final";

interface IFixtureRunResult {
  readonly name: string;
  readonly bucket: RunBucket;
  readonly detail: string;
  readonly final_bucket: FinalBucket;
  readonly final_detail: string;
}

function read_manifest(): readonly IManifestEntry[] {
  const text = readFileSync(join(COMPILE_OUT, "manifest.json"), "utf8");
  return JSON.parse(text) as readonly IManifestEntry[];
}

function read_schedule(name: string): readonly IArrivalBatch[] {
  const text = readFileSync(join(COMPILE_OUT, `${name}.schedule.json`), "utf8");
  return JSON.parse(text) as readonly IArrivalBatch[];
}

function read_oracle_lines(name: string): readonly string[] | null {
  try {
    const text = readFileSync(join(COMPILE_OUT, `${name}.oracle.jsonl`), "utf8");
    return text.split("\n").filter((line) => line.length > 0);
  } catch {
    return null;
  }
}

function read_oracle_final_line(name: string): string | null {
  try {
    const text = readFileSync(join(COMPILE_OUT, `${name}.oracle.final.jsonl`), "utf8");
    return text.split("\n").filter((line) => line.length > 0)[0] ?? null;
  } catch {
    return null;
  }
}

/** The oracle side encodes a value with ticklog.pl's `value_json/2`: an
 *  integer as a JSON number, a json value as canonical JSON text, everything
 *  else as a JSON string. The emitted side reads an INTEGER-affinity column
 *  back as a JS number and a TEXT-affinity column as a JS string (rows.ts's
 *  own note), so the SAME rule applied here is what makes a TEXT-collapsed
 *  integer ("12" stored in a TEXT column) show up as a diff instead of
 *  passing silently.
 *
 *  This now delegates to `TickLogEmitter.valueText`, the per-tick leg's own
 *  encoder, rather than restating the rule. The two legs HAD drifted: the
 *  json_ticklog_encoding regrade taught the per-tick leg to canonicalize
 *  object/array text and left this copy behind, which no corpus value
 *  exercised until a struct column arrived whose stored value IS canonical
 *  JSON text -- the tick log printed the object and the final state printed
 *  the same bytes wrapped in quotes. */
function final_value_json(value: unknown, type?: IRowColumnType): string {
  if (typeof value === "bigint") return value.toString();
  // A list column arrives here already parsed by row_value_from_sql; String()
  // would flatten it to comma-joined elements.
  if (Array.isArray(value)) return TickLogEmitter.value_text(value as IRowValue, type);
  if (typeof value === "number" || typeof value === "boolean") {
    return TickLogEmitter.value_text(value as IRowValue, type);
  }
  return TickLogEmitter.value_text(String(value), type);
}

function final_state_line(
  rows_by_rel: Record<string, readonly (readonly unknown[])[]>,
  rel_column_types?: Readonly<Record<string, readonly IRowColumnType[]>>,
): string {
  const rel_names = Object.keys(rows_by_rel).sort();
  const parts: string[] = [];
  for (const rel of rel_names) {
    const rows = rows_by_rel[rel]!;
    if (rows.length === 0) continue;
    const types = rel_column_types?.[rel];
    const row_texts = rows
      .map((row) => `[${row.map((value, index) => final_value_json(value, types?.[index])).join(",")}]`)
      .sort();
    parts.push(`${JSON.stringify(rel)}:[${row_texts.join(",")}]`);
  }
  return `{"final":{${parts.join(",")}}}`;
}

function read_final_state(seam: ISqlSeam, program: EmittedProgram): Observable<string> {
  const rel_names = Object.keys(program.final_select);
  if (rel_names.length === 0) return of(final_state_line({}));
  return forkJoin(
    rel_names.map((rel) =>
      seam.runner.execute(seam.db, program.final_select[rel]!).pipe(
        map((result) => ({
          rel,
          rows: result.rows.map((row) =>
            (program.rel_columns[rel] ?? []).map((column, index) =>
              row_value_from_sql(program.rel_column_types?.[rel]?.[index], row[column]),
            ),
          ),
        })),
      ),
    ),
  ).pipe(
    map((entries) => {
      const rows_by_rel: Record<string, readonly (readonly unknown[])[]> = {};
      for (const entry of entries) rows_by_rel[entry.rel] = entry.rows;
      return final_state_line(rows_by_rel, program.rel_column_types);
    }),
  );
}

function grade_final_state(name: string, actual_line: string): { bucket: FinalBucket; detail: string } {
  const oracle_line = read_oracle_final_line(name);
  if (oracle_line === null) return { bucket: "no_oracle_final", detail: "oracle run threw; no final state to diff" };
  if (oracle_line === actual_line) return { bucket: "final_identical", detail: "" };
  return { bucket: "final_wrong", detail: `actual=${actual_line.slice(0, 400)} oracle=${oracle_line.slice(0, 400)}` };
}

function load_emitted(name: string): Promise<EmittedProgram> {
  const specifier = ["..", "gen_emitted", `${name}.ts`].join("/");
  return import(specifier).then((loaded: { program: EmittedProgram }) => loaded.program);
}

function grade_against_oracle(
  name: string,
  actual_lines: readonly string[],
  final_grade: { bucket: FinalBucket; detail: string },
): IFixtureRunResult {
  const oracle = read_oracle_lines(name);
  const with_final = (bucket: RunBucket, detail: string): IFixtureRunResult => ({
    name,
    bucket,
    detail,
    final_bucket: final_grade.bucket,
    final_detail: final_grade.detail,
  });
  if (oracle === null) {
    return with_final("no_oracle_log", "oracle run threw (see oracle_dump.pl ORACLE_THROW output); nothing to diff");
  }
  const identical = actual_lines.length === oracle.length && actual_lines.every((line, index) => line === oracle[index]);
  if (identical) return with_final("identical", "");
  const first_diff_index = actual_lines.findIndex((line, index) => line !== oracle[index]);
  const excerpt_index = first_diff_index === -1 ? Math.min(actual_lines.length, oracle.length) : first_diff_index;
  const actual_excerpt = actual_lines[excerpt_index] ?? "<missing tick>";
  const oracle_excerpt = oracle[excerpt_index] ?? "<missing tick>";
  return with_final("wrong", `first diff at line ${excerpt_index + 1}: actual=${actual_excerpt} oracle=${oracle_excerpt}`);
}

/** The replay cache (issues/sweep-timings-report, rulings.pl
 *  oracle_demoted_to_snapshots). A fixture is replayed only when something it
 *  reads changed: its emitted module, its schedule, either oracle snapshot, or
 *  the runtime the module runs on. The runtime term is what makes the cache
 *  answerable -- an emitted module unchanged over a changed tickLoop.ts grades
 *  differently, and a key over the fixture's own files alone would hand back
 *  the previous runtime's verdict. */
interface IReplayCacheEntry {
  readonly key: string;
  readonly result: IFixtureRunResult;
}

function file_digest(path: string): string {
  try {
    return createHash("sha256").update(readFileSync(path)).digest("hex");
  } catch {
    return "absent";
  }
}

function tree_digest(dir: string): string {
  const hash = createHash("sha256");
  const walk = (here: string): void => {
    for (const entry of readdirSync(here, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
      const path = join(here, entry.name);
      if (entry.isDirectory()) walk(path);
      else hash.update(path.slice(dir.length)).update("\u0000").update(file_digest(path)).update("\u0000");
    }
  };
  walk(dir);
  return hash.digest("hex");
}

const RUNNER_DIGEST = ((): string => {
  const hash = createHash("sha256");
  hash.update(tree_digest(RUNTIME_DIR)).update("\u0000");
  hash.update(file_digest(fileURLToPath(import.meta.url)));
  return hash.digest("hex");
})();

function replay_key(name: string): string {
  const hash = createHash("sha256");
  hash.update(RUNNER_DIGEST).update("\u0000");
  for (const path of [
    join(GEN_EMITTED, `${name}.ts`),
    join(COMPILE_OUT, `${name}.schedule.json`),
    join(COMPILE_OUT, `${name}.oracle.jsonl`),
    join(COMPILE_OUT, `${name}.oracle.final.jsonl`),
  ]) {
    hash.update(file_digest(path)).update("\u0000");
  }
  return hash.digest("hex");
}

function read_replay_cache(): Record<string, IReplayCacheEntry> {
  if (process.env["SWEEP_FORCE"] !== undefined && process.env["SWEEP_FORCE"] !== "0") return {};
  try {
    return JSON.parse(readFileSync(REPLAY_DIGESTS, "utf8")) as Record<string, IReplayCacheEntry>;
  } catch {
    return {};
  }
}

function append_timings(entries: readonly (readonly [string, number])[]): void {
  if (!existsSync(TIMINGS)) writeFileSync(TIMINGS, "fixture\tstage\tms\n");
  if (entries.length === 0) return;
  appendFileSync(TIMINGS, entries.map(([name, ms]) => `${name}\treplay\t${ms}\n`).join(""));
}

function report_slowest(entries: readonly (readonly [string, number])[]): void {
  if (entries.length === 0) {
    process.stdout.write("SWEEP_TIMINGS replay: nothing to do this pass\n");
    return;
  }
  const ranked = [...entries].sort((left, right) => right[1] - left[1]).slice(0, 10);
  process.stdout.write(`SWEEP_TIMINGS replay slowest ${ranked.length} of ${entries.length}\n`);
  for (const [name, ms] of ranked) process.stdout.write(`  ${name} ${ms}ms\n`);
}

function run_fixture(name: string): Observable<IFixtureRunResult> {
  return from(load_emitted(name)).pipe(
    concatMap((program) => {
      const schedule = read_schedule(name);
      const seam = ScratchStore.open(":memory:");
      return ScratchStore.boot(seam, program.ddl).pipe(
        concatMap(() => BootRunner.run(seam, program.boot)),
        concatMap(() => TickFold.run(program, seam, schedule).pipe(toArray())),
        concatMap((lines) => read_final_state(seam, program).pipe(map((final_line) => ({ lines, final_line })))),
      );
    }),
    map(({ lines, final_line }) => grade_against_oracle(name, lines, grade_final_state(name, final_line))),
    catchError((error: unknown) => {
      // Same split on the final leg, same discriminator: a rejection fixture
      // has no oracle FINAL line either, so calling its missing final state
      // `final_wrong` was the identical noise one column over.
      const rejection = read_oracle_lines(name) === null;
      return of<IFixtureRunResult>({
        name,
        bucket: rejection ? "rejection" : "emitted_crash",
        detail: error instanceof Error ? error.message : String(error),
        final_bucket: rejection ? "no_oracle_final" : "final_wrong",
        final_detail: rejection
          ? "oracle threw on this schedule too; no final state to diff"
          : "run threw before the final state could be read",
      });
    }),
  );
}

function summary_line(results: readonly IFixtureRunResult[]): string {
  const count_of = (bucket: RunBucket): number => results.filter((result) => result.bucket === bucket).length;
  return `RUN total=${results.length} identical=${count_of("identical")} wrong=${count_of("wrong")} emitted_crash=${count_of("emitted_crash")} rejection=${count_of("rejection")} no_oracle_log=${count_of("no_oracle_log")}`;
}

function final_summary_line(results: readonly IFixtureRunResult[]): string {
  const count_of = (bucket: FinalBucket): number => results.filter((result) => result.final_bucket === bucket).length;
  return `FINAL total=${results.length} final_identical=${count_of("final_identical")} final_wrong=${count_of("final_wrong")} no_oracle_final=${count_of("no_oracle_final")}`;
}

/** gen_emitted/ also holds checked-in modules that are not fixture outputs, so
 *  only the names the manifest calls `compiled` are ever written here. A file
 *  whose bytes already match is left alone: its mtime is what the emitted
 *  module's own loader cache keys on. */
function sync_emitted_modules(names: readonly string[]): void {
  mkdirSync(GEN_EMITTED, { recursive: true });
  for (const name of names) {
    const source = readFileSync(join(COMPILE_OUT, `${name}.ts`));
    const target = join(GEN_EMITTED, `${name}.ts`);
    let same = false;
    try {
      same = readFileSync(target).equals(source);
    } catch {
      same = false;
    }
    if (!same) writeFileSync(target, source);
  }
}

/** Fan-out (this arc). The replay leg was one node process walking the whole
 *  compiled set, and its per-fixture work is independent: every fixture opens
 *  its OWN `:memory:` seam and reads only files keyed to its own name. A shard
 *  child replays an assigned slice and hands back its results and its timings;
 *  the parent re-orders everything into MANIFEST order before writing
 *  anything, so run-results.json and every printed line are byte-identical to
 *  the single-process pass whatever order the children finish in.
 *
 *  A child is this same file re-entered with SWEEP_REPLAY_SHARD naming its
 *  slice file. `stdio: inherit` so a child's SWEEP_SLOW line reaches the
 *  terminal while it is still slow. */
interface IShardPayload {
  readonly results: readonly IFixtureRunResult[];
  readonly timings: readonly (readonly [string, number])[];
}

const SHARD_FILE = process.env["SWEEP_REPLAY_SHARD"];

function replay_jobs(): number {
  for (const name of ["SWEEP_REPLAY_JOBS", "SWEEP_JOBS"]) {
    const text = process.env[name];
    if (text === undefined) continue;
    const count = Number.parseInt(text, 10);
    if (Number.isInteger(count) && count > 0) return count;
  }
  return availableParallelism();
}

function replay_names(names: readonly string[]): Observable<IShardPayload> {
  const timings: (readonly [string, number])[] = [];
  const graded = (name: string): Observable<IFixtureRunResult> => {
    const started = Date.now();
    return run_fixture(name).pipe(
      map((result) => {
        const elapsed = Date.now() - started;
        timings.push([name, elapsed]);
        if (elapsed > 10_000) process.stdout.write(`SWEEP_SLOW ${name} ${(elapsed / 1000).toFixed(2)}\n`);
        return result;
      }),
    );
  };
  return from(names).pipe(
    concatMap((name) => graded(name)),
    toArray(),
    map((results) => ({ results, timings })),
  );
}

/** NODE_NO_WARNINGS so the parent's one ExperimentalWarning stays one line
 *  instead of one per child. */
function spawn_shard_child(path: string): Observable<IShardPayload> {
  return new Observable<IShardPayload>((subscriber) => {
    const child = spawn(process.execPath, ["--experimental-transform-types", fileURLToPath(import.meta.url)], {
      env: { ...process.env, SWEEP_REPLAY_SHARD: path, NODE_NO_WARNINGS: "1" },
      stdio: ["ignore", "inherit", "inherit"],
    });
    child.on("error", (failure: unknown) => subscriber.error(failure));
    child.on("exit", (code: number | null, signal: string | null) => {
      if (code !== 0) {
        subscriber.error(new Error(`replay shard ${path} exited code=${String(code)} signal=${String(signal)}`));
        return;
      }
      try {
        subscriber.next(JSON.parse(readFileSync(`${path}.out`, "utf8")) as IShardPayload);
        subscriber.complete();
      } catch (failure: unknown) {
        subscriber.error(failure);
      }
    });
    return () => child.kill("SIGKILL");
  });
}

/** ScratchStore.open has no close (runtime/scratchStore.ts), so a replay
 *  process holds one libsql handle per fixture it ran and a long slice can die
 *  on a native SIGSEGV: measured once in ~20 passes at jobs=2, where a child
 *  carried 168 fixtures. The retry re-runs THAT SLICE ONLY, in a fresh
 *  process, and says so; a second death is reported, never swallowed. Until
 *  the seam closes its handle this is the rail that keeps the fan-out strictly
 *  no worse than the single-process shape, which has the same leak at 335. */
function run_shard_child(path: string): Observable<IShardPayload> {
  return spawn_shard_child(path).pipe(
    catchError((failure: unknown) => {
      process.stderr.write(`REPLAY_SHARD_RETRY ${path}: ${failure instanceof Error ? failure.message : String(failure)}\n`);
      return spawn_shard_child(path);
    }),
  );
}

/** Round-robin over the corpus order. The per-fixture spread is small (mean
 *  8ms, slowest 82ms), so the cost-aware bin-pack a wider spread would need
 *  buys nothing measurable here. */
function shard_slices(names: readonly string[], jobs: number): readonly (readonly string[])[] {
  const slices: string[][] = Array.from({ length: jobs }, () => []);
  names.forEach((name, index) => slices[index % jobs]?.push(name));
  return slices.filter((slice) => slice.length > 0);
}

function replay_fanned_out(names: readonly string[], jobs: number): Observable<IShardPayload> {
  const dir = mkdtempSync(join(tmpdir(), "sweep-replay-"));
  const slices = shard_slices(names, jobs);
  const paths = slices.map((slice, index) => {
    const path = join(dir, `shard.${index}.json`);
    writeFileSync(path, JSON.stringify(slice));
    return path;
  });
  return forkJoin(paths.map((path) => run_shard_child(path))).pipe(
    map((payloads) => {
      rmSync(dir, { recursive: true, force: true });
      const by_name = new Map<string, IFixtureRunResult>();
      const millis = new Map<string, number>();
      for (const payload of payloads) {
        for (const result of payload.results) by_name.set(result.name, result);
        for (const [name, ms] of payload.timings) millis.set(name, ms);
      }
      return {
        results: names.map((name) => by_name.get(name)).filter((result): result is IFixtureRunResult => result !== undefined),
        timings: names.map((name) => [name, millis.get(name) ?? 0] as const),
      };
    }),
  );
}

function shard_main(path: string): void {
  const names = JSON.parse(readFileSync(path, "utf8")) as readonly string[];
  replay_names(names).subscribe({
    next: (payload) => writeFileSync(`${path}.out`, JSON.stringify(payload)),
    error: (failure: unknown) => {
      process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
      process.exit(1);
    },
  });
}

/** Below this many cache misses the fan-out costs more in child boots than the
 *  slice saves, so the pass stays in this process. */
const FANOUT_FLOOR = 32;

function main(): void {
  const manifest = read_manifest();
  const compiled_names = manifest.filter((entry) => entry.bucket === "compiled").map((entry) => entry.name);
  sync_emitted_modules(compiled_names);
  const cache = read_replay_cache();
  const keys = new Map(compiled_names.map((name) => [name, replay_key(name)] as const));
  const cached = new Map<string, IFixtureRunResult>();
  const misses: string[] = [];
  for (const name of compiled_names) {
    const entry = cache[name];
    if (entry !== undefined && entry.key === keys.get(name)) cached.set(name, entry.result);
    else misses.push(name);
  }
  const jobs = replay_jobs();
  const replayed = misses.length >= FANOUT_FLOOR && jobs > 1 ? replay_fanned_out(misses, jobs) : replay_names(misses);

  replayed.subscribe({
    next: (payload) => {
      const by_name = new Map(payload.results.map((result) => [result.name, result] as const));
      const results = compiled_names
        .map((name) => cached.get(name) ?? by_name.get(name))
        .filter((result): result is IFixtureRunResult => result !== undefined);
      const timings = payload.timings;
      const fresh: Record<string, IReplayCacheEntry> = {};
      for (const result of results) fresh[result.name] = { key: keys.get(result.name) ?? "", result };
      writeFileSync(REPLAY_DIGESTS, `${JSON.stringify(fresh, null, 2)}\n`);
      append_timings(timings);
      report_slowest(timings);
      process.stdout.write(`REPLAY_CACHE hit=${cached.size} replayed=${results.length - cached.size}\n`);
      writeFileSync(join(COMPILE_OUT, "run-results.json"), `${JSON.stringify(results, null, 2)}\n`);
      process.stdout.write(`${summary_line(results)}\n`);
      for (const result of results) {
        if (result.bucket !== "identical") process.stdout.write(`  ${result.bucket.toUpperCase()} ${result.name} ${result.detail}\n`);
      }
      process.stdout.write(`${final_summary_line(results)}\n`);
      for (const result of results) {
        if (result.final_bucket !== "final_identical") {
          process.stdout.write(`  ${result.final_bucket.toUpperCase()} ${result.name} ${result.final_detail}\n`);
        }
      }
      // The ratchet the split exists for. `emitted_crash` is zero today and
      // an emitted module dying where the oracle completed the schedule is
      // never an acceptable outcome -- the compiler owes that program a
      // named refusal instead. Gating it here is what stops the next one
      // reading as one more expected line in a summary nobody diffs.
      // `wrong` stays ungated, as it has been for every earlier arc.
      const crashes = results.filter((result) => result.bucket === "emitted_crash");
      if (crashes.length > 0) {
        process.stderr.write(
          `SWEEP GATE: ${crashes.length} emitted module(s) crashed on a schedule the oracle completed: ${crashes.map((result) => result.name).join(", ")}\n`,
        );
        process.exitCode = 1;
      }
    },
    error: (failure: unknown) => {
      process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
      process.exit(1);
    },
  });
}

if (SHARD_FILE !== undefined) shard_main(SHARD_FILE);
else main();
