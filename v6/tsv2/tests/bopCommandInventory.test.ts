/**
 * bopCommandInventory.test.ts — the two verb inventories agree.
 * registry.pl's `cli_command/3` facts are the single source (its own header
 * says why: a JSON manifest was the other option offered and was rejected as
 * one more artifact to keep in sync for five rows that rename rarely); this
 * test is the grep-style cross-check that stands in for that manifest,
 * reading each side in its own native form -- swipl for the prolog rows,
 * a source-text scan for bop.ts's `.command("...")` lines -- rather than
 * inventing a shared file either side has to remember to update.
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const BOP_TS = fileURLToPath(new URL("../cli/bop.ts", import.meta.url));
const REGISTRY_PL = fileURLToPath(new URL("../../prolog/0_dot_expand/registry.pl", import.meta.url));
const EMITTER_PL = fileURLToPath(new URL("../../prolog/compile/2_emit_cli_inventory.pl", import.meta.url));
const INVENTORY_TS = fileURLToPath(new URL("../cli/0_inventory.ts", import.meta.url));

function registry_verbs(): readonly string[] {
  const result = spawnSync(
    "swipl",
    [
      "-q",
      "-l",
      REGISTRY_PL,
      "-g",
      "forall(cli_command(Verb, _, _), (write(Verb), nl))",
      "-g",
      "halt",
    ],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  return result.stdout.split("\n").map((line) => line.trim()).filter((line) => line.length > 0);
}

function bop_ts_verbs(): readonly string[] {
  const source = readFileSync(BOP_TS, "utf8");
  return [...source.matchAll(/\.command\("([a-z]+)"\)/g)].map((match) => match[1] ?? "");
}

test("registry.pl cli_command/3 and cli/bop.ts's commander verbs name the same set", () => {
  const from_registry = [...registry_verbs()].sort();
  const from_bop_ts = [...bop_ts_verbs()].sort();
  assert.deepEqual(from_bop_ts, from_registry, `registry: ${from_registry.join(",")} vs bop.ts: ${from_bop_ts.join(",")}`);
});

test("generated CLI and HTTP inventory is current with canonical Prolog facts", () => {
  const result = spawnSync(
    "swipl",
    ["-q", "-l", EMITTER_PL, "-g", "emit_cli_inventory:cli_inventory_text(Text),format('~s',[Text])", "-g", "halt"],
    { encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.equal(readFileSync(INVENTORY_TS, "utf8"), result.stdout);
});
