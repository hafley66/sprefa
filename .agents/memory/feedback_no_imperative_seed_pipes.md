---
name: no-imperative-seed-pipes
description: "user dislikes the imperative top-level \"rule append\" pattern; prefers `rule(:r, cols?) { body }` form. EXACT preferred shape NOT confirmed — ask before extrapolating."
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 8f4d8731-9012-4431-891f-2974c87306f9
---

User flagged (2026-05-20) that examples keep defaulting to imperative
"rule append" form and they hate it. They want `rule(:name, cols?) { body }`
form instead.

EXACT semantics they want for the body — NOT confirmed. Per user
correction in the same session: do not invent a framework ("body
terminal step yields row", "no tail-write", "rule-as-function") on
their behalf without asking. Reference shape they actually typed:

    rule(:fuck_your_stupid_ai_fucking_face, fuck?, you?) {
        print(`dumb bitch ${fuck}`)
    }

That body uses `print(...)` referencing a head-declared capture. It
doesn't bind or yield anything visible. Whatever the materialization
rule actually is, the user hasn't spelled it out in writing — do not
write it for them.

**How to apply:**
- Reach for `rule(:name, cols?) { … }` over top-level `… > name(X, Y)`.
- Before sketching anything fancier than the simplest body, ask which shape they want.
- Before updating THIS memory with refined semantics, ask. The user has called out auto-gaslighting (revising their preferences from my own inferences) as a recurring failure mode.
