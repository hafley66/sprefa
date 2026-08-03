/** Runtime-side trace publisher: one record per emitted statement that ran.
 *
 * The runtime knows WHICH rule a statement came from (`ruleId`, assigned by
 * lower.pl:statement_rule_ids/3) and how many rows it produced. It does not
 * know where those records should go, so it only publishes; serve/0_trace.ts
 * folds them into the tick line and chooses the sink. A library publishes, an
 * application picks the logger.
 *
 * Off-path cost is one `hasSubscribers` read. `enabled` is exported separately
 * so a caller can skip its own `performance.now()` pair as well, which is the
 * only cost this file cannot absorb.
 */

import diagnostics_channel from "node:diagnostics_channel";

import type { IRuntimeTrace, IServeRuleEvent } from "./types.ts";

export const RUNTIME_CHANNEL_NAMES = { rule: "sprefa:rule" } as const;

const ruleChannel = diagnostics_channel.channel(RUNTIME_CHANNEL_NAMES.rule);

export const RuntimeTrace: IRuntimeTrace = {
  get enabled(): boolean {
    return ruleChannel.hasSubscribers;
  },

  rule(ruleId, rows, wallMs): void {
    if (!ruleChannel.hasSubscribers) return;
    ruleChannel.publish({ rule: ruleId, rows, wall_ms: wallMs } satisfies IServeRuleEvent);
  },
};
