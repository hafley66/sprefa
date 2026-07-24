/**
 * 0_scaffold.test.ts — M0 gate: the package boots, store imports resolve and run
 * under node --experimental-transform-types, rxjs + lodash-es are present.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { variable, relRef } from "sprefa-store-engine/src/lower/ast.ts";
import { of, firstValueFrom } from "rxjs";
import { mapValues } from "lodash-es";

test("scaffold: sprefa-store-engine ast constructors import and run", () => {
  assert.equal(variable("path").kind, "var");
  assert.equal(relRef("file", variable("path")).rel, "file");
});

test("scaffold: rxjs and lodash-es resolve", async () => {
  assert.equal(await firstValueFrom(of(41).pipe()), 41);
  assert.deepEqual(mapValues({ tick: 1 }, (n) => n + 1), { tick: 2 });
});
