import type { Meta, StoryObj } from "@storybook/react"
import { Row } from "./0_Row"

const meta = { component: Row, decorators: [(Story) => <svg width="320" height="80"><Story /></svg>] } satisfies Meta<typeof Row>
export default meta
type Story = StoryObj<typeof meta>
export const Insert: Story = { args: { id: "ada", row: { key: "ada", values: ["ada", 7], weight: 2 }, x: 10, y: 20 } }
export const Retract: Story = { args: { id: "bob", row: { key: "bob", values: ["bob", 3], weight: -1 }, x: 10, y: 20 } }
