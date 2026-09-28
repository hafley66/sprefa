import type { Step } from "../../lib/scene/0_types"

const ada = { key: "ada", values: ["ada"], weight: 1 } as const
const bea = { key: "bea", values: ["bea"], weight: 1 } as const

export const unionNegateSteps: readonly Step[] = [
  { caption: "Union adds weights for equal rows from both inputs.", elements: [{ kind: "zset", key: "a", x: 120, y: 180, model: { title: "A", rows: [ada, bea] } }, { kind: "op", key: "union", x: 390, y: 180, label: "union", state: "add weights" }, { kind: "zset", key: "b", x: 610, y: 190, model: { title: "B", rows: [ada] } }], codeStep: 0 },
  { caption: "Ada occurs once in each input, so union gives Ada weight 2. Bea stays at 1.", elements: [{ kind: "zset", key: "out", x: 370, y: 170, model: { title: "A ∪ B", rows: [{ ...ada, weight: 2 }, bea] } }], codeStep: 1 },
  { caption: "Negate flips every weight while keeping each row unchanged.", elements: [{ kind: "zset", key: "a", x: 150, y: 180, model: { title: "A", rows: [ada, bea] } }, { kind: "op", key: "negate", x: 410, y: 180, label: "negate", state: "w ↦ −w" }, { kind: "zset", key: "out", x: 620, y: 180, model: { title: "−A", rows: [{ ...ada, weight: -1 }, { ...bea, weight: -1 }] } }], codeStep: 2 },
  { caption: "Union with a negated collection computes weighted difference A − B.", elements: [{ kind: "zset", key: "a", x: 120, y: 185, model: { title: "A", rows: [ada, bea] } }, { kind: "op", key: "minus", x: 390, y: 180, label: "union", state: "with −B" }, { kind: "zset", key: "out", x: 610, y: 195, model: { title: "A − B", rows: [bea] } }], codeStep: 3 },
  { caption: "Equal positive and negative weights cancel to zero and disappear from the materialized rows.", elements: [{ kind: "text", key: "math", x: 180, y: 160, text: "ada: +1 + (−1) = 0", size: 29 }, { kind: "text", key: "result", x: 180, y: 235, text: "A − B = {bea × +1}", size: 27 }], codeStep: 4 },
  { caption: "Union and negate are linear: input deltas pass through by addition and sign reversal.", elements: [{ kind: "delta", key: "in", x: 130, y: 205, width: 240, model: { title: "ΔA", rows: [{ ...ada, weight: 2 }] } }, { kind: "op", key: "negate", x: 410, y: 180, label: "negate" }, { kind: "delta", key: "out", x: 610, y: 205, width: 250, model: { title: "Δ(−A)", rows: [{ ...ada, weight: -2 }] } }], codeStep: 5 },
]
