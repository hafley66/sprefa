import type { Step } from "../../lib/scene/0_types"

const item = (key: string, value: string, rank: string) => ({ key, values: [value, rank], weight: 1 } as const)

export const termLtSteps: readonly Step[] = [
  { caption: "any.lt compares values from different runtime shapes with one standard term order.", elements: [{ kind: "zset", key: "items", x: 280, y: 125, showWeights: false, model: { title: "value · kind", rows: [item("float", "1.5", "number"), item("int", "3", "number"), item("bool", "true", "bool"), item("str", "\"apple\"", "string")] } }], codeStep: 0 },
  { caption: "Numbers come first and compare by numeric value; an integer precedes an equal float.", elements: [{ kind: "text", key: "order", x: 230, y: 210, text: "1  <  1.0  <  1.5  <  3", size: 32 }], codeStep: 1 },
  { caption: "The kind order continues through booleans, strings, [], atoms and compounds.", elements: [{ kind: "text", key: "order-a", x: 250, y: 170, text: "numbers < false < true < strings", size: 28 }, { kind: "text", key: "order-b", x: 310, y: 235, text: "strings < [] < atoms < compounds", size: 28 }], codeStep: 2 },
  { caption: "For the concrete items, any.lt holds for 1.5 < 3 < true < \"apple\".", elements: [{ kind: "zset", key: "sorted", x: 245, y: 130, showWeights: false, model: { title: "standard order", rows: [item("a", "1.5", "1"), item("b", "3", "2"), item("c", "true", "3"), item("d", "\"apple\"", "4")] } }], codeStep: 3 },
  { caption: "any.lt is a predicate: it emits the pair when left precedes right and emits no row otherwise.", elements: [{ kind: "op", key: "yes", x: 230, y: 150, label: "any.lt", state: "1.5 < true" }, { kind: "op", key: "no", x: 620, y: 150, label: "any.lt", state: "true < 3 · no row" }], codeStep: 4 },
]
