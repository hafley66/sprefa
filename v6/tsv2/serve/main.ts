/** Starts the served tsv2 engine with its single terminal subscription.
 *
 *   TSV2_DB=file:/tmp/x.sqlite TSV2_PORT=17501 TSV2_WATCH_ROOT=/repo \
 *     node --experimental-transform-types serve/main.ts
 *
 * DB default is `:memory:`; a file URL makes restart persistence meaningful.
 *
 * TSV2_WATCH_ROOT is the directory every `bind watch(...)` glob resolves
 * against and every emitted path is relative to; it defaults to the process
 * cwd. TSV2_WATCH_COALESCE_MS is the burst window (default 100), stated here
 * because a `git checkout` inside the watched tree is one batch per window and
 * not one tick per file.
 */

import { serve_tsv2 } from "./4_http.ts";

const db_url = process.env.TSV2_DB ?? ":memory:";
const port = Number(process.env.TSV2_PORT ?? "17500");
const watch_root = process.env.TSV2_WATCH_ROOT ?? process.cwd();
const watch_coalesce_ms = Number(process.env.TSV2_WATCH_COALESCE_MS ?? "100");

serve_tsv2({ db_url, port, watch_root, watch_coalesce_ms }).subscribe({
  next: (event) => {
    if (event.kind === "listening") process.stdout.write(`tsv2 serving on ${event.port} (db ${db_url})\n`);
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
