import type { Meta, StoryObj } from "@storybook/react"
import { Op } from "./1_Op"
const meta = { component: Op, decorators: [(Story) => <svg width="210" height="150"><Story /></svg>] } satisfies Meta<typeof Op>
export default meta
type Story = StoryObj<typeof meta>
export const Stateless: Story = { args: { id: "map", label: "map", x: 20, y: 20 } }
export const Stateful: Story = { args: { id: "reduce", label: "reduce", state: "sum = 10", x: 20, y: 20 } }
