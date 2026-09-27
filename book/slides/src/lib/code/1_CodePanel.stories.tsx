import type { Meta, StoryObj } from "@storybook/react"
import { CodePanel } from "./1_CodePanel"

const meta = { component: CodePanel } satisfies Meta<typeof CodePanel>
export default meta
type Story = StoryObj<typeof meta>
export const Highlighted: Story = { args: { codeblock: { lang: "text", meta: "", value: "S₁ = S₀ + Δ₁", code: "S₁ = S₀ + Δ₁", themeName: "github-dark", annotations: [], tokens: [["S₁ = S₀ + Δ₁", "#c9d1d9"]], style: { color: "#c9d1d9", background: "#0d1117" } } } }
