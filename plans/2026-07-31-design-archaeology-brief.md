# Design archaeology: v3/v4 ideas -> dl6 — brief (codex luna, analysis only)

User: "research how far we have come since my language design ideas back in
this repo's revs of time when v4 still existed and v3; once v5 had liftoff
we archived them into git history." ONE retrospective document, no code.

## Sources (read-only, all local)

- ~/projects/sprefa-archive-20260701 — v3/v4 working trees + FULL git
  history (the pre-v5 line). Design docs, plans/, READMEs, early DSL
  sketches live in the revs, not just the tips: walk `git log` for
  design-heavy commits and read files AT those revs.
- ~/projects/sprefa-archive-20260428 — the OG coordinate model
  (strings/refs/byte-spans).
- THIS repo's own history before v5 liftoff (v5 lifted to root
  2026-07-01) and after: README.md, plans/, docs/vision-*.
- The present tense: v6/prolog/compile/SYNTAX.md, TICK-MODEL.md,
  conformance/rulings.pl (the full ruling record), v6/GETTING-STARTED.md.

If reading the archive paths is sandbox-blocked, STOP AND REPORT.

## The document: plans/2026-07-31-design-archaeology.md

1. TIMELINE of the language ideas: for each era (coordinate model, v3,
   v4, v5, v6/dl6) the core model in 2-3 sentences WITH file@rev
   citations (path + short sha + a verbatim quote each).
2. IDEA LEDGER, the heart: every recurring design idea traced across
   eras — where born, how each generation spelled it, what dl6 does
   today. Expected members (verify, do not assume): facts-from-code
   extraction, coordinates/byte-spans, reactive recomputation,
   datalog-over-SQL, the daemon, globs/scan, diagnostics/LSP, codegen,
   the tick/delta model, content addressing, self-hosting/dogfood.
   Mark each SURVIVED / TRANSFORMED (into what) / DIED (why, if the
   record says).
3. WHAT ONLY EXISTS NOW: things v6 has that no earlier generation
   attempted (byte-graded oracle, named refusals, endurance law,
   emitted-SQL IVM, executed docs).
4. WHAT WAS LOST: anything an old generation had that no v6 row or plan
   carries — each becomes a named candidate for the shelf, cited.
5. Keep it readable: this is a story for the language's author, not a
   scorecard. Quotes over paraphrase where the old text is vivid.

## Fences

- Write ONLY the one plans/ doc. Read anything (archives are read-only
  by nature; do not run git commands that write in the archives —
  `git -C <archive> log/show` only).
- Commit `git commit -n`; no push.
