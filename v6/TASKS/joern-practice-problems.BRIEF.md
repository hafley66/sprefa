# joern-practice-problems

## Goal
Write `docs/joern-practice-problems.md`: 5 practice problems for learning Joern
(the CPG query tool) applied to THIS repo's taint-walk corpus, so Chris can
learn Joern's query vocabulary and immediately map it onto sprefa's own CPG
machinery.

## First action
`git merge --ff-only 0fcf78c03` (origin/main). If that fails or the worktree is
missing, STOP AND REPORT. Never work around a blocked command.

## The corpus (read all of it first)
- `v6/tsv2/goldens/cpg_taint_walk_golden/corpus/` — 4 rust files:
  `tainted_handler.rs`, `sanitized_handler.rs`, `two_site_handler.rs`,
  `unrelated_handler.rs`
- `v6/tsv2/goldens/cpg_taint_walk_golden/0_cpg_taint_walk.dl6` — the dl6 taint
  walk that already runs on it
- `v6/tsv2/goldens/cpg_taint_walk_golden/README.md` and
  `2_expected.walk.tsv` — what the walk finds today
- `v6/prolog/cpg_edge_vocab.pl` — the vendored Joern 34-edge reference enum
- `plans/2026-08-16-cpg-spec-research.REPORT.md` — the Joern/CPG research
  report (edge semantics, protobuf study)

## Deliverable shape
One doc, `docs/joern-practice-problems.md`, opening with a TOC. Then exactly 5
problems, ordered easy to hard. Each problem carries:

1. **Scenario** in plain words (2-3 sentences, written for a human learning
   Joern, zero jargon that is not immediately defined).
2. **The Joern concept it teaches** — name the actual Joern query-language
   construct (e.g. `reachableBy`, `ast` steps, `cfgNext`, `dominates`,
   `method.parameter`, data-flow semantics of REACHING_DEF) and cite which
   `cpg_edge_vocab.pl` edge rows it exercises, by line number.
3. **The Joern query** you would type in a Joern shell to solve it (best-effort
   scala snippet; mark any construct you could not verify against the research
   report with `UNVERIFIED`).
4. **Expected answer on this corpus** — concrete: file, function, line/span.
   Derive it by reading the corpus files; do not invent nodes.
5. **The sprefa equivalent** — which existing dl6 rel/walk (or which gap) covers
   the same question. Cite `0_cpg_taint_walk.dl6` line numbers where the
   machinery exists; where it does not exist, say "gap" and name the missing
   edge kind from the vocab enum.

At least one problem must be answerable today by the existing taint walk, and
at least one must land on a named gap.

## File ownership
You own ONLY `docs/joern-practice-problems.md` (new file). Touch nothing else.
No code changes, no corpus edits, no cargo commands, no `cargo fmt` ever.

## Style laws
No em dashes. Descriptive names in every snippet. Prose is captions under
lists/tables. Banned words: provenance, substrate, load-bearing, regime,
refusal.

## Landing
Commit the one file to your branch, push, open a GitHub PR titled
`docs: 5 joern practice problems on the taint-walk corpus`. Body lists the 5
problem titles and which are answerable-today vs gap-landing.
