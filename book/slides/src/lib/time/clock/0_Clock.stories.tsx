import type { Meta, StoryObj } from "@storybook/react"
import { Clock } from "./0_Clock"
const meta = { component: Clock, decorators: [(Story) => <svg width="130" height="110"><Story /></svg>] } satisfies Meta<typeof Clock>
export default meta
type Story = StoryObj<typeof meta>
export const UpdateTime: Story = { args: { id: "clock", t: 3, x: 65, y: 55 } }
export const WithRound: Story = { args: { id: "clock-round", t: 3, round: 1, x: 65, y: 55 } }
