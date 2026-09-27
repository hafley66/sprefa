import { describe, expect, it } from "vitest"
import { builtIdeas } from "../lib/0_glossary"
import { bilinearOpJoinDeltaRuleSteps } from "./bilinear-op-join-delta-rule/0_steps"
import { clockCheckerSteps } from "./clock-checker/0_steps"
import { deltaSteps } from "./delta/0_steps"
import { delayZ1Steps } from "./delay-z-1/0_steps"
import { distinctThresholdSteps } from "./distinct-threshold/0_steps"
import { frontierSteps } from "./frontier/0_steps"
import { integrateDifferentiateSteps } from "./integrate-differentiate/0_steps"
import { letrecFixpointSteps } from "./letrec-fixpoint/0_steps"
import { linearOpSteps } from "./linear-op/0_steps"
import { nestedTimeSteps } from "./nested-time/0_steps"
import { strataSteps } from "./strata/0_steps"
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

  it("keeps the complete frontier key timeline visible", () => {
    expect(frontierSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "w1",
          "w2",
          "labels",
        ],
        [
          "frontier",
          "w1",
          "w2",
        ],
        [
          "done",
          "status",
          "boundary",
          "open",
        ],
        [
          "w1",
          "w2",
          "frontier",
        ],
        [
          "frontier",
          "release",
        ],
      ]
    `)
  })

  it("keeps the complete strata key timeline visible", () => {
    expect(strataSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "people",
        ],
        [
          "people",
          "filter",
          "active",
        ],
        [
          "banned",
          "not",
          "allowed",
        ],
        [
          "s0",
          "facts",
          "arrow",
          "s1",
          "allowed",
        ],
        [
          "p",
          "cycle",
          "q",
        ],
      ]
    `)
  })

  it("keeps the complete LetRec fixpoint key timeline visible", () => {
    expect(letrecFixpointSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "edge",
        ],
        [
          "round",
          "reach",
        ],
        [
          "round",
          "join",
          "new",
        ],
        [
          "round",
          "new",
        ],
        [
          "fixed",
          "reach",
        ],
      ]
    `)
  })

  it("keeps the complete nested time key timeline visible", () => {
    expect(nestedTimeSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "outer",
          "edge",
        ],
        [
          "pair",
          "clock",
        ],
        [
          "clock",
          "join",
          "path",
        ],
        [
          "clock",
          "empty",
        ],
        [
          "clock",
          "edge",
        ],
        [
          "t4",
          "t5",
        ],
      ]
    `)
  })

  it("keeps the complete delay z⁻¹ key timeline visible", () => {
    expect(delayZ1Steps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "t0",
          "v0",
          "t1",
          "v1",
        ],
        [
          "input",
          "delay",
          "output",
        ],
        [
          "clock",
          "delay",
        ],
        [
          "clock",
          "delay",
        ],
        [
          "clock",
          "delay",
        ],
        [
          "formula",
          "values",
        ],
      ]
    `)
  })

  it("keeps the complete clock checker key timeline visible", () => {
    expect(clockCheckerSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "clock",
          "map",
          "value",
        ],
        [
          "input",
          "arrow",
          "mapped",
        ],
        [
          "total",
          "error",
        ],
        [
          "previous",
          "delay",
          "current",
        ],
        [
          "before",
          "arrow",
          "after",
          "ok",
        ],
        [
          "check",
          "result",
          "hint",
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
        {
          "captions": [
            "Workers have reported progress through times 3 and 5.",
            "The frontier is the earliest time that may still receive data: min(3, 5) = 3.",
            "Times before 3 are complete. No later message may change their answers.",
            "Worker A advances to 6. Worker B at 5 now holds the frontier.",
            "When the frontier passes 5, a result for time 5 can be released as final.",
          ],
          "group": "time",
          "idea": "frontier",
        },
        {
          "captions": [
            "Base facts enter stratum 0: Ada and Bob are people.",
            "A positive dependency stays in the same stratum: active reads people.",
            "Negation must read a settled lower stratum. banned is completed before allowed starts.",
            "The dependency graph assigns allowed to stratum 1 because it negates banned.",
            "A negative cycle has no evaluation order: p needs not q while q needs not p.",
          ],
          "group": "time",
          "idea": "strata",
        },
        {
          "captions": [
            "The graph starts with edges Ada → Bob and Bob → Cy.",
            "LetRec seeds reach with the direct edges in round 0.",
            "Round 1 joins reach with edge and derives Ada → Cy.",
            "Round 2 produces no new rows.",
            "The empty delta marks the fixpoint. reach contains all three paths.",
          ],
          "group": "time",
          "idea": "letrec-fixpoint",
        },
        {
          "captions": [
            "An outer time records the input update. At t=4 the edge Ada → Bob arrives.",
            "Recursive work adds an inner round: logical time is the pair (outer, round).",
            "Round (4, 1) derives Ada → Cy from the new edge and an existing Bob → Cy path.",
            "Round (4, 2) is empty, so the inner iteration for outer time 4 is complete.",
            "The next input advances outer time to 5 and starts its own round 0.",
            "Nested time keeps iteration rounds ordered inside each input update.",
          ],
          "group": "time",
          "idea": "nested-time",
        },
        {
          "captions": [
            "The input signal has concrete values: 8 at t=0, then 13 at t=1.",
            "Delay z⁻¹ shifts each value forward by one logical time.",
            "At t=0 there is no previous value, so the delay emits the initial value 0.",
            "At t=1 the delayed output is the prior input 8.",
            "At t=2 the delayed output is 13. The feedback edge now crosses time.",
            "A delayed feedback loop computes running state: s(t) = input(t) + s(t−1).",
          ],
          "group": "time",
          "idea": "delay-z-1",
        },
        {
          "captions": [
            "In the planned clock model, input and mapped both live at the current time t.",
            "An illustrative checker accepts this dependency: mapped(t) reads input(t).",
            "The proposed check rejects recursive same-time dependencies because they make no temporal progress.",
            "In that model, adding z⁻¹ makes the recursive read come from t−1.",
            "The proposed check accepts the delayed cycle because each trip advances logical time.",
            "This planned pre-evaluation check would report the dependency that failed.",
          ],
          "group": "time",
          "idea": "clock-checker",
        },
      ]
    `)
    expect(builtIdeas.every(({ idea }) => idea.steps && idea.steps.length > 0 && idea.steps.every((step) => step.caption.trim().length > 0))).toBe(true)
  })
})
