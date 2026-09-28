import { motion } from "motion/react"

export type StatePanelProps = { id: string; label: string; x: number; y: number; width?: number }

export function StatePanel({ id, label, x, y, width = 126 }: StatePanelProps) {
  return <motion.g layoutId={id}><rect x={x} y={y} width={width} height="34" rx="6" fill="#e2e8f0" /><text x={x + width / 2} y={y + 22} textAnchor="middle" fill="#334155" fontSize="13">{label}</text></motion.g>
}
