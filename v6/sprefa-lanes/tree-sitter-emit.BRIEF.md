# lab/tree-sitter-emit: one grammar to feed them all

## Mission (user decree 2026-08-11)
Three phases, in order, each with its own gate and its own honest-FAIL
exit. The question behind all three: can v6/prolog/compile/parse_dl_dcg.pl
be the ONE hand-written description of dl6, with tree-sitter grammar,
topiary formatting, and an LSP all emitted from it or bought off the
shelf — instead of six hand-maintained encodings of the language.

## First action (worktree law)
Verify your base: `git log --oneline -1` must show 91e47adb
("prolog: declare utf8 encoding at the compile hub"). If it shows a
DIFFERENT sha, that is the coordinator moving the branch forward, which is
normal: report the sha you see in your final message and PROCEED. Only an
absent worktree or a missing v6/labs/tree-sitter-door/ is a stop.
Your tree already contains
v6/labs/tree-sitter-door/ (hand grammar covering a 62-line slice,
run-tests.sh gate, REPORT.md with a DCG->grammar.js mapping table) AND
the post-alchemy DCG parser (26473 non-ws chars, pure --> notation).

## Phase A: finish the hand-made grammar

The text-door corpus is GENERATED, not committed. A prior lane stopped
because the directory was absent; that was a brief defect, now fixed.
Generate it FIRST, before any inventory:

```bash
cd <worktree>/v6 && just text-door     # writes prolog/compile/out/text-door/*.dl6
ls <worktree>/v6/prolog/compile/out/text-door/*.dl6 | wc -l   # expect 266
```

Running gates and generators is always allowed; file OWNERSHIP (below)
restricts only what you EDIT. If the count is not 266, report the number
you got and keep going against what exists.

Extend v6/labs/tree-sitter-door/grammar.js until:
1. ALL 630 lines of v6/dl/fixtures/golden-flex.dl6 parse with zero
   ERROR/MISSING nodes (the current gate checks lines 175-236 only).
2. Every generated .dl6 file in v6/prolog/compile/out/text-door/ parses
   with zero ERROR/MISSING nodes.
Extend run-tests.sh to assert both; print counts
(TS_CORPUS total=<n> clean=<n> errors=<n>) so the coordinator can rerun.
Precedence conflicts are expected: resolve with prec/prec.left and note
each one in the REPORT — the emitter phase needs that list.

## Phase B: the emitter probe (the forbidden jutsu)
Write v6/labs/tree-sitter-door/emit_grammar.pl: SWI-Prolog that reads
parse_dl_dcg.pl as terms (read_term/3 loop; the file is pure DCG -->
clauses after PRs #169/#172/#173) and EMITS a grammar.js for whatever subset maps
mechanically. Then diff emitted vs your Phase-A hand grammar. Deliverable
is a table: every grammar.js rule classified EMITTED-IDENTICAL /
EMITTED-NEEDS-OVERLAY(reason) / HAND-ONLY(reason). Known non-derivable
(from the door lab): precedence declarations, regex-shaped tokens vs
char-code predicates, error recovery. Measure the overlay: non-ws chars
of hand-overlay code vs emitted code. The verdict we need: does the
overlay stay a small fixed file, or does it grow per-construct (= a
third grammar in disguise, jutsu fails).

## Phase C: LSP candidate analysis (research, NOT implementation)
Build-vs-buy law: written candidate-by-candidate analysis, no one-line
dismissals, no bespoke server code. Survey at minimum:
1. langium (v6/dl/grammar/dl.langium is a stale narrow slice; langium
   GENERATES a full LSP from a grammar — could Phase-B-style emission
   target langium's grammar language instead of/as well as grammar.js?)
2. tree-sitter-based LSP glue (what do existing langs get from CST alone:
   highlight, folding, outline — vs what needs the compiler: diagnostics,
   go-to-def, completion).
3. The compiler itself as the language server backend (swipl speaks
   stdio; the manifest/oracle already computes diagnostics per fixture).
4. Anything else you find maintained (cite repo + commit activity).
Deliverable: a fan-out table — LSP capability x source (CST / compiler /
hand code) x candidate — and ONE recommended shot at the all-in-1, with
its price. Design forks come back cited for the user to rule; do not
settle language design.

## Deliverables
- Commits in the worktree (prefix `lab:`), max 6, gates green per phase.
- v6/labs/tree-sitter-door/REPORT2.md: phase verdicts, the
  emitted-vs-hand table, overlay measurement, LSP fan-out table,
  recommendation with price.
- Blocked or phase failed: FAILURE-REPORT-EMIT.md, exact command +
  output, exit nonzero. A clean Phase-A + failed Phase-B is a VALUABLE
  result; report it straight.

## Files you own
v6/labs/tree-sitter-door/** only. Never touch parse_dl_dcg.pl, print_dl,
or anything outside the lab. NEVER git merge/pull/rebase past the first
action. No push, no PR; coordinator judges. Lanes never spawn subagents.

## Style
Banned words, prose and identifiers: provenance, substrate, load-bearing,
regime, refusal. Package manager is pnpm, never npm. If reality deviates
from this brief, STOP and report; do not improvise.
