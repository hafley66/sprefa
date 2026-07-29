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
 * demand rel's +deltas, spawn the template once per unanswered witness, decode
 * stdout into the declared output columns, submit the result as an ordinary
 * arrival on the response rel. Nothing in the engine learns the word "host".
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
  mergeMap,
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
  IShHostRunner,
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
  return typeof value === "number" ? String(value) : value;
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

// ─────────────────────────────────────────────────────────────────────────────
// Output decode. Three shapes, tried in order: a JSON array of objects, JSON
// lines of objects, whitespace-separated columns. The declared column type is
// what converts a value, never a guess from a neighbouring row's text (that
// cross-contamination IS failure class 36).
// ─────────────────────────────────────────────────────────────────────────────

function coerce(host: string, column: IHostColumnPlan, raw: unknown): IRowValue {
  if (column.type === "int") {
    const value = typeof raw === "number" ? raw : Number(String(raw ?? "").trim());
    if (!Number.isFinite(value)) {
      throw new Error(
        `sh host '${host}' produced a non-numeric value for int column '${column.name}': ${JSON.stringify(raw)}`,
      );
    }
    return value;
  }
  if (typeof raw === "string") return raw;
  if (raw === null || raw === undefined) return "";
  if (typeof raw === "number" || typeof raw === "boolean") return String(raw);
  // A json-typed column keeps the document as its canonical text: the emitted
  // SQL reads json columns through json1, so the value must stay parseable.
  return JSON.stringify(raw);
}

function objectRow(host: string, item: Record<string, unknown>, outputs: readonly IHostColumnPlan[]): IRowValue[] {
  const byName = outputs.every((column) => column.name in item);
  const positional = Object.values(item);
  return outputs.map((column, index) => coerce(host, column, byName ? item[column.name] : positional[index]));
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

/** ONE row, one VALUE per line when the line count matches the output column
 *  count exactly (`printf '%s\n%s\n%s'`, ghcacher's own template shape): a
 *  field's own value routinely contains internal whitespace, so splitting such
 *  a line into words shreds it. Otherwise one row per line, columns split on
 *  whitespace. */
function parseWhitespace(host: string, text: string, outputs: readonly IHostColumnPlan[]): IRowValue[][] {
  const lines = text.split("\n").map((line) => line.trim()).filter((line) => line.length > 0);
  if (outputs.length > 1 && lines.length === outputs.length) {
    return [outputs.map((column, index) => coerce(host, column, lines[index] ?? ""))];
  }
  return lines.map((line) => {
    const parts = line.split(/\s+/);
    return outputs.map((column, index) => coerce(host, column, parts[index] ?? ""));
  });
}

function decodeOutput(host: string, stdout: string, outputs: readonly IHostColumnPlan[]): IRowValue[][] {
  const trimmed = stdout.trim();
  if (trimmed.length === 0) return [];
  if (outputs.length === 0) return [];
  const items = parseJsonItems(trimmed);
  if (items) {
    return items.map((item) =>
      item !== null && typeof item === "object" && !Array.isArray(item)
        ? objectRow(host, item as Record<string, unknown>, outputs)
        : outputs.map((column, index) => coerce(host, column, Array.isArray(item) ? item[index] : item)),
    );
  }
  return parseWhitespace(host, trimmed, outputs);
}

// ─────────────────────────────────────────────────────────────────────────────
// ShHostRunner.
// ─────────────────────────────────────────────────────────────────────────────

/** One demand row, already split into the parts the run needs. */
type HostDemand = {
  readonly plan: IHostPlan;
  readonly witnessDigest: string;
  readonly inputs: ReadonlyMap<string, IRowValue>;
};

export class ShHostRunner implements IShHostRunner {
  readonly effects$: Observable<IHostEffectDone>;

  private readonly claimed = new Set<string>();

  constructor(
    private readonly engine: ILiveEngine,
    private readonly seam: ISqlSeam,
    plans: readonly IHostPlan[],
  ) {
    const live = plans.filter((plan) => plan.execution === "live_sh");
    const refused = plans.filter((plan) => plan.execution !== "live_sh");
    this.effects$ =
      live.length === 0 && refused.length === 0
        ? EMPTY
        : merge(
            // An executor this runtime does not know is named, once, rather
            // than skipped in silence.
            from(refused).pipe(
              map((plan): IHostEffectDone => {
                throw new Error(`unknown host executor '${plan.execution}' for sh host '${plan.name}'`);
              }),
            ),
            merge(this.bootDemand$(live), this.liveDemand$(live)).pipe(
              filter((demand) => this.claimOnce(demand)),
              concatMap((demand) => this.runOnce(demand)),
            ),
          );
  }

  /** Boot replay: every live demand row, minus the witnesses the durable cache
   *  already answered. `defer` holds the scan to subscribe time. */
  private bootDemand$(plans: readonly IHostPlan[]): Observable<HostDemand> {
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
                  ServeTrace.effect(plan.name, demand.witnessDigest, "cache_hit", 0, 0);
                  return false;
                }),
              ),
            ),
          ),
        ),
      ),
    );
  }

  /** The live half: this tick's +deltas on each demand rel. */
  private liveDemand$(plans: readonly IHostPlan[]): Observable<HostDemand> {
    const planByRel = new Map(plans.map((plan) => [plan.demandRel, plan]));
    return this.engine.ticks$.pipe(
      mergeMap((outcome) =>
        from(
          outcome.deltas.rels.flatMap((delta) => {
            const plan = planByRel.get(delta.rel);
            return plan ? delta.add.map((row) => this.demandOf(plan, row)) : [];
          }),
        ),
      ),
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
    const key = `${demand.plan.name} ${demand.witnessDigest}`;
    if (this.claimed.has(key)) return false;
    this.claimed.add(key);
    return true;
  }

  /** One witness: claim it durably, spawn, decode, submit the arrival, settle
   *  the cache row. Every failure lands as an 'error' cache row and one
   *  reported effect; the stream never dies. */
  private runOnce(demand: HostDemand): Observable<IHostEffectDone> {
    const { plan, witnessDigest } = demand;
    const startedAt = performance.now();
    const responseColumns = this.engine.program.relColumns[plan.responseRel] ?? [];
    return WitnessCache.claim(this.seam, plan.name, witnessDigest).pipe(
      concatMap(() =>
        runShellLine(plan.name, fillTemplate(plan.template, demand.inputs), envForInputs(demand.inputs)),
      ),
      concatMap((stdout) => {
        const outputRows = decodeOutput(plan.name, stdout, plan.outputs);
        const arrivals: IArrivalRow[] = outputRows.map((outputRow) => ({
          rel: plan.responseRel,
          sign: "add" as const,
          row: responseColumns.map((column) => {
            if (column === "witness_digest") return witnessDigest;
            const input = demand.inputs.get(column);
            if (input !== undefined) return input;
            return outputRow[plan.outputs.findIndex((output) => output.name === column)] ?? "";
          }),
        }));
        // A host that answers with zero rows still settles its cache row: the
        // response rel gets nothing, so only the cache records that this
        // witness was asked and answered.
        const landed: Observable<unknown> =
          arrivals.length === 0 ? of(undefined) : this.engine.submit(arrivals).pipe(toArray());
        return landed.pipe(
          concatMap(() => WitnessCache.settle(this.seam, plan.name, witnessDigest, "done", arrivals.length)),
          map((): IHostEffectDone => {
            ServeTrace.effect(plan.name, witnessDigest, "done", arrivals.length, performance.now() - startedAt);
            return { host: plan.name, witnessDigest, responseRows: arrivals.length, outcome: "done" };
          }),
        );
      }),
      catchError((failure: unknown) =>
        WitnessCache.settle(this.seam, plan.name, witnessDigest, "error", 0).pipe(
          catchError(() => of(undefined)),
          map((): IHostEffectDone => {
            ServeTrace.effect(plan.name, witnessDigest, "error", 0, performance.now() - startedAt, failure);
            return { host: plan.name, witnessDigest, responseRows: 0, outcome: "error" };
          }),
        ),
      ),
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
