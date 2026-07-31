# Self-map single-file dogfood — brief (codex luna)

User ruling release_gate_v620 = arch_from_single_dl6_file: "no release till
u give me arch from single dl6 file. dogfood." The v6.2.0 push is gated on
this lane.

## Current state

`just self-map` emits v6/ARCH-MAP.md (4 mermaid diagrams). The dl6 share is
high (~80-95% per diagram) but a python renderer does the mermaid TEXT
ASSEMBLY because dl6 had no string aggregate. That gap died today:
`group_concat(Value, Sep, Ordinal)` is live (ordered-aggregate landing) —
the mermaid-line-assembly sighting was the design driver for exactly this.

## The task

ONE dl6 program (v6/dl/fixtures/self-map.dl6 or successor) produces the
ENTIRE ARCH-MAP.md text: fact ingestion, derivation, mermaid line
rendering, section assembly, final document — all as rels; the python
renderer is DELETED. The only non-dl6 residue allowed: the sh host that
reads source fact files in, and the effect that writes the one output file
out (world I/O is host territory by spine_residency; text CONSTRUCTION is
not).

Shape hints (do not treat as law, measure): line rels carry
(section, ordinal, line_text); `group_concat(LineText, '\n', Ordinal)`
folds sections; a final concat joins sections. Ordinals via arithmetic or
the existing cursor idiom (seq may land from a concurrent lane — do NOT
depend on it).

## Receipts required

- `just self-map` runs the new path; output byte-comparable to the current
  ARCH-MAP.md (structural identity; state any deliberate rendering diffs).
- Run-twice-identical (the existing rail receipt).
- The dl6-vs-glue split stated per diagram: the point is the dl6 share hits
  ~100% minus the two I/O hosts. Any remaining glue = a named language gap
  in your report.
- python renderer deleted; `git grep` shows no python in the self-map path.
- Battery: conformance, green (self-map is in green-all; EPERM legs
  reported not worked around), staleness gate.

## Fences

- Touch: self-map.dl6 + its rail script + justfile self-map recipe +
  ARCH-MAP.md regen. Nothing else.
- Do NOT touch: registry/parser/emitter (a concurrent seq lane owns them),
  devlog files, labs/**.
- No-commit flow. STOP AND REPORT on blocked commands.
