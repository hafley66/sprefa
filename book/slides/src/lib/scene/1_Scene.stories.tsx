import type { Meta, StoryObj } from "@storybook/react"
import { Scene } from "./1_Scene"
const meta = { component: Scene, parameters: { layout: "fullscreen" } } satisfies Meta<typeof Scene>
export default meta
type Story = StoryObj<typeof meta>
export const PersistingKeys: Story = { args: { stepIndex: 0, steps: [{ caption: "A weighted row", elements: [{ kind: "zset", key: "state", x: 250, y: 170, model: { title: "S", rows: [{ key: "ada", values: ["ada", 7], weight: 1 }] } }] }] } }
