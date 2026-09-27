import type { Step } from "../../lib/scene/0_types"

const orders = [
  { key: "o7", values: ["7", "ada"], weight: 1 },
  { key: "o8", values: ["8", "bea"], weight: 1 },
] as const
const customers = [
  { key: "ada", values: ["ada", "gold"], weight: 1 },
  { key: "cy", values: ["cy", "silver"], weight: 1 },
] as const

export const joinSteps: readonly Step[] = [
  { caption: "Orders carry a customer name; customers carry a tier.", elements: [{ kind: "zset", key: "orders", x: 180, y: 170, model: { title: "orders(id, name)", rows: orders } }, { kind: "zset", key: "customers", x: 570, y: 170, model: { title: "customers(name, tier)", rows: customers } }], codeStep: 0 },
  { caption: "Join compares the shared name column. Ada matches Ada.", elements: [{ kind: "zset", key: "orders", x: 100, y: 170, model: { title: "orders", rows: orders } }, { kind: "op", key: "join", x: 390, y: 180, label: "join", state: "name = name" }, { kind: "zset", key: "customers", x: 610, y: 170, model: { title: "customers", rows: customers } }], codeStep: 1 },
  { caption: "The matching pair produces order 7 with tier gold.", elements: [{ kind: "zset", key: "result", x: 360, y: 180, model: { title: "joined", rows: [{ key: "7gold", values: ["7", "ada", "gold"], weight: 1 }] } }], codeStep: 2 },
  { caption: "Bea has no customer row, so order 8 produces no output.", elements: [{ kind: "zset", key: "bea", x: 180, y: 190, model: { title: "order", rows: [orders[1]] } }, { kind: "op", key: "join", x: 450, y: 180, label: "join", state: "no bea key" }, { kind: "text", key: "empty", x: 690, y: 225, text: "0 rows", size: 28 }], codeStep: 3 },
  { caption: "Weights multiply. Two copies of an order and three copies of Ada produce weight 6.", elements: [{ kind: "zset", key: "left", x: 120, y: 190, model: { title: "orders", rows: [{ ...orders[0], weight: 2 }] } }, { kind: "op", key: "join", x: 400, y: 180, label: "join", state: "2 × 3" }, { kind: "zset", key: "right", x: 620, y: 190, model: { title: "joined", rows: [{ key: "7gold", values: ["7", "gold"], weight: 6 }] } }], codeStep: 4 },
]
