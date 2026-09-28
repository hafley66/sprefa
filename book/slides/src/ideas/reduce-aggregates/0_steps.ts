import type { Step } from "../../lib/scene/0_types"

const sales = [
  { key: "ada4", values: ["ada", "4"], weight: 1 },
  { key: "ada6", values: ["ada", "6"], weight: 1 },
  { key: "bea3", values: ["bea", "3"], weight: 1 },
] as const

export const reduceAggregatesSteps: readonly Step[] = [
  { caption: "Sales rows pair each clerk with a concrete amount.", elements: [{ kind: "zset", key: "sales", x: 360, y: 155, model: { title: "sales(clerk, amount)", rows: sales } }], codeStep: 0 },
  { caption: "Reduce groups rows by clerk before applying an aggregate.", elements: [{ kind: "zset", key: "sales", x: 120, y: 155, model: { title: "sales", rows: sales } }, { kind: "op", key: "group", x: 410, y: 180, label: "group", state: "by clerk" }, { kind: "text", key: "ada-group", x: 630, y: 170, text: "ada: [4, 6]", size: 24 }, { kind: "text", key: "bea-group", x: 630, y: 220, text: "bea: [3]", size: 24 }], codeStep: 1 },
  { caption: "Sum emits one value per key: Ada 10 and Bea 3.", elements: [{ kind: "op", key: "sum", x: 230, y: 180, label: "sum" }, { kind: "zset", key: "out", x: 560, y: 170, model: { title: "totals", rows: [{ key: "ada", values: ["ada", "10"], weight: 1 }, { key: "bea", values: ["bea", "3"], weight: 1 }] } }], codeStep: 2 },
  { caption: "A new Ada sale of 2 changes her aggregate from 10 to 12.", elements: [{ kind: "delta", key: "in", x: 130, y: 205, width: 250, model: { title: "Δ sales", rows: [{ key: "ada2", values: ["ada", "2"], weight: 1 }] } }, { kind: "op", key: "sum", x: 410, y: 180, label: "sum", state: "10 → 12" }, { kind: "text", key: "old", x: 620, y: 170, text: "−(ada, 10)", size: 25 }, { kind: "text", key: "new", x: 620, y: 220, text: "+(ada, 12)", size: 25 }], codeStep: 3 },
  { caption: "Retracting Ada's sale of 4 changes the stored total from 12 to 8.", elements: [{ kind: "delta", key: "in", x: 130, y: 205, width: 250, model: { title: "Δ sales", rows: [{ key: "ada4", values: ["ada", "4"], weight: -1 }] } }, { kind: "op", key: "sum", x: 410, y: 180, label: "sum", state: "12 → 8" }, { kind: "text", key: "old", x: 620, y: 170, text: "−(ada, 12)", size: 25 }, { kind: "text", key: "new", x: 620, y: 220, text: "+(ada, 8)", size: 25 }], codeStep: 4 },
  { caption: "Count, minimum, maximum, and sum share the same group-and-replace shape.", elements: [{ kind: "text", key: "ops", x: 180, y: 135, text: "count(ada) = 2", size: 24 }, { kind: "text", key: "min", x: 180, y: 200, text: "min(ada) = 4", size: 24 }, { kind: "text", key: "max", x: 520, y: 135, text: "max(ada) = 6", size: 24 }, { kind: "text", key: "sum", x: 520, y: 200, text: "sum(ada) = 10", size: 24 }], codeStep: 5 },
]
