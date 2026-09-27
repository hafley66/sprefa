import type { Step } from "../../lib/scene/0_types"

const text = (key: string, left: string, right: string, whole: string) => ({ key, values: [left, right, whole], weight: 1 } as const)

export const strConsSteps: readonly Step[] = [
  { caption: "str.nil/1 produces the empty string.", elements: [{ kind: "op", key: "nil", x: 310, y: 175, label: "str.nil" }, { kind: "text", key: "empty", x: 590, y: 210, text: "\"\"", size: 34 }], codeStep: 0 },
  { caption: "With both parts bound, str.cons/3 concatenates them.", elements: [{ kind: "zset", key: "parts", x: 190, y: 155, showWeights: false, model: { title: "head · tail · text", rows: [text("greet", "\"h\"", "\"ello\"", "\"hello\"")] } }], codeStep: 1 },
  { caption: "The head may contain more than one character when constructing text.", elements: [{ kind: "zset", key: "parts", x: 190, y: 155, showWeights: false, model: { title: "head · tail · text", rows: [text("greet", "\"hi \"", "\"Ada\"", "\"hi Ada\"")] } }], codeStep: 2 },
  { caption: "With only the whole string bound, str.cons splits its first Unicode character from the rest.", elements: [{ kind: "zset", key: "split", x: 195, y: 155, showWeights: false, model: { title: "head · tail · text", rows: [text("split", "\"é\"", "\"lan\"", "\"élan\"")] } }], codeStep: 3 },
  { caption: "The empty string has no first character, so split mode yields no row.", elements: [{ kind: "text", key: "empty", x: 230, y: 210, text: "\"\"", size: 34 }, { kind: "op", key: "cons", x: 440, y: 175, label: "str.cons", state: "no row" }], codeStep: 4 },
]
