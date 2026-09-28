import { describe, expect, it } from "vitest"
import { builtIdeas } from "../lib/0_glossary"
import { antijoinSteps } from "./antijoin/0_steps"
import { bilinearOpJoinDeltaRuleSteps } from "./bilinear-op-join-delta-rule/0_steps"
import { clockCheckerSteps } from "./clock-checker/0_steps"
import { collisionGridSteps } from "./collision-grid/0_steps"
import { consNilListsSteps } from "./cons-nil-lists/0_steps"
import { deltaSteps } from "./delta/0_steps"
import { delayZ1Steps } from "./delay-z-1/0_steps"
import { distinctThresholdSteps } from "./distinct-threshold/0_steps"
import { frontierSteps } from "./frontier/0_steps"
import { fsWatchContinuingSteps } from "./fs-watch-continuing/0_steps"
import { integrateDifferentiateSteps } from "./integrate-differentiate/0_steps"
import { internSteps } from "./intern/0_steps"
import { joinSteps } from "./join/0_steps"
import { letrecFixpointSteps } from "./letrec-fixpoint/0_steps"
import { linearOpSteps } from "./linear-op/0_steps"
import { magicSetsDemandSteps } from "./magic-sets-demand/0_steps"
import { mintDictionarySteps } from "./mint-dictionary/0_steps"
import { nestedTimeSteps } from "./nested-time/0_steps"
import { reduceAggregatesSteps } from "./reduce-aggregates/0_steps"
import { requestAnswerEffectsSteps } from "./request-answer-effects/0_steps"
import { retractionSteps } from "./retraction/0_steps"
import { strataSteps } from "./strata/0_steps"
import { strConsSteps } from "./str-cons/0_steps"
import { semiNaiveSteps } from "./semi-naive/0_steps"
import { termLtSteps } from "./term-lt/0_steps"
import { topKSteps } from "./top-k/0_steps"
import { unionNegateSteps } from "./union-negate/0_steps"
import { windowSteps } from "./window/0_steps"
import { zsetSteps } from "./zset/0_steps"
import { dredSteps } from "./dred/0_steps"

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

  it("keeps the complete join key timeline visible", () => {
    expect(joinSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "orders",
          "customers",
        ],
        [
          "orders",
          "join",
          "customers",
        ],
        [
          "result",
        ],
        [
          "bea",
          "join",
          "empty",
        ],
        [
          "left",
          "join",
          "right",
        ],
      ]
    `)
  })

  it("keeps the complete antijoin key timeline visible", () => {
    expect(antijoinSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "tasks",
          "done",
        ],
        [
          "tasks",
          "anti",
          "out",
        ],
        [
          "a",
          "blocked",
          "none",
        ],
        [
          "done",
          "anti",
          "pending",
        ],
        [
          "state",
          "effect",
        ],
      ]
    `)
  })

  it("keeps the complete reduce / aggregates key timeline visible", () => {
    expect(reduceAggregatesSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "sales",
        ],
        [
          "sales",
          "group",
          "ada-group",
          "bea-group",
        ],
        [
          "sum",
          "out",
        ],
        [
          "in",
          "sum",
          "old",
          "new",
        ],
        [
          "in",
          "sum",
          "old",
          "new",
        ],
        [
          "ops",
          "min",
          "max",
          "sum",
        ],
      ]
    `)
  })

  it("keeps the complete top-k key timeline visible", () => {
    expect(topKSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "scores",
        ],
        [
          "scores",
          "top",
          "out",
        ],
        [
          "in",
          "top",
          "out",
        ],
        [
          "out",
        ],
        [
          "in",
          "top",
          "out",
        ],
        [
          "order",
          "tie",
        ],
      ]
    `)
  })

  it("keeps the complete window key timeline visible", () => {
    expect(windowSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "events",
        ],
        [
          "now",
          "range",
        ],
        [
          "inside",
        ],
        [
          "now",
          "range",
        ],
        [
          "window",
          "out",
        ],
        [
          "in",
          "window",
          "out",
        ],
      ]
    `)
  })

  it("keeps the complete union / negate key timeline visible", () => {
    expect(unionNegateSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "a",
          "union",
          "b",
        ],
        [
          "out",
        ],
        [
          "a",
          "negate",
          "out",
        ],
        [
          "a",
          "minus",
          "out",
        ],
        [
          "math",
          "result",
        ],
        [
          "in",
          "negate",
          "out",
        ],
      ]
    `)
  })

  it("keeps the complete semi-naive key timeline visible", () => {
    expect(semiNaiveSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "edges",
        ],
        [
          "round",
          "frontier",
        ],
        [
          "in",
          "join",
          "out",
        ],
        [
          "round",
          "in",
          "join",
          "out",
        ],
        [
          "round",
          "empty",
          "fixed",
        ],
        [
          "formula",
          "flow",
        ],
      ]
    `)
  })

  it("keeps the complete DRed key timeline visible", () => {
    expect(dredSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "edges",
          "reach",
        ],
        [
          "delete",
          "support",
        ],
        [
          "overdelete",
          "maybe",
        ],
        [
          "survivors",
          "join",
          "restored",
        ],
        [
          "delete",
          "overdelete",
          "gone",
        ],
        [
          "phase1",
          "phase2",
        ],
      ]
    `)
  })

  it("keeps the complete magic sets / demand key timeline visible", () => {
    expect(magicSetsDemandSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "graph",
        ],
        [
          "query",
          "demand",
        ],
        [
          "magic",
          "guard",
        ],
        [
          "found",
          "next",
        ],
        [
          "wanted",
          "expand",
          "answer",
        ],
        [
          "demand",
          "arrow",
          "answer",
        ],
        [
          "before",
          "arrow1",
          "magic",
          "arrow2",
          "work",
        ],
      ]
    `)
  })

  it("keeps the complete mint dictionary key timeline visible", () => {
    expect(mintDictionarySteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "terms",
        ],
        [
          "lookup",
          "intern",
          "result",
        ],
        [
          "terms",
        ],
        [
          "rows",
          "equal",
        ],
      ]
    `)
  })

  it("keeps the complete cons / nil lists key timeline visible", () => {
    expect(consNilListsSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "nil",
          "empty",
        ],
        [
          "head",
          "cons",
          "list",
        ],
        [
          "head",
          "tail",
          "cons",
          "list",
        ],
        [
          "list",
          "cons",
          "parts",
        ],
        [
          "empty",
          "cons",
        ],
      ]
    `)
  })

  it("keeps the complete intern key timeline visible", () => {
    expect(internSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "call",
        ],
        [
          "intern",
          "identity",
        ],
        [
          "calls",
        ],
        [
          "calls",
        ],
        [
          "known",
          "intern",
          "result",
        ],
      ]
    `)
  })

  it("keeps the complete str.cons key timeline visible", () => {
    expect(strConsSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "nil",
          "empty",
        ],
        [
          "parts",
        ],
        [
          "parts",
        ],
        [
          "split",
        ],
        [
          "empty",
          "cons",
        ],
      ]
    `)
  })

  it("keeps the complete TermLt key timeline visible", () => {
    expect(termLtSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "items",
        ],
        [
          "order",
        ],
        [
          "order-a",
          "order-b",
        ],
        [
          "sorted",
        ],
        [
          "yes",
          "no",
        ],
      ]
    `)
  })

  it("keeps the complete request / answer effects key timeline visible", () => {
    expect(requestAnswerEffectsSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "effect",
          "input",
          "output",
        ],
        [
          "requests",
        ],
        [
          "requests",
          "host",
          "disk",
        ],
        [
          "host",
          "answers",
        ],
        [
          "answers",
          "derive",
          "tasks",
        ],
        [
          "cycle",
          "again",
        ],
      ]
    `)
  })

  it("keeps the complete retraction key timeline visible", () => {
    expect(retractionSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "request",
          "watcher",
        ],
        [
          "answer",
          "derived",
        ],
        [
          "delete",
        ],
        [
          "reconcile",
          "watcher",
        ],
        [
          "answer",
          "gone",
        ],
        [
          "chain",
          "result",
        ],
      ]
    `)
  })

  it("keeps the complete fs.watch continuing key timeline visible", () => {
    expect(fsWatchContinuingSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "request",
        ],
        [
          "request",
          "watch",
        ],
        [
          "time",
          "body",
        ],
        [
          "time",
          "body",
        ],
        [
          "time",
          "body",
        ],
        [
          "request",
          "cancel",
        ],
      ]
    `)
  })

  it("keeps the complete collision grid key timeline visible", () => {
    expect(collisionGridSteps.map((step) => step.elements.map((element) => element.key))).toMatchInlineSnapshot(`
      [
        [
          "before",
          "arrow",
          "after",
        ],
        [
          "sql",
          "redux",
          "rxjs",
          "emissions",
        ],
        [
          "dbsp",
          "dd",
          "dd-record",
        ],
        [
          "sql",
          "sql-value",
          "redux",
          "redux-value",
          "rxjs",
          "rxjs-value",
          "dbsp",
          "dbsp-value",
          "dd",
          "dd-value",
        ],
        [
          "relational",
          "relational-value",
          "redux",
          "redux-value",
          "rxjs",
          "rxjs-value",
        ],
        [
          "sql",
          "sql-value",
          "redux",
          "redux-value",
          "rxjs",
          "rxjs-value",
          "weighted",
          "weighted-value",
        ],
        [
          "head",
          "sql",
          "dbsp-dd",
          "redux",
          "rxjs",
          "change",
          "sql-change",
          "dbsp-change",
          "redux-change",
          "rxjs-change",
          "time",
          "sql-time",
          "dbsp-time",
          "redux-time",
          "rxjs-time",
          "remove",
          "sql-remove",
          "dbsp-remove",
          "redux-remove",
          "rxjs-remove",
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
        {
          "captions": [
            "Orders carry a customer name; customers carry a tier.",
            "Join compares the shared name column. Ada matches Ada.",
            "The matching pair produces order 7 with tier gold.",
            "Bea has no customer row, so order 8 produces no output.",
            "Weights multiply. Two copies of an order and three copies of Ada produce weight 6.",
          ],
          "group": "operators",
          "idea": "join",
        },
        {
          "captions": [
            "Tasks a and b are candidates. The done set contains a.",
            "Antijoin keeps left rows whose key is absent on the right.",
            "Task a is blocked because its key appears in done.",
            "Retracting done(a) makes a absent and inserts task a into pending.",
            "Antijoin must remember right-side presence to react when a key crosses zero.",
          ],
          "group": "operators",
          "idea": "antijoin",
        },
        {
          "captions": [
            "Sales rows pair each clerk with a concrete amount.",
            "Reduce groups rows by clerk before applying an aggregate.",
            "Sum emits one value per key: Ada 10 and Bea 3.",
            "A new Ada sale of 2 changes her aggregate from 10 to 12.",
            "Retracting Ada's sale of 4 changes the stored total from 12 to 8.",
            "Count, minimum, maximum, and sum share the same group-and-replace shape.",
          ],
          "group": "operators",
          "idea": "reduce-aggregates",
        },
        {
          "captions": [
            "Three scores arrive in arbitrary row order.",
            "Top 2 orders by score and retains Ada 91 and Bea 84.",
            "A new score Dan 88 enters the retained prefix.",
            "Bea falls out of the prefix, so the delta retracts Bea and inserts Dan.",
            "Retracting Ada promotes Bea from the ordered remainder.",
            "A stable tie key makes the selected prefix deterministic when scores are equal.",
          ],
          "group": "operators",
          "idea": "top-k",
        },
        {
          "captions": [
            "Events carry timestamps: a at 10, b at 12, and c at 16.",
            "At time 16, a five-second window covers times 12 through 16.",
            "Events b and c are inside the interval. Event a at 10 is outside.",
            "The clock advances to 18, moving the interval to times 14 through 18.",
            "Event b expires at the left boundary and is retracted from the window.",
            "A late event at time 17 is still inside the current interval and enters with weight +1.",
          ],
          "group": "operators",
          "idea": "window",
        },
        {
          "captions": [
            "Union adds weights for equal rows from both inputs.",
            "Ada occurs once in each input, so union gives Ada weight 2. Bea stays at 1.",
            "Negate flips every weight while keeping each row unchanged.",
            "Union with a negated collection computes weighted difference A − B.",
            "Equal positive and negative weights cancel to zero and disappear from the materialized rows.",
            "Union and negate are linear: input deltas pass through by addition and sign reversal.",
          ],
          "group": "operators",
          "idea": "union-negate",
        },
        {
          "captions": [
            "The graph has a chain Ada → Bob → Cy → Di.",
            "Round 0 seeds the frontier with the three direct edges.",
            "Round 1 joins only Δreach₀ with edge. It discovers Ada → Cy and Bob → Di.",
            "Round 2 feeds only those two new paths back. Ada → Di is the sole new result.",
            "Round 3 emits an empty frontier. Six accumulated paths form the fixpoint.",
            "Each round reads the accumulated relation for membership but expands only the frontier.",
          ],
          "group": "recursion",
          "idea": "semi-naive",
        },
        {
          "captions": [
            "Two routes support Ada → Cy: a direct edge and the path through Bob.",
            "Deleting Ada → Cy removes one derivation but leaves the route through Bob.",
            "DRed first overdeletes every recursive fact that may depend on the removed edge.",
            "Rederivation checks the surviving graph. Ada → Bob and Bob → Cy prove Ada → Cy again.",
            "Deleting Bob → Cy removes the last remaining support for Ada → Cy.",
            "DRed separates possible damage from confirmed damage: delete broadly then restore surviving proofs.",
          ],
          "group": "recursion",
          "idea": "dred",
        },
        {
          "captions": [
            "The graph contains two disconnected regions: Ada's chain and Uma's chain.",
            "The query asks only which nodes Ada can reach.",
            "A magic predicate carries the bound source Ada into the recursive computation.",
            "The first demanded edge yields Bob and creates demand to continue from Bob.",
            "Demand from Bob yields Cy. No demand reaches Uma so Uma's region is never expanded.",
            "A later query for Uma adds a second demand seed and evaluates that region independently.",
            "Magic sets rewrite a bound query into demand facts and guarded recursive rules.",
          ],
          "group": "recursion",
          "idea": "magic-sets-demand",
        },
        {
          "captions": [
            "The evaluator stores each distinct term once and gives it a compact TermId.",
            "Looking up atom(ada) again returns TermId 0 instead of adding another term.",
            "A new compound term receives the next identity and refers to its arguments by TermId.",
            "Rows carry TermIds, so equality of two cells is integer identity equality.",
          ],
          "group": "kernel",
          "idea": "mint-dictionary",
        },
        {
          "captions": [
            "nil/1 produces the empty proper list [].",
            "cons/3 builds [cy] from the head cy and the empty tail.",
            "A second cons builds [bob, cy] by placing bob before [cy].",
            "With the list bound, the same cons/3 relation splits it into head bob and tail [cy].",
            "Splitting [] has no result because an empty list has no head.",
          ],
          "group": "kernel",
          "idea": "cons-nil-lists",
        },
        {
          "captions": [
            "intern/3 takes a constructor and a proper argument list.",
            "The result is the application identity for Option applied to [str].",
            "Repeating the same constructor and arguments returns node 42.",
            "Changing the argument list produces another application identity.",
            "Constructor and arguments must be bound before intern can construct the result.",
          ],
          "group": "kernel",
          "idea": "intern",
        },
        {
          "captions": [
            "str.nil/1 produces the empty string.",
            "With both parts bound, str.cons/3 concatenates them.",
            "The head may contain more than one character when constructing text.",
            "With only the whole string bound, str.cons splits its first Unicode character from the rest.",
            "The empty string has no first character, so split mode yields no row.",
          ],
          "group": "kernel",
          "idea": "str-cons",
        },
        {
          "captions": [
            "any.lt compares values from different runtime shapes with one standard term order.",
            "Numbers come first and compare by numeric value; an integer precedes an equal float.",
            "The kind order continues through booleans, strings, [], atoms and compounds.",
            "For the concrete items, any.lt holds for 1.5 < 3 < true < "apple".",
            "any.lt is a predicate: it emits the pair when left precedes right and emits no row otherwise.",
          ],
          "group": "kernel",
          "idea": "term-lt",
        },
        {
          "captions": [
            "An effect type connects an input product to an output sum.",
            "A program derives one request row for todo.json.",
            "The host reads the request and performs the filesystem operation.",
            "The host inserts an answer row keyed to request req-7.",
            "Evaluation resumes with the answer row and derives tasks = 3.",
            "Compiler and runtime use the same request, answer, re-evaluate contract.",
          ],
          "group": "host",
          "idea": "request-answer-effects",
        },
        {
          "captions": [
            "A live fs.watch application keeps a watch active for app.ts.",
            "The current UTF-8 body is the answer row for that path.",
            "When the program withdraws the application its weight changes by −1.",
            "The host sees no remaining application for the file and cancels its watcher.",
            "Answers supported by the withdrawn request retract as well.",
            "Retraction propagates through the same weighted relations as insertion.",
          ],
          "group": "host",
          "idea": "retraction",
        },
        {
          "captions": [
            "One fs.watch application asks for the current UTF-8 body of app.ts.",
            "The host watches the file's parent directory so replace-on-save updates are observed.",
            "The existing file yields its current body v1.",
            "When the body changes to v2 the old body retracts and the new body inserts.",
            "Deleting app.ts retracts v2 and leaves no current body row.",
            "The application keeps listening after deletion. Retracting it cancels the continuing watch.",
          ],
          "group": "host",
          "idea": "fs-watch-continuing",
        },
        {
          "captions": [
            "One concrete update anchors the comparison: task 7 changes from open to done.",
            "State names different stored values: a SQL table and Redux store hold the current task, while an RxJS stream emits values over time.",
            "DBSP and Differential Dataflow encode the update as weighted changes. Differential Dataflow also attaches a logical timestamp.",
            "The word time selects different coordinates: commit order, dispatch order, scheduler delivery, or logical progress.",
            "Join also collides. SQL, DBSP, and Differential Dataflow match keyed collections. Redux selectors and RxJS combineLatest have different update contracts.",
            "Retraction is explicit in weighted systems. SQL deletes a row. Redux replaces state. RxJS needs a stateful operator or an explicit domain event to remove prior data.",
            "Translate by operation and contract: current value, change record, time coordinate, keyed match, and removal behavior.",
          ],
          "group": "cross-system",
          "idea": "collision-grid",
        },
      ]
    `)
    expect(builtIdeas.every(({ idea }) => idea.steps && idea.steps.length > 0 && idea.steps.every((step) => step.caption.trim().length > 0))).toBe(true)
  })
})
