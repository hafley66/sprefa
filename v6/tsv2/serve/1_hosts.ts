/** Live `sh` host execution for the served engine.
 *
 * The compiler provides demand and response relations:
 *
 *   __host_demand_<name>(identity_digest, witness_digest, inputs..., salts...)
 *   __host_response_<name>(witness_digest, inputs..., outputs...)
 *
 * The demand rel is a derived level rel. The response
 * rel is an arrival target. So a live host is exactly one loop: read the
 * demand rel's +deltas, group compatible extractor projections inside that
 * frontier, spawn once per invocation key, decode stdout into each declared
 * output shape, and submit the results as ordinary arrivals on response rels.
 * Witnesses are deduplicated in process and across restarts:
 *   - in process, a Set of claimed witnesses. RX-H1 spells this
 *     `groupBy(witness) -> take(1)`; a groupBy over an endless tick stream
 *     retains one group object per witness forever, and the Set is the same
 *     dedupe with the retention made explicit and bounded to what it holds.
 *   - across restarts, `__host_witness` (IWitnessCache below). The response rel
 *     alone cannot serve as that cache: a host that legitimately answers with
 *     ZERO rows leaves no response row behind and would refire on every boot.
 *
 * Demand rows are durable while deltas are not, so at subscribe time every
 * live demand row is replayed through the same
 * pipeline. The durable cache turns already-answered witnesses into no-ops, so
 * "replay everything" is correct without a separate unanswered-demand query --
 * replays through the same pipeline.
 *
 * Output columns carry declared types, so invalid values are rejected by name.
 */

import { spawn } from "node:child_process";

import {
  EMPTY,
  Observable,
  catchError,
  concatMap,
  defer,
  filter,
  from,
  map,
  merge,
  of,
  toArray,
} from "rxjs";

import { select_rows } from "../runtime/rows.ts";
import type {
  IArrivalRow,
  IHostColumnPlan,
  IHostEffectDone,
  IHostPlan,
  ILiveEngine,
  IRow,
  IRowValue,
  IHostRunner,
  ISqlSeam,
  IWitnessCache,
  IWitnessRows,
} from "../runtime/types.ts";
import { ServeTrace } from "./0_trace.ts";
import { base64_to_bytes } from "../runtime/boundary.ts";

const WITNESS_TABLE = "__host_witness";

// Durable witness cache.

export const WitnessCache: IWitnessCache = {
  ddl(): readonly string[] {
    return [
      `CREATE TABLE IF NOT EXISTS "${WITNESS_TABLE}" (` +
        `"host" TEXT NOT NULL, "witness_digest" TEXT NOT NULL, ` +
        `"state" TEXT NOT NULL, "response_rows" INTEGER NOT NULL DEFAULT 0, ` +
        `PRIMARY KEY ("host", "witness_digest")) WITHOUT ROWID`,
    ];
  },

  clear_dead_locks(seam: ISqlSeam): Observable<void> {
    return seam.runner
      .execute(seam.db, `DELETE FROM "${WITNESS_TABLE}" WHERE "state" = 'pending'`)
      .pipe(map(() => undefined));
  },

  answered(seam: ISqlSeam, host: string): Observable<ReadonlySet<string>> {
    return seam.runner
      .execute(seam.db, {
        sql: `SELECT "witness_digest" FROM "${WITNESS_TABLE}" WHERE "host" = ? AND "state" = 'done'`,
        args: [host],
      })
      .pipe(map((result) => new Set(result.rows.map((row) => String(row.witness_digest)))));
  },

  claim(seam: ISqlSeam, host: string, witness_digest: string): Observable<void> {
    return seam.runner
      .execute(seam.db, {
        sql:
          `INSERT INTO "${WITNESS_TABLE}" ("host", "witness_digest", "state") VALUES (?, ?, 'pending') ` +
          `ON CONFLICT("host", "witness_digest") DO NOTHING`,
        args: [host, witness_digest],
      })
      .pipe(map(() => undefined));
  },

  settle(
    seam: ISqlSeam,
    host: string,
    witness_digest: string,
    state: "done" | "error",
    rows: number,
  ): Observable<void> {
    return seam.runner
      .execute(seam.db, {
        sql:
          `INSERT INTO "${WITNESS_TABLE}" ("host", "witness_digest", "state", "response_rows") VALUES (?, ?, ?, ?) ` +
          `ON CONFLICT("host", "witness_digest") DO UPDATE SET "state" = excluded."state", ` +
          `"response_rows" = excluded."response_rows"`,
        args: [host, witness_digest, state, BigInt(rows)],
      })
      .pipe(map(() => undefined));
  },
};

// ─────────────────────────────────────────────────────────────────────────────
// Template fill + spawn. `{col}` splices the value into the command line,
// ESCAPED for the quoting context it lands in; `$col` is left in the text and
// exported as an environment variable so the child's own shell expands it
// (1_host_expand.pl `validate_template/3` already refuses a template that
// references an output or an unknown column, so nothing here has to re-check
// the reference set).
// ─────────────────────────────────────────────────────────────────────────────

function shell_text(value: IRowValue): string {
  return String(value);
}

/** The shell quoting context a `{col}` placeholder sits in, which is what
 *  decides how its value must be escaped. */
type QuoteContext = "bare" | "single" | "double";

/**
 * POSIX sh escaping of one value FOR THE CONTEXT IT LANDS IN.
 *
 * `{col}` used to splice raw into a command line that `runShellLine` spawns
 * with `shell: true`, which made every host input arbitrary code: the review's
 * probe created a file on disk from an ordinary arrival, and host inputs are
 * file paths and globs read off disk. Receipt and the five payloads:
 * tests/hostTemplateQuoting.test.ts.
 *
 * Three escapings and not one, because all three contexts are in live use and
 * the usual advice (wrap in single quotes) is wrong in two of them: inside
 * `'...'` it yields `''value''`, and inside `"..."` it puts literal quote
 * characters into the child's output. Either would break byte identity for
 * goldens that depend on the exact text a host prints.
 *
 *   single  everything is literal until the next `'`, so the only escape needed
 *           is `'` itself: close, emit an escaped quote, reopen.
 *   double  `\`, backtick, `$` and `"` are the four characters the shell still
 *           acts on; backslash-escape them and the rest is literal.
 *   bare    nothing is safe, so the value is wrapped in single quotes. That
 *           also fixes a latent bug: a bare `{path}` holding a space used to be
 *           word-split into two arguments.
 *
 * A value carrying no metacharacters comes out unchanged in all three, which is
 * what keeps every shipped template's output byte identical.
 */
function escape_for_shell(value: string, context: QuoteContext): string {
  if (context === "single") return value.split("'").join(`'\\''`);
  if (context === "double") return value.replace(/[\\$`"]/g, (character) => `\\${character}`);
  return `'${value.split("'").join(`'\\''`)}'`;
}

/**
 * Walk the template as the shell would, tracking quote state, and splice each
 * value escaped for the state it lands in.
 *
 * The scan models only what a template can contain: `\` escapes the next
 * character outside single quotes, `'` toggles single quoting unless already
 * inside double, `"` toggles double unless already inside single. A `{name}`
 * that is not an input column is left alone, which is what lets
 * `printf '{"path":"%s"}\n'` (v5-git-diags.dl6:108) keep its JSON braces.
 */
function fillTemplate(template: string, inputs: ReadonlyMap<string, IRowValue>): string {
  let context: QuoteContext = "bare";
  let filled = "";
  let index = 0;
  while (index < template.length) {
    const character = template[index]!;
    if (character === "\\" && context !== "single") {
      filled += character + (template[index + 1] ?? "");
      index += 2;
      continue;
    }
    if (character === "'" && context !== "double") {
      context = context === "single" ? "bare" : "single";
      filled += character;
      index += 1;
      continue;
    }
    if (character === '"' && context !== "single") {
      context = context === "double" ? "bare" : "double";
      filled += character;
      index += 1;
      continue;
    }
    if (character === "{") {
      const close = template.indexOf("}", index);
      const name = close === -1 ? null : template.slice(index + 1, close);
      const value = name === null ? undefined : inputs.get(name);
      if (value !== undefined) {
        filled += escape_for_shell(shell_text(value), context);
        index = close + 1;
        continue;
      }
    }
    filled += character;
    index += 1;
  }
  return filled;
}

function envForInputs(inputs: ReadonlyMap<string, IRowValue>): Record<string, string> {
  const variables: Record<string, string> = {};
  for (const [name, value] of inputs) variables[name] = shell_text(value);
  return variables;
}

function runShellLine(host: string, commandLine: string, env: Record<string, string>): Observable<string> {
  return new Observable<string>((subscriber) => {
    const child = spawn(commandLine, [], { shell: true, env: { ...process.env, ...env } });
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
    });
    child.on("error", (failure) => subscriber.error(failure));
    child.on("close", (code) => {
      if (code !== 0) {
        subscriber.error(new Error(`sh host '${host}' exited ${code}: ${stderr.trim()}`));
        return;
      }
      subscriber.next(stdout);
      subscriber.complete();
    });
    return () => child.kill();
  });
}

type HostExecutor = (host: string, commandLine: string, env: Record<string, string>) => Observable<string>;

function runSprefaExtract(host: string, commandLine: string, env: Record<string, string>): Observable<string> {
  return runShellLine(host, commandLine, env);
}

/**
 * The Rust target links Soopy's stage store and commit engine in-process.
 * The TypeScript target has no Rust FFI boundary, so it returns one ordinary
 * capability-refusal row instead of spawning the template or duplicating the
 * mutation engine. The declared source-mutation fixture exposes this outcome
 * as data for callers that selected the TS target.
 */
function runSoopyMutation(host: string): Observable<string> {
  const detail = "soopy_mutation requires the Rust runtime target";
  const row = host === "source_stage"
    ? { stage_id: "", outcome: "unsupported", detail, document: [] }
    : { outcome: "unsupported", detail, document: {} };
  return of(JSON.stringify(row));
}

/** Executor registry. `sprefa_extract` retains the declaration's current
 * subprocess command while isolating the V6.2 process boundary from DL6.
 * `sprefa_extract_repo` is the repo-scoped twin (ruling repo_column_spelling =
 * distinct_name_hosts): same subprocess, different declared contract, and the
 * compiler picks between them from the template (registry.pl host_execution). */
export const HostExecutors: ReadonlyMap<string, HostExecutor> = new Map([
  ["shell", runShellLine],
  ["sprefa_extract", runSprefaExtract],
  ["sprefa_extract_repo", runSprefaExtract],
  ["soopy_mutation", runSoopyMutation],
]);

/**
 * The executors whose invocations may be FOLDED (see groupInvocations). This is
 * a set and not a `=== "sprefa_extract"` test because the repo-scoped twin
 * earns the fold on the same argument -- its command is a pure read of a file
 * that the demand row already names -- and a name test that silently excluded
 * it would have cost one subprocess per named projection with nothing saying so.
 */
const ApplicativeExecutors: ReadonlySet<string> = new Set(["sprefa_extract", "sprefa_extract_repo"]);

// ─────────────────────────────────────────────────────────────────────────────
// Output decode. Three shapes, tried in order: a JSON array of objects, JSON
// lines of objects, whitespace-separated columns. The declared column type is
// what converts a value, never a guess from a neighbouring row's text (that
// cross-contamination IS failure class 36).
// ─────────────────────────────────────────────────────────────────────────────

function coerce(host: string, column: IHostColumnPlan, raw: unknown): IRowValue {
  if (column.type === "bytes") {
    if (raw !== null && typeof raw === "object" && !Array.isArray(raw) && Object.keys(raw).length === 1 && typeof (raw as { readonly $bytes?: unknown }).$bytes === "string") {
      try {
        return base64_to_bytes((raw as { readonly $bytes: string }).$bytes);
      } catch {
        throw new Error(`host '${host}' produced invalid_bytes_base64 for bytes column '${column.name}'`);
      }
    }
    throw new Error(`host '${host}' produced bytes_host_transport_unsupported for bytes column '${column.name}'`);
  }
  if (column.type === "bool") {
    if (raw === true || raw === "true") return true;
    if (raw === false || raw === "false") return false;
    throw new Error(
      `sh host '${host}' produced a non-boolean value for bool column '${column.name}': ${JSON.stringify(raw)}`,
    );
  }
  if (column.type === "float") {
    const value = typeof raw === "number" ? raw : Number(String(raw ?? "").trim());
    if (!Number.isFinite(value)) {
      throw new Error(
        `sh host '${host}' produced a non-finite value for float column '${column.name}': ${JSON.stringify(raw)}`,
      );
    }
    return Object.is(value, -0) ? 0 : value;
  }
  if (column.type === "int") {
    const value = typeof raw === "number" ? raw : Number(String(raw ?? "").trim());
    if (!Number.isSafeInteger(value)) {
      throw new Error(
        `sh host '${host}' produced a non-integer value for int column '${column.name}': ${JSON.stringify(raw)}`,
      );
    }
    return value;
  }
  // Every non-int host value crosses the arrival seam as text. For json and
  // declared struct columns, object stdout becomes JSON text here; the emitted
  // ref-column map tells StructPlane which values to parse and intern.
  if (typeof raw === "string") return raw;
  if (raw === null || raw === undefined) return "";
  if (typeof raw === "number" || typeof raw === "boolean") return String(raw);
  return JSON.stringify(raw);
}

function carriesEveryColumn(item: Record<string, unknown>, outputs: readonly IHostColumnPlan[]): boolean {
  return outputs.every((column) => column.name in item && item[column.name] !== null);
}

function objectRow(host: string, item: Record<string, unknown>, outputs: readonly IHostColumnPlan[]): IRowValue[] {
  return outputs.map((column) => coerce(host, column, item[column.name]));
}

function parseJsonItems(text: string): unknown[] | null {
  try {
    const parsed: unknown = JSON.parse(text);
    if (Array.isArray(parsed)) return parsed;
  } catch {
    // fall through to the JSON-lines attempt
  }
  const lines = text.split("\n").filter((line) => line.trim().length > 0);
  if (lines.length === 0) return null;
  const items: unknown[] = [];
  for (const line of lines) {
    try {
      items.push(JSON.parse(line));
    } catch {
      return null;
    }
  }
  return items;
}

/**
 * Whitespace stdout has TWO readings and no marker to pick between them, so the
 * order they are tried in is the whole semantics:
 *
 *   GRID       one row per line, N whitespace fields per line. `files_at`'s
 *              `printf '%s %s\n' "$entry" "$oid"` per tracked path.
 *   PER-COLUMN one row total, one VALUE per line. ghcacher's
 *              `printf '%s\n%s\n%s'`, where a value routinely carries internal
 *              whitespace and word-splitting it shreds it.
 *
 * The GRID reading goes first, and only when it is UNAMBIGUOUS: every nonempty
 * line splits into exactly the declared column count. Nothing about such a
 * stdout has to be guessed -- the shape is already the answer.
 *
 * PER-COLUMN then takes the line-count match, which is what it always was, and
 * that is the reading a value-with-spaces stream lands in (its lines do NOT
 * split into N fields, which is precisely why word-splitting would shred them).
 *
 * The precedence used to be the other way round, and the cost was silent: a
 * two-column grid host answering exactly TWO rows -- `files_at` on a
 * two-file glob, nothing more exotic -- had its two lines folded into one row
 * whose first column was the entire first line. Right at one file, right at
 * three, wrong at two. That is failure class 36's cross-contamination again,
 * one layer up from the parse that caused it (bug host_grid_answer_folded).
 *
 * What is left ambiguous after this is narrow and stated: a per-column host
 * whose every line happens to hold exactly N words reads as a grid. There is no
 * information in the stdout that separates those two, and the class-36 rule
 * (never guess a value from a neighbouring row's text) says take the reading
 * that keeps each line's fields inside their own row.
 */
function parseWhitespace(host: string, text: string, outputs: readonly IHostColumnPlan[]): IRowValue[][] {
  const lines = text.split("\n").map((line) => line.trim()).filter((line) => line.length > 0);
  const fieldsPerLine = lines.map((line) => line.split(/\s+/));
  const isGrid = fieldsPerLine.every((fields) => fields.length === outputs.length);
  if (isGrid) {
    return fieldsPerLine.map((fields) => outputs.map((column, index) => coerce(host, column, fields[index] ?? "")));
  }
  if (outputs.length > 1 && lines.length === outputs.length) {
    return [outputs.map((column, index) => coerce(host, column, lines[index] ?? ""))];
  }
  return fieldsPerLine.map((fields) => outputs.map((column, index) => coerce(host, column, fields[index] ?? "")));
}

/**
 * A JSON OBJECT stream is a NAMED PROJECTION, and a heterogeneous one is the
 * normal case: `sprefa-extract` interleaves `{"record":"node",...,"name":...}`
 * with `{"record":"site",...,"callee":...}` on one stdout, and a host declaring
 * `-> (record: text, callee: text)` wants the `site` lines and only those.
 *
 * So an object item that does not carry every declared output column (or
 * carries it as JSON null) contributes NO ROW. That is the ruled Option shape
 * read at the world boundary -- absence is row absence, never a null column and
 * never a positional guess at a neighbouring field (positional fallback is what
 * made this a miscompile before: `Object.values(item)[0]` happily wrote a
 * `node` line's span object into a `callee` column).
 *
 * WHERE THE SILENCE IS ANSWERED, since skipping is a silence and this repo does
 * not accept those unnamed. A "nonempty stdout, zero rows" answer cannot be
 * distinguished HERE from a legitimately empty projection -- a source file with
 * no call sites emits only `node` lines, and failing that would break the rail
 * on ordinary input. So it is not decided here at all: `ServeTrace.effect`
 * already publishes the row count of every host run and `__host_witness`
 * durably stores `response_rows`, so a misspelled output column reads as a host
 * that answers zero rows every time, in the self-diagnosis surface that exists
 * for exactly this question. A load-time check is impossible (the compiler
 * cannot know a stream's key set), which is why this is a trace obligation.
 */
function decodeObjectItems(host: string, items: readonly unknown[], outputs: readonly IHostColumnPlan[]): IRowValue[][] {
  const objects = items.filter(
    (item): item is Record<string, unknown> => item !== null && typeof item === "object" && !Array.isArray(item),
  );
  if (objects.length !== items.length) {
    // Mixed or non-object JSON (an array of arrays, or of scalars) keeps the
    // positional reading it always had.
    return items.map((item) =>
      outputs.map((column, index) => coerce(host, column, Array.isArray(item) ? item[index] : item)),
    );
  }
  return objects.filter((item) => carriesEveryColumn(item, outputs)).map((item) => objectRow(host, item, outputs));
}

function decodeOutput(host: string, stdout: string, outputs: readonly IHostColumnPlan[]): IRowValue[][] {
  const trimmed = stdout.trim();
  if (trimmed.length === 0) return [];
  if (outputs.length === 0) return [];
  const items = parseJsonItems(trimmed);
  if (items) return decodeObjectItems(host, items, outputs);
  return parseWhitespace(host, trimmed, outputs);
}

// ─────────────────────────────────────────────────────────────────────────────
// HostRunner.
// ─────────────────────────────────────────────────────────────────────────────

/** One demand row, already split into the parts the run needs. */
type HostDemand = {
  readonly plan: IHostPlan;
  readonly witness_digest: string;
  readonly inputs: ReadonlyMap<string, IRowValue>;
};

type HostInvocation = {
  readonly demands: readonly HostDemand[];
};

type HostProjection = {
  readonly demand: HostDemand;
  readonly arrivals: readonly IArrivalRow[];
  readonly failure?: unknown;
};

function invocationKey(demand: HostDemand): string {
  const orderedInputs = demand.plan.inputs.map((input) => [
    input.name,
    input.type,
    demand.inputs.get(input.name) ?? "",
  ]);
  return JSON.stringify([
    demand.plan.execution,
    demand.plan.template,
    orderedInputs,
  ]);
}

/**
 * The `sprefa_extract` family is applicative at one engine frontier: named
 * projections with the same command and ordered inputs read the same stdout.
 * Generic shell declarations remain singleton invocations because their
 * commands may carry effects even when their text and inputs happen to match.
 */
function groupInvocations(demands: readonly HostDemand[]): readonly HostInvocation[] {
  const groups: HostInvocation[] = [];
  const extractGroupByKey = new Map<string, HostDemand[]>();
  for (const demand of demands) {
    if (!ApplicativeExecutors.has(demand.plan.execution)) {
      groups.push({ demands: [demand] });
      continue;
    }
    const key = invocationKey(demand);
    const group = extractGroupByKey.get(key);
    if (group) {
      group.push(demand);
    } else {
      const created = [demand];
      extractGroupByKey.set(key, created);
      groups.push({ demands: created });
    }
  }
  return groups;
}

export class HostRunner implements IHostRunner {
  readonly effects$: Observable<IHostEffectDone>;

  private readonly claimed = new Set<string>();

  constructor(
    private readonly engine: ILiveEngine,
    private readonly seam: ISqlSeam,
    plans: readonly IHostPlan[],
    private readonly executors: ReadonlyMap<string, HostExecutor> = HostExecutors,
  ) {
    const executable = plans.filter((plan) => executors.has(plan.execution));
    const refused = plans.filter((plan) => !executors.has(plan.execution));
    this.effects$ =
      executable.length === 0 && refused.length === 0
        ? EMPTY
        : merge(
            // An executor this runtime does not know is named, once, rather
            // than skipped in silence.
            from(refused).pipe(
              map((plan): IHostEffectDone => {
                throw new Error(`unknown host executor '${plan.execution}' for host '${plan.name}'`);
              }),
            ),
            merge(this.bootDemand$(executable), this.liveDemand$(executable)).pipe(
              map((batch) => batch.filter((demand) => this.claimOnce(demand))),
              filter((batch) => batch.length > 0),
              concatMap((batch) =>
                from(groupInvocations(batch)).pipe(
                  concatMap((invocation) => this.runInvocation(invocation)),
                ),
              ),
            ),
          );
  }

  /** Boot replay: every live demand row, minus the witnesses the durable cache
   *  already answered. `defer` holds the scan to subscribe time. */
  private bootDemand$(plans: readonly IHostPlan[]): Observable<readonly HostDemand[]> {
    return defer(() =>
      WitnessCache.clear_dead_locks(this.seam).pipe(
        concatMap(() => from(plans)),
        concatMap((plan) =>
          WitnessCache.answered(this.seam, plan.name).pipe(
            concatMap((answered) =>
              this.engine.rows(plan.demand_rel).pipe(
                concatMap((rows) => from(rows.map((row) => this.demandOf(plan, row)))),
                filter((demand) => {
                  if (!answered.has(demand.witness_digest)) return true;
                  this.claimOnce(demand);
                  ServeTrace.effect(plan.name, demand.witness_digest, "cache_hit", 0, 0);
                  return false;
                }),
              ),
            ),
          ),
        ),
        toArray(),
      ),
    );
  }

  /** The live half: this tick's +deltas on each demand rel. */
  private liveDemand$(plans: readonly IHostPlan[]): Observable<readonly HostDemand[]> {
    const planByRel = new Map(plans.map((plan) => [plan.demand_rel, plan]));
    return this.engine.ticks$.pipe(
      map((outcome) =>
        outcome.deltas.rels.flatMap((delta) => {
          const plan = planByRel.get(delta.rel);
          return plan ? delta.add.map((row) => this.demandOf(plan, row)) : [];
        }),
      ),
      filter((batch) => batch.length > 0),
    );
  }

  private demandOf(plan: IHostPlan, row: IRow): HostDemand {
    const columns = this.engine.program.rel_columns[plan.demand_rel] ?? [];
    const inputs = new Map<string, IRowValue>();
    for (const input of plan.inputs) {
      const index = columns.indexOf(input.name);
      inputs.set(input.name, index >= 0 ? (row[index] ?? "") : "");
    }
    const witness_index = columns.indexOf("witness_digest");
    return { plan, witness_digest: String(row[witness_index] ?? ""), inputs };
  }

  private claimOnce(demand: HostDemand): boolean {
    const key = `${demand.plan.name}|${demand.witness_digest}`;
    if (this.claimed.has(key)) return false;
    this.claimed.add(key);
    return true;
  }

  private project(demand: HostDemand, stdout: string): HostProjection {
    const { plan, witness_digest: witnessDigest } = demand;
    try {
      const outputRows = decodeOutput(plan.name, stdout, plan.outputs);
      const response_columns = this.engine.program.rel_columns[plan.response_rel] ?? [];
      const arrivals: IArrivalRow[] = outputRows.map((outputRow, ordinal) => ({
        rel: plan.response_rel,
        sign: "add" as const,
        row: response_columns.map((column) => {
          if (column === "witness_digest") return witnessDigest;
          if (column === "ordinal") return ordinal;
          const input = demand.inputs.get(column);
          if (input !== undefined) return input;
          return outputRow[plan.outputs.findIndex((output) => output.name === column)] ?? "";
        }),
      }));
      return { demand, arrivals };
    } catch (failure: unknown) {
      return { demand, arrivals: [], failure };
    }
  }

  private settleProjection(projection: HostProjection, startedAt: number): Observable<IHostEffectDone> {
    const { plan, witness_digest: witnessDigest } = projection.demand;
    const outcome: "done" | "error" = projection.failure === undefined ? "done" : "error";
    const rows = projection.failure === undefined ? projection.arrivals.length : 0;
    return WitnessCache.settle(this.seam, plan.name, witnessDigest, outcome, rows).pipe(
      map((): IHostEffectDone => {
        ServeTrace.effect(
          plan.name,
          witnessDigest,
          outcome,
          rows,
          performance.now() - startedAt,
          projection.failure,
        );
        return { host: plan.name, witness_digest: witnessDigest, response_rows: rows, outcome };
      }),
      catchError((settleFailure: unknown) => {
        ServeTrace.effect(
          plan.name,
          witnessDigest,
          "error",
          0,
          performance.now() - startedAt,
          settleFailure,
        );
        return of({
          host: plan.name,
          witness_digest: witnessDigest,
          response_rows: 0,
          outcome: "error" as const,
        });
      }),
    );
  }

  private settleInvocationError(
    invocation: HostInvocation,
    failure: unknown,
    startedAt: number,
  ): Observable<IHostEffectDone> {
    return from(invocation.demands).pipe(
      concatMap((demand) =>
        this.settleProjection({ demand, arrivals: [], failure }, startedAt),
      ),
    );
  }

  /** One frontier-compatible invocation: claim every witness, spawn once,
   * project the shared stdout into each response rel, submit all successful
   * projections in one engine batch, then settle every witness separately. */
  private runInvocation(invocation: HostInvocation): Observable<IHostEffectDone> {
    const first = invocation.demands[0];
    if (!first) return EMPTY;
    const startedAt = performance.now();
    return from(invocation.demands).pipe(
      concatMap((demand) =>
        WitnessCache.claim(this.seam, demand.plan.name, demand.witness_digest),
      ),
      toArray(),
      concatMap(() => {
        const executor = this.executors.get(first.plan.execution);
        if (!executor) {
          throw new Error(`unknown host executor '${first.plan.execution}' for host '${first.plan.name}'`);
        }
        return executor(
          first.plan.name,
          fillTemplate(first.plan.template, first.inputs),
          envForInputs(first.inputs),
        );
      }),
      concatMap((stdout) => {
        const projections = invocation.demands.map((demand) => this.project(demand, stdout));
        const arrivals = projections.flatMap((projection) =>
          projection.failure === undefined ? projection.arrivals : [],
        );
        const landed: Observable<unknown> =
          arrivals.length === 0 ? of(undefined) : this.engine.submit(arrivals).pipe(toArray());
        return landed.pipe(
          concatMap(() => from(projections)),
          concatMap((projection) => this.settleProjection(projection, startedAt)),
        );
      }),
      catchError((failure: unknown) => this.settleInvocationError(invocation, failure, startedAt)),
    );
  }
}

/** Read one rel's current rows through an explicit SELECT. Exported for the
 *  endurance receipt, which reads the witness table directly to prove a cached
 *  witness did not refire. */
export const witnessRows: IWitnessRows = (seam: ISqlSeam): Observable<readonly IRow[]> =>
  select_rows(
    seam,
    `SELECT "host", "witness_digest", "state", "response_rows" FROM "${WITNESS_TABLE}" ORDER BY "host", "witness_digest"`,
    ["host", "witness_digest", "state", "response_rows"],
  );
