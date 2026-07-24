/**
 * main.ts — boot: db path + port via env (DL_DB_PATH, DL_PORT, default :7171),
 * startup log lists routes. `pnpm serve` runs this.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { ROUTE_LIST, startServer } from "./6_http.ts";

const dbPath = process.env.DL_DB_PATH ?? path.join(os.homedir(), ".local", "state", "dl", "mvp.sqlite");
const port = Number(process.env.DL_PORT ?? 7171);

fs.mkdirSync(path.dirname(dbPath), { recursive: true });

const server = await startServer({ dbPath, port });

console.log(`dl serve: listening on :${server.port} (db ${dbPath})`);
for (const route of ROUTE_LIST) console.log(`  ${route}`);
