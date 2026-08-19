import assert from "node:assert/strict";
import { test } from "node:test";

import { ProcessAdapters } from "../serve/1_hosts.ts";
import type { IRowValue } from "../runtime/types.ts";

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

test("the extractor adapter constructs argv from demand columns", () => {
  const adapter = ProcessAdapters.get("sprefa_extract");
  assert.ok(adapter);
  const spec = adapter.command({
    plan: { name: "extract" } as never,
    witness_digest: "",
    inputs: new Map([
      ["flags", ["--family", "call"] as unknown as IRowValue],
      ["path", "src/main.ts"],
    ]) as never,
  });
  assert.deepEqual(spec, {
    argv: [process.env.DL_EXTRACT_BIN ?? "extract", "--family", "call", "src/main.ts"],
    env: {},
  });
});
