import { motion } from "motion/react"
import { StatePanel } from "./state/0_StatePanel"

export type OpProps = { id: string; label: string; x: number; y: number; state?: string }

export function Op({ id, label, x, y, state }: OpProps) {
  const height = state ? 104 : 62
  return <motion.g layoutId={id} initial={{ opacity: 0, scale: 0.9 }} animate={{ opacity: 1, scale: 1 }} exit={{ opacity: 0, scale: 0.9 }}><rect x={x} y={y} width="160" height={height} rx="14" fill="#eff6ff" stroke="#2563eb" strokeWidth="2" /><text x={x + 80} y={y + 38} textAnchor="middle" fill="#1e3a8a" fontSize="20" fontWeight="700">{label}</text>{state && <StatePanel id={`${id}:state`} label={state} x={x + 17} y={y + 56} />}</motion.g>
}
