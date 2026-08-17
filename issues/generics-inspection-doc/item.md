---
created: 2026-08-15
updated: 2026-08-15
type: task
reporter: fable
status: done
priority: normal
epic: type-plane-design
labels:
- size:med
- area:design
- pkg:prolog
closed: 2026-08-15
commits:
- hash: d3e96959
  summary: wrapper inspection doc pair; 8 spellings, 19 stops, 2 stale claims
---

# Written inspection of the wrapper set before any generics work

## Description

User decision (CLAUDE.md): generics need a written inspection, in docs, before any generics implementation. Document the current wrapper inventory and how each lowers. Starting points: 0_type_plane.pl:145-151 (wrapper inventory), 0_generic_expand.pl:125-176 (collection artifacts), 0_option_expand.pl:39-49 (scalar-vs-reference split). Med: reading + faithful writing, no code.
