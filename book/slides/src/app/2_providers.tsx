import { CssBaseline } from "@mui/material"
import { ThemeProvider } from "@mui/material/styles"
import { RouterProvider } from "@tanstack/react-router"
import { MotionConfig } from "motion/react"
import { router } from "./0_router"
import { theme } from "./1_theme"

export function Providers() {
  return <ThemeProvider theme={theme}><CssBaseline /><MotionConfig reducedMotion="user"><RouterProvider router={router} /></MotionConfig></ThemeProvider>
}
