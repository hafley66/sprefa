import type { Step } from "../../lib/scene/0_types"

const application = (key: string, ctor: string, args: string, result: string) => ({ key, values: [ctor, args, result], weight: 1 } as const)

export const internSteps: readonly Step[] = [
  { caption: "intern/3 takes a constructor and a proper argument list.", elements: [{ kind: "zset", key: "call", x: 205, y: 155, showWeights: false, model: { title: "intern request", rows: [application("one", "Option", "[str]", "?")] } }], codeStep: 0 },
  { caption: "The result is the application identity for Option applied to [str].", elements: [{ kind: "op", key: "intern", x: 270, y: 175, label: "intern" }, { kind: "text", key: "identity", x: 535, y: 210, text: "Option[str]  ↦  node 42", size: 29 }], codeStep: 1 },
  { caption: "Repeating the same constructor and arguments returns node 42.", elements: [{ kind: "zset", key: "calls", x: 180, y: 135, showWeights: false, model: { title: "requests", rows: [application("first", "Option", "[str]", "42"), application("again", "Option", "[str]", "42")] } }], codeStep: 2 },
  { caption: "Changing the argument list produces another application identity.", elements: [{ kind: "zset", key: "calls", x: 180, y: 135, showWeights: false, model: { title: "applications", rows: [application("str", "Option", "[str]", "42"), application("int", "Option", "[int]", "77")] } }], codeStep: 3 },
  { caption: "Constructor and arguments must be bound before intern can construct the result.", elements: [{ kind: "text", key: "known", x: 210, y: 150, text: "constructor + arguments", size: 27 }, { kind: "op", key: "intern", x: 520, y: 115, label: "intern" }, { kind: "text", key: "result", x: 760, y: 150, text: "result", size: 27 }], codeStep: 4 },
]
