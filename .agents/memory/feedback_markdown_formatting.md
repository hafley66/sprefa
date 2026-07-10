---
name: feedback-markdown-formatting
description: render answers with markdown lists/tables/links/footnotes; reserve code fences for runnable code only (VSCode webview shades fenced blocks ugly)
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 72e3adda-ecd3-4611-a57b-c7644b80c664
---

Chris (2026-05-23): the Claude Code VSCode extension renders fenced ```code``` blocks
with a per-token syntax-theme background that hugs content width, so ASCII art and
prose-in-fences come out as ragged gray patchwork. He asked: "use markdown lists,
footnotes, citations, links, tables usefully please."

**Why:** plain prose, lists, and tables render uniformly in the webview; fences get
the shading treatment. The fence is the thing that looks bad, not the content.

**How to apply:**
- Reserve ``` fenced blocks ``` for actual runnable code (stuff he'd paste/run).
- Explain concepts with prose, **lists**, and tables instead of ASCII diagrams.
- Use clickable markdown links to repo files: `[scc.rs](v5/src/scc.rs)` (relative
  to workspace root; the IDE makes them navigable). Line refs: `[file:42](path#L42)`.
- Use footnote syntax for citations (`text[^id]` + `[^id]: ...` at the bottom);
  it renders clean and keeps the prose uncluttered.
- Inline `code` for identifiers mid-sentence is fine.

Supersedes the earlier ASCII-diagram habit. Links: [[feedback-no-casual-codenames]],
[[feedback_build_dont_analyze]].
