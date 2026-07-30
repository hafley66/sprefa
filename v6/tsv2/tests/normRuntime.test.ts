import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom, toArray } from "rxjs";

import { ProgramCompiler } from "../serve/0_compile.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { TickFold } from "../runtime/tickLoop.ts";

const SOURCE = [
  "rel route(raw: text).",
  "rel route_key(raw: text, key: text).",
  "route_key(raw, norm(raw)) <- route(raw).",
].join("\n");

test("norm: emitted SQLite keeps V5 ASCII alphanumerics and lowercases them", async () => {
  const program = await firstValueFrom(ProgramCompiler.compile(SOURCE));
  const seam = ScratchStore.open(":memory:");
  await firstValueFrom(ScratchStore.boot(seam, program.ddl));
  await firstValueFrom(
    TickFold.run(program, seam, [[
      { rel: "route", sign: "add", row: ["Route /V2: Café_42"] },
      { rel: "route", sign: "add", row: ["---"] },
      { rel: "route", sign: "add", row: ["AZ-09_é"] },
    ]]).pipe(toArray()),
  );
  const result = await firstValueFrom(seam.runner.execute(seam.db, program.finalSelect.route_key!));
  assert.deepEqual(
    [...result.rows].sort((left, right) => String(left.raw).localeCompare(String(right.raw))),
    [
      { raw: "---", key: "" },
      { raw: "AZ-09_é", key: "az09" },
      { raw: "Route /V2: Café_42", key: "routev2caf42" },
    ],
  );
});
