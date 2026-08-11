# feature/parse-dcg-v2: the DCG parser, side by side behind a toggle

## User decree 2026-08-11
The v2 parser lives BESIDE the current one with a boolean toggle, same
inputs, so the existing fast battery splices the experiment. No isolated
divergent tree. A sibling lane (feature/parse-splice-harness) builds the
comparator; you build the parser and the toggle. DO NOT build comparison
scripts, that lane owns them.

## Integrity rail, stated because a prior lane violated it
Exiting rc=0 with a dirty tree, no commits, or red gates is a DEFECT.
Blocked means FAILURE-REPORT-DCG.md with the exact command + output and a
NONZERO exit. Your work will be independently re-verified; unverifiable
claims are treated as failures.

## The work
1. New file v6/prolog/compile/parse_dl_dcg.pl: the SAME grammar as
   parse_dl.pl rewritten as real DCGs (`-->`). parse_dl.pl already uses
   `-->` in 15 places; those clauses move over nearly verbatim. The
   hand-threaded clauses (`p(Args, S0, S) :- q(S0,S1), r(S1,S)`) become
   `p(Args) --> q, r` with term construction in `{}` escapes. Preserve
   clause order, cuts, and every thrown error term EXACTLY: the two
   parsers must produce identical program terms AND identical
   unsupported_construct throws on the same input.
2. The toggle: one seam in the compile entry (find where compile.pl calls
   the parse entry point) selecting by environment variable
   DL_PARSER=dcg (default absent = classic parse_dl.pl). One read site,
   no scattered branching.
3. Migrate section by section; a section not yet migrated FAILS LOUDLY
   under DL_PARSER=dcg (missing nonterminal = existence error is
   acceptable), never silently falls back to the classic parser.
4. Receipt in each commit message: which grammar sections are migrated,
   and the corpus parity count under the toggle if the sibling harness
   has landed by then (run it if present at
   v6/prolog/compile/scripts/parse_parity.*; do not write it).

## Files you own
- v6/prolog/compile/parse_dl_dcg.pl (new)
- the ONE toggle seam in the compile entry (smallest possible diff there)
Do NOT touch parse_dl.pl itself, print_dl.pl, 0_generic_expand.pl,
golden-flex.dl6: three other lanes own them.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```
All of the above run with the toggle OFF and must stay green (proves the
default path untouched). Additionally run `DL_PARSER=dcg just conformance`
and report its pass count for migrated sections in the final commit
message; full green under the toggle is the arc's finish line, partial
migration with an accurate count is an acceptable commit state.

## Rails
- NEVER git merge / pull / rebase in the worktree.
- NEVER --no-verify. Up to 3 commits, prefix `prolog:`. Comment budget:
  max 2 consecutive comment lines per touched hunk.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
