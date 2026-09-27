import { describe, expect, it } from "vitest"
import { builtIdeas } from "../lib/0_glossary"
import { deltaSteps } from "./delta/0_steps"
import { zsetSteps } from "./zset/0_steps"

describe("keyed scene elements", () => {
  it("keeps the complete Z-set key timeline visible", () => {
    expect(zsetSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "people",
        ],
        [
          "people",
        ],
        [
          "left",
          "plus",
          "right",
          "equals",
          "sum",
        ],
        [
          "people",
        ],
        [
          "people",
        ],
        [
          "relation",
        ],
        [
          "set",
          "bag",
          "zset",
        ],
      ]
    `)
  })

  it("keeps the complete Delta key timeline visible", () => {
    expect(deltaSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "clock",
          "state",
        ],
        [
          "state",
          "update",
        ],
        [
          "formula",
          "state",
          "clock",
        ],
        [
          "formula",
          "update",
        ],
        [
          "clock",
          "update",
          "state",
        ],
        [
          "clock1",
          "run1",
          "clock2",
          "run2",
          "clock3",
          "run3",
        ],
        [
          "input",
          "map",
          "output",
        ],
      ]
    `)
  })
})

describe("built glossary scenes", () => {
  it("provides a captioned scene for every built idea", () => {
    expect(builtIdeas.map(({ group, idea }) => ({ group: group.slug, idea: idea.slug, captions: idea.steps?.map((step) => step.caption) }))).toMatchInlineSnapshot(`
      [
        {
          "captions": [
            "Start with two rows. Each tuple appears once.",
            "A Z-set stores an integer weight beside every row.",
            "Adding Z-sets means adding the weights of matching rows: Ada has 1 + 2 = 3.",
            "Negative weights record retractions. Bob now contributes minus one.",
            "Equal insertion and retraction cancel. A row at weight zero disappears from the result.",
            "An ordinary relation is the special case where every stored weight is zero or one.",
            "Sets track presence. Bags count copies. Z-sets also represent retractions.",
          ],
          "group": "core-algebra",
          "idea": "z-set",
        },
        {
          "captions": [
            "At time t the current state Sₜ is a Z-set with concrete weights.",
            "An update arrives as a delta. Ada is inserted and Bob is retracted.",
            "Integration adds the delta to the old state: S₁ = S₀ + Δ₁.",
            "Differentiation recovers the change: Δ₁ = S₁ − S₀.",
            "At t=0 the empty state receives Ada and stores one copy.",
            "Across t=1 to t=3 each delta changes the running total: Ada reaches two and Bob is removed.",
            "Operators receive deltas and emit deltas. A map changes row values while preserving their weights.",
          ],
          "group": "core-algebra",
          "idea": "delta",
        },
      ]
    `)
    expect(builtIdeas.every(({ idea }) => idea.steps && idea.steps.length > 0 && idea.steps.every((step) => step.caption.trim().length > 0))).toBe(true)
  })
})
