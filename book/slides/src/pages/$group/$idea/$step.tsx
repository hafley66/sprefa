import ArrowBackRounded from "@mui/icons-material/ArrowBackRounded"
import ArrowForwardRounded from "@mui/icons-material/ArrowForwardRounded"
import GridViewRounded from "@mui/icons-material/GridViewRounded"
import { AppBar, Box, Breadcrumbs, IconButton, LinearProgress, Paper, Stack, Toolbar, Typography } from "@mui/material"
import { createFileRoute, notFound, useNavigate } from "@tanstack/react-router"
import type { MDXContent } from "mdx/types"
import { useEffect } from "react"
import { builtIdeas, findIdea } from "../../../lib/0_glossary"
import { CodeSteps } from "../../../lib/code/1_CodeSteps"
import { Scene } from "../../../lib/scene/1_Scene"

export const Route = createFileRoute("/$group/$idea/$step")({ component: Deck, loader: ({ params }) => {
  const found = findIdea(params.group, params.idea)
  const step = Number(params.step)
  if (!found?.idea.built || !found.idea.steps || !Number.isInteger(step) || step < 1 || step > found.idea.steps.length) throw notFound()
  return { ...found, stepIndex: step - 1 }
} })

const contentModules = import.meta.glob("../../../content/*.mdx", { eager: true, import: "default" }) as Record<string, MDXContent>
const contentByIdea = Object.fromEntries(Object.entries(contentModules).map(([path, content]) => {
  const filename = path.split("/").at(-1)!.replace(/\.mdx$/, "")
  return [filename === "zset" ? "z-set" : filename, content]
})) as Record<string, MDXContent>

function Deck() {
  const { group, idea, stepIndex } = Route.useLoaderData()
  const navigate = useNavigate()
  const steps = idea.steps!
  const go = (index: number) => navigate({ to: "/$group/$idea/$step", params: { group: group.slug, idea: idea.slug, step: String(index + 1) } })
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") return void navigate({ to: "/" })
      if (event.key === "Home") return void go(0)
      if (event.key === "ArrowLeft" && stepIndex > 0) return void go(stepIndex - 1)
      if (event.key === "ArrowRight" && stepIndex < steps.length - 1) return void go(stepIndex + 1)
      if (event.key === "ArrowUp" || event.key === "ArrowDown") {
        const current = builtIdeas.findIndex((entry) => entry.idea.slug === idea.slug)
        const next = builtIdeas[current + (event.key === "ArrowUp" ? -1 : 1)]
        if (next) void navigate({ to: "/$group/$idea/$step", params: { group: next.group.slug, idea: next.idea.slug, step: "1" } })
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [group.slug, idea.slug, navigate, stepIndex, steps.length])
  const codeStep = steps[stepIndex].codeStep
  const content = contentByIdea[idea.slug]
  return <Box sx={{ minHeight: "100vh" }}>
    <AppBar position="sticky" color="inherit" elevation={0} sx={{ borderBottom: "1px solid", borderColor: "divider" }}><Toolbar><IconButton aria-label="Open idea grid" onClick={() => navigate({ to: "/" })}><GridViewRounded /></IconButton><Breadcrumbs sx={{ ml: 1, flex: 1 }}><Typography>{group.title}</Typography><Typography>{idea.title}</Typography><Typography color="text.primary">step {stepIndex + 1} of {steps.length}</Typography></Breadcrumbs><IconButton aria-label="Previous step" disabled={stepIndex === 0} onClick={() => go(stepIndex - 1)}><ArrowBackRounded /></IconButton><IconButton aria-label="Next step" disabled={stepIndex === steps.length - 1} onClick={() => go(stepIndex + 1)}><ArrowForwardRounded /></IconButton></Toolbar><LinearProgress variant="determinate" value={((stepIndex + 1) / steps.length) * 100} /></AppBar>
    <Box sx={{ p: { xs: 2, md: 4 }, maxWidth: 1280, mx: "auto" }}><Stack direction={{ xs: "column", lg: "row" }} spacing={3} alignItems="stretch"><Paper variant="outlined" sx={{ flex: 1, overflow: "hidden", minHeight: 430 }}><Scene steps={steps} stepIndex={stepIndex} /></Paper>{codeStep !== undefined && <Box sx={{ width: { xs: "100%", lg: 380 }, alignSelf: "center" }}><CodeSteps content={content} stepIndex={codeStep} /></Box>}</Stack><Typography variant="h5" sx={{ maxWidth: 800, mx: "auto", mt: 3, textAlign: "center", lineHeight: 1.45 }}>{steps[stepIndex].caption}</Typography></Box>
  </Box>
}
