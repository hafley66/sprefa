import { antijoinSteps } from "../ideas/antijoin/0_steps"
import { bilinearOpJoinDeltaRuleSteps } from "../ideas/bilinear-op-join-delta-rule/0_steps"
import { clockCheckerSteps } from "../ideas/clock-checker/0_steps"
import { consNilListsSteps } from "../ideas/cons-nil-lists/0_steps"
import { deltaSteps } from "../ideas/delta/0_steps"
import { delayZ1Steps } from "../ideas/delay-z-1/0_steps"
import { distinctThresholdSteps } from "../ideas/distinct-threshold/0_steps"
import { frontierSteps } from "../ideas/frontier/0_steps"
import { fsWatchContinuingSteps } from "../ideas/fs-watch-continuing/0_steps"
import { integrateDifferentiateSteps } from "../ideas/integrate-differentiate/0_steps"
import { internSteps } from "../ideas/intern/0_steps"
import { joinSteps } from "../ideas/join/0_steps"
import { letrecFixpointSteps } from "../ideas/letrec-fixpoint/0_steps"
import { linearOpSteps } from "../ideas/linear-op/0_steps"
import { magicSetsDemandSteps } from "../ideas/magic-sets-demand/0_steps"
import { mintDictionarySteps } from "../ideas/mint-dictionary/0_steps"
import { nestedTimeSteps } from "../ideas/nested-time/0_steps"
import { reduceAggregatesSteps } from "../ideas/reduce-aggregates/0_steps"
import { requestAnswerEffectsSteps } from "../ideas/request-answer-effects/0_steps"
import { retractionSteps } from "../ideas/retraction/0_steps"
import { strataSteps } from "../ideas/strata/0_steps"
import { strConsSteps } from "../ideas/str-cons/0_steps"
import { semiNaiveSteps } from "../ideas/semi-naive/0_steps"
import { termLtSteps } from "../ideas/term-lt/0_steps"
import { topKSteps } from "../ideas/top-k/0_steps"
import { unionNegateSteps } from "../ideas/union-negate/0_steps"
import { windowSteps } from "../ideas/window/0_steps"
import { zsetSteps } from "../ideas/zset/0_steps"
import { dredSteps } from "../ideas/dred/0_steps"
import type { Step } from "./scene/0_types"

export type GlossaryIdea = { slug: string; title: string; summary: string; built: boolean; steps?: readonly Step[] }
export type GlossaryGroup = { slug: string; title: string; ideas: readonly GlossaryIdea[] }

export const glossary: readonly GlossaryGroup[] = [
  { slug: "core-algebra", title: "Core algebra", ideas: [
    { slug: "z-set", title: "Z-set", summary: "Rows carrying integer weights", built: true, steps: zsetSteps },
    { slug: "delta", title: "Delta", summary: "A change between two states", built: true, steps: deltaSteps },
    { slug: "integrate-differentiate", title: "Integrate / differentiate", summary: "Move between states and changes", built: true, steps: integrateDifferentiateSteps },
    { slug: "linear-op", title: "Linear op", summary: "An operator that distributes over addition", built: true, steps: linearOpSteps },
    { slug: "bilinear-op-join-delta-rule", title: "Bilinear op / join delta rule", summary: "Incrementalize operators with two inputs", built: true, steps: bilinearOpJoinDeltaRuleSteps },
    { slug: "distinct-threshold", title: "Distinct / threshold", summary: "Turn integer weights into presence", built: true, steps: distinctThresholdSteps },
  ] },
  { slug: "time", title: "Time", ideas: [
    { slug: "frontier", title: "Frontier", summary: "The boundary of incomplete time", built: true, steps: frontierSteps },
    { slug: "strata", title: "Strata", summary: "An evaluation order for dependent computations", built: true, steps: strataSteps },
    { slug: "letrec-fixpoint", title: "LetRec fixpoint", summary: "Iterate a recursive definition to stability", built: true, steps: letrecFixpointSteps },
    { slug: "nested-time", title: "Nested time", summary: "Outer updates with inner iteration rounds", built: true, steps: nestedTimeSteps },
    { slug: "delay-z-1", title: "Delay z⁻¹", summary: "Shift a value by one logical time", built: true, steps: delayZ1Steps },
    { slug: "clock-checker", title: "Clock checker", summary: "A planned check for temporal dependencies", built: true, steps: clockCheckerSteps },
  ] },
  { slug: "operators", title: "Operators", ideas: [
    { slug: "join", title: "Join", summary: "Match rows through shared keys", built: true, steps: joinSteps },
    { slug: "antijoin", title: "Antijoin", summary: "Keep rows with no matching counterpart", built: true, steps: antijoinSteps },
    { slug: "reduce-aggregates", title: "Reduce / aggregates", summary: "Maintain grouped summaries", built: true, steps: reduceAggregatesSteps },
    { slug: "top-k", title: "Top-k", summary: "Maintain the first ranked rows", built: true, steps: topKSteps },
    { slug: "window", title: "Window", summary: "Restrict input to a range of time", built: true, steps: windowSteps },
    { slug: "union-negate", title: "Union / negate", summary: "Combine collections and reverse weights", built: true, steps: unionNegateSteps },
  ] },
  { slug: "recursion", title: "Recursion", ideas: [
    { slug: "semi-naive", title: "Semi-naive", summary: "Feed only new rows into each round", built: true, steps: semiNaiveSteps },
    { slug: "dred", title: "DRed", summary: "Delete and rederive recursive facts", built: true, steps: dredSteps },
    { slug: "magic-sets-demand", title: "Magic sets / demand", summary: "Restrict recursion to requested results", built: true, steps: magicSetsDemandSteps },
  ] },
  { slug: "kernel", title: "Kernel", ideas: [
    { slug: "mint-dictionary", title: "Mint dictionary", summary: "Assign stable identities to values", built: true, steps: mintDictionarySteps },
    { slug: "cons-nil-lists", title: "cons / nil lists", summary: "Build lists from a head and tail", built: true, steps: consNilListsSteps },
    { slug: "intern", title: "intern", summary: "Construct the same node from the same arguments", built: true, steps: internSteps },
    { slug: "str-cons", title: "str.cons", summary: "Construct strings from two parts", built: true, steps: strConsSteps },
    { slug: "term-lt", title: "TermLt", summary: "Order terms across their runtime shapes", built: true, steps: termLtSteps },
  ] },
  { slug: "host", title: "Host", ideas: [
    { slug: "request-answer-effects", title: "Request / answer effects", summary: "Connect relational requests to host answers", built: true, steps: requestAnswerEffectsSteps },
    { slug: "retraction", title: "Retraction", summary: "Withdraw a request or derived answer", built: true, steps: retractionSteps },
    { slug: "fs-watch-continuing", title: "fs.watch continuing", summary: "Stream filesystem changes over time", built: true, steps: fsWatchContinuingSteps },
  ] },
  { slug: "cross-system", title: "Cross-system", ideas: [
    { slug: "collision-grid", title: "DBSP / DD / SQL / Redux / RxJS collision grid", summary: "Map shared concepts across data systems", built: false },
  ] },
]

export const builtIdeas = glossary.flatMap((group) => group.ideas.filter((idea) => idea.built).map((idea) => ({ group, idea })))

export function findIdea(groupSlug: string, ideaSlug: string) {
  const group = glossary.find((entry) => entry.slug === groupSlug)
  const idea = group?.ideas.find((entry) => entry.slug === ideaSlug)
  return group && idea ? { group, idea } : undefined
}
