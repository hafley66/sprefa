/**
 * 4_fanin_gap.ts — RECEIPT 4 of the effect chain-and-batch lab.
 *
 * v5's `collect` is FAN-IN: N body solutions become ONE request whose hole
 * carries the comma-joined value set. Receipt 2 showed v6's grouping is FAN-OUT
 * dedupe instead. This receipt asks whether fan-in can be written by hand in v6
 * today, without any new construct.
 *
 * It cannot, and the reason is one missing aggregate. A host input is an
 * ordinary column, so an aggregate head that produced the joined text would
 * carry the whole set into one demand row and one spawn. `registry.pl` ships
 * count/sum/min/max/avg live and json_array/json_object REFUSED, so no shipped
 * aggregate returns a list or a string. 4b proves the mechanism works with the
 * aggregates that DO ship (an aggregated head really does collapse N rows into
 * one demand and one spawn); 4a proves the list-shaped one is refused.
 *
 * Run: node --experimental-transform-types labs/effect-chain/4_fanin_gap.ts
 */

import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { postArrivals, postProgram, request, startServed } from "../../tests/serveHelpers.ts";

function receipt(name: string, value: unknown): void {
  console.log(`  ${name} = ${JSON.stringify(value)}`);
}

function waitUntil(predicate: () => Promise<boolean>, what: string, timeoutMs = 20_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  return (async () => {
    while (!(await predicate())) {
      if (Date.now() >= deadline) throw new Error(`timeout waiting for ${what}`);
      await new Promise<void>((resolve) => setTimeout(resolve, 15));
    }
  })();
}

/** The fan-in a v5 `collect(id)` writes: one host call over the whole set. */
const LIST_AGGREGATE = `
rel item(id: text).
rel joined(items: text).
joined(json_array(id)) <- item(id).
sh gather(items: text) -> (out: text) = \`printf '%s' "{items}"\`.
rel gathered(items: text, out: text).
gathered(items, out) <- joined(items), gather(items, out).
`;

/** The same shape with a SHIPPED aggregate. Nothing about the demand plane
 *  objects to an aggregated head; only the list-shaped aggregate is missing. */
const SCALAR_AGGREGATE = `
rel item(id: int).
rel highest(id: int).
highest(max(id)) <- item(id).
sh gather(id: int) -> (out: text) =
  \`printf 's' >> "$LAB_SPAWNS"; printf '%s' "{id}-gathered"\`.
rel gathered(id: int, out: text).
gathered(id, out) <- highest(id), gather(id, out).
`;

/** Two aggregates over the same set, to show the fold is per-column and cannot
 *  become a set-valued column. */
const COUNT_AGGREGATE = `
rel item(id: text).
rel tally(n: int).
tally(count(id)) <- item(id).
rel report(n: int).
report(n) <- tally(n).
`;

async function main(): Promise<void> {
  const workDir = mkdtempSync(join(tmpdir(), "effectchain-4-"));
  try {
    console.log("RECEIPT 4a: a list-shaped aggregate head (the v5 collect shape)");
    {
      const served = await startServed(0);
      try {
        const answer = await postProgram(served.port, LIST_AGGREGATE);
        receipt("list_aggregate_status", answer.statusCode);
        receipt(
          "list_aggregate_refusal",
          (JSON.parse(answer.body) as { error?: string }).error?.split("\n")[0]?.slice(-160) ?? answer.body,
        );
      } finally {
        await served.stop();
      }
    }

    console.log("\nRECEIPT 4b: a SHIPPED aggregate head feeding the same host");
    {
      const ledger = join(workDir, "scalar-aggregate");
      writeFileSync(ledger, "", "utf8");
      process.env.LAB_SPAWNS = ledger;
      const served = await startServed(0);
      try {
        const loaded = await postProgram(served.port, SCALAR_AGGREGATE);
        receipt("scalar_aggregate_status", loaded.statusCode);
        if (loaded.statusCode !== 200) throw new Error(loaded.body);
        const batch = Array.from({ length: 5 }, (_, index) => ({
          rel: "item",
          sign: "add" as const,
          row: [index],
        }));
        await postArrivals(served.port, batch);
        await waitUntil(async () => {
          const rows = JSON.parse((await request(served.port, "/idb/gathered", "GET")).body) as {
            rows: unknown[];
          };
          return rows.rows.length === 1;
        }, "the aggregated demand's answer");
        receipt("scalar_aggregate_item_rows", 5);
        receipt("scalar_aggregate_spawns", readFileSync(ledger, "utf8").length);
        receipt(
          "scalar_aggregate_answer",
          (JSON.parse((await request(served.port, "/idb/gathered", "GET")).body) as {
            rows: readonly (readonly unknown[])[];
          }).rows,
        );
      } finally {
        await served.stop();
      }
    }

    console.log("\nRECEIPT 4b2: min/max over a TEXT column");
    {
      const served = await startServed(0);
      try {
        const answer = await postProgram(
          served.port,
          "rel item(id: text).\nrel highest(id: text).\nhighest(max(id)) <- item(id).\n",
        );
        receipt("text_max_status", answer.statusCode);
        receipt(
          "text_max_refusal",
          (JSON.parse(answer.body) as { error?: string }).error?.split("refusal ")[1]?.slice(0, 80) ?? answer.body,
        );
      } finally {
        await served.stop();
      }
    }

    console.log("\nRECEIPT 4c: the shipped aggregate inventory, as the compiler answers it");
    {
      const served = await startServed(0);
      try {
        receipt("count_head_status", (await postProgram(served.port, COUNT_AGGREGATE)).statusCode);
        for (const [name, head] of [
          ["sum", "tally(sum(n))"],
          ["min", "tally(min(n))"],
          ["max", "tally(max(n))"],
          ["avg", "tally(avg(n))"],
          ["json_array", "tally(json_array(n))"],
          ["json_object", "tally(json_object(n, n))"],
          ["group_concat", "tally(group_concat(n))"],
        ] as const) {
          const source = `rel item(n: int).\nrel tally(out: text).\n${head} <- item(n).\n`;
          const answer = await postProgram(served.port, source);
          receipt(`aggregate_${name}_status`, answer.statusCode);
        }
      } finally {
        await served.stop();
      }
    }
    console.log("\nRECEIPT 4d: what `group_concat` actually compiled to");
    {
      const served = await startServed(0);
      try {
        const source = "rel item(n: int).\nrel tally(out: text).\ntally(group_concat(n)) <- item(n).\n";
        receipt("group_concat_status", (await postProgram(served.port, source)).statusCode);
        const replied = await postArrivals(
          served.port,
          [1, 2, 3].map((n) => ({ rel: "item", sign: "add" as const, row: [n] })),
        );
        receipt("group_concat_tick_log", replied.ticks.map((entry) => entry.line));
        receipt(
          "group_concat_rows",
          (JSON.parse((await request(served.port, "/idb/tally", "GET")).body) as {
            rows: readonly (readonly unknown[])[];
          }).rows,
        );
      } finally {
        await served.stop();
      }
    }
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

await main();
