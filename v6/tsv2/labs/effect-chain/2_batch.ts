/**
 * 2_batch.ts — RECEIPT 2 of the effect chain-and-batch lab.
 *
 * Question: what batching exists today, what makes two demands compatible, and
 * what are the spawn counts at 1 / 10 / 100 demands.
 *
 * `serve/1_hosts.ts:393 groupInvocations` is the whole of today's batching. It
 * groups by (execution, template, ordered input VALUES) and only when the
 * execution is `sprefa_extract`. Every count below is read off a ledger file the
 * spawned shell appends to, so the number is the process count, not a proxy.
 *
 * Run: node --experimental-transform-types labs/effect-chain/2_batch.ts
 */

import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  post_arrivals,
  post_program,
  request,
  start_served,
  tick_events,
} from "../../tests/serveHelpers.ts";

function receipt(name: string, value: unknown): void {
  console.log(`  ${name} = ${JSON.stringify(value)}`);
}

function waitUntil(predicate: () => Promise<boolean>, what: string, timeoutMs = 60_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return (async () => {
    while (!(await predicate())) {
      if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
      await new Promise<void>((resolve) => setTimeout(resolve, 15));
    }
  })();
}

function spawnCount(ledger: string): number {
  return readFileSync(ledger, "utf8").length;
}

function rowsOf(port: number, rel: string): Promise<readonly (readonly unknown[])[]> {
  return request(port, `/idb/${rel}`, "GET").then(
    (result) => (JSON.parse(result.body) as { rows: readonly (readonly unknown[])[] }).rows,
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2a. A plain `sh` host over N demand rows. `shell` execution never groups.
// ─────────────────────────────────────────────────────────────────────────────

const PLAIN_SHELL = `
rel item(id: text).
sh probe_item(id: text) -> (out: text) =
  \`printf 's' >> "$LAB_SPAWNS"; printf '%s' "{id}-seen"\`.
rel seen(id: text, out: text).
seen(id, out) <- item(id), probe_item(id, out).
`;

async function plainShellAt(count: number, workDir: string): Promise<void> {
  const ledger = join(workDir, `plain-${count}`);
  writeFileSync(ledger, "", "utf8");
  process.env.LAB_SPAWNS = ledger;
  const served = await start_served(0);
  try {
    const loaded = await post_program(served.port, PLAIN_SHELL);
    if (loaded.statusCode !== 200) throw new Error(loaded.body);
    const batch = Array.from({ length: count }, (_, index) => ({
      rel: "item",
      sign: "add" as const,
      row: [`id-${index}`],
    }));
    const startedAt = performance.now();
    await post_arrivals(served.port, batch);
    await waitUntil(async () => (await rowsOf(served.port, "seen")).length === count, `${count} seen rows`);
    receipt(`plain_shell_${count}_demands_spawns`, spawnCount(ledger));
    receipt(`plain_shell_${count}_demands_ticks`, tick_events(served.events).length);
    receipt(`plain_shell_${count}_demands_wall_ms`, Math.round(performance.now() - startedAt));
  } finally {
    await served.stop();
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2b. The `sprefa_extract` grouping path. Seven declarations, one command.
//
// `registry.pl host_execution/3` selects `sprefa_extract` for any template that
// starts `"$DL_EXTRACT_BIN" ` and ends `{path}`, and `host_input_contract/3`
// gives these seven registered names the (identity, freshness) input roles the
// path/digest pair needs. Both are the shipped rows; nothing here is a fixture
// of the lab's own making.
// ─────────────────────────────────────────────────────────────────────────────

const FLOW_PROJECTIONS: readonly { readonly host: string; readonly outputs: string }[] = [
  { host: "df_node_at", outputs: "record: text, kind: text" },
  { host: "df_edge_at", outputs: "record: text, from_id: text, to_id: text" },
  { host: "df_param_at", outputs: "record: text, pos: text" },
  { host: "df_arg_at", outputs: "record: text, arg: text" },
  { host: "call_node_at", outputs: "record: text, name: text" },
  { host: "type_node_at", outputs: "record: text, ty: text" },
  { host: "sig_at", outputs: "record: text, slot: text" },
];

function extractProgram(projectionCount: number, distinctTemplates = false): string {
  const used = FLOW_PROJECTIONS.slice(0, projectionCount);
  const lines = ["rel file(path: text, digest: text)."];
  for (const [index, projection] of used.entries()) {
    // Still `sprefa_extract` execution either way (registry.pl selects on the
    // `"$DL_EXTRACT_BIN" ` prefix and the `{path}` suffix); only the middle of
    // the text differs, which is exactly what the invocation key compares.
    const flag = distinctTemplates ? ` --slot${index}` : "";
    lines.push(
      `sh ${projection.host}(path: text, digest: text) -> (${projection.outputs}) =` +
        `\n  \`"$DL_EXTRACT_BIN" --family call${flag} {path}\`.`,
    );
  }
  for (const projection of used) {
    const outputNames = projection.outputs.split(",").map((column) => column.split(":")[0]!.trim());
    lines.push(`rel out_${projection.host}(path: text, ${projection.outputs}).`);
    lines.push(
      `out_${projection.host}(path, ${outputNames.join(", ")}) <-\n` +
        `  file(path, digest), ${projection.host}(path, digest, ${outputNames.join(", ")}).`,
    );
  }
  return `${lines.join("\n")}\n`;
}

/** The extractor stand-in: counts its own invocations and answers one JSONL
 *  object carrying every column the seven projections declare. */
function installExtractShim(workDir: string, ledger: string): string {
  const shim = join(workDir, "extract-shim");
  writeFileSync(
    shim,
    [
      "#!/bin/sh",
      `printf 's' >> "${ledger}"`,
      `printf '%s\\n' '{"record":"r","kind":"k","from_id":"f","to_id":"t","pos":"0","arg":"a","name":"n","ty":"number","slot":"return"}'`,
      "",
    ].join("\n"),
    "utf8",
  );
  chmodSync(shim, 0o755);
  return shim;
}

async function extractGroupingAt(
  paths: number,
  projections: number,
  workDir: string,
  distinctTemplates = false,
): Promise<void> {
  const label = distinctTemplates ? "distinct_templates" : "one_template";
  const ledger = join(workDir, `extract-${paths}x${projections}-${label}`);
  writeFileSync(ledger, "", "utf8");
  process.env.DL_EXTRACT_BIN = installExtractShim(workDir, ledger);
  const source = extractProgram(projections, distinctTemplates);
  const served = await start_served(0);
  try {
    const loaded = await post_program(served.port, source);
    if (loaded.statusCode !== 200) throw new Error(loaded.body);
    const batch = Array.from({ length: paths }, (_, index) => ({
      rel: "file",
      sign: "add" as const,
      row: [`src/f${index}.ts`, `digest-${index}`],
    }));
    await post_arrivals(served.port, batch);
    // EVERY projection must have settled, not just the last one declared: with
    // distinct templates the invocations are separate and finish out of order,
    // so watching one rel undercounts the spawns still in flight.
    const targets = FLOW_PROJECTIONS.slice(0, projections).map((projection) => `out_${projection.host}`);
    await waitUntil(async () => {
      for (const rel of targets) {
        if ((await rowsOf(served.port, rel)).length !== paths) return false;
      }
      return true;
    }, `${paths} rows in each of ${targets.length} projection rels`);
    receipt(`extract_${paths}x${projections}_${label}_demands`, paths * projections);
    receipt(`extract_${paths}x${projections}_${label}_spawns`, spawnCount(ledger));
    receipt(`extract_${paths}x${projections}_${label}_ticks`, tick_events(served.events).length);
  } finally {
    await served.stop();
  }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2c. Two plain `sh` declarations with a BYTE-IDENTICAL template and identical
// input values. Incompatible today: `shell` execution is always singleton.
// ─────────────────────────────────────────────────────────────────────────────

const TWO_IDENTICAL_SHELLS = `
rel item(id: text).
sh left_probe(id: text) -> (out: text) =
  \`printf 's' >> "$LAB_SPAWNS"; printf '%s' "{id}"\`.
sh right_probe(id: text) -> (out: text) =
  \`printf 's' >> "$LAB_SPAWNS"; printf '%s' "{id}"\`.
rel left_seen(id: text, out: text).
rel right_seen(id: text, out: text).
left_seen(id, out) <- item(id), left_probe(id, out).
right_seen(id, out) <- item(id), right_probe(id, out).
`;

// ─────────────────────────────────────────────────────────────────────────────
// 2d. Two `sprefa_extract` projections on DIFFERENT paths. Same template, same
// execution, different input values: two invocation keys, two spawns.
// ─────────────────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  const workDir = mkdtempSync(join(tmpdir(), "effectchain-2-"));
  try {
    console.log("RECEIPT 2a: a plain `sh` host, spawns against demand rows");
    for (const count of [1, 10, 100]) await plainShellAt(count, workDir);

    console.log("\nRECEIPT 2b: the sprefa_extract grouping path, 7 projections over N paths");
    for (const paths of [1, 10, 100]) await extractGroupingAt(paths, 7, workDir);
    console.log("  (projection-count sweep at 10 paths)");
    for (const projections of [1, 2, 7]) await extractGroupingAt(10, projections, workDir);
    console.log("  (control: the SAME 7 projections with one flag of template text differing)");
    await extractGroupingAt(10, 7, workDir, true);

    console.log("\nRECEIPT 2c: two `shell` decls, byte-identical template AND inputs");
    {
      const ledger = join(workDir, "identical-shells");
      writeFileSync(ledger, "", "utf8");
      process.env.LAB_SPAWNS = ledger;
      const served = await start_served(0);
      try {
        const loaded = await post_program(served.port, TWO_IDENTICAL_SHELLS);
        if (loaded.statusCode !== 200) throw new Error(loaded.body);
        await post_arrivals(served.port, [{ rel: "item", sign: "add", row: ["only"] }]);
        await waitUntil(
          async () =>
            (await rowsOf(served.port, "left_seen")).length === 1 &&
            (await rowsOf(served.port, "right_seen")).length === 1,
          "both shell answers",
        );
        receipt("two_identical_shell_decls_demands", 2);
        receipt("two_identical_shell_decls_spawns", spawnCount(ledger));
      } finally {
        await served.stop();
      }
    }

    console.log("\nRECEIPT 2d: identical demand re-asserted (content-addressed cache)");
    {
      const ledger = join(workDir, "reassert");
      writeFileSync(ledger, "", "utf8");
      process.env.LAB_SPAWNS = ledger;
      const served = await start_served(0);
      try {
        const loaded = await post_program(served.port, PLAIN_SHELL);
        if (loaded.statusCode !== 200) throw new Error(loaded.body);
        await post_arrivals(served.port, [{ rel: "item", sign: "add", row: ["repeat"] }]);
        await waitUntil(async () => (await rowsOf(served.port, "seen")).length === 1, "first answer");
        receipt("reassert_spawns_after_first_add", spawnCount(ledger));
        await post_arrivals(served.port, [{ rel: "item", sign: "del", row: ["repeat"] }]);
        await post_arrivals(served.port, [{ rel: "item", sign: "add", row: ["repeat"] }]);
        await waitUntil(async () => (await rowsOf(served.port, "seen")).length === 1, "answer back");
        await new Promise<void>((resolve) => setTimeout(resolve, 300));
        receipt("reassert_spawns_after_retract_and_readd", spawnCount(ledger));
      } finally {
        await served.stop();
      }
    }
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

await main();
