import type { Preview } from "@storybook/react"
import { ThemeProvider } from "@mui/material/styles"
import { theme } from "../src/app/1_theme"
import "../src/app/4_global.css"

const preview: Preview = {
  decorators: [(Story) => <ThemeProvider theme={theme}><Story /></ThemeProvider>],
  parameters: { layout: "centered" },
}
export default preview
