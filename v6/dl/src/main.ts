/**
 * main.ts — boot: db path + port via env (DL_DB_PATH, DL_PORT, default :7171),
 * startup log lists routes. `pnpm serve` runs this.
 *
 * THE ONE SUBSCRIPTION. Everything above this file is cold: `serveDl` composes the
 * listener, every request, the loaded program's tick loop, its host effects, and every
 * SSE client into a single observable, and the subscribe below is what starts all of
 * it. A second manual subscribe call anywhere in src is a design failure
 * (v6/tools/one-subscribe.sh holds the count at 1).
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { ROUTE_LIST, serveDl } from "./6_http.ts";

const dbPath = process.env.DL_DB_PATH ?? path.join(os.homedir(), ".local", "state", "dl", "mvp.sqlite");
const port = Number(process.env.DL_PORT ?? 7171);

fs.mkdirSync(path.dirname(dbPath), { recursive: true });

serveDl({ dbPath, port }).subscribe({
  next: (event) => {
    if (event.kind !== "listening") return;
    console.log(`dl serve: listening on :${event.server.port} (db ${dbPath})`);
    for (const route of ROUTE_LIST) console.log(`  ${route}`);
  },
  error: (failure: unknown) => {
    console.error(`dl serve: ${failure instanceof Error ? failure.message : String(failure)}`);
    process.exitCode = 1;
  },
});
