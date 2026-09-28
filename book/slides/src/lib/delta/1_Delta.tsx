import { motion } from "motion/react"
import type { ZSetModel } from "../zset/0_types"
import { ZSet } from "../zset/1_ZSet"
import { DeltaArrow } from "./arrow/0_DeltaArrow"

export type DeltaProps = { id: string; model: ZSetModel; x: number; y: number; width?: number }

export function Delta({ id, model, x, y, width = 300 }: DeltaProps) {
  return <motion.g layoutId={id}><circle cx={x} cy={y} r="6" fill="#2563eb" /><circle cx={x + width} cy={y} r="6" fill="#2563eb" /><DeltaArrow id={`${id}:arrow`} x1={x + 8} x2={x + width - 8} y={y} /><ZSet id={`${id}:rows`} model={{ ...model, title: model.title ?? "Δ" }} x={x + 58} y={y - 62} /></motion.g>
}
