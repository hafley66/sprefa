/**
 * 3_subscribe.ts — the emitted module's subscribe-cone filter (ladder step 2,
 * plans/2026-08-03-impact-lazy-fused.md section 6 step 2).
 *
 * Every emitted module carries `subscribedRels` (2_subscribe.pl's cone) and
 * calls these five filters ONCE, at module scope, before the tick path names
 * any statement list. `SPREFA_TSV2_SUBSCRIBE_PRUNE=on` is the only value that
 * prunes; every other value, unset included, hands back the SAME ARRAY the
 * emitter wrote, by reference, so the default path is byte-identical rather
 * than merely equivalent.
 *
 * Two keep rules, and they are not the same rule:
 *   - derivation (level, edge, boot recompute) survives only inside the cone;
 *   - storage (relations, retention, boot seeds) survives inside the cone OR
 *     for any rel in `arrivalTargets`, because ruling
 *     edge_before_first_subscribe fixes ingestion as eager: a rel outside
 *     every query still absorbs its arrivals into its table.
 * DDL is never pruned at all; the tables exist whatever the cone says.
 *
 * The flag lives here rather than in the 200-odd generated files so the env
 * name has one spelling; SPREFA_TSV2_EMITTER_MODE is read in the generated
 * files and is the precedent this deliberately does not copy.
 */

import type { ISubscribeCone, ISubscribedRel, ISubscribePruneMode } from "./types.ts";

/** "captured/1" -> "captured". A rel name is unique per program (one decl per
 *  name), so dropping the arity cannot merge two different rels. */
function coneNames(subscribedRels: readonly ISubscribedRel[]): ReadonlySet<string> {
  const names = new Set<string>();
  for (const ref of subscribedRels) {
    const slash = ref.lastIndexOf("/");
    names.add(slash === -1 ? ref : ref.slice(0, slash));
  }
  return names;
}

function storedNames(
  subscribedRels: readonly ISubscribedRel[],
  arrivalTargets: readonly string[],
): ReadonlySet<string> {
  const names = new Set<string>(coneNames(subscribedRels));
  for (const target of arrivalTargets) names.add(target);
  return names;
}

export const SubscribeCone: ISubscribeCone = {
  mode(): ISubscribePruneMode {
    return process.env["SPREFA_TSV2_SUBSCRIBE_PRUNE"] === "on" ? "on" : "off";
  },

  relations(mode, relations, subscribedRels, arrivalTargets) {
    if (mode === "off") return relations;
    const kept = storedNames(subscribedRels, arrivalTargets);
    return relations.filter((relation) => kept.has(relation.rel));
  },

  levels(mode, statements, subscribedRels) {
    if (mode === "off") return statements;
    const kept = coneNames(subscribedRels);
    return statements.filter((statement) => kept.has(statement.headRel));
  },

  edges(mode, statements, subscribedRels) {
    if (mode === "off") return statements;
    const kept = coneNames(subscribedRels);
    return statements.filter((statement) => kept.has(statement.headRel));
  },

  retention(mode, statements, subscribedRels, arrivalTargets) {
    if (mode === "off") return statements;
    const kept = storedNames(subscribedRels, arrivalTargets);
    return statements.filter((statement) => kept.has(statement.rel));
  },

  boot(mode, statements, subscribedRels, arrivalTargets) {
    if (mode === "off") return statements;
    const kept = storedNames(subscribedRels, arrivalTargets);
    return statements.filter((statement) => kept.has(statement.rel));
  },
};
