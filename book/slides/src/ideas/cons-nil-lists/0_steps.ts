import type { Step } from "../../lib/scene/0_types"

const list = (key: string, value: string) => ({ key, values: [value], weight: 1 } as const)

export const consNilListsSteps: readonly Step[] = [
  { caption: "nil/1 produces the empty proper list [].", elements: [{ kind: "op", key: "nil", x: 300, y: 175, label: "nil" }, { kind: "zset", key: "empty", x: 560, y: 155, showWeights: false, model: { title: "tail", rows: [list("empty", "[]")] } }], codeStep: 0 },
  { caption: "cons/3 builds [cy] from the head cy and the empty tail.", elements: [{ kind: "text", key: "head", x: 180, y: 205, text: "cy", size: 30 }, { kind: "op", key: "cons", x: 360, y: 170, label: "cons" }, { kind: "zset", key: "list", x: 650, y: 155, showWeights: false, model: { title: "list", rows: [list("cy", "[cy]")] } }], codeStep: 1 },
  { caption: "A second cons builds [bob, cy] by placing bob before [cy].", elements: [{ kind: "text", key: "head", x: 140, y: 205, text: "bob", size: 30 }, { kind: "zset", key: "tail", x: 285, y: 155, showWeights: false, model: { title: "tail", rows: [list("cy", "[cy]")] } }, { kind: "op", key: "cons", x: 535, y: 170, label: "cons" }, { kind: "zset", key: "list", x: 780, y: 155, showWeights: false, model: { title: "list", rows: [list("both", "[bob, cy]")] } }], codeStep: 2 },
  { caption: "With the list bound, the same cons/3 relation splits it into head bob and tail [cy].", elements: [{ kind: "zset", key: "list", x: 150, y: 155, showWeights: false, model: { title: "list", rows: [list("both", "[bob, cy]")] } }, { kind: "op", key: "cons", x: 450, y: 170, label: "cons" }, { kind: "zset", key: "parts", x: 700, y: 155, showWeights: false, model: { title: "head · tail", rows: [{ key: "parts", values: ["bob", "[cy]"], weight: 1 }] } }], codeStep: 3 },
  { caption: "Splitting [] has no result because an empty list has no head.", elements: [{ kind: "zset", key: "empty", x: 250, y: 155, showWeights: false, model: { title: "list", rows: [list("empty", "[]")] } }, { kind: "op", key: "cons", x: 520, y: 170, label: "cons", state: "no row" }], codeStep: 4 },
]
