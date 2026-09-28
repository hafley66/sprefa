import type { Meta, StoryObj } from "@storybook/react"
import { StatePanel } from "./0_StatePanel"
const meta = { component: StatePanel, decorators: [(Story) => <svg width="180" height="80"><Story /></svg>] } satisfies Meta<typeof StatePanel>
export default meta
type Story = StoryObj<typeof meta>
export const RunningState: Story = { args: { id: "state", label: "Sₜ = { ada: +1 }", x: 20, y: 20 } }
