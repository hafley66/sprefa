---
name: feedback-rule-form-and-call
description: "Sprefa rule surface preference — keep `rule(:name){body}` nested form; if it needs to run, ADD a bare `name();` call. Do not inline the body at top level."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 75ab2457-57b4-47c9-8abc-95a9959366bf
---

When a sprf rule's body needs to execute, KEEP the nested `rule(:name, ...) { ... }` form
and add a separate bare invocation `name();` (or `> name(args)` at the end of a pipe).
Do NOT rewrite the rule as a top-level inline pipe; that loses the rule abstraction.

**Why:** the nested form is the canonical functional surface. "Just call it" is the
correct fix for `auto_run` removal (commit f2ef0ac), not "inline the body". Inlining
forfeits reuse, naming, and the rule-as-function model. User flagged this 2026-05-20
in tier-0 bench fixups when I rewrote `rule(:hits){...}` to a top-level pipe instead
of adding `hits();`.

**How to apply:** if a `rule(:name){...}` block isn't producing facts because nothing
invokes it, append `name();` (or whatever the right call surface is for the use site)
RIGHT BELOW the rule decl. Leave the body in the block.

Related: [[project-recursion-surface-gaps]] (the run-time wire that consumes bare
rule invocations), [[feedback-rule-is-function-not-channel]].
