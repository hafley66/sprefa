/**
 * traceSchema.test.ts — the DL_PERF_LOG wire contract holds on this side.
 *
 * registry.pl's `trace_event/2` rows are the single source, and the point of
 * having them at all is a SECOND emitter: a rust runtime on `tracing` has to
 * write the same keys in the same order for the same program. Three ways this
 * drifts and three receipts:
 *
 *   1. the generated file goes stale against the facts     -> re-emit and diff
 *   2. a key stops following the convention 6_profile.pl    -> spell the rule
 *      states (lower snake case, elapsed in *_ms)              out and check it
 *   3. the RUNTIME publishes keys the schema does not have  -> drive a real
 *      (the failure the rename of `ms`/`witnessDigest`         publish through
 *      would otherwise have reintroduced silently)            the channel
 *
 * Receipt 3 is the one that bites: 1 and 2 both pass on a schema nobody emits.
 * SABOTAGE: publishing `ms` instead of `wall_ms` from serve/0_trace.ts leaves 1
 * and 2 green and fails 3 with
 *   actual: [ 'tick', 'rels', 'rows', 'statements', 'ms' ]
 *   expected: [ 'tick', 'rels', 'rows', 'statements', 'wall_ms' ]
 */

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import diagnostics_channel from "node:diagnostics_channel";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { SERVE_CHANNEL_NAMES, ServeTrace } from "../serve/0_trace.ts";
import { TRACE_SCHEMA } from "../runtime/0_traceSchema.ts";

const EMITTER_PL = fileURLToPath(new URL("../../prolog/compile/3_emit_trace_schema.pl", import.meta.url));
const SCHEMA_TS = fileURLToPath(new URL("../runtime/0_traceSchema.ts", import.meta.url));

function eventFields(name: string): readonly string[] {
  const event = TRACE_SCHEMA.find((candidate) => candidate.name === name);
  assert.ok(event, `no trace_event row named ${name}`);
  return event.fields.map((field) => field.key);
}

test("the generated schema is not stale against registry.pl's trace_event rows", () => {
  const emitted = spawnSync(
    "swipl",
    ["-q", "-l", EMITTER_PL, "-g", "trace_schema_text(Text), write(Text)", "-g", "halt"],
    { encoding: "utf8" },
  );
  assert.equal(emitted.status, 0, emitted.stderr);
  assert.equal(
    emitted.stdout,
    readFileSync(SCHEMA_TS, "utf8"),
    "runtime/0_traceSchema.ts is stale; re-run swipl -q -l v6/prolog/compile/3_emit_trace_schema.pl -g emit_trace_schema -g halt",
  );
});

test("every wire key is lower snake case and every elapsed value ends _ms", () => {
  for (const event of TRACE_SCHEMA) {
    for (const field of event.fields) {
      assert.match(field.key, /^[a-z][a-z0-9_]*$/, `${event.name}.${field.key} is not lower snake case`);
      if (field.stability === "timing") {
        assert.ok(field.key.endsWith("_ms"), `${event.name}.${field.key} is a clock and does not end _ms`);
      }
    }
  }
});

test("the tick and effect records the runtime publishes carry exactly the schema's keys, in order", () => {
  const tickChannel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.tick);
  const effectChannel = diagnostics_channel.channel(SERVE_CHANNEL_NAMES.effect);
  const published: Record<string, readonly string[]> = {};

  const onTick = (message: unknown): void => {
    published.tick = Object.keys(message as object);
  };
  const onEffect = (message: unknown): void => {
    published.effect = Object.keys(message as object);
  };
  tickChannel.subscribe(onTick);
  effectChannel.subscribe(onEffect);
  try {
    ServeTrace.tick(7, 4, 312, 18, 41);
    ServeTrace.effect("weigh", "abc123", "done", 1, 2, undefined);
  } finally {
    tickChannel.unsubscribe(onTick);
    effectChannel.unsubscribe(onEffect);
  }

  // The tick LINE adds the three drained arrays after the tick's own fields;
  // the tick EVENT is that prefix, which is what the channel carries.
  const tickLineKeys = eventFields("tick_line");
  assert.deepEqual(published.tick, tickLineKeys.slice(0, tickLineKeys.indexOf("effects")));
  assert.deepEqual(published.effect, eventFields("effect"));
});
