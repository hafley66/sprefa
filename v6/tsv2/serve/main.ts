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
 *   TSV2_DB=file:/tmp/x.sqlite TSV2_PORT=17501 \
 *     node --experimental-transform-types serve/main.ts
 *
 * DB default is `:memory:`, so a run that names no db keeps nothing. A file url
 * is what makes restart (and therefore the endurance receipt) meaningful.
 */

import { serveTsv2 } from "./4_http.ts";

const dbUrl = process.env.TSV2_DB ?? ":memory:";
const port = Number(process.env.TSV2_PORT ?? "17500");

serveTsv2({ dbUrl, port }).subscribe({
  next: (event) => {
    if (event.kind === "listening") process.stdout.write(`tsv2 serving on ${event.port} (db ${dbUrl})\n`);
    if (event.kind === "loaded") process.stdout.write(`program loaded: ${event.program}\n`);
    if (event.kind === "tick") process.stdout.write(`${event.outcome.line}\n`);
  },
  error: (failure: unknown) => {
    process.stderr.write(`${failure instanceof Error ? (failure.stack ?? failure.message) : String(failure)}\n`);
    process.exit(1);
  },
});
