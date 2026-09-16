/**
 * flatten-probe.ts -- the four flatteners, run over sprefa's OWN tick stream.
 *
 * This file imports `rxjs` and node builtins and NOTHING from this repository,
 * the same discipline rxoracle's leg A holds and for the same reason: what it
 * claims is that sprefa's demand deltas are a sufficient input to every
 * flattener, and a claim like that is worthless if the checker shares code with
 * the thing being checked. It speaks to a running `bop serve` over HTTP.
 *
 * WHAT IT MEASURES. `GET /ticks` carries, per tick, the add and del rows of
 * every relation including the compiler-minted `__host_demand_<name>`. That
 * stream is mapped to one event per demand row:
 *
 *     {step, witnessDigest, inputs, sign}      sign is "add" or "del"
 *
 * and then handed to four different flattening strategies, each of which spawns
 * its own child processes and appends to its own ledger. The ledgers are the
 * receipt. The engine is not modified, not imported, and does not know this file
 * exists: the probe runs the effects a second time, beside the engine's own, so
 * what it proves is about the SIGNAL, not about a patch.
 *
 * THE POINT. `concat` here reproduces the ledger `serve/1_hosts.ts` produces,
 * which is what makes the other three credible: same input, same spawn shape,
 * one operator changed.
 */

import { spawn } from "node:child_process";
import { appendFileSync, writeFileSync } from "node:fs";
import * as http from "node:http";

import {
  EMPTY,
  Observable,
  Subject,
  concatMap,
  exhaustMap,
  filter,
  groupBy,
  mergeMap,
  switchMap,
} from "rxjs";

// ─────────────────────────────────────────────────────────────────────────────
// The demand event stream, read off the served engine's SSE.
// ─────────────────────────────────────────────────────────────────────────────

interface DemandEvent {
  readonly tick: number;
  readonly witnessDigest: string;
  readonly slot: string;
  readonly inputs: readonly string[];
  readonly sign: "add" | "del";
}

/** `identity|fetch_body|route_id:text=r1` -> `fetch_body`. The host name is the
 *  only part of a digest this probe reads; it never parses the inputs out of a
 *  digest, it takes them from the row's own trailing columns. */
function hostOfDigest(digest: string): string {
  return digest.split("|")[1] ?? "";
}

/**
 * THE SLOT KEY, and the honest note about it. `switch` has to know which inner
 * a new inner REPLACES. This probe derives that from the demand row's first
 * input column, which for the corpus below is the session id.
 *
 * That derivation is the probe's, not the language's: the demand rel carries
 * `identity_digest` and `witness_digest` and both are content-addressed over
 * the inputs, so two demands competing for one slot get two unrelated digests
 * and NO column says they compete. Section "the slot key" of the verdict is
 * about exactly this. It matters for `exhaust` and for a keyed `switch`; the
 * `switch` variant this file grades as the recommended one does NOT use it, and
 * that is the finding.
 */
function slotOf(inputs: readonly string[]): string {
  return inputs[0] ?? "";
}

/**
 * THE INTRA-TICK SIGN ORDER, which is a free choice and not a detail.
 *
 * A supersession puts the winner's `add` and the loser's `del` in the SAME
 * tick (receipt R1: both are on tick 3). `runtime/ticklog.ts` sorts each half
 * lexicographically and emits `add` before `del` per relation, so the tick log
 * is a SET per relation per tick and carries no sequence between the two
 * halves. Nothing in the data says whether the teardown or the new start
 * happens first.
 *
 * So the probe takes it as a parameter and the receipts run `switch` both
 * ways. That difference is invisible while concurrency is unbounded and
 * becomes a real decision the moment it is bounded: `del` first frees the slot
 * before the winner claims it, `add` first can momentarily exceed the bound.
 */
type SignOrder = readonly ["add" | "del", "add" | "del"];

const SIGN_ORDERS: ReadonlyMap<string, SignOrder> = new Map([
  ["add-first", ["add", "del"]],
  ["del-first", ["del", "add"]],
]);

function demandEvents(
  port: number,
  hostName: string,
  columnCount: number,
  signOrder: SignOrder,
): Observable<DemandEvent> {
  return new Observable<DemandEvent>((subscriber) => {
    const demandRel = `__host_demand_${hostName}`;
    const request = http.get(
      { host: "127.0.0.1", port, path: "/ticks" },
      (response) => {
        let buffer = "";
        response.on("data", (chunk: Buffer) => {
          buffer += chunk.toString();
          let newline = buffer.indexOf("\n");
          while (newline >= 0) {
            const line = buffer.slice(0, newline);
            buffer = buffer.slice(newline + 1);
            if (line.startsWith("data: ")) {
              const parsed = JSON.parse(line.slice(6)) as {
                tick: number;
                deltas: Record<string, { add: string[][]; del: string[][] }>;
              };
              const delta = parsed.deltas[demandRel];
              if (delta) {
                for (const sign of signOrder) {
                  for (const row of delta[sign]) {
                    const inputs = row.slice(2, 2 + columnCount).map(String);
                    subscriber.next({
                      tick: parsed.tick,
                      witnessDigest: String(row[1]),
                      slot: slotOf(inputs),
                      inputs,
                      sign,
                    });
                  }
                }
              }
            }
            newline = buffer.indexOf("\n");
          }
        });
        response.on("end", () => subscriber.complete());
      },
    );
    request.on("error", (failure) => subscriber.error(failure));
    return () => request.destroy();
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// The effect. Same spawn shape `serve/1_hosts.ts` uses, including the teardown:
// the returned function kills the child, so an unsubscribe of this observable
// terminates the subprocess. That line already exists in the production runner
// (`1_hosts.ts` `runShellLine`); what this probe changes is whether anything
// ever calls it.
// ─────────────────────────────────────────────────────────────────────────────

function runHost(ledger: string, napSeconds: string, event: DemandEvent): Observable<string> {
  return new Observable<string>((subscriber) => {
    const tag = event.inputs.join("-");
    appendFileSync(ledger, `start ${tag}\n`);
    const child = spawn(
      `sleep ${napSeconds}; printf '%s' "${tag}-body"`,
      [],
      { shell: true },
    );
    let stdout = "";
    let tornDown = false;
    let settled = false;
    child.stdout?.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.on("close", (code) => {
      settled = true;
      // A torn-down child already wrote its line in the teardown function
      // below. Writing another one here would report the teardown at the
      // moment the OS got around to reaping the process, which is a different
      // event from the moment the program stopped wanting it.
      if (tornDown) {
        subscriber.complete();
        return;
      }
      if (code === 0) {
        appendFileSync(ledger, `done ${tag}\n`);
        subscriber.next(stdout);
        subscriber.complete();
        return;
      }
      appendFileSync(ledger, `failed ${tag}\n`);
      subscriber.complete();
    });
    // THE TEARDOWN, recorded where it happens. rxjs calls this synchronously
    // the instant the inner is unsubscribed, which is the event this lab is
    // about; the child's `close` fires later, on a subsequent event loop turn.
    // An earlier draft of this file logged the teardown from `close` instead,
    // and the ledger then reported every teardown AFTER the winner's start
    // even when the teardown had been issued first -- the measurement was
    // ordering itself by process reaping rather than by the demand stream.
    // rxjs unsubscribes an inner on COMPLETION too, so an unguarded log here
    // marks every finished run as torn down. Only an unsubscribe that arrives
    // while the child is still running is a teardown, and `settled` is what
    // tells the two apart.
    return () => {
      if (settled) return;
      tornDown = true;
      appendFileSync(ledger, `torn down ${tag}\n`);
      child.kill();
    };
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// THE FOUR FLATTENERS. One function each, over the identical event stream.
// ─────────────────────────────────────────────────────────────────────────────

type Flattener = (
  events: Observable<DemandEvent>,
  effect: (event: DemandEvent) => Observable<string>,
) => Observable<string>;

/** CONCAT -- what the shipped runner does. `del` is not in the pipeline at all,
 *  which is the deafness this lab is about: the filter below is the whole of
 *  `1_hosts.ts` `liveDemand$`'s `delta.add.map(...)`. */
const concatFlattener: Flattener = (events, effect) =>
  events.pipe(
    filter((event) => event.sign === "add"),
    concatMap((event) => effect(event)),
  );

/** MERGE -- every demand runs immediately, unbounded. Still deaf to `del`. */
const mergeFlattener: Flattener = (events, effect) =>
  events.pipe(
    filter((event) => event.sign === "add"),
    mergeMap((event) => effect(event)),
  );

/**
 * SWITCH -- and the central result of this lab.
 *
 * There is no grouping by any slot key here. The stream is grouped by the
 * WITNESS, which is the demand row's own identity, and each group is flattened
 * with `switchMap` over its own sign: an `add` starts the inner, a `del` for
 * that same witness switches to `EMPTY`, and switching away from an inner is
 * exactly what unsubscribes it and kills the child.
 *
 * So a program does not tell the runtime "these two demands compete". The RULE
 * that retracted the row already applied whatever key discipline the program
 * declared -- `key(1)` replace, a `not(...)` guard, a scope closing. `del`
 * arrives already carrying that decision, per row, so teardown-on-`del` covers
 * every one of those without a slot column and without a new construct.
 */
const switchFlattener: Flattener = (events, effect) =>
  events.pipe(
    groupBy((event) => event.witnessDigest),
    mergeMap((perWitness) =>
      perWitness.pipe(switchMap((event) => (event.sign === "add" ? effect(event) : EMPTY))),
    ),
  );

/**
 * EXHAUST -- the one that needs something the language does not have.
 *
 * Exhaust means "while a slot is busy, drop new work for that slot". Dropping
 * requires knowing which demands share a slot, and per `slotOf` above, no
 * column says so. This grades the probe-derived slot to show what the operator
 * does; the verdict records the missing column rather than pretending the
 * derivation is the language's.
 */
const exhaustFlattener: Flattener = (events, effect) =>
  events.pipe(
    filter((event) => event.sign === "add"),
    groupBy((event) => event.slot),
    mergeMap((perSlot) => perSlot.pipe(exhaustMap((event) => effect(event)))),
  );

export const Flatteners: ReadonlyMap<string, Flattener> = new Map([
  ["concat", concatFlattener],
  ["merge", mergeFlattener],
  ["switch", switchFlattener],
  ["exhaust", exhaustFlattener],
]);

// ─────────────────────────────────────────────────────────────────────────────
// Entry point:
//   flatten-probe.ts <port> <host> <columnCount> <ledger> <nap> <flattener> [signOrder]
// ─────────────────────────────────────────────────────────────────────────────

const [, , portText, hostName, columnCountText, ledger, napSeconds, flattenerName, signOrderName] =
  process.argv;
const flattener = Flatteners.get(flattenerName ?? "");
if (!flattener) {
  console.error(`unknown flattener '${flattenerName}'; known: ${[...Flatteners.keys()].join(", ")}`);
  process.exit(2);
}
const signOrder = SIGN_ORDERS.get(signOrderName ?? "add-first");
if (!signOrder) {
  console.error(`unknown sign order '${signOrderName}'; known: ${[...SIGN_ORDERS.keys()].join(", ")}`);
  process.exit(2);
}
writeFileSync(ledger!, "");

const stop = new Subject<void>();
const events = demandEvents(Number(portText), hostName!, Number(columnCountText), signOrder);
flattener(events, (event) => runHost(ledger!, napSeconds!, event)).subscribe({
  error: (failure: unknown) => {
    console.error(`probe error: ${String(failure)}`);
  },
});
process.on("SIGTERM", () => stop.next());
