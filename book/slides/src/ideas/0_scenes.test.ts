import { describe, expect, it } from "vitest"
import { builtIdeas } from "../lib/0_glossary"
import { bilinearOpJoinDeltaRuleSteps } from "./bilinear-op-join-delta-rule/0_steps"
import { deltaSteps } from "./delta/0_steps"
import { distinctThresholdSteps } from "./distinct-threshold/0_steps"
import { integrateDifferentiateSteps } from "./integrate-differentiate/0_steps"
import { linearOpSteps } from "./linear-op/0_steps"
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

  it("keeps the complete integrate / differentiate key timeline visible", () => {
    expect(integrateDifferentiateSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "clock",
          "state",
        ],
        [
          "before",
          "minus",
          "change",
        ],
        [
          "before",
          "plus",
          "change",
          "equals",
          "after",
        ],
        [
          "t0",
          "d0",
          "t1",
          "d1",
          "t2",
          "d2",
        ],
        [
          "s0",
          "s1",
          "s2",
        ],
      ]
    `)
  })

  it("keeps the complete linear op key timeline visible", () => {
    expect(linearOpSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "input",
          "map",
          "output",
        ],
        [
          "a",
          "plus",
          "b",
        ],
        [
          "sum",
          "map",
          "mapped",
        ],
        [
          "la",
          "plus",
          "lb",
          "equals",
        ],
        [
          "change",
          "linear",
          "result",
        ],
      ]
    `)
  })

  it("keeps the complete bilinear op key timeline visible", () => {
    expect(bilinearOpJoinDeltaRuleSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "orders",
          "join",
          "customers",
        ],
        [
          "result",
          "weight",
        ],
        [
          "da",
          "join",
          "out",
        ],
        [
          "a",
          "join",
          "db",
          "out",
        ],
        [
          "da",
          "join",
          "db",
          "equals",
          "out",
        ],
        [
          "formula1",
          "formula2",
          "formula3",
        ],
      ]
    `)
  })

  it("keeps the complete distinct / threshold key timeline visible", () => {
    expect(distinctThresholdSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "input",
        ],
        [
          "input",
          "threshold",
          "output",
        ],
        [
          "bag",
          "distinct",
          "set",
        ],
        [
          "input",
          "distinct",
          "zero",
        ],
        [
          "input",
          "distinct",
          "output",
        ],
        [
          "rule1",
          "rule2",
          "rule3",
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
        {
          "captions": [
            "A state records the accumulated rows at one logical time.",
            "Differentiation compares adjacent states. Bob leaves and Ada gains one copy.",
            "Integration adds that change to S₀. Equal Bob weights cancel and Ada reaches two.",
            "A stream of deltas is integrated by a running sum: +1 then +2 then −1 gives state 2.",
            "Differentiating the integrated states recovers the original changes exactly.",
          ],
          "group": "core-algebra",
          "idea": "integrate-differentiate",
        },
        {
          "captions": [
            "A linear operator transforms each weighted row without consulting stored history.",
            "Split the input into A and B. A contains Ada and B contains Bob.",
            "Applying the operator after addition produces both transformed rows.",
            "Applying the operator to each part and then adding gives the same weighted collection.",
            "Linearity lets the operator consume only ΔA. The output change is L(ΔA), with no old-state lookup.",
          ],
          "group": "core-algebra",
          "idea": "linear-op",
        },
        {
          "captions": [
            "A join has two inputs. Order 7 matches customer Ada through the shared key ada.",
            "The matching pair emits one joined row with weight 1 × 1 = 1.",
            "When only A changes, join ΔA with the old B. A new order for Ada emits one row.",
            "When only B changes, join old A with ΔB. Retracting Ada's tier retracts the joined row.",
            "If both inputs change together, the new rows can also match each other. The ΔA ⋈ ΔB term includes that pair.",
            "The join delta is the sum of three products: changed-left, changed-right, and changed-with-changed.",
          ],
          "group": "core-algebra",
          "idea": "bilinear-op-join-delta-rule",
        },
        {
          "captions": [
            "A Z-set may contain multiple copies, retractions, and zero-weight rows.",
            "Threshold keeps every positive weight as one and maps zero or negative weights to zero.",
            "Distinct over a bag uses the same positive threshold: Ada's three copies become one.",
            "A fourth Ada copy changes weight 3 to 4, but the distinct output stays at one. Its output delta is zero.",
            "Retracting three copies crosses the presence boundary from one to zero. Distinct emits Ada with weight −1.",
            "Threshold is stateful under updates because it must detect crossings of zero.",
          ],
          "group": "core-algebra",
          "idea": "distinct-threshold",
        },
      ]
    `)
    expect(builtIdeas.every(({ idea }) => idea.steps && idea.steps.length > 0 && idea.steps.every((step) => step.caption.trim().length > 0))).toBe(true)
  })
})
