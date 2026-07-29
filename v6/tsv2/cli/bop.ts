#!/usr/bin/env -S node --experimental-transform-types
/**
 * cli/bop.ts — "the bop": v6's CLI (registry.pl `cli_command/3` is the
 * inventory this file mirrors; `tests/bopCommandInventory.test.ts` asserts
 * the two agree on verb names). Five verbs: serve / run / check / load / q.
 * commander is the bought dependency (user directive 2026-07-29 late); clap
 * derive on the rust side is a later arc, not this one.
 *
 * NO DAEMON CONCEPT (same directive): `run` and `check` each boot the served
 * tsv2 engine IN-PROCESS -- the server calls itself -- use it for exactly one
 * job, and tear it down. `serve` is the only long-running verb; `load`/`q`
 * are plain HTTP clients against a `bop serve` (or a `run`/`check`, though
 * those own their server for their own lifetime, not for another process to
 * reach) that is already listening.
 *
 * ONE-SUBSCRIBE LAW EXEMPTION, stated up front because `serve()` below calls
 * `.subscribe()` and a reviewer's first question is "why does this file get
 * to do that when tsv2/serve/main.ts is supposed to be the only one":
 * `v6/tools/one-subscribe.sh` scans exactly two directories by name, `dl/src`
 * and `tsv2/serve` (its own header states this) -- `cli/` is outside both
 * scan paths. This mirrors `tsv2/scripts/run-fixture.ts`'s own stated
 * exemption: `bop serve` is its own standalone entry point (a separate OS
 * process with its own cold graph), not a second subscription living inside
 * an app that already has one. `bop run`'s own `.subscribe()` (in its own
 * function below) is the same case again, one more standalone process.
 *
 * ASYNC AT THE CLI RIM ONLY: commander's action handlers are callback/async
 * by its own design, and everything below them that talks over HTTP
 * (`fetch`, node's own global, no client library) or spawns swipl is async
 * too. This is exactly the exemption "async becomes rxjs; sync stays sync"
 * names for itself: the law binds runtime code above the SqlRunner seam, not
 * a one-shot CLI process entry. Nothing in `runtime/` or `serve/` gains a
 * `Promise` because of this file; `run`/`serve` compose the existing cold
 * `serveTsv2` unchanged and this file's only job is the one terminal
 * `.subscribe()` plus a couple of `fetch` calls around it.
 *
 * NO SECOND HTTP CLIENT LAYER: `fetch` is node's own global (same choice
 * `tests/serveHelpers.ts` makes with `node:http` directly); no axios/got/
 * undici-wrapper is added for this.
 */

import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { Command } from "commander";

import { serveTsv2 } from "../serve/4_http.ts";
import type { IRow, IServeEvent } from "../runtime/types.ts";

const BOP_CHECK_PL = fileURLToPath(new URL("../../prolog/compile/scripts/bop_check.pl", import.meta.url));
const DEFAULT_PORT = 17500;

// ─────────────────────────────────────────────────────────────────────────────
// Shared: classifying a compile-door failure's TEXT as a named refusal (exit
// 2) or a broken program (exit 1). MIRRORS bop_check.pl's own
// `named_reason_functor/1` facts -- see that predicate's header for why this
// is a second, hand-kept list rather than one shared file: the prolog side
// classifies a live exception TERM, this side only ever sees the http
// error body's TEXT (0_compile.ts's `runSwipl` surfaces swipl's stderr as
// an Error message, and 4_http.ts's `loadProgram$` turns that into the 400
// body `run`/`load` read here), so there is no term to pattern-match on.
// ─────────────────────────────────────────────────────────────────────────────

const NAMED_REASON_FUNCTORS = [
  "unsupported_construct",
  "not_stratified",
  "column_mismatch",
  "bind_mismatch",
  "bind_and_rule_head",
  "probe_mismatch",
  "query_mismatch",
  "refused_host_decl",
  "template_mismatch",
  "unmapped_feature",
] as const;

function classifyCompileFailureText(text: string): 1 | 2 {
  return NAMED_REASON_FUNCTORS.some((functor) => text.includes(`${functor}(`)) ? 2 : 1;
}

function writeErrorLine(text: string): void {
  process.stderr.write(text.endsWith("\n") ? text : `${text}\n`);
}

// ─────────────────────────────────────────────────────────────────────────────
// check
// ─────────────────────────────────────────────────────────────────────────────

/** Text-door validation only -- no server boots (compilation is pure). Exit
 *  code IS the answer; bop_check.pl already writes finding/refusal/broken
 *  lines to its own stderr, inherited straight through. */
function check(file: string): void {
  const absolutePath = resolve(file);
  const result = spawnSync("swipl", ["-q", "-l", BOP_CHECK_PL, "-g", "bop_check_env", "-g", "halt"], {
    stdio: "inherit",
    env: { ...process.env, BOP_CHECK_FILE: absolutePath },
  });
  if (result.error) {
    writeErrorLine(`broken: could not run swipl: ${result.error.message}`);
    process.exitCode = 1;
    return;
  }
  process.exitCode = result.status ?? 1;
}

// ─────────────────────────────────────────────────────────────────────────────
// serve
// ─────────────────────────────────────────────────────────────────────────────

/** Exactly what tsv2/serve/main.ts does, reused rather than re-derived --
 *  same event switch, same three lines. `bop serve` IS that entry, wearing
 *  commander's option parsing instead of raw env vars. */
function serve(options: { readonly port: string; readonly db: string }): void {
  const port = Number(options.port);
  const dbUrl = options.db;
  serveTsv2({ dbUrl, port }).subscribe({
    next: (event: IServeEvent) => {
      if (event.kind === "listening") process.stdout.write(`tsv2 serving on ${event.port} (db ${dbUrl})\n`);
      if (event.kind === "loaded") process.stdout.write(`program loaded: ${event.program}\n`);
      if (event.kind === "tick") process.stdout.write(`${event.outcome.line}\n`);
      if (event.kind === "watch") {
        process.stdout.write(`watch ${event.fired.glob}: +${event.fired.added} -${event.fired.removed}\n`);
      }
    },
    error: (failure: unknown) => {
      writeErrorLine(failure instanceof Error ? (failure.stack ?? failure.message) : String(failure));
      process.exit(1);
    },
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// run
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Boots its own served engine on an ephemeral port (or `--port` if pinned),
 * POSTs the compiled program to itself, then streams every `tick` event's
 * log line to stdout -- the standard tick-log JSONL, one line per tick,
 * identical in shape to what a schedule-fed oracle run or `bop serve` itself
 * would print for the same ticks.
 *
 * STOPPING RULE, a design call this brief left open (no numeric quiesce
 * window was specified): `--ticks N` stops after N ticks, deterministic.
 * Absent `--ticks`, the process watches for IDLE -- no event of any kind for
 * `BOP_RUN_IDLE_MS` (default 2000) -- and shuts down cleanly once nothing has
 * fired for that long; a program with a live `bind interval`/`bind watch`
 * keeps the idle timer resetting forever, which is correct (the whole point
 * of `run` is "let binds/hosts run"), so an unbounded program needs
 * `--ticks` or an external kill, same as `bop serve` needing an external
 * kill. Flagged here, not silently assumed: report this call in review if a
 * different quiesce contract was intended.
 */
function run(file: string, options: { readonly ticks?: string; readonly port?: string }): void {
  const source = readFileSync(file, "utf8");
  const port = options.port !== undefined ? Number(options.port) : 0;
  const ticksLimit = options.ticks !== undefined ? Number(options.ticks) : undefined;
  const idleMs = Number(process.env.BOP_RUN_IDLE_MS ?? "2000");

  let ticksSeen = 0;
  let loadRequested = false;
  let idleTimer: NodeJS.Timeout | undefined;

  const subscription = serveTsv2({ dbUrl: ":memory:", port }).subscribe({
    next: (event: IServeEvent) => {
      if (idleTimer !== undefined) clearTimeout(idleTimer);

      if (event.kind === "listening" && !loadRequested) {
        loadRequested = true;
        fetch(`http://127.0.0.1:${event.port}/program`, { method: "POST", body: source }).then(
          async (response) => {
            if (response.status !== 200) {
              const text = await response.text();
              writeErrorLine(text);
              process.exitCode = classifyCompileFailureText(text);
              subscription.unsubscribe();
            }
          },
          (failure: unknown) => {
            writeErrorLine(`broken: ${String(failure)}`);
            process.exitCode = 1;
            subscription.unsubscribe();
          },
        );
      }

      if (event.kind === "tick") {
        process.stdout.write(`${event.outcome.line}\n`);
        ticksSeen += 1;
        if (ticksLimit !== undefined && ticksSeen >= ticksLimit) {
          process.exitCode = 0;
          subscription.unsubscribe();
          return;
        }
      }

      idleTimer = setTimeout(() => {
        process.exitCode = 0;
        subscription.unsubscribe();
      }, idleMs);
    },
    error: (failure: unknown) => {
      writeErrorLine(failure instanceof Error ? (failure.stack ?? failure.message) : String(failure));
      process.exitCode = 1;
    },
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// load
// ─────────────────────────────────────────────────────────────────────────────

/** POST to an ALREADY RUNNING server's own `/program` route -- this verb
 *  starts nothing. A connection failure (nothing listening) is reported as
 *  its own clear line, distinct from a compile failure. */
function load(file: string, options: { readonly port: string }): void {
  const source = readFileSync(file, "utf8");
  const port = Number(options.port);
  fetch(`http://127.0.0.1:${port}/program`, { method: "POST", body: source }).then(
    async (response) => {
      const text = await response.text();
      if (response.status === 200) {
        process.stdout.write(`${text}\n`);
        process.exitCode = 0;
      } else {
        writeErrorLine(text);
        process.exitCode = classifyCompileFailureText(text);
      }
    },
    (failure: unknown) => {
      writeErrorLine(`no server listening on port ${port}: ${String(failure)}`);
      process.exitCode = 1;
    },
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// q
// ─────────────────────────────────────────────────────────────────────────────

/** One rel's current rows via the existing `GET /idb/:rel` route. No column
 *  names cross that seam (runtime/types.ts `IRow` is positional values
 *  only), so the default table rendering is positional too -- tab-separated,
 *  one row per line, no header. `--json` prints the raw response body. */
function query(rel: string, options: { readonly port: string; readonly json?: boolean }): void {
  const port = Number(options.port);
  fetch(`http://127.0.0.1:${port}/idb/${rel}`).then(
    async (response) => {
      const text = await response.text();
      if (response.status !== 200) {
        writeErrorLine(text);
        process.exitCode = 1;
        return;
      }
      if (options.json === true) {
        process.stdout.write(`${text}\n`);
      } else {
        const parsed = JSON.parse(text) as { readonly rows: readonly IRow[] };
        for (const row of parsed.rows) process.stdout.write(`${row.join("\t")}\n`);
      }
      process.exitCode = 0;
    },
    (failure: unknown) => {
      writeErrorLine(`no server listening on port ${port}: ${String(failure)}`);
      process.exitCode = 1;
    },
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// wiring
// ─────────────────────────────────────────────────────────────────────────────

const program = new Command();
program
  .name("bop")
  .description("v6 CLI over the served tsv2 engine (registry.pl cli_command/3 is the generated inventory).");

program
  .command("serve")
  .description("boot the served tsv2 engine and keep it running")
  .option("--port <port>", "TCP port", String(DEFAULT_PORT))
  .option("--db <url>", "sqlite db url (default :memory:)", ":memory:")
  .action(serve);

program
  .command("run")
  .argument("<file>", ".dl6 program")
  .description("compile + load on an in-process ephemeral server, stream ticks to stdout, then shut down")
  .option("--ticks <n>", "stop after this many ticks")
  .option("--port <port>", "TCP port (default: ephemeral)")
  .action(run);

program
  .command("check")
  .argument("<file>", ".dl6 program")
  .description("validate through the text door; no server boots (exit 0 clean / 2 findings / 1 broken)")
  .action(check);

program
  .command("load")
  .argument("<file>", ".dl6 program")
  .description("POST a compiled program to an already-running bop serve")
  .option("--port <port>", "TCP port", String(DEFAULT_PORT))
  .action(load);

program
  .command("q")
  .argument("<rel>", "relation name")
  .description("read one rel's current rows from a running bop serve")
  .option("--port <port>", "TCP port", String(DEFAULT_PORT))
  .option("--json", "print raw JSON instead of a table")
  .action(query);

program.parse();
