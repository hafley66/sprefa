import type { Step } from "../../lib/scene/0_types"

const tasks = [
  { key: "a", values: ["a", "write"], weight: 1 },
  { key: "b", values: ["b", "test"], weight: 1 },
] as const

export const antijoinSteps: readonly Step[] = [
  { caption: "Tasks a and b are candidates. The done set contains a.", elements: [{ kind: "zset", key: "tasks", x: 150, y: 175, model: { title: "tasks", rows: tasks } }, { kind: "zset", key: "done", x: 660, y: 190, model: { title: "done", rows: [{ key: "a", values: ["a"], weight: 1 }] } }], codeStep: 0 },
  { caption: "Antijoin keeps left rows whose key is absent on the right.", elements: [{ kind: "zset", key: "tasks", x: 100, y: 175, model: { title: "tasks", rows: tasks } }, { kind: "op", key: "anti", x: 390, y: 180, label: "antijoin", state: "by id" }, { kind: "zset", key: "out", x: 610, y: 190, model: { title: "pending", rows: [tasks[1]] } }], codeStep: 1 },
  { caption: "Task a is blocked because its key appears in done.", elements: [{ kind: "zset", key: "a", x: 190, y: 190, model: { title: "candidate", rows: [tasks[0]] } }, { kind: "op", key: "blocked", x: 520, y: 180, label: "key exists", state: "a = a" }, { kind: "text", key: "none", x: 820, y: 225, text: "emit 0", size: 28 }], codeStep: 2 },
  { caption: "Retracting done(a) makes a absent and inserts task a into pending.", elements: [{ kind: "delta", key: "done", x: 130, y: 205, width: 240, model: { title: "Δ done", rows: [{ key: "a", values: ["a"], weight: -1 }] } }, { kind: "op", key: "anti", x: 410, y: 180, label: "antijoin" }, { kind: "delta", key: "pending", x: 610, y: 205, width: 260, model: { title: "Δ pending", rows: [tasks[0]] } }], codeStep: 3 },
  { caption: "Antijoin must remember right-side presence to react when a key crosses zero.", elements: [{ kind: "text", key: "state", x: 210, y: 150, text: "count(done[a]): 1 → 0", size: 28 }, { kind: "text", key: "effect", x: 210, y: 230, text: "pending gains (a, write) × +1", size: 25 }], codeStep: 4 },
]
