import type { Step } from "../../lib/scene/0_types"

const request = { key: "todo", values: ["req-7", "todo.json"], weight: 1 } as const
const answer = { key: "body", values: ["req-7", "tasks=3"], weight: 1 } as const

export const requestAnswerEffectsSteps: readonly Step[] = [
  { caption: "An effect type connects an input product to an output sum.", elements: [{ kind: "op", key: "effect", x: 340, y: 175, label: "effect", state: "Request → Result" }, { kind: "text", key: "input", x: 90, y: 225, text: "path: str", size: 27 }, { kind: "text", key: "output", x: 610, y: 225, text: "ok: Body | error: str", size: 25 }], codeStep: 0 },
  { caption: "A program derives one request row for todo.json.", elements: [{ kind: "zset", key: "requests", x: 315, y: 165, model: { title: "fs.json request", rows: [request] } }], codeStep: 1 },
  { caption: "The host reads the request and performs the filesystem operation.", elements: [{ kind: "zset", key: "requests", x: 100, y: 175, model: { title: "request", rows: [request] } }, { kind: "op", key: "host", x: 530, y: 180, label: "host", state: "read todo.json" }, { kind: "text", key: "disk", x: 765, y: 215, text: "disk", size: 29 }], codeStep: 2 },
  { caption: "The host inserts an answer row keyed to request req-7.", elements: [{ kind: "op", key: "host", x: 180, y: 180, label: "host", state: "answer" }, { kind: "zset", key: "answers", x: 500, y: 165, model: { title: "fs.json ok", rows: [answer] } }], codeStep: 3 },
  { caption: "Evaluation resumes with the answer row and derives tasks = 3.", elements: [{ kind: "zset", key: "answers", x: 120, y: 165, model: { title: "answer", rows: [answer] } }, { kind: "op", key: "derive", x: 555, y: 180, label: "evaluate" }, { kind: "text", key: "tasks", x: 800, y: 215, text: "tasks = 3", size: 28 }], codeStep: 4 },
  { caption: "Compiler and runtime use the same request, answer, re-evaluate contract.", elements: [{ kind: "text", key: "cycle", x: 155, y: 170, text: "request row → host → answer row", size: 29 }, { kind: "text", key: "again", x: 250, y: 250, text: "↖      re-evaluate      ↙", size: 26 }], codeStep: 5 },
]
