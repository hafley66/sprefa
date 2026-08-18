import assert from "node:assert/strict";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { HostExecutors } from "../serve/1_hosts.ts";

test("the TypeScript target reports the Rust-only Soopy mutation capability as ordinary output", async () => {
  const executor = HostExecutors.get("soopy_mutation");
  assert.ok(executor);

  const stage = JSON.parse(await firstValueFrom(executor("source_stage", "ignored", {}))) as {
    readonly stage_id: string;
    readonly outcome: string;
    readonly detail: string;
    readonly document: unknown;
  };
  assert.deepEqual(stage, {
    stage_id: "",
    outcome: "unsupported",
    detail: "soopy_mutation requires the Rust runtime target",
    document: [],
  });

  const commit = JSON.parse(await firstValueFrom(executor("source_commit", "ignored", {}))) as {
    readonly outcome: string;
    readonly detail: string;
    readonly document: unknown;
  };
  assert.deepEqual(commit, {
    outcome: "unsupported",
    detail: "soopy_mutation requires the Rust runtime target",
    document: {},
  });
});
