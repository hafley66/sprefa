import { motion } from "motion/react"
import type { ZSetModel } from "./0_types"
import { Row } from "./row/0_Row"

export type ZSetProps = { id: string; model: ZSetModel; x: number; y: number; showWeights?: boolean }

export function ZSet({ id, model, x, y, showWeights = true }: ZSetProps) {
  return (
    <motion.g layoutId={id} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
      {model.title && <text x={x} y={y - 14} fill="#334155" fontSize="15" fontWeight="700">{model.title}</text>}
      {model.rows.map((row, index) => <Row key={row.key} id={`${id}:${row.key}`} row={row} x={x} y={y + index * 48} showWeight={showWeights} />)}
    </motion.g>
  )
}
