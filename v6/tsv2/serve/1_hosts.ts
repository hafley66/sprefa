/**
 * 1_hosts.ts — LIVE `sh` host execution for the served engine (hosts phase 2,
 * plans/2026-07-29-runtime-bridge-header.md scope 2). RX-H1 from
 * plans/2026-07-29-hosts-extraction-verdict.md, made real.
 *
 * The shape the compiler already built (1_host_expand.pl):
 *
 *   __host_demand_<name>(identity_digest, witness_digest, inputs..., salts...)
 *   __host_response_<name>(witness_digest, inputs..., outputs...)
 *
 * The demand rel is a DERIVED level rel: rules put rows in it. The response
 * rel is an arrival target. So a live host is exactly one loop: read the
 * demand rel's +deltas, group compatible extractor projections inside that
 * frontier, spawn once per invocation key, decode stdout into each declared
 * output shape, and submit the results as ordinary arrivals on response rels.
 * Nothing in the engine learns the word "host".
 *
 * FIRE ONCE PER WITNESS, two dedupes, both needed:
 *   - in process, a Set of claimed witnesses. RX-H1 spells this
 *     `groupBy(witness) -> take(1)`; a groupBy over an endless tick stream
 *     retains one group object per witness forever, and the Set is the same
 *     dedupe with the retention made explicit and bounded to what it holds.
 *   - across restarts, `__host_witness` (IWitnessCache below). The response rel
 *     alone cannot serve as that cache: a host that legitimately answers with
 *     ZERO rows leaves no response row behind and would refire on every boot.
 *
 * BOOT REPLAY (endurance law, scope 5): demand rows are durable, deltas are
 * not, so at subscribe time every live demand row is replayed through the same
 * pipeline. The durable cache turns already-answered witnesses into no-ops, so
 * "replay everything" is correct without a separate unanswered-demand query --
 * the same reasoning v6/dl's HostRunner records for its own boot branch.
 *
 * OUTPUT DECODE is the F7-hardened shape (docs/failure-modes.md class 36) with
 * one improvement the compiled path affords: the output columns carry DECLARED
 * types, so an int column is parsed as an int and rejected by name when the
 * text is not finite, instead of a per-column guess from the first row's text.
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

import { selectRows } from "../runtime/rows.ts";
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
} from "../runtime/types.ts";
import { ServeTrace } from "./0_trace.ts";

const WITNESS_TABLE = "__host_witness";

// ─────────────────────────────────────────────────────────────────────────────
// The durable witness cache.
// ─────────────────────────────────────────────────────────────────────────────

export const WitnessCache: IWitnessCache = {
  ddl(): readonly string[] {
    return [
      `CREATE TABLE IF NOT EXISTS "${WITNESS_TABLE}" (` +
        `"host" TEXT NOT NULL, "witness_digest" TEXT NOT NULL, ` +
        `"state" TEXT NOT NULL, "response_rows" INTEGER NOT NULL DEFAULT 0, ` +
        `PRIMARY KEY ("host", "witness_digest")) WITHOUT ROWID`,
    ];
  },

  clearDeadLocks(seam: ISqlSeam): Observable<void> {
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

  claim(seam: ISqlSeam, host: string, witnessDigest: string): Observable<void> {
    return seam.runner
      .execute(seam.db, {
        sql:
          `INSERT INTO "${WITNESS_TABLE}" ("host", "witness_digest", "state") VALUES (?, ?, 'pending') ` +
          `ON CONFLICT("host", "witness_digest") DO NOTHING`,
        args: [host, witnessDigest],
      })
      .pipe(map(() => undefined));
  },

  settle(
    seam: ISqlSeam,
    host: string,
    witnessDigest: string,
    state: "done" | "error",
    rows: number,
  ): Observable<void> {
    return seam.runner
      .execute(seam.db, {
        sql:
          `INSERT INTO "${WITNESS_TABLE}" ("host", "witness_digest", "state", "response_rows") VALUES (?, ?, ?, ?) ` +
          `ON CONFLICT("host", "witness_digest") DO UPDATE SET "state" = excluded."state", ` +
          `"response_rows" = excluded."response_rows"`,
        args: [host, witnessDigest, state, BigInt(rows)],
      })
      .pipe(map(() => undefined));
  },
};

// ─────────────────────────────────────────────────────────────────────────────
// Template fill + spawn. `{col}` splices the value straight into the command
// line; `$col` is left in the text and exported as an environment variable so
// the child's own shell expands it (1_host_expand.pl `validate_template/3`
// already refuses a template that references an output or an unknown column,
// so nothing here has to re-check the reference set).
// ─────────────────────────────────────────────────────────────────────────────

function shellText(value: IRowValue): string {
  return String(value);
}

function fillTemplate(template: string, inputs: ReadonlyMap<string, IRowValue>): string {
  let filled = template;
  for (const [name, value] of inputs) filled = filled.split(`{${name}}`).join(shellText(value));
  return filled;
}

function envForInputs(inputs: ReadonlyMap<string, IRowValue>): Record<string, string> {
  const variables: Record<string, string> = {};
  for (const [name, value] of inputs) variables[name] = shellText(value);
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

/** Executor registry. `sprefa_extract` retains the declaration's current
 * subprocess command while isolating the V6.2 process boundary from DL6. */
export const HostExecutors: ReadonlyMap<string, HostExecutor> = new Map([
  ["shell", runShellLine],
  ["sprefa_extract", runSprefaExtract],
]);

// ─────────────────────────────────────────────────────────────────────────────
// Output decode. Three shapes, tried in order: a JSON array of objects, JSON
// lines of objects, whitespace-separated columns. The declared column type is
// what converts a value, never a guess from a neighbouring row's text (that
// cross-contamination IS failure class 36).
// ─────────────────────────────────────────────────────────────────────────────

function coerce(host: string, column: IHostColumnPlan, raw: unknown): IRowValue {
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
 *   GRID       one row per line, N whitespace fields per line. `enumerate_at`'s
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
 * two-column grid host answering exactly TWO rows -- `enumerate_at` on a
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
  readonly witnessDigest: string;
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
 * `sprefa_extract` is applicative at one engine frontier: named projections
 * with the same command and ordered inputs read the same stdout. Generic shell
 * declarations remain singleton invocations because their commands may carry
 * effects even when their text and inputs happen to match.
 */
function groupInvocations(demands: readonly HostDemand[]): readonly HostInvocation[] {
  const groups: HostInvocation[] = [];
  const extractGroupByKey = new Map<string, HostDemand[]>();
  for (const demand of demands) {
    if (demand.plan.execution !== "sprefa_extract") {
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
      WitnessCache.clearDeadLocks(this.seam).pipe(
        concatMap(() => from(plans)),
        concatMap((plan) =>
          WitnessCache.answered(this.seam, plan.name).pipe(
            concatMap((answered) =>
              this.engine.rows(plan.demandRel).pipe(
                concatMap((rows) => from(rows.map((row) => this.demandOf(plan, row)))),
                filter((demand) => {
                  if (!answered.has(demand.witnessDigest)) return true;
                  this.claimOnce(demand);
                  ServeTrace.effect(plan.name, demand.witnessDigest, "cache_hit", 0, 0);
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
    const planByRel = new Map(plans.map((plan) => [plan.demandRel, plan]));
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
    const columns = this.engine.program.relColumns[plan.demandRel] ?? [];
    const inputs = new Map<string, IRowValue>();
    for (const input of plan.inputs) {
      const index = columns.indexOf(input.name);
      inputs.set(input.name, index >= 0 ? (row[index] ?? "") : "");
    }
    const witnessIndex = columns.indexOf("witness_digest");
    return { plan, witnessDigest: String(row[witnessIndex] ?? ""), inputs };
  }

  private claimOnce(demand: HostDemand): boolean {
    const key = `${demand.plan.name}|${demand.witnessDigest}`;
    if (this.claimed.has(key)) return false;
    this.claimed.add(key);
    return true;
  }

  private project(demand: HostDemand, stdout: string): HostProjection {
    const { plan, witnessDigest } = demand;
    try {
      const outputRows = decodeOutput(plan.name, stdout, plan.outputs);
      const responseColumns = this.engine.program.relColumns[plan.responseRel] ?? [];
      const arrivals: IArrivalRow[] = outputRows.map((outputRow, ordinal) => ({
        rel: plan.responseRel,
        sign: "add" as const,
        row: responseColumns.map((column) => {
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
    const { plan, witnessDigest } = projection.demand;
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
        return { host: plan.name, witnessDigest, responseRows: rows, outcome };
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
          witnessDigest,
          responseRows: 0,
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
        WitnessCache.claim(this.seam, demand.plan.name, demand.witnessDigest),
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
export function witnessRows(seam: ISqlSeam): Observable<readonly IRow[]> {
  return selectRows(
    seam,
    `SELECT "host", "witness_digest", "state", "response_rows" FROM "${WITNESS_TABLE}" ORDER BY "host", "witness_digest"`,
    ["host", "witness_digest", "state", "response_rows"],
  );
}
