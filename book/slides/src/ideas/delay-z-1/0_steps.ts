import type { Step } from "../../lib/scene/0_types"

export const delayZ1Steps: readonly Step[] = [
  { caption: "The input signal has concrete values: 8 at t=0, then 13 at t=1.", elements: [{ kind: "clock", key: "t0", x: 145, y: 145, t: 0 }, { kind: "text", key: "v0", x: 115, y: 245, text: "x = 8", size: 26 }, { kind: "clock", key: "t1", x: 475, y: 145, t: 1 }, { kind: "text", key: "v1", x: 440, y: 245, text: "x = 13", size: 26 }] },
  { caption: "Delay z⁻¹ shifts each value forward by one logical time.", elements: [{ kind: "text", key: "input", x: 90, y: 210, text: "x(t)", size: 30 }, { kind: "op", key: "delay", x: 300, y: 180, label: "z⁻¹" }, { kind: "text", key: "output", x: 590, y: 210, text: "x(t−1)", size: 30 }], codeStep: 0 },
  { caption: "At t=0 there is no previous value, so the delay emits the initial value 0.", elements: [{ kind: "clock", key: "clock", x: 220, y: 205, t: 0 }, { kind: "op", key: "delay", x: 385, y: 175, label: "z⁻¹", state: "output 0" }], codeStep: 1 },
  { caption: "At t=1 the delayed output is the prior input 8.", elements: [{ kind: "clock", key: "clock", x: 220, y: 205, t: 1 }, { kind: "op", key: "delay", x: 385, y: 175, label: "z⁻¹", state: "output 8" }], codeStep: 2 },
  { caption: "At t=2 the delayed output is 13. The feedback edge now crosses time.", elements: [{ kind: "clock", key: "clock", x: 220, y: 205, t: 2 }, { kind: "op", key: "delay", x: 385, y: 175, label: "z⁻¹", state: "output 13" }], codeStep: 3 },
  { caption: "A delayed feedback loop computes running state: s(t) = input(t) + s(t−1).", elements: [{ kind: "text", key: "formula", x: 170, y: 115, text: "s(t) = input(t) + s(t−1)", size: 30 }, { kind: "zset", key: "values", x: 285, y: 165, model: { title: "running state", rows: [{ key: "t0", values: ["t=0", 8], weight: 1 }, { key: "t1", values: ["t=1", 21], weight: 1 }, { key: "t2", values: ["t=2", 26], weight: 1 }] } }], codeStep: 4 },
]
