---
name: build-dont-analyze
description: "when Chris is stuck or low on a project, bias to the smallest RUNNABLE thing over analysis; verify don't assert; check latest lib versions"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 72e3adda-ecd3-4611-a57b-c7644b80c664
---

Observed across the 2026-05-21 session: a long despair/analysis phase about v4 broke the instant Chris said "just build it" and a small v5 actually ran on the kernel. His turns flipped from self-insult/drowning to curious and technical.

**Why:** his mood tracks comprehension and traction, not project quality. A small legible thing that runs lifts the pit; more analysis or "redeem the old project" framing feeds it.

**How to apply:**
- When he is stuck or down, propose the smallest thing that RUNS, not a better plan. Build, then discuss.
- Always run/verify and show real output. He hates green-faking; treat unverified claims as drafts.
- Check LATEST crate versions every time — default-to-stale was caught twice (ast-grep 0.38 vs 0.42; kuzu 0.6 vs 0.11).
- Front-load his constraints: weld the store (don't trait it), keep it small/legible, no shade at his old work, use latest.
- He retains skill by reading his own code. Be scaffolder / critic / reflector; do not become the place the architecture lives (the "brain-shard" fear is real).
- The mood swings (he named manic-depression) are bigger than code; help with traction and reflection, point the heavy part to humans, don't pretend to be the fix.

Links: [[v5-dl-engine]].
