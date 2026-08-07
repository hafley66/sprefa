/**
 * ruleTrace.test.ts — a tick line can name WHICH rule fired.
 *
 * Before this, the trace could say "tick 7 ran 18 statements" and nothing
 * more; `IIncrementalEdgeStatement` carried a `headRel` and no rule identity,
 * so two arms of one relation were indistinguishable in the log. lower.pl
 * assigns "<program>:<name>/<arity>#<ordinal>" and the runtime publishes one
 * record per statement that ran.
 *
 * WHAT THE ORDINAL COUNTS, stated exactly, because it is not "rules in the
 * source file": it counts LOWERED STATEMENTS sharing a head. An edge rule
 * lowers to one statement per arm, so `out/1#1` and `out/1#2` below really are
 * this program's two arms. A LEVEL rule's clauses fold into a single UNION'd
 * insert by construction (lower.pl:level_statement_group/3 hands the emitter a
 * LIST of insert SQLs under one head), so a level head is always `#1` and its
 * clauses are not separable at this seam. Naming them would mean emitting them
 * as separate statements, which is a change to the plan, not to the trace.
 *
 * The receipt drives a REAL emitted module through a real tick rather than
 * asserting on the plan literal: a `ruleId` present in gen_emitted/*.ts and
 * never published would pass any static check and leave the log exactly as
 * uninformative as before.
 *
 * SABOTAGE: publishing `statement.headRel` instead of `statement.ruleId` from
 * runtime/1_incremental.ts turns ALL THREE red (measured, not predicted -- the
 * note here first claimed only the second would move):
 *   1  actual: 'out'  expected: /^[a-z0-9_]+:[a-z0-9_]+\/\d+#\d+$/
 *   2  - 'merge_batches_per_tick:out/1#1'  - 'merge_batches_per_tick:out/1#2'
 *   3  the `:out/` filter matches nothing, so the derived total is 0, not 3
 */

import assert from "node:assert/strict";
import diagnostics_channel from "node:diagnostics_channel";
import { test } from "node:test";

import { firstValueFrom } from "rxjs";

import { program } from "../gen_emitted/merge_batches_per_tick.ts";
import { ScratchStore } from "../runtime/scratchStore.ts";
import { RUNTIME_CHANNEL_NAMES } from "../runtime/trace.ts";
import type { IArrivalRow, IServeRuleEvent } from "../runtime/types.ts";

/** Both arms carry rows, so neither id can appear for a reason other than
 *  having actually derived something. */
function arrivals(): readonly IArrivalRow[] {
  return [
    { rel: "event_a", sign: "add", row: ["a1"] },
    { rel: "event_a", sign: "add", row: ["a2"] },
    { rel: "event_b", sign: "add", row: ["b1"] },
  ];
}

async function tick_with_trace(): Promise<readonly IServeRuleEvent[]> {
  const channel = diagnostics_channel.channel(RUNTIME_CHANNEL_NAMES.rule);
  const captured: IServeRuleEvent[] = [];
  const on_rule = (message: unknown): void => {
    captured.push(message as IServeRuleEvent);
  };
  channel.subscribe(on_rule);
  try {
    const seam = ScratchStore.open(":memory:");
    await firstValueFrom(ScratchStore.boot(seam, program.ddl));
    await firstValueFrom(program.tick(seam, arrivals()));
    seam.db.close();
  } finally {
    channel.unsubscribe(on_rule);
  }
  return captured;
}

test("a tick publishes one record per emitted statement that ran", async () => {
  const records = await tick_with_trace();
  assert.ok(records.length > 0, "a tick over two rels published no rule records at all");
  for (const record of records) {
    assert.match(
      record.rule,
      /^[a-z0-9_]+:[a-z0-9_]+\/\d+#\d+$/,
      `rule id is not <program>:<name>/<arity>#<ordinal>: ${record.rule}`,
    );
    assert.ok(Number.isInteger(record.rows) && record.rows >= 0, `rows is not a count: ${record.rows}`);
    assert.ok(record.wall_ms >= 0, `wall_ms is not an elapsed value: ${record.wall_ms}`);
  }
});

test("two arms of one head are two different rules in the log", async () => {
  const records = await tick_with_trace();
  const out_arms = [...new Set(records.map((record) => record.rule).filter((rule) => rule.includes(":out/")))].sort();

  assert.deepEqual(
    out_arms,
    ["merge_batches_per_tick:out/1#1", "merge_batches_per_tick:out/1#2"],
    "the event_a arm and the event_b arm must be distinguishable in the trace",
  );
});

test("the records account for the rows the tick actually derived", async () => {
  const records = await tick_with_trace();
  const derived = records
    .filter((record) => record.rule.includes(":out/"))
    .reduce((total, record) => total + record.rows, 0);

  // Two `event_a` rows and one `event_b` row all merge into `out`.
  assert.equal(derived, 3, "out derived three rows, so its arms must report three");
});
