import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom, of } from "rxjs";

import { EnumPlane } from "../runtime/enumPlane.ts";
import type { IEnumRefColumns, IEnumTypePlan, ISqlSeam } from "../runtime/types.ts";

const types: readonly IEnumTypePlan[] = [
  { name: "inner", variants: [{ tag: "text", rel: "inner_text", fields: ["value"], field_types: ["text"], select_sql: "SELECT id, value FROM inner_text" }] },
  { name: "choice", variants: [
    { tag: "none", rel: "choice_none", fields: [], select_sql: "SELECT id FROM choice_none" },
    { tag: "text", rel: "choice_text", fields: ["value"], field_types: ["text"], select_sql: "SELECT id, value FROM choice_text" },
    { tag: "list", rel: "choice_list", fields: ["value"], field_types: ["list"], select_sql: "SELECT id, value FROM choice_list" },
    { tag: "relation", rel: "choice_relation", fields: ["value"], field_types: ["ref"], select_sql: "SELECT id, value FROM choice_relation" },
    { tag: "nested", rel: "choice_nested", fields: ["value"], field_types: ["relation_id"], field_enums: ["inner"], select_sql: "SELECT id, value FROM choice_nested" },
  ] },
];

const refs: IEnumRefColumns = { resident: [null, { name: "choice", endpoint_index: 0 }] };
let active = "none";

const seam = {
  db: {},
  runner: {
    execute(_db: unknown, statement: { readonly sql: string }) {
      const rows = statement.sql.includes(`choice_${active}`)
        ? active === "none" ? [{ id: 7 }]
          : active === "text" ? [{ id: 7, value: "hello" }]
          : active === "list" ? [{ id: 7, value: "[1,2]" }]
          : active === "relation" ? [{ id: 7, value: '{"id":9}' }]
          : [{ id: 7, value: 7 }]
        : statement.sql.includes("inner_text") && active === "nested" ? [{ id: 7, value: "inside" }] : [];
      return of({ rows, columns: [], rows_affected: 0 });
    },
  },
} as unknown as ISqlSeam;

test("enum ingress preserves add and delete signs and scalar-safe list carriers", () => {
  for (const sign of ["add", "del"] as const) {
    const rows = EnumPlane.intern(types, refs, [{ rel: "resident", sign, row: [7, { tag: "list", value: [1, 2] }] }]);
    assert.deepEqual(rows, [
      { rel: "choice_list", sign, row: [7, "[1,2]"] },
      { rel: "resident", sign, row: [7, 7] },
    ]);
  }
});

test("enum egress yields structured nullary text list relation and nested values", async () => {
  for (const [tag, expected] of [
    ["none", { tag: "none" }], ["text", { tag: "text", value: "hello" }],
    ["list", { tag: "list", value: [1, 2] }], ["relation", { tag: "relation", value: { id: 9 } }],
    ["nested", { tag: "nested", value: { tag: "text", value: "inside" } }],
  ] as const) {
    active = tag;
    const rows = await firstValueFrom(EnumPlane.decode_rows(seam, types, refs, [], "resident", [[7, 7]]));
    assert.deepEqual(rows, [[7, expected]], tag);
  }
});

test("structured enum rows survive final response serialization", async () => {
  active = "none";
  const rows = await firstValueFrom(EnumPlane.decode_rows(seam, types, refs, [], "resident", [[7, 7]]));
  assert.equal(JSON.stringify({ rows }), '{"rows":[[7,{"tag":"none"}]]}');
});
