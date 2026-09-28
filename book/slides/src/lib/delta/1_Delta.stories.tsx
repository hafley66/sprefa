import type { Meta, StoryObj } from "@storybook/react"
import { Delta } from "./1_Delta"
const meta = { component: Delta, decorators: [(Story) => <svg width="420" height="180"><Story /></svg>] } satisfies Meta<typeof Delta>
export default meta
type Story = StoryObj<typeof meta>
export const InsertAndRetract: Story = { args: { id: "delta", x: 30, y: 100, model: { rows: [{ key: "ada", values: ["ada", 7], weight: 1 }, { key: "bob", values: ["bob", 3], weight: -1 }] } } }
