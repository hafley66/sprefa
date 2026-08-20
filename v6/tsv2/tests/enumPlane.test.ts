import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { EnumPlane } from "../runtime/enumPlane.ts";
import type { IEnumRefColumns, IEnumTypePlan, ISqlSeam } from "../runtime/types.ts";

/* FAIL-PRE-FIX: this door interned a tagged object and minted variant
 * arrivals, so `picked(101, 401)` threw
 * `enum_arrival_shape_mismatch: not_an_object(grade)` and seven fixtures
 * crashed. Sabotage: restore the validate_tagged/encode pair in
 * runtime/enumPlane.ts and the first test below refuses the integer again. */

const types: readonly IEnumTypePlan[] = [
  { name: "grade", variants: [
    { tag: "ripe", rel: "grade_ripe", fields: ["sugar"], field_types: ["int"], select_sql: "SELECT id, sugar FROM grade_ripe" },
    { tag: "green", rel: "grade_green", fields: ["days"], field_types: ["int"], select_sql: "SELECT id, days FROM grade_green" },
  ] },
];

const refs: IEnumRefColumns = { picked: [null, { name: "grade", endpoint_index: 0 }] };

/** No statement reaches SQL on either direction, so the seam refuses to run. */
const seam = { db: {}, runner: { execute() { throw new Error("enum plane ran a statement"); } } } as unknown as ISqlSeam;

test("an enum column carries its referenced instance id through the arrival door", async () => {
  for (const sign of ["add", "del"] as const) {
    const rows = await firstValueFrom(EnumPlane.intern(seam, types, refs, [{ rel: "picked", sign, row: [101, 401] }]));
    assert.deepEqual(rows, [{ rel: "picked", sign, row: [101, 401] }]);
  }
});

test("a tagged value in an enum column is named, never sniffed into a variant row", () => {
  for (const value of [{ tag: "ripe", sugar: 12 }, '{"tag":"ripe","sugar":12}', 12.5]) {
    assert.throws(
      () => EnumPlane.intern(seam, types, refs, [{ rel: "picked", sign: "add", row: [101, value] }]),
      /enum_arrival_shape_mismatch: not_a_reference\(picked, grade\)/,
    );
  }
});

test("the boundary reads an enum column back as the same reference id", async () => {
  const rows = await firstValueFrom(EnumPlane.decode_rows(seam, types, refs, [], "picked", [[101, 401]]));
  assert.deepEqual(rows, [[101, 401]]);
  const deltas = await firstValueFrom(EnumPlane.decode_deltas(seam, types, refs, [], [{ rel: "picked", add: [[101, 401]], del: [] }]));
  assert.deepEqual(deltas, [{ rel: "picked", add: [[101, 401]], del: [] }]);
});
