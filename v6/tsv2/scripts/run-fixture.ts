/**
 * run-fixture.ts — the tsv2-side CLI counterpart to
 * v6/prolog/conformance/ticklog.pl: boots a scratch SQLite db, runs a
 * gen/*.ts program over an arrival schedule, prints the shared tick-log
 * envelope to stdout, one line per tick.
 *
 * The one manual `.subscribe()` in THIS script. Each tsv2 script (run-fixture,
 * run-emitted, sweep) is its own one-shot CLI entry point with its own single
 * terminal subscription; none of them is a second subscribe inside v6/dl, whose
 * ratchet (v6/tools/one-subscribe.sh) scans dl/src only.
 *
 * Usage: node --experimental-transform-types scripts/run-fixture.ts <name>
 * where <name> is a key of REGISTRY below.
 */

import { concatMap } from "rxjs";

import { DemandLazinessEffectRows } from "../gen/demand_laziness_effect_rows.ts";
import { SwitchAsKeyedReplace } from "../gen/switch_as_keyed_replace.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, IGenProgram } from "../runtime/types.ts";
import {
  DEMAND_LAZINESS_SCHEDULE,
  DEMAND_LAZINESS_SCHEDULE_PERTURBED,
  SWITCH_AS_KEYED_REPLACE_SCHEDULE,
} from "../tests/schedules.ts";

type RegistryEntry = { readonly program: IGenProgram; readonly schedule: readonly IArrivalBatch[] };

const REGISTRY: Readonly<Record<string, RegistryEntry>> = {
  demand_laziness_effect_rows: { program: DemandLazinessEffectRows, schedule: DEMAND_LAZINESS_SCHEDULE },
  demand_laziness_effect_rows_perturbed: {
    program: DemandLazinessEffectRows,
    schedule: DEMAND_LAZINESS_SCHEDULE_PERTURBED,
  },
  switch_as_keyed_replace: { program: SwitchAsKeyedReplace, schedule: SWITCH_AS_KEYED_REPLACE_SCHEDULE },
};

function main(): void {
  const name = process.argv[2];
  const entry = name === undefined ? undefined : REGISTRY[name];
  if (entry === undefined) {
    process.stderr.write(`usage: run-fixture.ts <${Object.keys(REGISTRY).join("|")}>\n`);
    process.exitCode = 2;
    return;
  }

  const seam = ScratchStore.open(":memory:");
  ScratchStore.boot(seam, entry.program.ddl)
    .pipe(concatMap(() => TickFold.run(entry.program, seam, entry.schedule)))
    .subscribe({
      next: (line) => process.stdout.write(`${line}\n`),
      error: (error: unknown) => {
        process.stderr.write(`${String(error)}\n`);
        process.exitCode = 1;
      },
    });
}

main();
