import assert from "node:assert/strict";
import { test } from "node:test";

import { ProcessAdapters } from "../serve/1_hosts.ts";

test("the TypeScript target reports the Rust-only Soopy mutation capability as ordinary output", async () => {
  const adapter = ProcessAdapters.get("soopy");
  assert.ok(adapter);

  const stage = JSON.parse(adapter.command({ plan: { name: "source_stage" } as never, witness_digest: "", inputs: new Map() }).stdin ?? "") as {
    readonly stage_id: string;
    readonly outcome: string;
    readonly detail: string;
    readonly document: unknown;
  };
  assert.deepEqual(stage, {
    stage_id: "",
    outcome: "unsupported",
    detail: "soopy requires the Rust runtime target",
    document: [],
  });

  const commit = JSON.parse(adapter.command({ plan: { name: "source_commit" } as never, witness_digest: "", inputs: new Map() }).stdin ?? "") as {
    readonly outcome: string;
    readonly detail: string;
    readonly document: unknown;
  };
  assert.deepEqual(commit, {
    outcome: "unsupported",
    detail: "soopy requires the Rust runtime target",
    document: {},
  });
});
