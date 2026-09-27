import type { Step } from "../../lib/scene/0_types"

export const frontierSteps: readonly Step[] = [
  { caption: "Workers have reported progress through times 3 and 5.", elements: [{ kind: "clock", key: "w1", x: 220, y: 180, t: 3 }, { kind: "clock", key: "w2", x: 560, y: 180, t: 5 }, { kind: "text", key: "labels", x: 172, y: 290, text: "worker A                 worker B", size: 20 }] },
  { caption: "The frontier is the earliest time that may still receive data: min(3, 5) = 3.", elements: [{ kind: "text", key: "frontier", x: 190, y: 120, text: "frontier = min(3, 5) = 3", size: 30 }, { kind: "clock", key: "w1", x: 300, y: 235, t: 3 }, { kind: "clock", key: "w2", x: 530, y: 235, t: 5 }], codeStep: 0 },
  { caption: "Times before 3 are complete. No later message may change their answers.", elements: [{ kind: "text", key: "done", x: 165, y: 150, text: "0   1   2", size: 30 }, { kind: "text", key: "status", x: 195, y: 215, text: "complete", size: 24 }, { kind: "text", key: "boundary", x: 390, y: 150, text: "│ 3   4   5 …", size: 30 }, { kind: "text", key: "open", x: 485, y: 215, text: "may change", size: 24 }], codeStep: 1 },
  { caption: "Worker A advances to 6. Worker B at 5 now holds the frontier.", elements: [{ kind: "clock", key: "w1", x: 300, y: 205, t: 6 }, { kind: "clock", key: "w2", x: 530, y: 205, t: 5 }, { kind: "text", key: "frontier", x: 250, y: 95, text: "frontier = 5", size: 30 }], codeStep: 2 },
  { caption: "When the frontier passes 5, a result for time 5 can be released as final.", elements: [{ kind: "clock", key: "frontier", x: 210, y: 210, t: 6 }, { kind: "op", key: "release", x: 390, y: 180, label: "release", state: "t = 5 final" }], codeStep: 3 },
]
