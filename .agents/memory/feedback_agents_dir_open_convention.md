---
name: feedback_agents_dir_open_convention
description: "Put agent + skill definitions under `.agents/`, never `.claude/`; open tool-agnostic convention, de-binding from Anthropic-specific paths"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 49ec9f3c-dc92-4981-9b4f-2401f27d05f2
---

Agent and skill definitions live under **`.agents/`**, not `.claude/`. In the
sprefa repo: skills at `.agents/skills/<name>/SKILL.md`, agents at
`.agents/agents/<name>.md`. `.agents/` is tracked (clone-surviving); `.claude/`
is gitignored and Anthropic-tool-specific.

**Why:** Chris is done overbinding tooling to Anthropic — "we play open ball or
none at all." The open `.agents/` convention keeps definitions portable across
coding agents and committable to the repo.

**How to apply:** When creating or moving an agent/skill, target `.agents/`.
Match the neighbor prefix in `.agents/skills/` (`sprf-*`). Do not create files in
`.claude/agents` or `.claude/skills` as the source of truth. Repo-facing skills
that must survive a clone still also have `assets/*.skill.md` wired by
`dl setup --project`; `.agents/` is the open home for the rest.
