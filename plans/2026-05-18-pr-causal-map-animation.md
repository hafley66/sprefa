# PR causal-map animation (filtration -> animated D2 notebook)

Status: PARKED IDEA (not scheduled, no impl). 2026-05-18.
Discussion + full design: `chat_log/20260518.4.pr-causal-map-animation-idea.md`.
Throwaway scaffold left parked at `~/projects/sprf-prmap/` (unverified,
not wired, do not build as-is).

## One line

A PR's changed files become one labeled DAG from sprf rule rows; its
filtration (decl ⊆ +impl ⊆ +test ⊆ +dependents) is walked as D2
boards and emitted as an animated-SVG notebook fileset for explaining
PRs / design docs to coworkers.

## Why it fits sprf

- Graph source is sprf itself: `rev` × `diff` × blast-radius /
  implements / tests rule rows. No external static-analysis tool.
- Render is not special-cased: a barrier op emits one cursor per
  bundle file; existing `> write` materializes it. Matches the v4
  self-doc cursor contract.
- Node identity = `blake3(repo,file,lo,hi,kind)`, content-addressed on
  code location -> stable across boards -> D2 morph + click-to-byte.

## Sequence (when picked up)

1. **Rule layer first** (the real work, sprf not Rust): rules that
   turn `diff` into node rows (KIND/NAME/FILE/LO/HI) and edge rows
   (FROM/TO/REL). Nothing visual until this exists.
2. Renderer op `prmap(:tier,:ordering,:out)` — barrier; buffer rows,
   build DAG, walk filtration, emit fileset. (Scaffolded, throwaway.)
3. Host glue: run a `.sprf` file with a custom Registry that
   registered the op.

## Open questions / verification debts

- v4 entrypoint to run a `.sprf` with a custom `Registry` (saw
  `Registry::new()`, not `::default()`).
- `term(:KEY,val)` shape for writing node columns.
- `Value::as_atom`, `Cursor::set`/`Cursor::default`,
  `effect_runtime::v2::queue` re-export surface. Barrier modeled on
  v4 `CollectComponent`.

## Decision

Start at step 1 (rule layer) if revisited. Renderer is the easy half
and is deliberately dumb about the codebase so it never competes with
sprf's graph.
