import type { Meta, StoryObj } from "@storybook/react"
import { DeltaArrow } from "./0_DeltaArrow"
const meta = { component: DeltaArrow, decorators: [(Story) => <svg width="280" height="100"><Story /></svg>] } satisfies Meta<typeof DeltaArrow>
export default meta
type Story = StoryObj<typeof meta>
export const Forward: Story = { args: { id: "arrow", x1: 20, x2: 250, y: 50 } }
