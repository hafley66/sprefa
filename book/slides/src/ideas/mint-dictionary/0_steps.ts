import type { Step } from "../../lib/scene/0_types"

const entry = (key: string, term: string, id: string) => ({ key, values: [term, id], weight: 1 } as const)

export const mintDictionarySteps: readonly Step[] = [
  { caption: "The evaluator stores each distinct term once and gives it a compact TermId.", elements: [{ kind: "zset", key: "terms", x: 300, y: 150, showWeights: false, model: { title: "term dictionary", rows: [entry("ada", "atom(ada)", "TermId 0"), entry("seven", "int(7)", "TermId 1")] } }], codeStep: 0 },
  { caption: "Looking up atom(ada) again returns TermId 0 instead of adding another term.", elements: [{ kind: "text", key: "lookup", x: 250, y: 125, text: "intern atom(ada)", size: 28 }, { kind: "op", key: "intern", x: 525, y: 90, label: "dictionary" }, { kind: "text", key: "result", x: 760, y: 125, text: "TermId 0", size: 28 }], codeStep: 1 },
  { caption: "A new compound term receives the next identity and refers to its arguments by TermId.", elements: [{ kind: "zset", key: "terms", x: 260, y: 135, showWeights: false, model: { title: "term dictionary", rows: [entry("ada", "ada", "0"), entry("seven", "7", "1"), entry("pair", "pair(0,1)", "2")] } }], codeStep: 2 },
  { caption: "Rows carry TermIds, so equality of two cells is integer identity equality.", elements: [{ kind: "zset", key: "rows", x: 245, y: 145, showWeights: false, model: { title: "row cells", rows: [entry("r1", "person", "0"), entry("r2", "owner", "0")] } }, { kind: "text", key: "equal", x: 700, y: 210, text: "0 = 0", size: 34 }], codeStep: 3 },
]
