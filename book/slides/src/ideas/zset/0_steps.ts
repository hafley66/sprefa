import type { Step } from "../../lib/scene/0_types"

const ada = { key: "ada", values: ["ada", 7], weight: 1 } as const
const bob = { key: "bob", values: ["bob", 3], weight: 1 } as const

export const zsetSteps: readonly Step[] = [
  { caption: "Start with two rows. Each tuple appears once.", elements: [{ kind: "zset", key: "people", x: 330, y: 150, showWeights: false, model: { title: "rows", rows: [ada, bob] } }] },
  { caption: "A Z-set stores an integer weight beside every row.", elements: [{ kind: "zset", key: "people", x: 330, y: 150, model: { title: "Z-set", rows: [ada, bob] } }], codeStep: 0 },
  { caption: "Adding Z-sets means adding the weights of matching rows: Ada has 1 + 2 = 3.", elements: [{ kind: "zset", key: "left", x: 80, y: 145, model: { title: "A", rows: [ada, bob] } }, { kind: "text", key: "plus", x: 350, y: 205, text: "+" }, { kind: "zset", key: "right", x: 420, y: 145, model: { title: "B", rows: [{ ...ada, weight: 2 }] } }, { kind: "text", key: "equals", x: 650, y: 205, text: "=" }, { kind: "zset", key: "sum", x: 710, y: 145, model: { title: "A + B", rows: [{ ...ada, weight: 3 }, bob] } }], codeStep: 1 },
  { caption: "Negative weights record retractions. Bob now contributes minus one.", elements: [{ kind: "zset", key: "people", x: 330, y: 150, model: { title: "Z-set", rows: [ada, { ...bob, weight: -1 }] } }], codeStep: 2 },
  { caption: "Equal insertion and retraction cancel. A row at weight zero disappears from the result.", elements: [{ kind: "zset", key: "people", x: 330, y: 150, model: { title: "1 + (−1) = 0", rows: [{ ...ada, weight: 0 }] } }], codeStep: 3 },
  { caption: "An ordinary relation is the special case where every stored weight is zero or one.", elements: [{ kind: "zset", key: "relation", x: 330, y: 150, model: { title: "relation ⊂ Z-set", rows: [ada, bob] } }], codeStep: 4 },
  { caption: "Sets track presence. Bags count copies. Z-sets also represent retractions.", elements: [{ kind: "zset", key: "set", x: 55, y: 150, showWeights: false, model: { title: "set", rows: [ada, bob] } }, { kind: "zset", key: "bag", x: 350, y: 150, model: { title: "bag", rows: [{ ...ada, weight: 2 }, bob] } }, { kind: "zset", key: "zset", x: 645, y: 150, model: { title: "Z-set", rows: [{ ...ada, weight: 2 }, { ...bob, weight: -1 }] } }], codeStep: 5 },
]
