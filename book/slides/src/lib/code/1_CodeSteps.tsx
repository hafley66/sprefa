import { Block, HighlightedCodeBlock, parseRoot } from "codehike/blocks"
import { z } from "zod"
import { CodePanel } from "./1_CodePanel"
import type { MDXContent } from "mdx/types"

const Schema = Block.extend({ steps: z.array(Block.extend({ code: HighlightedCodeBlock })) })

export function CodeSteps({ content, stepIndex }: { content: MDXContent; stepIndex: number }) {
  const { steps } = parseRoot(content, Schema)
  const step = steps[Math.min(stepIndex, steps.length - 1)]
  return <CodePanel codeblock={step.code} />
}
