import type { Step } from "../../lib/scene/0_types"

const scores = [
  { key: "ada", values: ["ada", "91"], weight: 1 },
  { key: "bea", values: ["bea", "84"], weight: 1 },
  { key: "cy", values: ["cy", "77"], weight: 1 },
] as const

export const topKSteps: readonly Step[] = [
  { caption: "Three scores arrive in arbitrary row order.", elements: [{ kind: "zset", key: "scores", x: 365, y: 150, model: { title: "scores", rows: scores } }], codeStep: 0 },
  { caption: "Top 2 orders by score and retains Ada 91 and Bea 84.", elements: [{ kind: "zset", key: "scores", x: 120, y: 150, model: { title: "scores", rows: scores } }, { kind: "op", key: "top", x: 410, y: 180, label: "top 2", state: "score desc" }, { kind: "zset", key: "out", x: 620, y: 170, model: { title: "leaders", rows: scores.slice(0, 2) } }], codeStep: 1 },
  { caption: "A new score Dan 88 enters the retained prefix.", elements: [{ kind: "delta", key: "in", x: 120, y: 205, width: 250, model: { title: "Δ scores", rows: [{ key: "dan", values: ["dan", "88"], weight: 1 }] } }, { kind: "op", key: "top", x: 400, y: 180, label: "top 2" }, { kind: "zset", key: "out", x: 610, y: 180, model: { title: "new leaders", rows: [scores[0], { key: "dan", values: ["dan", "88"], weight: 1 }] } }], codeStep: 2 },
  { caption: "Bea falls out of the prefix, so the delta retracts Bea and inserts Dan.", elements: [{ kind: "delta", key: "out", x: 360, y: 175, width: 360, model: { title: "Δ leaders", rows: [{ ...scores[1], weight: -1 }, { key: "dan", values: ["dan", "88"], weight: 1 }] } }], codeStep: 3 },
  { caption: "Retracting Ada promotes Bea from the ordered remainder.", elements: [{ kind: "delta", key: "in", x: 120, y: 205, width: 250, model: { title: "Δ scores", rows: [{ ...scores[0], weight: -1 }] } }, { kind: "op", key: "top", x: 400, y: 180, label: "top 2" }, { kind: "delta", key: "out", x: 610, y: 185, width: 290, model: { title: "Δ leaders", rows: [{ ...scores[0], weight: -1 }, scores[1]] } }], codeStep: 4 },
  { caption: "A stable tie key makes the selected prefix deterministic when scores are equal.", elements: [{ kind: "text", key: "order", x: 190, y: 145, text: "ORDER BY score DESC, name ASC", size: 27 }, { kind: "text", key: "tie", x: 190, y: 225, text: "(ada, 90) precedes (bea, 90)", size: 24 }], codeStep: 5 },
]
