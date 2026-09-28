import type { Meta, StoryObj } from "@storybook/react"
import { ZSet } from "./1_ZSet"

const meta = { component: ZSet, decorators: [(Story) => <svg width="360" height="180"><Story /></svg>] } satisfies Meta<typeof ZSet>
export default meta
type Story = StoryObj<typeof meta>
export const Empty: Story = { args: { id: "empty", x: 20, y: 45, model: { title: "Empty Z-set", rows: [] } } }
export const Set: Story = { args: { id: "set", x: 20, y: 45, showWeights: false, model: { title: "Set", rows: [{ key: "ada", values: ["ada", 7], weight: 1 }, { key: "bob", values: ["bob", 3], weight: 1 }] } } }
export const Bag: Story = { args: { id: "bag", x: 20, y: 45, model: { title: "Bag", rows: [{ key: "ada", values: ["ada", 7], weight: 2 }, { key: "bob", values: ["bob", 3], weight: 1 }] } } }
export const WithNegatives: Story = { args: { id: "people", x: 20, y: 45, model: { title: "Z-set", rows: [{ key: "ada", values: ["ada", 7], weight: 2 }, { key: "bob", values: ["bob", 3], weight: -1 }] } } }
