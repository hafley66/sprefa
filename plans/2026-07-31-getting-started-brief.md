# GETTING-STARTED dogfood — brief (opus worktree)

Beta gate item 4 (plans/2026-07-30-v6-beta-plan.md). A stranger's first 30
minutes: install, first program, watch + extract, query, read an error.
Every code block must be EXECUTED, not imagined — the devlog/self-map
precedent says docs are graded artifacts here.

## Shape

- v6/GETTING-STARTED.md. Sections: (1) prereqs + one-command setup (pnpm
  install + swipl; state versions from your own machine checks); (2) first
  program: a 10-line .dl6 through `bop run`, shown with its actual output;
  (3) making it react: a watch bind + extraction over a scratch dir, real
  transcript; (4) queries via `bop q`; (5) reading an error: a deliberately
  broken program and the ACTUAL rendered refusal (file:line, functor) —
  the refusal-messages landing makes this section possible; (6) where to go
  next (SYNTAX.md, READINESS.md, DEVLOG.md).
- A receipt script (v6/tsv2/scripts/getting-started.sh) that runs every
  code block from the doc and diffs captured outputs, so the doc CANNOT go
  stale silently (gen_staleness_gate class). Wire `just getting-started`.
  Outputs with volatile parts (timings, paths) normalize before diff.
- Keep prose lean. The doc teaches by transcript, not essay.

## Receipts

`just getting-started` exit 0 twice; conformance untouched; the broken
program's rendered error shown verbatim in the doc matches the live run.

## Fences

- Touch: GETTING-STARTED.md, its receipt script, one justfile recipe.
- Do NOT touch: compiler/runtime/emitter, clock files, bench-cli/**,
  fixtures, READINESS.md content (link to it only), labs/**.
- pnpm install per package, NEVER symlink outer node_modules.
- Commit per step `git commit -n`; no push.
