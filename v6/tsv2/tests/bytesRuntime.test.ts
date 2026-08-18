import assert from "node:assert/strict";
import test from "node:test";
import { base64_to_bytes, bytes_to_base64, decode_json_arrivals } from "../runtime/boundary.ts";
import { TickLogEmitter } from "../runtime/ticklog.ts";

test("bytes use RFC 4648 tagged transport and deterministic tick rendering", () => {
  const value = new Uint8Array([0x00, 0x7f, 0x80, 0xff]);
  assert.deepEqual([...base64_to_bytes(bytes_to_base64(value))], [...value]);
  assert.equal(bytes_to_base64(new Uint8Array()), "");
  const decoded = decode_json_arrivals(
    [[{ rel: "payload", sign: "add", row: [{ $bytes: "AH+A/w==" }] }]],
    { payload: ["bytes"] },
  );
  assert.deepEqual([...decoded[0]![0]!.row[0] as Uint8Array], [...value]);
  assert.throws(() => decode_json_arrivals(
    [[{ rel: "payload", sign: "add", row: ["AH+A/w=="] }]],
    { payload: ["bytes"] },
  ), /tagged \$bytes object/);
  assert.equal(
    TickLogEmitter.line(1, { carry_pending: false, rels: [{ rel: "payload", add: [[value]], del: [] }] }, { payload: ["bytes"] }),
    '{"tick":1,"deltas":{"payload":{"add":[[{"$bytes":"AH+A/w=="}]],"del":[]}}}',
  );
  assert.throws(() => base64_to_bytes("%%%"), /invalid_bytes_base64/);
  assert.throws(() => base64_to_bytes("AB=="), /invalid_bytes_base64/);
  assert.throws(() => base64_to_bytes("AAB="), /invalid_bytes_base64/);
});
