/**
 * traceGolden.test.ts — the pinned artifact a SECOND emitter is graded on.
 *
 * The point of registry.pl's trace_event/2 table is that a rust runtime on
 * `tracing` writes the same record this one writes for the same program and
 * the same arrivals. That claim needs a file to point at, and this is it:
 * goldens/trace-line.jsonl.
 *
 * WHAT IS GRADED, stated precisely, because "the same bytes" is not achievable
 * and claiming it would be a lie: the line's PROJECTION onto the schema's
 * declared fields. Every declared field must be present and equal, in the
 * declared order. Anything the SINK adds is ignored, because sink decoration
 * differs by ecosystem and is nobody's contract -- pino prepends a numeric
 * `level` here (and suppressing it via `formatters.level` makes pino emit
 * `{,"tick":...`, malformed JSON, since the formatter's object is spliced in
 * as a prefix chunk), while a `tracing` json layer would add its own `target`
 * and `fields`. A second emitter reproduces the record, not the envelope.
 *
 * Three things are normalized out of the record itself:
 *   timing  a wall clock (`wall_ms`), marked `timing` in the schema. The
 *           stripping reads that mark rather than a list kept in this file, so
 *           a new timing field is excluded the day it is declared.
 *   host    target-specific text (an error rendering), marked `host`.
 *   the program prefix on every rule id. At the TEXT door a program is named
 *           by its source digest (serve/0_compile.ts:99 `sourceDigest`), so the
 *           prefix is stable for fixed source but says nothing about the
 *           emitter, and every whitespace edit to the program below would
 *           rewrite every line of the golden. The rule NAME and ORDINAL are
 *           what a second emitter must reproduce, so the prefix becomes
 *           `<program>` and those are pinned.
 *
 * The program is deliberately dull: two source rels and one head with two
 * arms, no host, no bind, no watcher. A `live_interval` bind or a shell host
 * would make the line depend on a clock or a subprocess, and then the golden
 * would pin this machine rather than the contract.
 *
 * SABOTAGE: dropping `rules` from the tick line in serve/0_trace.ts fails the
 * declared-field check BY NAME before the byte compare runs; renaming an
 * ordinal in lower.pl (#2 -> #3) fails the byte compare on the lines that rule
 * appears in.
 */

import assert from "node:assert/strict";
import { mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { TRACE_SCHEMA } from "../runtime/0_traceSchema.ts";

const GOLDEN = fileURLToPath(new URL("../goldens/trace-line.jsonl", import.meta.url));

const PROGRAM = `rel event_a(id: text) log keep(all).
rel event_b(id: text) log keep(all).
rel merged(id: text) log keep(all).

merged(Id) <+ event_a(Id).
merged(Id) <+ event_b(Id).
`;

function schema_fields(event_name: string): readonly { readonly key: string; readonly stability: string }[] {
  const event = TRACE_SCHEMA.find((candidate) => candidate.name === event_name);
  assert.ok(event, `no trace_event row named ${event_name}`);
  return event.fields;
}

/** The declared record, in declared order, with the unportable marks dropped.
 *  A field the sink failed to write is named rather than silently absent. */
function project(line: Record<string, unknown>, event_name: string): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const field of schema_fields(event_name)) {
    assert.ok(field.key in line, `the sink dropped "${field.key}", which trace_event(${event_name}) declares`);
    if (field.stability !== "stable") continue;
    const value = line[field.key];
    if (field.key === "rules" && Array.isArray(value)) {
      out[field.key] = value.map((record: Record<string, unknown>) => {
        const rule = project(record, "rule");
        return { ...rule, rule: String(rule.rule).replace(/^[^:]+:/, "<program>:") };
      });
      continue;
    }
    out[field.key] = value;
  }
  return out;
}

test("the declared half of the trace matches the pinned golden", async () => {
  const log_path = join(mkdtempSync(join(tmpdir(), "trace-golden-")), "perf.jsonl");
  // Read by ServeTrace.installFromEnv() when serve/0_trace.ts first loads, so
  // the import has to happen after this line -- hence the dynamic import.
  process.env.DL_PERF_LOG = log_path;
  const { start_served, post_program, post_arrivals } = await import("./serveHelpers.ts");

  const served = await start_served();
  try {
    const loaded = await post_program(served.port, PROGRAM);
    assert.equal(loaded.statusCode, 200, loaded.body);
    await post_arrivals(served.port, [
      { rel: "event_a", sign: "add", row: ["a1"] },
      { rel: "event_a", sign: "add", row: ["a2"] },
      { rel: "event_b", sign: "add", row: ["b1"] },
    ]);
    await post_arrivals(served.port, [{ rel: "event_b", sign: "add", row: ["b2"] }]);
  } finally {
    await served.stop();
  }

  const actual = `${readFileSync(log_path, "utf8")
    .trim()
    .split("\n")
    .map((line) => JSON.stringify(project(JSON.parse(line) as Record<string, unknown>, "tick_line")))
    .join("\n")}\n`;

  if (process.env.TRACE_GOLDEN_WRITE === "1") {
    const { writeFileSync } = await import("node:fs");
    writeFileSync(GOLDEN, actual, "utf8");
  }
  assert.equal(actual, readFileSync(GOLDEN, "utf8"), "re-pin with TRACE_GOLDEN_WRITE=1 after a deliberate change");
});
