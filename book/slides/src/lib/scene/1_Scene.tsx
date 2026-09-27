import { AnimatePresence, motion } from "motion/react"
import { Delta } from "../delta/1_Delta"
import { Op } from "../operator/1_Op"
import { Clock } from "../time/clock/0_Clock"
import { ZSet } from "../zset/1_ZSet"
import type { SceneElement, Step } from "./0_types"

export type SceneProps = { steps: readonly Step[]; stepIndex: number }

function Element({ element }: { element: SceneElement }) {
  switch (element.kind) {
    case "zset": return <ZSet id={element.key} model={element.model} x={element.x} y={element.y} showWeights={element.showWeights} />
    case "delta": return <Delta id={element.key} model={element.model} x={element.x} y={element.y} width={element.width} />
    case "op": return <Op id={element.key} label={element.label} x={element.x} y={element.y} state={element.state} />
    case "clock": return <Clock id={element.key} t={element.t} round={element.round} x={element.x} y={element.y} />
    case "text": return <motion.text layoutId={element.key} x={element.x} y={element.y} fontSize={element.size ?? 24} fontWeight="700" fill="#172033" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>{element.text}</motion.text>
  }
}

export function Scene({ steps, stepIndex }: SceneProps) {
  const step = steps[stepIndex]
  return <svg viewBox="0 0 920 440" role="img" aria-label={step.caption}><AnimatePresence mode="popLayout">{step.elements.map((element) => <Element key={element.key} element={element} />)}</AnimatePresence></svg>
}
