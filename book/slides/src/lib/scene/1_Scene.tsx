import { AnimatePresence, motion } from "motion/react"
import { Delta } from "../delta/1_Delta"
import { Op } from "../operator/1_Op"
import { Clock } from "../time/clock/0_Clock"
import { ZSet } from "../zset/1_ZSet"
import type { SceneElement, Step } from "./0_types"

export type SceneProps = { steps: readonly Step[]; stepIndex: number }

type Bounds = { left: number; top: number; right: number; bottom: number }

function elementBounds(element: SceneElement): Bounds {
  switch (element.kind) {
    case "zset": {
      const columns = Math.max(1, ...element.model.rows.map((row) => row.values.length))
      return { left: element.x, top: element.y - 36, right: element.x + columns * 92 + (element.showWeights === false ? 0 : 48), bottom: element.y + Math.max(1, element.model.rows.length) * 48 }
    }
    case "delta": {
      const columns = Math.max(1, ...element.model.rows.map((row) => row.values.length))
      return { left: element.x - 8, top: element.y - 100, right: Math.max(element.x + (element.width ?? 300) + 8, element.x + 58 + columns * 92 + 48), bottom: element.y + Math.max(1, element.model.rows.length - 1) * 48 + 22 }
    }
    case "op": return { left: element.x, top: element.y, right: element.x + 160, bottom: element.y + (element.state ? 104 : 62) }
    case "clock": return { left: element.x - 42, top: element.y - 42, right: element.x + 42, bottom: element.y + 42 }
    case "text": {
      const size = element.size ?? 24
      return { left: element.x, top: element.y - size, right: element.x + element.text.length * size * 0.65, bottom: element.y + 5 }
    }
  }
}

function fittedViewBox(elements: readonly SceneElement[]) {
  const bounds = elements.map(elementBounds)
  const left = Math.min(...bounds.map((bound) => bound.left)) - 36
  const top = Math.min(...bounds.map((bound) => bound.top)) - 36
  const right = Math.max(...bounds.map((bound) => bound.right)) + 36
  const bottom = Math.max(...bounds.map((bound) => bound.bottom)) + 36
  const width = right - left
  const height = bottom - top
  const canvasRatio = 1.65

  if (width / height > canvasRatio) {
    const fittedHeight = width / canvasRatio
    return `${left} ${top - (fittedHeight - height) / 2} ${width} ${fittedHeight}`
  }

  const fittedWidth = height * canvasRatio
  return `${left - (fittedWidth - width) / 2} ${top} ${fittedWidth} ${height}`
}

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
  return <svg viewBox={fittedViewBox(step.elements)} preserveAspectRatio="xMidYMid meet" role="img" aria-label={step.caption} style={{ display: "block", width: "100%", minHeight: 430 }}><AnimatePresence initial={false} mode="popLayout">{step.elements.map((element) => <Element key={element.key} element={element} />)}</AnimatePresence></svg>
}
