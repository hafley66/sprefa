---
name: feedback-pin-subagent-models-haiku
description: "Deterministic ceremony becomes a justfile/bash script, never an agent; remaining agents pin model explicitly (haiku default, sonnet for codegen)"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: d301bcde-b31f-4523-b7be-16fcdb95311c
---

Two-step test for packaging a recurring workflow:
1. Deterministic procedure (fixed commands, objective pass/fail)? Then it is a
   justfile recipe calling a bash script — zero tokens. Gotchas live as header
   comments in the script, not in a skill.
2. Only if the task generates code/content from a spec does it become an
   agent, and the definition pins `model:` explicitly — never `inherit`.
   Default haiku; `sonnet` for Rust-writing implementers per
   [[feedback_sonnet5_for_coding]].

**Why:** Chris, 2026-07-10, after I drafted six agents: "what can just be a
justfile call to a bash script tho ... literally all of this is deterministic
... i guess maybe not extract op impler". Also a prior Opus subagent hit 394k
tokens (chat_log 20260709.1) — model pinning is the cost guard.

**How to apply (sprefa):** `just verify` (suite + flake policy + rails),
`just regen-docs` (generators + convergence), `just cut X.Y.Z` (release) are
the script conversions — extend those before proposing any new agent. Kept
agents: `extraction-op-implementer`, `builtin-rel-implementer` (both sonnet,
both write Rust from a spec), `magic-rel-auditor`.
