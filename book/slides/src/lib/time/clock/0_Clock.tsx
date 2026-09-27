import { motion } from "motion/react"

export type ClockProps = { id: string; t: number; round?: number; x: number; y: number }

export function Clock({ id, t, round, x, y }: ClockProps) {
  return <motion.g layoutId={id} initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}><circle cx={x} cy={y} r="35" fill="#fff7ed" stroke="#f97316" strokeWidth="2" /><text x={x} y={y + 2} textAnchor="middle" fill="#9a3412" fontSize="16" fontWeight="700">t = {t}</text>{round !== undefined && <text x={x} y={y + 19} textAnchor="middle" fill="#9a3412" fontSize="11">round {round}</text>}</motion.g>
}
