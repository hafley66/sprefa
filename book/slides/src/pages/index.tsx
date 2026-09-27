import { Box, Card, CardActionArea, CardContent, Chip, Container, Grid, Stack, Typography } from "@mui/material"
import { createFileRoute, useNavigate } from "@tanstack/react-router"
import { glossary } from "../lib/0_glossary"

export const Route = createFileRoute("/")({ component: Home })

function Home() {
  const navigate = useNavigate()
  return <Container maxWidth="lg" sx={{ py: 7 }}>
    <Typography variant="h3" gutterBottom>Incremental view maintenance</Typography>
    <Typography color="text.secondary" sx={{ mb: 5, maxWidth: 660 }}>A visual glossary of the algebra underneath changing queries.</Typography>
    <Stack spacing={4}>{glossary.map((group) => <Box key={group.slug}><Typography variant="h5" sx={{ mb: 1.5 }}>{group.title}</Typography><Grid container spacing={2}>{group.ideas.map((idea) => <Grid key={idea.slug} size={{ xs: 12, sm: 6, md: 4 }}><Card variant="outlined" sx={{ height: "100%", opacity: idea.built ? 1 : 0.58 }}><CardActionArea disabled={!idea.built} sx={{ height: "100%" }} onClick={() => navigate({ to: "/$group/$idea/$step", params: { group: group.slug, idea: idea.slug, step: "1" } })}><CardContent><Stack direction="row" justifyContent="space-between" alignItems="center"><Typography variant="h6">{idea.title}</Typography>{!idea.built && <Chip label="coming" size="small" />}</Stack><Typography color="text.secondary" sx={{ mt: 1 }}>{idea.summary}</Typography></CardContent></CardActionArea></Card></Grid>)}</Grid></Box>)}</Stack>
  </Container>
}
