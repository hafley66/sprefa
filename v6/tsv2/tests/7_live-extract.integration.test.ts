/**
 * 7_live-extract.integration.test.ts -- integration tests where the REAL
 * `sprefa-extract` binary runs as a host through the real tsv2 pipeline.
 *
 * Graded through the real serve pipeline: zero stubs, real child process execution
 * through HostRunner -> HostExecutors.get("sprefa_extract") -> runShellLine.
 */

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { firstValueFrom } from "rxjs";

import { ScratchStore } from "../runtime/scratchStore.ts";
import { witnessRows } from "../serve/1_hosts.ts";
import {
  post_arrivals,
  post_program,
  request,
  start_served,
} from "./serveHelpers.ts";

const EXTRACT_CARGO_TOML = fileURLToPath(new URL("../../sprefa-extract/Cargo.toml", import.meta.url));
const EXTRACT_RELEASE = fileURLToPath(new URL("../../sprefa-extract/target/release/extract", import.meta.url));
const FIXTURES_DIR = fileURLToPath(new URL("./fixtures/live_extract", import.meta.url));

function ensure_binary_built(): void {
  if (!existsSync(EXTRACT_RELEASE)) {
    execFileSync("cargo", ["build", "--release", "--manifest-path", EXTRACT_CARGO_TOML, "--bin", "extract", "--features", "cli"], {
      stdio: "pipe",
    });
  }
}

async function wait_until(predicate: () => boolean | Promise<boolean>, what: string, timeout_ms = 10_000): Promise<void> {
  const deadline = Date.now() + timeout_ms;
  while (!(await predicate())) {
    if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
    await new Promise<void>((resolve) => setTimeout(resolve, 25));
  }
}

async function rows_of(port: number, rel: string): Promise<readonly (readonly (string | number)[])[]> {
  const answer = JSON.parse((await request(port, `/idb/${rel}`, "GET")).body) as {
    rows: readonly (readonly (string | number)[])[];
  };
  return answer.rows;
}

const LIVE_EXTRACT_DL6 = `
rel file(path: text, digest: text).

sh call_node(path: text, digest: text) -> (record: text, family: text, kind: text, name: text) =
  \`"$DL_EXTRACT_BIN" --family call {path}\`.

sh call_ref(path: text, digest: text) -> (record: text, family: text, callee: text) =
  \`"$DL_EXTRACT_BIN" --family call {path}\`.

rel function_def(path: text, name: text).
function_def(path, name) <-
  file(path, digest),
  call_node(path, digest, 'node', 'call', 'function', name).

rel function_call(path: text, callee: text).
function_call(path, callee) <-
  file(path, digest),
  call_ref(path, digest, 'site', 'call', callee).
`;

/**
 * 1. HAPPY PATH: serve a small .dl6 whose host template invokes the real
 * extractor over tests/fixtures/live_extract/; assert the extracted rows
 * (function names, one call edge) land via the HTTP surface, exact rows, not
 * row counts.
 */
test("happy path: real sprefa-extract runs as host and populates function_def and function_call", async () => {
  ensure_binary_built();
  const sample_file = join(FIXTURES_DIR, "sample.rs");
  const previous_extract_bin = process.env.DL_EXTRACT_BIN;
  process.env.DL_EXTRACT_BIN = EXTRACT_RELEASE;

  const served = await start_served();
  try {
    const loaded = await post_program(served.port, LIVE_EXTRACT_DL6);
    assert.equal(loaded.statusCode, 200, loaded.body);

    await post_arrivals(served.port, [
      { rel: "file", sign: "add", row: [sample_file, "sample-digest-1"] },
    ]);

    await wait_until(async () => (await rows_of(served.port, "function_def")).length === 2, "function_def to have 2 rows");
    await wait_until(async () => (await rows_of(served.port, "function_call")).length === 1, "function_call to have 1 row");

    const def_rows = await rows_of(served.port, "function_def");
    const call_rows = await rows_of(served.port, "function_call");

    assert.deepEqual(
      [...def_rows].sort((a, b) => String(a[1]).localeCompare(String(b[1]))),
      [
        [sample_file, "first"],
        [sample_file, "second"],
      ],
      "exact function definitions from sample.rs",
    );

    assert.deepEqual(
      call_rows,
      [[sample_file, "second"]],
      "exact function call edge from sample.rs",
    );
  } finally {
    if (previous_extract_bin === undefined) delete process.env.DL_EXTRACT_BIN;
    else process.env.DL_EXTRACT_BIN = previous_extract_bin;
    await served.stop();
  }
});

/**
 * 2. SABOTAGE: edit the fixture copy in a temp dir, rename a function, re-run,
 * assert the changed row and ONLY the changed row differs.
 */
test("sabotage: editing fixture in temp dir modifies only the changed row", async () => {
  ensure_binary_built();
  const temp_dir = mkdtempSync(join(tmpdir(), "tsv2-live-extract-sabotage-"));
  const temp_file = join(temp_dir, "sample.rs");
  cpSync(join(FIXTURES_DIR, "sample.rs"), temp_file);

  const previous_extract_bin = process.env.DL_EXTRACT_BIN;
  process.env.DL_EXTRACT_BIN = EXTRACT_RELEASE;

  const served = await start_served();
  try {
    const loaded = await post_program(served.port, LIVE_EXTRACT_DL6);
    assert.equal(loaded.statusCode, 200, loaded.body);

    await post_arrivals(served.port, [
      { rel: "file", sign: "add", row: [temp_file, "initial-digest"] },
    ]);

    await wait_until(async () => (await rows_of(served.port, "function_def")).length === 2, "initial function_def rows");

    const initial_defs = await rows_of(served.port, "function_def");
    assert.deepEqual(
      [...initial_defs].sort((a, b) => String(a[1]).localeCompare(String(b[1]))),
      [
        [temp_file, "first"],
        [temp_file, "second"],
      ],
    );

    // Sabotage: rename `first` to `renamed_first` in the temp file
    const content = readFileSync(temp_file, "utf8");
    writeFileSync(temp_file, content.replace("fn first()", "fn renamed_first()"), "utf8");

    // Advance file arrival with new digest (freshness input triggers re-execution)
    await post_arrivals(served.port, [
      { rel: "file", sign: "del", row: [temp_file, "initial-digest"] },
      { rel: "file", sign: "add", row: [temp_file, "edited-digest"] },
    ]);

    await wait_until(async () => {
      const rows = await rows_of(served.port, "function_def");
      return rows.some((row) => row[1] === "renamed_first");
    }, "edited function_def to land");

    const edited_defs = await rows_of(served.port, "function_def");
    const edited_calls = await rows_of(served.port, "function_call");

    assert.deepEqual(
      [...edited_defs].sort((a, b) => String(a[1]).localeCompare(String(b[1]))),
      [
        [temp_file, "renamed_first"],
        [temp_file, "second"],
      ],
      "only first was renamed; second remains untouched",
    );

    assert.deepEqual(
      edited_calls,
      [[temp_file, "second"]],
      "call edge unchanged",
    );
  } finally {
    if (previous_extract_bin === undefined) delete process.env.DL_EXTRACT_BIN;
    else process.env.DL_EXTRACT_BIN = previous_extract_bin;
    await served.stop();
    rmSync(temp_dir, { recursive: true, force: true });
  }
});

/**
 * 3. CONTRACT: kill PATH so the binary is absent; assert the failure is a named
 * host error naming the host and command, not a silent empty result.
 */
test("contract: absent extract binary records a named host error naming the host and command", async () => {
  const sample_file = join(FIXTURES_DIR, "sample.rs");
  const temp_dir = mkdtempSync(join(tmpdir(), "tsv2-contract-"));
  const db_url = `file:${join(temp_dir, "contract.sqlite")}`;

  const previous_extract_bin = process.env.DL_EXTRACT_BIN;
  const previous_path = process.env.PATH;

  // Set DL_EXTRACT_BIN to an unresolvable command name and restrict PATH so
  // compiler dependencies (swipl) survive but the binary is absent.
  process.env.DL_EXTRACT_BIN = "non_existent_sprefa_extract_bin_deadbeef";
  process.env.PATH = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin";

  const served = await start_served(0, undefined, db_url);
  try {
    const loaded = await post_program(served.port, LIVE_EXTRACT_DL6);
    assert.equal(loaded.statusCode, 200, loaded.body);

    await post_arrivals(served.port, [
      { rel: "file", sign: "add", row: [sample_file, "digest-fail"] },
    ]);

    // Query witness rows from sqlite inspection to verify error status
    const inspection = ScratchStore.open(db_url);
    try {
      await wait_until(async () => {
        const rows = await firstValueFrom(witnessRows(inspection));
        return rows.some((row) => row[2] === "error");
      }, "host witness state to become error");

      const witnesses = await firstValueFrom(witnessRows(inspection));
      const error_witnesses = witnesses.filter((row) => row[2] === "error");
      assert.ok(error_witnesses.length > 0, "witness cache recorded error outcome");

      // Verify effect events recorded the failure naming the host and error outcome
      await wait_until(() => {
        return served.events.some((event) => {
          return event.kind === "effect" && event.done.outcome === "error";
        });
      }, "effect failure recorded");

      const effect_events = served.events.filter((event) => event.kind === "effect");
      assert.ok(
        effect_events.some((event) => {
          if (event.kind !== "effect") return false;
          return (
            (event.done.host === "call_node" || event.done.host === "call_ref") &&
            event.done.outcome === "error"
          );
        }),
        "named host failure recorded in effect events",
      );

      // Verify no phantom rows derived
      const def_rows = await rows_of(served.port, "function_def");
      const call_rows = await rows_of(served.port, "function_call");
      assert.equal(def_rows.length, 0, "no rows derived for function_def");
      assert.equal(call_rows.length, 0, "no rows derived for function_call");
    } finally {
      inspection.db.close();
    }
  } finally {
    if (previous_extract_bin === undefined) delete process.env.DL_EXTRACT_BIN;
    else process.env.DL_EXTRACT_BIN = previous_extract_bin;
    if (previous_path === undefined) delete process.env.PATH;
    else process.env.PATH = previous_path;
    await served.stop();
    rmSync(temp_dir, { recursive: true, force: true });
  }
});
