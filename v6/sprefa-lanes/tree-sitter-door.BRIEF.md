# lab/tree-sitter-door: prove the tree-sitter + topiary door opens

## Mission (recon to FIRST PASSING TEST, not the full arc)
Two claims need proof or refutation, each with a runnable receipt:
1. A tree-sitter grammar for dl6 can parse a real fixture: write a
   MINIMAL grammar.js covering enough of the language to parse
   v6/dl/fixtures/golden-flex.dl6 (or, if that is too much, a contiguous
   subset you extract to a lab fixture — say which lines) with ZERO ERROR
   nodes, and a test that asserts it.
2. Topiary can format dl6 through that grammar: a formatting-queries
   file that reformats a small dl6 snippet to the repo's formatting law
   (single-line only rel decls and <=2-term facts; rules break per-goal,
   2-space indent) and is IDEMPOTENT (format(format(x)) == format(x)),
   with a test that asserts it.
The reference grammar is v6/prolog/compile/parse_dl_dcg.pl (READ ONLY):
677-file parity with classic makes it the single source of truth. Note
in your report where a grammar.js rule maps 1:1 from a DCG nonterminal
and where tree-sitter's model (GLR, no {} semantic actions) forces a
different shape — that mapping table IS the recon deliverable.

## Ground rules
- Everything lives under v6/labs/tree-sitter-door/ in YOUR worktree:
  grammar dir, queries, tests, REPORT.md. No file outside it changes.
- Tool availability: check `tree-sitter --version` and
  `topiary --version` / `npx tree-sitter-cli`. Local installs INSIDE the
  lab dir (npm/npx, cargo install --root ./tools) are allowed; global
  installs are NOT. If a tool cannot be obtained locally, STOP: report
  what you tried, exact errors, exit nonzero.
- Build-vs-buy law: before writing grammar.js from scratch, spend 10
  minutes checking whether an existing tree-sitter grammar (datalog,
  prolog, souffle) is a usable starting skeleton; name what you checked
  and why you kept or dropped it in REPORT.md.
- REPORT.md structure: verdict first (VIABLE / VIABLE-WITH-CAVEATS /
  BLOCKED per claim), then the receipts (test commands + output), then
  the DCG->grammar.js mapping table, then effort estimate for the full
  arc (real grammar, highlighting, sprefa-extract integration, topiary
  as the repo formatter).

## Gate
```bash
cd <worktree>/v6/labs/tree-sitter-door && ./run-tests.sh   # you write this; rc=0 = both claims' tests pass
```
A BLOCKED verdict with receipts and a nonzero exit is a valid outcome;
a report without runnable tests is not.

## Rails
- rc=0 with dirty tree, no commits, or red gates is a DEFECT. Blocked ->
  the REPORT.md verdict + exit NONZERO.
- NEVER git merge / pull / rebase in the worktree. NEVER --no-verify.
- Up to 3 commits, prefix `lab:`. No push, no PR; coordinator harvests.
- Labs die on landing: expect your files to be distilled and deleted;
  REPORT.md is the artifact that survives.

## Style
Comment budget: max 2 consecutive lines. Banned words, prose and
identifiers: provenance, substrate, load-bearing, regime, refusal.
