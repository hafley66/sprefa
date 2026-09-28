import type { Step } from "../../lib/scene/0_types"

const request = { key: "src", values: ["app.ts"], weight: 1 } as const
const v1 = { key: "v1", values: ["app.ts", "v1"], weight: 1 } as const
const retractV1 = { ...v1, weight: -1 } as const
const v2 = { key: "v2", values: ["app.ts", "v2"], weight: 1 } as const

export const fsWatchContinuingSteps: readonly Step[] = [
  { caption: "One fs.watch application asks for the current UTF-8 body of app.ts.", elements: [{ kind: "zset", key: "request", x: 315, y: 165, model: { title: "watch application", rows: [request] } }], codeStep: 0 },
  { caption: "The host watches the file's parent directory so replace-on-save updates are observed.", elements: [{ kind: "zset", key: "request", x: 100, y: 175, model: { title: "application", rows: [request] } }, { kind: "op", key: "watch", x: 500, y: 180, label: "watch parent", state: "continuing" }], codeStep: 1 },
  { caption: "The existing file yields its current body v1.", elements: [{ kind: "clock", key: "time", x: 120, y: 195, t: 4 }, { kind: "zset", key: "body", x: 350, y: 165, model: { title: "fs.watch", rows: [v1] } }], codeStep: 2 },
  { caption: "When the body changes to v2 the old body retracts and the new body inserts.", elements: [{ kind: "clock", key: "time", x: 100, y: 205, t: 7 }, { kind: "delta", key: "body", x: 300, y: 195, width: 390, model: { title: "Δ fs.watch", rows: [retractV1, v2] } }], codeStep: 3 },
  { caption: "Deleting app.ts retracts v2 and leaves no current body row.", elements: [{ kind: "clock", key: "time", x: 120, y: 195, t: 11 }, { kind: "delta", key: "body", x: 350, y: 195, width: 350, model: { title: "Δ fs.watch", rows: [{ ...v2, weight: -1 }] } }], codeStep: 4 },
  { caption: "The application keeps listening after deletion. Retracting it cancels the continuing watch.", elements: [{ kind: "zset", key: "request", x: 100, y: 175, model: { title: "Δ application", rows: [{ ...request, weight: -1 }] } }, { kind: "op", key: "cancel", x: 510, y: 180, label: "watch parent", state: "cancelled" }], codeStep: 5 },
]
