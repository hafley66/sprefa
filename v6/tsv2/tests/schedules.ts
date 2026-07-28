/**
 * schedules.ts — arrival schedules matching the prolog fixtures' Schedule
 * terms byte-for-byte (v6/prolog/conformance/fixtures/scopes.pl:31, :344),
 * plus one PERTURBED variant of fixture 1 (one extra trailing tick), used to
 * prove the tsv2 side computes deltas from the rules rather than replaying
 * the fixture's own expected answers (plan header HARD RULE). Consumed by
 * both scripts/run-fixture.ts and tests/*.test.ts.
 */

import type { IArrivalBatch } from "../runtime/types.ts";

// scopes.pl:344 demand_laziness_effect_rows
export const DEMAND_LAZINESS_SCHEDULE: readonly IArrivalBatch[] = [
  [{ rel: "open_feed", sign: "add", row: ["session_one", "alpha"] }],
  [{ rel: "open_feed", sign: "del", row: ["session_one", "alpha"] }],
  [{ rel: "open_feed", sign: "add", row: ["session_two", "alpha"] }],
  [{ rel: "open_feed", sign: "add", row: ["session_three", "beta"] }],
  [
    { rel: "open_feed", sign: "del", row: ["session_two", "alpha"] },
    { rel: "open_feed", sign: "del", row: ["session_three", "beta"] },
  ],
];

// same program, one extra tick appended (a brand new session/target the
// fixture's expectations never mention) — see ticklog.pl's
// perturbed_schedule/2 for the mirrored prolog side.
export const DEMAND_LAZINESS_SCHEDULE_PERTURBED: readonly IArrivalBatch[] = [
  ...DEMAND_LAZINESS_SCHEDULE,
  [{ rel: "open_feed", sign: "add", row: ["session_four", "gamma"] }],
];

// scopes.pl:31 switch_as_keyed_replace
export const SWITCH_AS_KEYED_REPLACE_SCHEDULE: readonly IArrivalBatch[] = [
  [{ rel: "route_change", sign: "add", row: ["session_one", "settings"] }],
  [{ rel: "route_change", sign: "add", row: ["session_one", "profile"] }],
];
