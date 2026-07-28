/**
 * run-emitted.ts — the phase A/B reconciliation runner: executes a PROLOG-
 * EMITTED program module (gen_emitted/*.ts, copied from v6/prolog/compile/
 * out/) on the phase A runtime, printing the shared tick-log envelope so the
 * output diffs directly against conformance/ticklog.pl's oracle log.
 *
 * Differs from run-fixture.ts in one seam: emitted programs carry a `boot`
 * field (parameterized Initial-row INSERTs, header addendum "extend by
 * adding fields") that runs after DDL and before tick 1.
 *
 * Usage: node --experimental-transform-types scripts/run-emitted.ts <name>
 */

import { concat, concatMap, ignoreElements, type Observable } from "rxjs";

import { program as demandLazinessEmitted } from "../gen_emitted/demand_laziness_effect_rows.ts";
import { program as switchAsKeyedReplaceEmitted } from "../gen_emitted/switch_as_keyed_replace.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";
import type { IArrivalBatch, IGenProgram, ISqlSeam } from "../runtime/types.ts";
import {
  DEMAND_LAZINESS_SCHEDULE,
  DEMAND_LAZINESS_SCHEDULE_PERTURBED,
  SWITCH_AS_KEYED_REPLACE_SCHEDULE,
} from "../tests/schedules.ts";

interface IEmittedBootStatement {
  readonly sql: string;
  readonly params: readonly (string | number)[];
}

type EmittedProgram = IGenProgram & { readonly boot: readonly IEmittedBootStatement[] };

type RegistryEntry = { readonly program: EmittedProgram; readonly schedule: readonly IArrivalBatch[] };

const REGISTRY: Readonly<Record<string, RegistryEntry>> = {
  demand_laziness_effect_rows: { program: demandLazinessEmitted, schedule: DEMAND_LAZINESS_SCHEDULE },
  demand_laziness_effect_rows_perturbed: {
    program: demandLazinessEmitted,
    schedule: DEMAND_LAZINESS_SCHEDULE_PERTURBED,
  },
  switch_as_keyed_replace: { program: switchAsKeyedReplaceEmitted, schedule: SWITCH_AS_KEYED_REPLACE_SCHEDULE },
};

function runBoot(seam: ISqlSeam, statements: readonly IEmittedBootStatement[]): Observable<never> {
  return concat(
    ...statements.map((statement) =>
      seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] }),
    ),
  ).pipe(ignoreElements());
}

function main(): void {
  const name = process.argv[2];
  const entry = name === undefined ? undefined : REGISTRY[name];
  if (entry === undefined) {
    process.stderr.write(`usage: run-emitted.ts <${Object.keys(REGISTRY).join("|")}>\n`);
    process.exitCode = 2;
    return;
  }

  const seam = ScratchStore.open(":memory:");
  ScratchStore.boot(seam, entry.program.ddl)
    .pipe(
      concatMap(() => concat(runBoot(seam, entry.program.boot), TickFold.run(entry.program, seam, entry.schedule))),
    )
    .subscribe({
      next: (line) => process.stdout.write(`${line}\n`),
      error: (failure) => {
        process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
        process.exit(1);
      },
    });
}

main();
