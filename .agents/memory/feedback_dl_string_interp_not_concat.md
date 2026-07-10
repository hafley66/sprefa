---
name: feedback_dl_string_interp_not_concat
description: "dl has no concat/format scalar; build strings via ${var} interpolation inside a string literal (var bound in the rule body)"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 975106ac-2855-4a9e-a1b4-51e1388a057d
---

To build a computed string in dl (e.g. a `diag` `msg:` that names the offending
values), use `${var}` interpolation inside the string literal — there is no
`concat`/`format` scalar function. The interpolated var must be bound elsewhere
in the rule body.

**Why:** Chris corrected "concat is string interp" after I fell back to a static
message believing no concat existed. The scalar-function table (`replace`,
`split`, `trim`, ...) has no concat, which misled me; interpolation is the
mechanism instead.

**How to apply:** `msg: "layer \`${layer_a}\` must not depend on \`${layer_b}\`"`
with `layer_a`/`layer_b` bound in the body (see examples/arch-conformance.dl,
ban.dl, doc-coverage.dl). Same `${}` hole syntax as node_ref/tour_step string
fields. Distinct from the regex `re`/`match` `${}` hole rule [[reference_re_dsl_hole_literal_rule]].
