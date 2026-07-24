/**
 * 5_diag.test.ts — M5.1/5.2/5.5 gate. Min tests max coverage (tasks.d.ts law):
 *  - the 9-col schema pin (the compat contract with v5's src/lsp.rs reader),
 *  - the diag_v5 view SQL against a real rel_diag table (NULL severity -> 'warn'),
 *  - the check_diag.mjs --check reader's three exit codes against a throwaway http
 *    server (unreachable/warn-only/error-present).
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createClient } from "@libsql/client";
import { diagDecl, DIAG_V5_VIEW_SQL } from "../src/5_diag.ts";

const CHECK_DIAG_SCRIPT = fileURLToPath(new URL("../scripts/check_diag.mjs", import.meta.url));

const DIAG_COLUMNS = ["path", "line", "col", "end_line", "end_col", "severity", "code", "msg", "hint"];

test("diagDecl pins the v5 9-column schema in order (compat contract with src/lsp.rs's reader)", () => {
  assert.equal(diagDecl.name, "diag");
  assert.deepEqual(diagDecl.columns, DIAG_COLUMNS);
});

test("diag_v5 view SQL applies over a real rel_diag table and defaults NULL severity to warn", async () => {
  const db = createClient({ url: ":memory:" });
  try {
    await db.execute(`CREATE TABLE rel_diag (
      path TEXT, line INTEGER, col INTEGER, end_line INTEGER, end_col INTEGER,
      severity TEXT, code TEXT, msg TEXT, hint TEXT,
      PRIMARY KEY (path, line, col, end_line, end_col, severity, code, msg, hint)
    )`);
    await db.execute(DIAG_V5_VIEW_SQL);

    // One batched insert, both rows in a single execute() call (N+1 law).
    await db.execute({
      sql: `INSERT INTO rel_diag
        (path, line, col, end_line, end_col, severity, code, msg, hint)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?), (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      args: [
        "a.ts", 1, 2, 1, 2, null, "no-console", "console.log left in source", null,
        "b.ts", 3, 4, 3, 4, "error", "bad-thing", "something is wrong", "fix it",
      ],
    });

    const result = await db.execute("SELECT * FROM diag_v5 ORDER BY path");
    assert.equal(result.rows.length, 2);
    const nullSeverityRow = result.rows.find((row) => row.path === "a.ts");
    assert.ok(nullSeverityRow);
    assert.equal(nullSeverityRow.severity, "warn");
    const errorRow = result.rows.find((row) => row.path === "b.ts");
    assert.ok(errorRow);
    assert.equal(errorRow.severity, "error");
  } finally {
    db.close();
  }
});

/** Starts a throwaway http server on an ephemeral port serving canned {rows} JSON
 *  for GET /idb/diag; returns its DL_URL and a close() to tear it down. */
async function startCannedDiagServer(rows: readonly Record<string, unknown>[]): Promise<{ url: string; close: () => Promise<void> }> {
  const server = createServer((request, response) => {
    if (request.url === "/idb/diag") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ rows }));
      return;
    }
    response.writeHead(404);
    response.end();
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (address === null || typeof address === "string") throw new Error("expected a bound TCP address");
  return {
    url: `http://127.0.0.1:${address.port}`,
    close: () => new Promise<void>((resolve, reject) => server.close((err) => (err ? reject(err) : resolve()))),
  };
}

/** Runs check_diag.mjs against dlUrl and resolves its exit code. MUST use async
 *  spawn, not spawnSync: the canned server in this same test process shares the
 *  event loop, and spawnSync blocks it synchronously while waiting for the
 *  child to exit — the child's fetch would then never get a response
 *  (deadlock). Async spawn lets the parent's event loop keep serving requests
 *  while it awaits the child's 'exit' event. */
function runCheckDiag(dlUrl: string): Promise<number | null> {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [CHECK_DIAG_SCRIPT], { env: { ...process.env, DL_URL: dlUrl } });
    child.on("error", reject);
    child.on("exit", (code) => resolve(code));
  });
}

test("check_diag exits 2 on an error row, 0 on warn-only, 1 on unreachable", async () => {
  const warnOnly = await startCannedDiagServer([{ path: "a.ts", severity: "warn" }]);
  try {
    assert.equal(await runCheckDiag(warnOnly.url), 0);
  } finally {
    await warnOnly.close();
  }

  const withError = await startCannedDiagServer([
    { path: "a.ts", severity: "warn" },
    { path: "b.ts", severity: "error" },
  ]);
  try {
    assert.equal(await runCheckDiag(withError.url), 2);
  } finally {
    await withError.close();
  }

  assert.equal(await runCheckDiag("http://127.0.0.1:1"), 1);
});
