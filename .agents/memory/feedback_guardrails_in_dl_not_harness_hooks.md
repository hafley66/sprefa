---
name: feedback_guardrails_in_dl_not_harness_hooks
description: "Word-ban / agent guardrails belong in sprefa DL rules, NOT Claude Code settings.json hooks"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 7322e02c-67ee-4fd7-8304-4c7ef80db5d0
---

When Chris asks for a "rule" to enforce something (e.g. keep a banned word like
"nuke" out of AI-authored files), he wants it expressed as a **sprefa dl rule**
(lint rail / convention relation, like the banned-words rail in `.dl/rails.dl` and
`examples/lints/rust.dl`), NOT as a Claude Code `settings.json` PreToolUse hook.
He rejected an `update-config` settings-hook attempt outright (2026-06-28).

**Why:** sprefa-as-the-policy-engine is the whole point — guardrails should join
the code graph and live in dl, not in harness config. The harness hook is at most
a dumb transport that calls dl; the decision logic stays in dl.

**How to apply:** for any "make a rule / enforce X" request, default to a dl
recipe (diag/lint rail to flag, or a convention sink). Only reach for harness
hooks when the trigger genuinely needs the conversation turn stream (the AI's own
output before it lands), and even then dl decides, the hook only transports. See
the [[programmable-hooks-and-agent-guardrails]] plan (Plan B). Self-policing
(model "remembering" to avoid a word) is explicitly NOT acceptable — it fails
across context loss; enforcement must be external and deterministic.
