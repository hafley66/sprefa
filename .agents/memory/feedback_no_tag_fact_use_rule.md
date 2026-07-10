---
name: feedback_no_tag_fact_use_rule
description: tag/fact ops deprecated and slated for deletion; use rule() as sink/declare everywhere
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 2be5eb40-4f91-46fe-84ea-a3848747273b
---

`tag` and `fact` ops are deprecated. User intends to delete them soon.
`rule(...)` already does the job (declare-only type + sink target).

**Why:** one mechanism. tag/fact are redundant with rule(); keeping
them in examples teaches the dead path.
**How to apply:** never write `tag(...)` or `fact(...)` in demos,
examples, tests authored fresh, or new code. Use `rule(:name, A?, B?)`
to declare and `> name(A, B)` (or pipe-tail bind) to write rows.
Relates to [[project_cross_file_entity_graph]].
