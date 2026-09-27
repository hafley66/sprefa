import type { Step } from "../../lib/scene/0_types"

const request = { key: "watch", values: ["app.ts"], weight: 1 } as const
const removedRequest = { ...request, weight: -1 } as const
const answer = { key: "body", values: ["app.ts", "v1"], weight: 1 } as const
const removedAnswer = { ...answer, weight: -1 } as const

export const retractionSteps: readonly Step[] = [
  { caption: "A live fs.watch application keeps a watch active for app.ts.", elements: [{ kind: "zset", key: "request", x: 300, y: 165, model: { title: "watch application", rows: [request] } }, { kind: "op", key: "watcher", x: 690, y: 180, label: "watcher", state: "active" }], codeStep: 0 },
  { caption: "The current UTF-8 body is the answer row for that path.", elements: [{ kind: "zset", key: "answer", x: 315, y: 165, model: { title: "fs.watch", rows: [answer] } }, { kind: "text", key: "derived", x: 690, y: 215, text: "compile v1", size: 28 }], codeStep: 1 },
  { caption: "When the program withdraws the application its weight changes by −1.", elements: [{ kind: "delta", key: "delete", x: 330, y: 190, width: 350, model: { title: "Δ watch application", rows: [removedRequest] } }], codeStep: 2 },
  { caption: "The host sees no remaining application for the file and cancels its watcher.", elements: [{ kind: "op", key: "reconcile", x: 225, y: 180, label: "reconcile", state: "path absent" }, { kind: "op", key: "watcher", x: 610, y: 180, label: "watcher", state: "cancelled" }], codeStep: 3 },
  { caption: "Answers supported by the withdrawn request retract as well.", elements: [{ kind: "delta", key: "answer", x: 330, y: 190, width: 410, model: { title: "Δ watch answer", rows: [removedAnswer] } }, { kind: "text", key: "gone", x: 760, y: 220, text: "rebuild app: −1", size: 26 }], codeStep: 4 },
  { caption: "Retraction propagates through the same weighted relations as insertion.", elements: [{ kind: "text", key: "chain", x: 105, y: 185, text: "application −1  →  cancel  →  body −1", size: 27 }, { kind: "text", key: "result", x: 275, y: 260, text: "derived rows return to weight 0", size: 25 }], codeStep: 5 },
]
