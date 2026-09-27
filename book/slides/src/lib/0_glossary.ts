import { deltaSteps } from "../ideas/delta/0_steps"
import { zsetSteps } from "../ideas/zset/0_steps"
import type { Step } from "./scene/0_types"

export type GlossaryIdea = { slug: string; title: string; summary: string; built: boolean; steps?: readonly Step[] }
export type GlossaryGroup = { slug: string; title: string; ideas: readonly GlossaryIdea[] }

export const glossary: readonly GlossaryGroup[] = [
  { slug: "core-algebra", title: "Core algebra", ideas: [
    { slug: "z-set", title: "Z-set", summary: "Rows carrying integer weights", built: true, steps: zsetSteps },
    { slug: "delta", title: "Delta", summary: "A change between two states", built: true, steps: deltaSteps },
  ] },
  { slug: "time", title: "Time", ideas: [{ slug: "round-counter", title: "Round counter", summary: "Progress inside an update time", built: false }, { slug: "window-frame", title: "Window frame", summary: "The time range visible to an operator", built: false }] },
  { slug: "operators", title: "Operators", ideas: [{ slug: "join-index", title: "Join index", summary: "Keys arranged for matching", built: false }, { slug: "arrangement", title: "Arrangement", summary: "Indexed reusable trace state", built: false }, { slug: "reduce-accumulator", title: "Reduce accumulator", summary: "State behind an incremental reduction", built: false }] },
  { slug: "recursion", title: "Recursion", ideas: [{ slug: "fixed-point", title: "Fixed point", summary: "Rounds until no delta remains", built: false }] },
  { slug: "kernel", title: "Kernel", ideas: [{ slug: "term-tree-node", title: "Term tree node", summary: "One node in a structured term", built: false }] },
  { slug: "host", title: "Host", ideas: [{ slug: "dictionary-entry", title: "Dictionary entry", summary: "A host key and value row", built: false }] },
  { slug: "cross-system", title: "Cross-system", ideas: [{ slug: "request-answer-pair", title: "Request/answer pair", summary: "An effect request and its answer", built: false }] },
]

export const builtIdeas = glossary.flatMap((group) => group.ideas.filter((idea) => idea.built).map((idea) => ({ group, idea })))

export function findIdea(groupSlug: string, ideaSlug: string) {
  const group = glossary.find((entry) => entry.slug === groupSlug)
  const idea = group?.ideas.find((entry) => entry.slug === ideaSlug)
  return group && idea ? { group, idea } : undefined
}
