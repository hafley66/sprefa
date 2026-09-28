import type { Step } from "../../lib/scene/0_types"

export const nestedTimeSteps: readonly Step[] = [
  { caption: "An outer time records the input update. At t=4 the edge Ada → Bob arrives.", elements: [{ kind: "clock", key: "outer", x: 210, y: 205, t: 4 }, { kind: "delta", key: "edge", x: 390, y: 205, model: { title: "edge update", rows: [{ key: "ab", values: ["ada", "bob"], weight: 1 }] } }] },
  { caption: "Recursive work adds an inner round: logical time is the pair (outer, round).", elements: [{ kind: "text", key: "pair", x: 280, y: 115, text: "time = (4, 0)", size: 32 }, { kind: "clock", key: "clock", x: 430, y: 225, t: 4, round: 0 }], codeStep: 0 },
  { caption: "Round (4, 1) derives Ada → Cy from the new edge and an existing Bob → Cy path.", elements: [{ kind: "clock", key: "clock", x: 175, y: 210, t: 4, round: 1 }, { kind: "op", key: "join", x: 330, y: 180, label: "join" }, { kind: "delta", key: "path", x: 575, y: 210, model: { title: "new path", rows: [{ key: "ac", values: ["ada", "cy"], weight: 1 }] } }], codeStep: 1 },
  { caption: "Round (4, 2) is empty, so the inner iteration for outer time 4 is complete.", elements: [{ kind: "clock", key: "clock", x: 245, y: 210, t: 4, round: 2 }, { kind: "delta", key: "empty", x: 455, y: 210, model: { title: "Δ = ∅", rows: [] } }], codeStep: 2 },
  { caption: "The next input advances outer time to 5 and starts its own round 0.", elements: [{ kind: "clock", key: "clock", x: 235, y: 210, t: 5, round: 0 }, { kind: "delta", key: "edge", x: 455, y: 210, model: { title: "edge update", rows: [{ key: "cd", values: ["cy", "dee"], weight: 1 }] } }], codeStep: 3 },
  { caption: "Nested time keeps iteration rounds ordered inside each input update.", elements: [{ kind: "text", key: "t4", x: 120, y: 155, text: "(4,0) → (4,1) → (4,2)", size: 27 }, { kind: "text", key: "t5", x: 470, y: 250, text: "→ (5,0) → …", size: 27 }], codeStep: 4 },
]
