import MenuBookRounded from "@mui/icons-material/MenuBookRounded"
import { Box, Chip, Divider, Drawer, List, ListItemButton, ListItemText, Toolbar, Typography } from "@mui/material"
import { createRootRoute, Outlet, useNavigate } from "@tanstack/react-router"
import { glossary } from "../lib/0_glossary"

const drawerWidth = 264

function AppShell() {
  const navigate = useNavigate()
  return <Box sx={{ display: "flex", minHeight: "100vh" }}>
    <Drawer variant="permanent" sx={{ width: drawerWidth, flexShrink: 0, "& .MuiDrawer-paper": { width: drawerWidth, boxSizing: "border-box", borderRightColor: "#dce1eb" } }}>
      <Toolbar sx={{ gap: 1.2 }}><MenuBookRounded color="primary" /><Typography variant="h6">IVM glossary</Typography></Toolbar>
      <Divider />
      <Box sx={{ overflowY: "auto", py: 1 }}>
        {glossary.map((group) => <Box key={group.slug} sx={{ mb: 1 }}>
          <Typography variant="overline" sx={{ px: 2, color: "text.secondary", letterSpacing: ".09em" }}>{group.title}</Typography>
          <List dense disablePadding>{group.ideas.map((idea) => <ListItemButton key={idea.slug} disabled={!idea.built} onClick={() => idea.built && navigate({ to: "/$group/$idea/$step", params: { group: group.slug, idea: idea.slug, step: "1" } })}><ListItemText primary={idea.title} />{!idea.built && <Chip label="coming" size="small" variant="outlined" />}</ListItemButton>)}</List>
        </Box>)}
      </Box>
    </Drawer>
    <Box component="main" sx={{ width: `calc(100% - ${drawerWidth}px)`, minWidth: 0 }}><Outlet /></Box>
  </Box>
}

export const Route = createRootRoute({ component: AppShell, notFoundComponent: () => <Box sx={{ p: 4 }}><Typography variant="h4">Slide not found</Typography></Box> })
