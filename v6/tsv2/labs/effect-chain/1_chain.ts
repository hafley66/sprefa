/**
 * 1_chain.ts — RECEIPT 1 of the effect chain-and-batch lab.
 *
 * Question: how does one shell run feed another today, how many TICKS does an
 * N-stage chain cost, and can two shell runs be chained INSIDE one tick?
 *
 * Nothing is faked. This posts real .dl6 text to the real served tsv2 engine
 * (serve/4_http.ts), the hosts really spawn `/bin/sh`, and the tick lines below
 * are the real tick log the engine printed.
 *
 * Run: node --experimental-transform-types labs/effect-chain/1_chain.ts
 */

import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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

function waitUntil(predicate: () => Promise<boolean>, what: string, timeoutMs = 15_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return (async () => {
    while (!(await predicate())) {
      if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
      await new Promise<void>((resolve) => setTimeout(resolve, 15));
    }
  })();
}

/** Stage k of the chain: reads stage k-1's output column, spawns, writes its
 *  own. Every template appends one byte to the spawn ledger so the subprocess
 *  count is measured rather than asserted. */
function chainProgram(stages: number): string {
  const lines: string[] = ["rel seed(name: text)."];
  for (let stage = 1; stage <= stages; stage += 1) {
    lines.push(
      `sh stage_${stage}(input: text) -> (out: text) = ` +
        `\`printf 's' >> "$LAB_SPAWNS"; printf '%s' "{input}-${stage}"\`.`,
    );
  }
  for (let stage = 1; stage <= stages; stage += 1) {
    const source = stage === 1 ? "seed(input)" : `s${stage - 1}(input)`;
    lines.push(`rel s${stage}(out: text).`);
    lines.push(`s${stage}(out) <- ${source}, stage_${stage}(input, out).`);
  }
  return `${lines.join("\n")}\n`;
}

/** Both hosts in ONE rule body: the same three shells, spelled as a single
 *  join rather than a chain of rels. */
const SAME_RULE_TWO_HOSTS = `
rel seed(name: text).
sh stage_1(input: text) -> (out: text) = \`printf '%s' "{input}-1"\`.
sh stage_2(input: text) -> (out: text) = \`printf '%s' "{input}-2"\`.
rel s2(out: text).
s2(out) <- seed(name), stage_1(name, mid), stage_2(mid, out).
`;

async function runChain(stages: number, workDir: string, seeds = 1): Promise<void> {
  const spawnLedger = join(workDir, `chain-${stages}x${seeds}-spawns`);
  writeFileSync(spawnLedger, "", "utf8");
  process.env.LAB_SPAWNS = spawnLedger;

  const source = chainProgram(stages);
  const served = await start_served(0);
  try {
    const loaded = await post_program(served.port, source);
    if (loaded.statusCode !== 200) throw new Error(`POST /program -> ${loaded.statusCode} ${loaded.body}`);
    const plans = JSON.parse(loaded.body) as { readonly hosts: readonly string[] };
    receipt(`chain_${stages}_hosts`, plans.hosts);

    const startedAt = performance.now();
    const seeded = await post_arrivals(
      served.port,
      Array.from({ length: seeds }, (_, index) => ({
        rel: "seed",
        sign: "add" as const,
        row: [`alpha-${index}`],
      })),
    );
    receipt(`chain_${stages}_ticks_returned_by_the_seed_post`, seeded.ticks.length);

    const last = `s${stages}`;
    await waitUntil(async () => {
      const rows = JSON.parse((await request(served.port, `/idb/${last}`, "GET")).body) as { rows: unknown[] };
      return rows.rows.length === seeds;
    }, `${last} to hold the chain's ${seeds} answers`);

    const outcomes = tick_events(served.events);
    receipt(`chain_${stages}_total_ticks`, outcomes.length);
    receipt(`chain_${stages}_spawns`, readFileSync(spawnLedger, "utf8").length);
    receipt(`chain_${stages}_wall_ms`, Math.round(performance.now() - startedAt));
    const finalRows = JSON.parse((await request(served.port, `/idb/${last}`, "GET")).body) as {
      rows: readonly (readonly unknown[])[];
    };
    receipt(`chain_${stages}_answer`, finalRows.rows.slice(0, 3));

    // What each tick actually carried: the rel names on each side of the
    // boundary, in tick order. This is the per-stage hop, made visible.
    const perTick = outcomes.map((outcome) => {
      const line = JSON.parse(outcome.line) as {
        readonly tick: number;
        readonly deltas: Record<string, { add: readonly unknown[]; del: readonly unknown[] }>;
      };
      return {
        tick: line.tick,
        rels: Object.entries(line.deltas)
          .filter(([, delta]) => delta.add.length > 0 || delta.del.length > 0)
          .map(([rel]) => rel)
          .sort(),
      };
    });
    if (seeds === 1) {
      receipt(`chain_${stages}_tick_shape`, perTick);
      console.log(`  chain_${stages}_tick_log:`);
      for (const outcome of outcomes) console.log(`    ${outcome.line}`);
    }
  } finally {
    await served.stop();
  }
}

async function main(): Promise<void> {
  const workDir = mkdtempSync(join(tmpdir(), "effectchain-1-"));
  try {
    console.log("RECEIPT 1a: ticks per stage, 1 / 2 / 3 / 4 stage chains");
    for (const stages of [1, 2, 3, 4]) await runChain(stages, workDir);

    console.log("\nRECEIPT 1b: two host atoms in ONE rule body");
    const served = await start_served(0);
    try {
      const refused = await post_program(served.port, SAME_RULE_TWO_HOSTS);
      receipt("two_hosts_one_body_status", refused.statusCode);
      receipt(
        "two_hosts_one_body_refusal",
        (JSON.parse(refused.body) as { error?: string }).error?.split("Unknown message: ")[1] ?? refused.body,
      );
    } finally {
      await served.stop();
    }

    console.log("\nRECEIPT 1c: the 3-stage chain over N seeds at once");
    for (const seeds of [1, 10, 50]) {
      console.log(`  --- ${seeds} seeds`);
      await runChain(3, workDir, seeds);
    }
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

await main();
