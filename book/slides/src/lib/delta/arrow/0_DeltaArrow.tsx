import { motion } from "motion/react"

export type DeltaArrowProps = { id: string; x1: number; x2: number; y: number }

export function DeltaArrow({ id, x1, x2, y }: DeltaArrowProps) {
  return <motion.path layoutId={id} d={`M ${x1} ${y} H ${x2 - 15} l -9 -7 m 9 7 l -9 7`} fill="none" stroke="#2563eb" strokeWidth="3" initial={{ pathLength: 0 }} animate={{ pathLength: 1 }} exit={{ opacity: 0 }} />
}
