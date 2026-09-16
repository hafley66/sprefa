# generics-wrapper-inspection (issue: generics-inspection-doc, size:med, DOC ONLY)

FIRST ACTION: `git merge --ff-only e23893b2ef8d3e4c5f60f0a98f015b95dea23128`. Failure = STOP AND REPORT.
Read CLAUDE.md at repo root. Issue body (worktree lacks issues/):
/Users/chrishafley/projects/sprefa/issues/generics-inspection-doc/item.md

GOAL: the user-decreed written inspection of the wrapper/type-constructor set,
required BEFORE any generics implementation. INSPECTION ONLY: document what
exists and how it lowers, with citations. NO design proposals, NO code changes.
Lang design happens with Chris in the room; your output is cited findings and
forks, never decisions.

STARTING POINTS (verify each, then go wider):
- 0_type_plane.pl:145-151 — wrapper inventory
- 0_generic_expand.pl:125-176 — collection artifacts
- 0_option_expand.pl:39-49 — scalar-vs-reference split; :43 is where
  option(<enum>) stops
- compile/out/bounded_template_*.types.* and
  mixed_bounded_and_free_parameters.* — template-instance fixtures already in
  the corpus; find their source fixtures and how instances mint
- issues/wrapper-composition (option(option(T)), option(<enum>)) — the open
  composition question your doc must give the factual base for
- manifest.json — for every wrapper spelling, which fixtures compile vs sit in
  the unsupported bucket, with reasons

DELIVERABLE (two docs, both mandatory; a plan without the second is undelivered):
1. plans/2026-08-15-generics-wrapper-inspection.md — for the auditor: per
   wrapper (list, option, every template/bounded form you find): its
   declaration syntax, expansion path (file:line), emitted DDL/type shape in
   TS and Rust, where each composition stops (throw site file:line), and which
   corpus fixtures exercise it. Every claim carries a path:line or a manifest
   grep. Open with a TOC.
2. plans/2026-08-15-generics-wrapper-inspection.visual.human.unga.md — for
   Chris: plain words, mermaid diagrams (wrapper lattice, lowering pipelines),
   zero citations, tables over prose.

FILES YOU OWN: those two docs only. Everything else read-only.
FORBIDDEN: all .pl, all emitters, fixtures, out/, TASKS/, issues edits other
than the close below.

VALIDATION: every throw site you name re-verified by opening the file at that
line; every "compiles"/"does not compile" claim backed by a manifest.json grep
pasted into doc 1. A construct you believe is impossible gets probed with a
throwaway compile before you write "stops at" (a named error is a hypothesis,
never an edict).

COMMIT plain (docs only). Close:
`issuectl --json close generics-inspection-doc --commit <sha>:<summary>`.
Report: wrapper count, composition matrix summary, top-3 surprises.
