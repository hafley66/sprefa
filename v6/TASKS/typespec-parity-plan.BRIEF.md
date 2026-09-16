# Lane brief: PLAN polyglot typegen vs TypeSpec (planning only, no code)

First action: `git merge --ff-only 4205d318`. Failure = STOP AND REPORT.

## Task

Fill the seeded contract at
`plans/2026-08-16-typespec-parity-typegen.SEED.md`. Read it top to bottom
first; it defines the question, the inventory to verify, the binding
decisions, the seven comparison axes, and the two deliverable documents.

This is a PLAN lane. You write exactly three files:
`plans/2026-08-16-typespec-parity-typegen.PLAN.md`,
`plans/2026-08-16-typespec-parity-typegen.PLAN.visual.human.unga.md`,
and edits to the SEED file only to correct stale inventory rows (mark
corrections `SEED-CORRECTION:`). You change NO other file.

## Method

1. Verify every inventory row in the seed against the code. A claim without
   path:line does not enter the PLAN.
2. TypeSpec facts from its docs (typespec.io) and repo README; where network
   is unavailable, state knowledge-cutoff facts as such and mark them
   UNVERIFIED. Never invent decorator names or emitter capabilities.
3. Fill all seven axes. Axis 7 (build-vs-buy: a foreign emitter consuming
   type_row IR) gets a real candidate-by-candidate table: TypeSpec emitter
   framework, protobuf descriptors, Smithy, quicktype, at minimum.
4. Sequence the arcs: smallest set of arcs that reaches "a new language
   target is one render_*.dl6 file". Each arc: size (small/med/large per
   `issues/AGENTS.md` routing), files it owns, the gate that proves it,
   what it is blocked on (user fork vs pure work).
5. Forks-for-Chris section: one line per open design call with its throw
   site or absent-code citation. Do NOT settle any of them.

## Receipts

Every number in the PLAN reproduced by a command the PLAN quotes. Run
`cd v6 && just plunit` once to confirm the tree is sane before writing
conclusions from it. Commit both docs in ONE commit with
`COMMENT_RAIL_IDLE_MS=3000 git commit ...`; never pipe a commit; check
`git log` before finishing.

## Laws

- The unga doc: lists, tables, mermaid; prose only as one-line captions.
  A doc that is walls of paragraphs is undelivered.
- Banned words in both docs: provenance, substrate, load-bearing, regime,
  refusal (say TODO / not built yet), "support" (say refCount where the
  rxjs sense is meant).
- No relitigating the binding decisions in the seed.
- A permission denial ends the approach; report, never work around.
