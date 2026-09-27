import { createTheme } from "@mui/material/styles"

export const theme = createTheme({
  palette: { mode: "light", primary: { main: "#2155d6" }, background: { default: "#f6f7fb", paper: "#ffffff" } },
  typography: { fontFamily: 'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif', h3: { fontWeight: 750 }, h5: { fontWeight: 700 } },
  shape: { borderRadius: 12 },
})
