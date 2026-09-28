import { motion } from "motion/react"

export type WeightChipProps = { id: string; weight: number; x: number; y: number }

export function WeightChip({ id, weight, x, y }: WeightChipProps) {
  const fill = weight > 0 ? "#087f5b" : weight < 0 ? "#c92a2a" : "#64748b"
  return (
    <motion.g layoutId={id} initial={{ opacity: 0, scale: 0.7 }} animate={{ opacity: weight === 0 ? 0.24 : 1, scale: 1 }} exit={{ opacity: 0, scale: 0.7 }}>
      <rect x={x} y={y} width="40" height="28" rx="14" fill={fill} />
      <text data-row-text x={x + 20} y={y + 21} textAnchor="middle" fill="white" fontSize="20" fontWeight="700">
        {weight > 0 ? `+${weight}` : weight}
      </text>
    </motion.g>
  )
}
