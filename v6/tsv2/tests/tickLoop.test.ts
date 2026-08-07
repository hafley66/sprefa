/**
 * tickLoop.test.ts — runs both phase-A gen/*.ts programs (plus one
 * perturbed schedule of demand_laziness_effect_rows) through the runtime
 * and checks the emitted tick log against the oracle log frozen in
 * fixtures/*.jsonl (captured from
 * `swipl -q -l v6/prolog/conformance/ticklog.pl -g "emit(...)" -g halt`).
 * Node's `--test` runner subscribes via `firstValueFrom`/`lastValueFrom` —
 * the same test-file exemption the rxjs law gives v6/dl's own tests.
 */

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { firstValueFrom, toArray } from "rxjs";

import { DemandLazinessEffectRows } from "../gen/demand_laziness_effect_rows.ts";
import { SwitchAsKeyedReplace } from "../gen/switch_as_keyed_replace.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, IGenProgram, ITickLogLine } from "../runtime/types.ts";
import {
  DEMAND_LAZINESS_SCHEDULE,
  DEMAND_LAZINESS_SCHEDULE_PERTURBED,
  SWITCH_AS_KEYED_REPLACE_SCHEDULE,
} from "./schedules.ts";

const HERE = dirname(fileURLToPath(import.meta.url));

function read_oracle_lines(fixture_file: string): readonly string[] {
  const text = readFileSync(join(HERE, "fixtures", fixture_file), "utf8");
  return text.split("\n").filter((line) => line.length > 0);
}

async function run_program(program: IGenProgram, schedule: readonly IArrivalBatch[]): Promise<readonly ITickLogLine[]> {
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  return firstValueFrom(TickFold.run(program, seam, schedule).pipe(toArray()));
}

test("demand_laziness_effect_rows matches the oracle tick log", async () => {
  const lines = await run_program(DemandLazinessEffectRows, DEMAND_LAZINESS_SCHEDULE);
  const oracle = read_oracle_lines("demand_laziness_effect_rows.jsonl");
  assert.deepEqual(lines, oracle);
  assert.equal(lines.length, 5);
});

test("switch_as_keyed_replace matches the oracle tick log, including the drain tick", async () => {
  const lines = await run_program(SwitchAsKeyedReplace, SWITCH_AS_KEYED_REPLACE_SCHEDULE);
  const oracle = read_oracle_lines("switch_as_keyed_replace.jsonl");
  assert.deepEqual(lines, oracle);
  assert.equal(lines.length, 3);
  assert.equal(lines[2], '{"tick":3,"deltas":{}}');
});

test("demand_laziness_effect_rows PERTURBED schedule matches the oracle's perturbed log (proves real computation, not replay)", async () => {
  const lines = await run_program(DemandLazinessEffectRows, DEMAND_LAZINESS_SCHEDULE_PERTURBED);
  const oracle = read_oracle_lines("demand_laziness_effect_rows_perturbed.jsonl");
  assert.deepEqual(lines, oracle);
  assert.equal(lines.length, 6);
  // the perturbed tick's payload is not in any fixture Expectations anywhere
  assert.match(lines[5]!, /"gamma"/);
});
