/**
 * serveHelpers.ts — shared plumbing for the served-engine receipts.
 *
 * Each receipt owns ONE `serveTsv2(...)` subscription (the app is cold; a test
 * is just another single terminal subscriber, exactly as v6/dl's server tests
 * are). Everything else here is a plain http client and the oracle door.
 */

import { spawnSync } from "node:child_process";
import http from "node:http";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import type { SchedulerLike, Subscription } from "rxjs";

import { serveTsv2 } from "../serve/4_http.ts";
import type { IArrivalBatch, IServeEvent, ITickOutcome } from "../runtime/types.ts";

const DL6_ORACLE_PL = fileURLToPath(new URL("../../prolog/compile/scripts/dl6_oracle.pl", import.meta.url));

export interface HttpResult {
  readonly statusCode: number;
  readonly body: string;
}

export interface ServedFixture {
  readonly port: number;
  readonly events: readonly IServeEvent[];
  readonly activeSubscribeCount: () => number;
  readonly stop: () => Promise<void>;
}

export function request(port: number, path: string, method: string, body?: string): Promise<HttpResult> {
  return new Promise((resolve, reject) => {
    const outgoing = http.request(
      { hostname: "127.0.0.1", port, path, method, agent: false, headers: { "content-type": "application/json" } },
      (response) => {
        let text = "";
        response.on("data", (chunk: Buffer) => {
          text += chunk.toString();
        });
        response.on("end", () => resolve({ statusCode: response.statusCode ?? 0, body: text }));
      },
    );
    outgoing.on("error", reject);
    if (body !== undefined) outgoing.write(body);
    outgoing.end();
  });
}

/** Start one served process in-process and wait until it is listening. */
export function startServed(port: number, scheduler?: SchedulerLike, dbUrl = ":memory:"): Promise<ServedFixture & { running: Subscription }> {
  return new Promise((resolve, reject) => {
    const events: IServeEvent[] = [];
    let settled = false;
    const running = serveTsv2({ dbUrl, port, scheduler }).subscribe({
      next: (event) => {
        events.push(event);
        if (event.kind === "listening" && !settled) {
          settled = true;
          resolve({
            port: event.port,
            events,
            activeSubscribeCount: event.activeSubscribeCount,
            running,
            stop: async () => {
              running.unsubscribe();
              await new Promise<void>((done) => setTimeout(done, 25));
            },
          });
        }
      },
      error: (failure: unknown) => {
        if (!settled) reject(failure instanceof Error ? failure : new Error(String(failure)));
      },
    });
  });
}

export function postProgram(port: number, source: string): Promise<HttpResult> {
  return request(port, "/program", "POST", source);
}

export interface TickReply {
  readonly ticks: readonly { readonly tick: number; readonly line: string }[];
}

export async function postArrivals(port: number, batch: IArrivalBatch): Promise<TickReply> {
  const result = await request(port, "/arrivals", "POST", JSON.stringify({ batch }));
  if (result.statusCode !== 200) throw new Error(`POST /arrivals -> ${result.statusCode} ${result.body}`);
  return JSON.parse(result.body) as TickReply;
}

/**
 * The oracle's tick log for the same program text and the same schedule file
 * the http client posted from. `compile/scripts/dl6_oracle.pl` calls
 * conformance/ticklog.pl's own `print_ticklog/3`, so this is the reference
 * engine's answer, not a second implementation of it.
 */
export function oracleLog(programSource: string, schedule: readonly IArrivalBatch[]): string {
  const workDir = mkdtempSync(join(tmpdir(), "tsv2-serve-oracle-"));
  const programPath = join(workDir, "program.dl6");
  const schedulePath = join(workDir, "schedule.json");
  writeFileSync(programPath, programSource, "utf8");
  writeFileSync(schedulePath, JSON.stringify(schedule), "utf8");
  const run = spawnSync(
    "swipl",
    ["-q", "-l", DL6_ORACLE_PL, "-g", `oracle('${programPath}', '${schedulePath}')`, "-g", "halt"],
    { encoding: "utf8" },
  );
  if (run.status !== 0) throw new Error(`dl6_oracle failed (${run.status}): ${run.stderr}`);
  return run.stdout;
}

/** The served log as one text, in tick order, exactly as the oracle prints it
 *  (one line per tick, LF terminated). */
export function servedLog(replies: readonly TickReply[]): string {
  return replies.flatMap((reply) => reply.ticks.map((entry) => entry.line)).map((line) => `${line}\n`).join("");
}

export function tickEvents(events: readonly IServeEvent[]): readonly ITickOutcome[] {
  return events.flatMap((event) => (event.kind === "tick" ? [event.outcome] : []));
}

export function logOfTicks(outcomes: readonly ITickOutcome[]): string {
  return outcomes.map((outcome) => `${outcome.line}\n`).join("");
}

/**
 * Rebuild the arrival SCHEDULE a served run actually consumed, one batch per
 * tick, from the ticks themselves: an arrival-target rel's boundary delta at a
 * tick IS the world's push at that tick (nothing derives such a rel). That is
 * what lets a run whose inputs were produced live -- a bind's wall-clock bucket,
 * a host's own stdout -- be replayed through the oracle as a schedule and graded
 * byte for byte, instead of only up to some deterministic prefix.
 *
 * One honest limit: an arrival that lands a row the rel ALREADY holds produces
 * no boundary delta, so it is invisible here. A program whose world pushes
 * duplicate rows needs its schedule recorded at the http seam instead.
 */
export function scheduleFromTicks(
  outcomes: readonly ITickOutcome[],
  arrivalTargets: readonly string[],
): readonly IArrivalBatch[] {
  const targets = new Set(arrivalTargets);
  return outcomes.map((outcome) =>
    outcome.deltas.rels
      .filter((delta) => targets.has(delta.rel))
      .flatMap((delta) => [
        ...delta.del.map((row) => ({ rel: delta.rel, sign: "del" as const, row })),
        ...delta.add.map((row) => ({ rel: delta.rel, sign: "add" as const, row })),
      ]),
  );
}
