---
name: rule-is-function-not-channel
description: "sprefa rule mental model — a rule is a function (call/return/yield), never a channel/sink/send. Vocabulary ban + correct framing for design and docs."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: b5f0ade9-540e-4fda-9f7a-284766ab6419
---

A sprefa `rule` is a **function**, not a channel/queue/sink. Never describe a
rule write as "send", "sink", "push", "chan", or "rule-sink path".

Correct model:
- `rule(:r, A?, B?) { body }` defines `fn r`; the body **yields/returns** tuples.
- A terminal `> r(X, Y)` inside a body is `return`/`yield (X, Y)` from that body.
- `<r>_facts` is `r`'s memoized return set, not a sink you push into.
- `r(X,Y)` = call (grounded, run body). `r!(...)` = call bypassing the
  return-memo. `r?(A?,B?)` = read returns, do not call. (locked semantics)
- A "rule of 1 value" = a nullary function with one return; `rule(:MAX){42}`
  is `fn MAX()->42`, read via `MAX?()`.

**Why:** the channel/send metaphor (used in prior chat logs and my framing)
misrepresents the semantics, breaks reasoning about call vs query vs the
return-memo, and the user explicitly rejected it ("kill rule send to the
grave ... use rule like a function not a chan").

**How to apply:** in all design, docs, commit messages, and the autodoc
dogfood, frame an extraction pipe ending in `> r(...)` as "the body of r
returning rows", and `r?(...)` as "reading r's returns". See
[[project_genericization_initiative]] for the locked call/query semantics this
builds on.
