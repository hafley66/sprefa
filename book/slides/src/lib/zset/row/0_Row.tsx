import { motion } from "motion/react"
import { WeightChip } from "../weight/0_WeightChip"
import type { ZRow } from "../0_types"

export type RowProps = { id: string; row: ZRow; x: number; y: number; showWeight?: boolean }

export function Row({ id, row, x, y, showWeight = true }: RowProps) {
  return (
    <motion.g data-row-visible={row.weight !== 0} layoutId={id} initial={{ opacity: 0, y: 8 }} animate={{ opacity: row.weight === 0 ? 0 : 1, y: 0 }} exit={{ opacity: 0, y: -8 }}>
      {row.values.map((value, index) => (
        <g key={`${id}:cell:${index}`}>
          <rect x={x + index * 92} y={y} width="86" height="36" rx="7" fill="#f8fafc" stroke="#94a3b8" />
          <text data-row-text x={x + index * 92 + 43} y={y + 25} textAnchor="middle" fill="#172033" fontSize="22">{String(value)}</text>
        </g>
      ))}
      {showWeight && <WeightChip id={`${id}:weight`} weight={row.weight} x={x + row.values.length * 92 + 8} y={y + 4} />}
    </motion.g>
  )
}
