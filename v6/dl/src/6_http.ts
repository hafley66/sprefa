/**
 * 6_http.ts — the http front. curl is the CLI. node:http, localhost, no auth.
 *
 * Contract (plan M6, tasks.d.ts HttpSurface):
 *   POST /edb/program        text/plain .dl -> bridge -> runtime (re)boot
 *   POST /edb/file_changed   {path} -> ingestFile -> TickReport
 *   POST /edb/:rel           {rows} -> commit insert batch -> TickReport
 *   GET  /idb/:rel           -> {rows}
 *   GET  /subscribe/:rel     -> SSE DeltaEvent stream; unsubscribes on socket close
 *   POST /query              `? rel(args).` -> one-shot SELECT -> {rows}
 * SSE = deltas$.pipe(filter(byRel)) per connection; teardown on close (refCount honesty).
 *
 * Owned by package M6 (http). Placeholder until that package lands.
 */
export {};
