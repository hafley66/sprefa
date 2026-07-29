/**
 * main.ts — the served tsv2 engine's ONE terminal subscription.
 *
 * Everything above this line is cold and composed. `serveTsv2(...)` IS the app;
 * this file starts it and prints what it emits. The one-subscribe law's ratchet
 * (v6/tools/one-subscribe.sh) scans this directory with its own baseline of 1;
 * a second terminal subscription anywhere under serve/ fails it. (The ratchet
 * greps for the call text and does not skip comments, so this sentence says it
 * in words rather than in code.)
 *
 *   TSV2_DB=file:/tmp/x.sqlite TSV2_PORT=17501 TSV2_WATCH_ROOT=/repo \
 *     node --experimental-transform-types serve/main.ts
 *
 * DB default is `:memory:`, so a run that names no db keeps nothing. A file url
 * is what makes restart (and therefore the endurance receipt) meaningful.
 *
 * TSV2_WATCH_ROOT is the directory every `bind watch(...)` glob resolves
 * against and every emitted path is relative to; it defaults to the process
 * cwd. TSV2_WATCH_COALESCE_MS is the burst window (default 100), stated here
 * because a `git checkout` inside the watched tree is one batch per window and
 * not one tick per file.
 */

import { serveTsv2 } from "./4_http.ts";

const dbUrl = process.env.TSV2_DB ?? ":memory:";
const port = Number(process.env.TSV2_PORT ?? "17500");
const watchRoot = process.env.TSV2_WATCH_ROOT ?? process.cwd();
const watchCoalesceMs = Number(process.env.TSV2_WATCH_COALESCE_MS ?? "100");

serveTsv2({ dbUrl, port, watchRoot, watchCoalesceMs }).subscribe({
  next: (event) => {
    if (event.kind === "listening") process.stdout.write(`tsv2 serving on ${event.port} (db ${dbUrl})\n`);
    if (event.kind === "loaded") process.stdout.write(`program loaded: ${event.program}\n`);
    if (event.kind === "tick") process.stdout.write(`${event.outcome.line}\n`);
    if (event.kind === "watch") {
      process.stdout.write(`watch ${event.fired.glob}: +${event.fired.added} -${event.fired.removed}\n`);
    }
  },
  error: (failure: unknown) => {
    process.stderr.write(`${failure instanceof Error ? (failure.stack ?? failure.message) : String(failure)}\n`);
    process.exit(1);
  },
});
