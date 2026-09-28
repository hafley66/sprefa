import type { Step } from "../../lib/scene/0_types"

const ada3 = { key: "ada", values: ["ada"], weight: 3 } as const
const bobNegative = { key: "bob", values: ["bob"], weight: -2 } as const
const cy0 = { key: "cy", values: ["cy"], weight: 0 } as const

export const distinctThresholdSteps: readonly Step[] = [
  { caption: "A Z-set may contain multiple copies, retractions, and zero-weight rows.", elements: [{ kind: "zset", key: "input", x: 370, y: 155, model: { title: "input weights", rows: [ada3, bobNegative, cy0] } }], codeStep: 0 },
  { caption: "Threshold keeps every positive weight as one and maps zero or negative weights to zero.", elements: [{ kind: "zset", key: "input", x: 100, y: 155, model: { title: "input", rows: [ada3, bobNegative, cy0] } }, { kind: "op", key: "threshold", x: 450, y: 190, label: "threshold" }, { kind: "zset", key: "output", x: 720, y: 190, model: { title: "present", rows: [{ ...ada3, weight: 1 }] } }], codeStep: 1 },
  { caption: "Distinct over a bag uses the same positive threshold: Ada's three copies become one.", elements: [{ kind: "zset", key: "bag", x: 210, y: 185, model: { title: "bag", rows: [ada3] } }, { kind: "op", key: "distinct", x: 500, y: 175, label: "distinct" }, { kind: "zset", key: "set", x: 760, y: 185, model: { title: "set", rows: [{ ...ada3, weight: 1 }] } }], codeStep: 2 },
  { caption: "A fourth Ada copy changes weight 3 to 4, but the distinct output stays at one. Its output delta is zero.", elements: [{ kind: "delta", key: "input", x: 140, y: 215, width: 250, model: { title: "Δ input", rows: [{ ...ada3, weight: 1 }] } }, { kind: "op", key: "distinct", x: 480, y: 175, label: "distinct", state: "ada: 3 → 4" }, { kind: "text", key: "zero", x: 760, y: 225, text: "Δ output = 0", size: 26 }], codeStep: 3 },
  { caption: "Retracting three copies crosses the presence boundary from one to zero. Distinct emits Ada with weight −1.", elements: [{ kind: "delta", key: "input", x: 120, y: 215, width: 270, model: { title: "Δ input", rows: [{ ...ada3, weight: -3 }] } }, { kind: "op", key: "distinct", x: 480, y: 175, label: "distinct", state: "ada: 3 → 0" }, { kind: "delta", key: "output", x: 750, y: 215, width: 270, model: { title: "Δ output", rows: [{ ...ada3, weight: -1 }] } }], codeStep: 4 },
  { caption: "Threshold is stateful under updates because it must detect crossings of zero.", elements: [{ kind: "text", key: "rule1", x: 185, y: 170, text: "old 0 → new 1   emits +1", size: 25 }, { kind: "text", key: "rule2", x: 185, y: 235, text: "old 1 → new 1   emits  0", size: 25 }, { kind: "text", key: "rule3", x: 185, y: 300, text: "old 1 → new 0   emits −1", size: 25 }], codeStep: 5 },
]
