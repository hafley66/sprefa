import type { Meta, StoryObj } from "@storybook/react"
import { WeightChip } from "./0_WeightChip"

const meta = { component: WeightChip, decorators: [(Story) => <svg width="100" height="60"><Story /></svg>] } satisfies Meta<typeof WeightChip>
export default meta
type Story = StoryObj<typeof meta>
export const Positive: Story = { args: { id: "plus", weight: 2, x: 20, y: 12 } }
export const Negative: Story = { args: { id: "minus", weight: -1, x: 20, y: 12 } }
export const Zero: Story = { args: { id: "zero", weight: 0, x: 20, y: 12 } }
