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

// gen_emitted/ is compiler OUTPUT under reconciliation: it is excluded from
// this package's type graph (tsconfig "exclude") until it conforms, so the
// package typecheck stays green while drafts iterate. Loading happens at
// runtime via a computed specifier; conformance is asserted by the log diff,
// and by copying into gen/ (type-checked) once a draft graduates.
const MODULE_OF: Readonly<Record<string, string>> = {
  demand_laziness_effect_rows: "demand_laziness_effect_rows",
  demand_laziness_effect_rows_perturbed: "demand_laziness_effect_rows",
  switch_as_keyed_replace: "switch_as_keyed_replace",
};

const SCHEDULE_OF: Readonly<Record<string, readonly IArrivalBatch[]>> = {
  demand_laziness_effect_rows: DEMAND_LAZINESS_SCHEDULE,
  demand_laziness_effect_rows_perturbed: DEMAND_LAZINESS_SCHEDULE_PERTURBED,
  switch_as_keyed_replace: SWITCH_AS_KEYED_REPLACE_SCHEDULE,
};

function loadEmitted(moduleName: string): Promise<EmittedProgram> {
  const specifier = ["..", "gen_emitted", `${moduleName}.ts`].join("/");
  return import(specifier).then((loaded: { program: EmittedProgram }) => loaded.program);
}

function runBoot(seam: ISqlSeam, statements: readonly IEmittedBootStatement[]): Observable<never> {
  return concat(
    ...statements.map((statement) =>
      seam.runner.execute(seam.db, { sql: statement.sql, args: [...statement.params] }),
    ),
  ).pipe(ignoreElements());
}

function main(): void {
  const name = process.argv[2];
  const moduleName = name === undefined ? undefined : MODULE_OF[name];
  const schedule = name === undefined ? undefined : SCHEDULE_OF[name];
  if (moduleName === undefined || schedule === undefined) {
    process.stderr.write(`usage: run-emitted.ts <${Object.keys(MODULE_OF).join("|")}>\n`);
    process.exitCode = 2;
    return;
  }

  void loadEmitted(moduleName).then((program) => {
    const seam = ScratchStore.open(":memory:");
    ScratchStore.boot(seam, program.ddl)
      .pipe(concatMap(() => concat(runBoot(seam, program.boot), TickFold.run(program, seam, schedule))))
      .subscribe({
        next: (line) => process.stdout.write(`${line}\n`),
        error: (failure) => {
          process.stderr.write(`${failure instanceof Error ? failure.stack : String(failure)}\n`);
          process.exit(1);
        },
      });
  });
}

main();
