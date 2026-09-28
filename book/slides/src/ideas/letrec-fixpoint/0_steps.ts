import type { Step } from "../../lib/scene/0_types"

const row = (key: string, from: string, to: string) => ({ key, values: [from, to], weight: 1 } as const)

export const letrecFixpointSteps: readonly Step[] = [
  { caption: "The graph starts with edges Ada → Bob and Bob → Cy.", elements: [{ kind: "zset", key: "edge", x: 285, y: 140, model: { title: "edge", rows: [row("ab", "ada", "bob"), row("bc", "bob", "cy")] } }] },
  { caption: "LetRec seeds reach with the direct edges in round 0.", elements: [{ kind: "clock", key: "round", x: 185, y: 210, t: 0, round: 0 }, { kind: "zset", key: "reach", x: 345, y: 140, model: { title: "reach₀", rows: [row("ab", "ada", "bob"), row("bc", "bob", "cy")] } }], codeStep: 0 },
  { caption: "Round 1 joins reach with edge and derives Ada → Cy.", elements: [{ kind: "clock", key: "round", x: 155, y: 210, t: 0, round: 1 }, { kind: "op", key: "join", x: 285, y: 180, label: "join" }, { kind: "delta", key: "new", x: 570, y: 210, model: { title: "new", rows: [row("ac", "ada", "cy")] } }], codeStep: 1 },
  { caption: "Round 2 produces no new rows.", elements: [{ kind: "clock", key: "round", x: 235, y: 210, t: 0, round: 2 }, { kind: "delta", key: "new", x: 420, y: 210, model: { title: "new = ∅", rows: [] } }], codeStep: 2 },
  { caption: "The empty delta marks the fixpoint. reach contains all three paths.", elements: [{ kind: "text", key: "fixed", x: 285, y: 95, text: "Δ = ∅  ·  fixed", size: 30 }, { kind: "zset", key: "reach", x: 300, y: 145, model: { title: "reach*", rows: [row("ab", "ada", "bob"), row("bc", "bob", "cy"), row("ac", "ada", "cy")] } }], codeStep: 3 },
]
